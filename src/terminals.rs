use std::io::Read as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use objc2_app_kit::{NSRunningApplication, NSWorkspace};
use objc2_foundation::NSString;
use raiden_core::terminals::{self, Ends, Found, Run, Terminal};
use tauri::{AppHandle, State};

use crate::files::Files;

const SOON: Duration = Duration::from_secs(10);
const STAYS: Duration = Duration::from_millis(300);
const POLL: Duration = Duration::from_millis(20);
const SAID: Duration = Duration::from_secs(1);

pub fn app_of(terminal: Terminal) -> Option<PathBuf> {
    let url = NSWorkspace::sharedWorkspace()
        .URLForApplicationWithBundleIdentifier(&NSString::from_str(terminal.bundle_id()))?;
    let path = PathBuf::from(url.path()?.to_string());
    path.is_dir().then_some(path)
}

fn running(terminal: Terminal) -> bool {
    !NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(
        terminal.bundle_id(),
    ))
    .is_empty()
}

pub fn in_use(chosen: Option<Terminal>) -> Terminal {
    terminals::in_use(chosen, |terminal| app_of(terminal).is_some())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listed {
    id: &'static str,
    name: &'static str,
    app: Option<String>,
}

pub fn listed() -> Vec<Listed> {
    let apps: Vec<(Terminal, Option<PathBuf>)> = Terminal::ALL
        .into_iter()
        .map(|terminal| (terminal, app_of(terminal)))
        .collect();
    let app = |terminal: Terminal| {
        apps.iter()
            .find(|(known, _)| *known == terminal)
            .and_then(|(_, app)| app.clone())
    };
    terminals::listed(|terminal| app(terminal).is_some())
        .into_iter()
        .map(|terminal| Listed {
            id: terminal.id(),
            name: terminal.name(),
            app: app(terminal).map(|path| path.display().to_string()),
        })
        .collect()
}

fn failed(terminal: Terminal, why: &str) -> String {
    let said = why.lines().map(str::trim).find(|line| !line.is_empty());
    match said {
        Some(line) => format!("{} did not open: {line}", terminal.name()),
        None => format!("{} did not open.", terminal.name()),
    }
}

fn stop_group(child: &Child) {
    if let Ok(group) = libc::pid_t::try_from(child.id()) {
        unsafe {
            libc::killpg(group, libc::SIGKILL);
        }
    }
}

fn start(terminal: Terminal, run: &Run) -> Result<(), String> {
    let limit = match run.ends {
        Ends::Soon => SOON,
        Ends::MayStay => STAYS,
    };
    start_within(terminal, run, limit)
}

fn said_within(reader: Option<JoinHandle<String>>, limit: Duration) -> String {
    let Some(reader) = reader else {
        return String::new();
    };
    let until = Instant::now() + limit;
    while !reader.is_finished() && Instant::now() < until {
        std::thread::sleep(POLL);
    }
    if reader.is_finished() {
        reader.join().unwrap_or_default()
    } else {
        String::new()
    }
}

fn start_within(terminal: Terminal, run: &Run, limit: Duration) -> Result<(), String> {
    let mut command = Command::new(&run.program);
    command
        .args(&run.arguments)
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .process_group(0);
    command.stderr(match run.ends {
        Ends::Soon => Stdio::piped(),
        Ends::MayStay => Stdio::null(),
    });
    let mut child = command.spawn().map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => format!(
            "Raiden could not find {} at {}.",
            terminal.name(),
            run.program.display()
        ),
        _ => format!("Raiden could not start {}: {error}", terminal.name()),
    })?;
    let stderr = child.stderr.take().map(|mut stream| {
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = stream.read_to_string(&mut text);
            text
        })
    });
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(_)) => return Err(failed(terminal, &said_within(stderr, SAID))),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
            Ok(None) if run.ends == Ends::MayStay => {
                std::thread::spawn(move || child.wait());
                return Ok(());
            }
            Ok(None) => {
                stop_group(&child);
                let _ = child.wait();
                return Err(format!("{} did not answer in time.", terminal.name()));
            }
            Err(error) => return Err(failed(terminal, &error.to_string())),
        }
    }
}

async fn open(app: &AppHandle, terminal: Terminal, folder: PathBuf) -> Result<(), String> {
    let bundle = app_of(terminal)
        .ok_or_else(|| format!("{} is not installed on this Mac.", terminal.name()))?;
    let tern = match terminal {
        Terminal::Tern => {
            raiden_core::ai::environment::which("tern", &crate::ai::login_path(app).await).filter(
                |found| std::fs::canonicalize(found).is_ok_and(|real| real.starts_with(&bundle)),
            )
        }
        _ => None,
    };
    let found = Found {
        app: &bundle,
        running: running(terminal),
        tern: tern.as_deref(),
    };
    let run = terminals::plan(terminal, &folder, &found)?;
    tauri::async_runtime::spawn_blocking(move || start(terminal, &run))
        .await
        .map_err(|error| error.to_string())?
}

async fn open_in_use(app: &AppHandle, folder: PathBuf) -> Result<(), String> {
    let terminal = in_use(crate::settings::terminal(app));
    open(app, terminal, folder).await?;
    crate::hide_panel(app);
    Ok(())
}

fn folder_found(files: &Files, path: &str) -> Result<PathBuf, String> {
    if !crate::actions::was_found(files, path) {
        return Err(format!(
            "Raiden opens only what file search found in a terminal, and {path} is not one."
        ));
    }
    let target = Path::new(path);
    Ok(terminals::folder_of(target, target.is_dir()).to_path_buf())
}

#[tauri::command]
pub async fn open_in_terminal(
    app: AppHandle,
    files: State<'_, Files>,
    path: String,
) -> Result<(), String> {
    let folder = folder_found(&files, &path)?;
    open_in_use(&app, folder).await
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    fn run(program: &str, arguments: &[&str], ends: Ends) -> Run {
        Run {
            program: PathBuf::from(program),
            arguments: arguments.iter().map(OsString::from).collect(),
            ends,
        }
    }

    const SHORT: Duration = Duration::from_millis(300);
    const LONG: Duration = Duration::from_secs(10);

    #[test]
    fn only_what_file_search_showed_is_opened() {
        let files = Files::start();
        let home = std::env::var("HOME").unwrap_or_default();
        for path in [home.as_str(), "/etc/passwd", "/tmp", "relative"] {
            assert!(
                folder_found(&files, path)
                    .is_err_and(|why| why.starts_with("Raiden opens only what file search found")),
                "{path}"
            );
        }
    }

    #[test]
    fn a_program_that_ends_well_opened_the_terminal() {
        let ended = run("/usr/bin/true", &[], Ends::Soon);
        assert_eq!(start_within(Terminal::Ghostty, &ended, LONG), Ok(()));
    }

    #[test]
    fn a_program_that_fails_says_what_it_printed() {
        let failing = run(
            "/bin/sh",
            &["-c", "echo '  '; echo no such folder >&2; exit 3"],
            Ends::Soon,
        );
        assert_eq!(
            start_within(Terminal::Alacritty, &failing, LONG),
            Err("Alacritty did not open: no such folder".to_owned())
        );
        let silent = run("/usr/bin/false", &[], Ends::Soon);
        assert_eq!(
            start_within(Terminal::Rex, &silent, LONG),
            Err("Rex did not open.".to_owned())
        );
    }

    #[test]
    fn a_missing_program_is_named() {
        let missing = run("/nowhere/kitty", &[], Ends::MayStay);
        assert_eq!(
            start_within(Terminal::Kitty, &missing, SHORT),
            Err("Raiden could not find Kitty at /nowhere/kitty.".to_owned())
        );
    }

    #[test]
    fn a_terminal_that_stays_running_counts_as_opened() {
        let staying = run("/bin/sleep", &["2"], Ends::MayStay);
        let started = Instant::now();
        assert_eq!(start_within(Terminal::Tern, &staying, SHORT), Ok(()));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_program_that_should_end_soon_and_hangs_is_stopped() {
        let marker = std::env::temp_dir().join(format!("raiden-hang-{}", std::process::id()));
        let script = format!("sleep 1; touch '{}'", marker.display());
        let hanging = run("/bin/sh", &["-c", &script], Ends::Soon);
        assert_eq!(
            start_within(Terminal::Terminal, &hanging, SHORT),
            Err("Terminal did not answer in time.".to_owned())
        );
        std::thread::sleep(Duration::from_millis(1500));
        assert!(!marker.exists());
    }
}
