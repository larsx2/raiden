use std::ffi::OsStr;
use std::io::Read as _;
use std::io::{BufRead as _, BufReader, Write as _};
use std::os::fd::AsRawFd as _;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::PoisonError;
use std::time::Duration;

use raiden_core::control::{self, Reply};
use tauri::AppHandle;

fn peer_pid(stream: &UnixStream) -> Option<libc::pid_t> {
    let mut pid: libc::pid_t = 0;
    let mut length = libc::socklen_t::try_from(size_of::<libc::pid_t>()).ok()?;
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_LOCAL,
            libc::LOCAL_PEERPID,
            (&raw mut pid).cast(),
            &raw mut length,
        )
    };
    (result == 0).then_some(pid)
}

fn peer_uid(stream: &UnixStream) -> Option<libc::uid_t> {
    let (mut uid, mut gid) = (0, 0);
    let result = unsafe { libc::getpeereid(stream.as_raw_fd(), &raw mut uid, &raw mut gid) };
    (result == 0).then_some(uid)
}

fn executable(pid: libc::pid_t) -> Option<PathBuf> {
    let mut buffer = vec![0_u8; usize::try_from(libc::PROC_PIDPATHINFO_MAXSIZE).ok()?];
    let capacity = u32::try_from(buffer.len()).ok()?;
    let length = unsafe { libc::proc_pidpath(pid, buffer.as_mut_ptr().cast(), capacity) };
    let length = usize::try_from(length).ok().filter(|length| *length > 0)?;
    Some(PathBuf::from(OsStr::from_bytes(&buffer[..length])))
}

fn from_raiden(stream: &UnixStream) -> bool {
    let same_user = peer_uid(stream) == Some(unsafe { libc::getuid() });
    let ours = std::env::current_exe().and_then(std::fs::canonicalize).ok();
    let theirs = peer_pid(stream)
        .and_then(executable)
        .and_then(|path| std::fs::canonicalize(path).ok());
    same_user && ours.is_some() && ours == theirs
}

fn hidden_for(app: &AppHandle) -> Reply {
    use tauri::Manager as _;
    let visible = app
        .get_webview_window(crate::PANEL)
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false);
    if visible {
        return Reply::ok("0");
    }
    let since = *app
        .state::<crate::HiddenSince>()
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    Reply::ok(since.elapsed().as_secs().to_string())
}

fn serve(stream: UnixStream, app: &AppHandle) {
    if !from_raiden(&stream) {
        let _ = (&stream).write_all(
            Reply::fail("Only raiden probe may ask Raiden.")
                .encode()
                .as_bytes(),
        );
        return;
    }
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut line = String::new();
    let read = BufReader::new((&stream).take(4096)).read_line(&mut line);
    if read.is_err() || !line.ends_with('\n') {
        let _ = (&stream).write_all(Reply::fail("The request was too long.").encode().as_bytes());
        return;
    }
    let reply = if line.trim_end() == control::HIDDEN_FOR {
        hidden_for(app)
    } else {
        Reply::fail(format!("Raiden does not know {:?}", line.trim()))
    };
    let _ = (&stream).write_all(reply.encode().as_bytes());
}

pub fn listen(app: &AppHandle, folder: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700))?;
    let path = folder.join(control::SOCKET);
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let app = app.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let app = app.clone();
            std::thread::spawn(move || serve(stream, &app));
        }
    });
    Ok(())
}

pub fn running(folder: &Path) -> bool {
    UnixStream::connect(folder.join(control::SOCKET)).is_ok()
}

pub fn ask_hidden_for(folder: PathBuf) -> impl Fn() -> Reply {
    move || {
        let path = folder.join(control::SOCKET);
        let Ok(mut stream) = UnixStream::connect(&path) else {
            return Reply::fail("Raiden is not running. Open it, then try again.");
        };
        if stream
            .write_all(format!("{}\n", control::HIDDEN_FOR).as_bytes())
            .is_err()
        {
            return Reply::fail("Raiden stopped listening.");
        }
        let mut line = String::new();
        match BufReader::new(stream).read_line(&mut line) {
            Ok(_) => Reply::decode(&line),
            Err(error) => Reply::fail(format!("Raiden did not answer: {error}")),
        }
    }
}
