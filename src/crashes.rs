use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, UNIX_EPOCH};

use raiden_core::crashes::{self, Crash, Report};
use tauri::Manager as _;

use crate::picks::now;

static LOG: OnceLock<PathBuf> = OnceLock::new();

const REPORTS_SETTLE: Duration = Duration::from_secs(10);

fn note(crash: &Crash) {
    let Some(path) = LOG.get() else {
        return;
    };
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
        })
        .and_then(|mut file| file.write_all(crashes::line(crash).as_bytes()));
    if let Err(error) = written {
        eprintln!("Raiden could not write {}: {error}", path.display());
    }
}

pub fn watch(development: bool) {
    let _ = LOG.set(crate::folders(development).0.join(crashes::LOG));
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        note(&Crash {
            at_seconds: now(),
            what: info.to_string(),
        });
        previous(info);
        std::process::abort();
    }));
    std::thread::spawn(|| {
        std::thread::sleep(REPORTS_SETTLE);
        note_reports();
    });
}

pub fn reload(webview: &tauri::Webview) {
    crate::settings::stop_recording(webview.app_handle());
    note(&Crash {
        at_seconds: now(),
        what: "the panel's web view stopped, so Raiden reloaded it".to_owned(),
    });
    if let Err(error) = webview.reload() {
        eprintln!("Raiden could not reload its panel: {error}");
    }
}

fn reports(folder: &Path, program: &str) -> Vec<Report> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !crashes::may_be_ours(program, &name) {
                return None;
            }
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some(Report {
                at_seconds: modified.duration_since(UNIX_EPOCH).ok()?.as_secs(),
                contents: std::fs::read_to_string(entry.path()).ok()?,
                name,
            })
        })
        .collect()
}

fn note_reports() {
    let (Some(log), Some(home)) = (LOG.get(), std::env::var_os("HOME")) else {
        return;
    };
    let Some(program) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.file_name()?.to_str().map(str::to_owned))
    else {
        return;
    };
    let folder = Path::new(&home).join("Library/Logs/DiagnosticReports");
    let noted = std::fs::read_to_string(log).unwrap_or_default();
    for crash in crashes::unnoted_reports(&noted, &program, &reports(&folder, &program)) {
        note(&crash);
    }
}
