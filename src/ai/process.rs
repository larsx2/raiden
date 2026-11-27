use std::path::Path;
use std::process::Stdio;
use std::sync::Once;
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;

use tokio::process::{Child, Command};

const MOST_GROUPS: usize = 64;
const POLL: Duration = Duration::from_millis(50);

static GROUPS: [AtomicI32; MOST_GROUPS] = [const { AtomicI32::new(0) }; MOST_GROUPS];
static GUARD: Once = Once::new();

pub fn kill_every_group() {
    for slot in &GROUPS {
        let group = slot.load(Ordering::SeqCst);
        if group > 0 {
            unsafe {
                libc::killpg(group, libc::SIGKILL);
            }
        }
    }
}

extern "C" fn at_exit() {
    kill_every_group();
}

extern "C" fn on_signal(signal: libc::c_int) {
    kill_every_group();
    unsafe {
        libc::signal(signal, libc::SIG_DFL);
        libc::raise(signal);
    }
}

fn guard() {
    GUARD.call_once(|| {
        unsafe {
            libc::atexit(at_exit);
            for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
                libc::signal(
                    signal,
                    on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t,
                );
            }
        }
        let earlier = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            kill_every_group();
            earlier(info);
        }));
    });
}

fn register(group: libc::pid_t) {
    for slot in &GROUPS {
        if slot
            .compare_exchange(0, group, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return;
        }
    }
}

fn unregister(group: libc::pid_t) {
    for slot in &GROUPS {
        let _ = slot.compare_exchange(group, 0, Ordering::SeqCst, Ordering::SeqCst);
    }
}

fn exited_unreaped(leader: libc::pid_t) -> bool {
    let mut info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
    let found = unsafe {
        libc::waitid(
            libc::P_PID,
            libc::id_t::try_from(leader).unwrap_or_default(),
            info.as_mut_ptr(),
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    found == 0 && unsafe { info.assume_init() }.si_pid == leader
}

pub struct Group {
    pub child: Child,
    group: Option<libc::pid_t>,
}

impl Group {
    pub fn spawn(command: &mut Command) -> std::io::Result<Self> {
        guard();
        command.kill_on_drop(true).process_group(0);
        let child = command.spawn()?;
        let group = child.id().and_then(|id| libc::pid_t::try_from(id).ok());
        if let Some(group) = group {
            register(group);
        }
        Ok(Self { child, group })
    }

    fn release_if_exited(&mut self) -> bool {
        let Some(group) = self.group else {
            return true;
        };
        if !exited_unreaped(group) {
            return false;
        }
        unsafe {
            libc::killpg(group, libc::SIGKILL);
        }
        unregister(group);
        self.group = None;
        true
    }

    pub fn running(&mut self) -> bool {
        if self.release_if_exited() {
            let _ = self.child.try_wait();
            return false;
        }
        true
    }

    async fn exited(&mut self) {
        while !self.release_if_exited() {
            tokio::time::sleep(POLL).await;
        }
    }
}

impl Drop for Group {
    fn drop(&mut self) {
        if let Some(group) = self.group.take() {
            unsafe {
                libc::killpg(group, libc::SIGKILL);
            }
            unregister(group);
        }
    }
}

pub fn command(program: &Path, env: &[(String, String)], cwd: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .env_clear()
        .envs(env.iter().map(|(name, value)| (name, value)))
        .current_dir(cwd);
    command
}

pub struct Ran {
    pub succeeded: bool,
    pub output: String,
}

pub async fn run(mut command: Command, limit: Duration) -> std::io::Result<Ran> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let group = Group::spawn(&mut command)?;
    let output = tokio::time::timeout(limit, wait(group))
        .await
        .map_err(|_| std::io::Error::other("it did not finish in time"))??;
    Ok(output)
}

async fn wait(mut group: Group) -> std::io::Result<Ran> {
    use tokio::io::AsyncReadExt;
    let mut stdout = group.child.stdout.take();
    let mut stderr = group.child.stderr.take();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let read_out = async {
        if let Some(stdout) = stdout.as_mut() {
            let _ = stdout.read_to_end(&mut out).await;
        }
    };
    let read_err = async {
        if let Some(stderr) = stderr.as_mut() {
            let _ = stderr.read_to_end(&mut err).await;
        }
    };
    tokio::join!(read_out, read_err);
    group.exited().await;
    let status = group.child.wait().await?;
    let mut output = String::from_utf8_lossy(&out).into_owned();
    if !err.is_empty() {
        output.push('\n');
        output.push_str(&String::from_utf8_lossy(&err));
    }
    Ok(Ran {
        succeeded: status.success(),
        output,
    })
}
