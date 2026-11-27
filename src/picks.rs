use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSString, NSURL};
use raiden_core::apps::App;
use raiden_core::{search, usage};
use tauri::{AppHandle, State};

pub struct Usage(pub PathBuf);

#[derive(serde::Serialize)]
pub struct Listing {
    apps: Vec<App>,
    catalog: String,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

pub fn listing(apps: Vec<App>, usage_file: &Path) -> Listing {
    let picks = usage::parse(&std::fs::read_to_string(usage_file).unwrap_or_default());
    let now = now();
    let mut rows_by_path: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, app) in apps.iter().enumerate() {
        rows_by_path
            .entry(app.path.as_str())
            .or_default()
            .push(index);
    }
    let picked = picks.iter().flat_map(|pick| {
        rows_by_path
            .get(pick.path.as_str())
            .into_iter()
            .flatten()
            .map(move |index| (*index, usage::age_days(pick, now), pick.query.as_str()))
    });
    let catalog = search::catalog_text(apps.iter().map(|app| app.name.as_str()), picked);
    Listing { apps, catalog }
}

fn record(file: &Path, pick: &usage::Pick) -> std::io::Result<()> {
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)?
        .write_all(usage::line(pick).as_bytes())
}

pub fn note(file: &Path, path: &str, query: String, row: usize) {
    let pick = usage::Pick {
        at_seconds: now(),
        row,
        query,
        path: path.to_owned(),
    };
    if let Err(error) = record(file, &pick) {
        eprintln!("Raiden could not note {path}: {error}");
    }
}

#[tauri::command]
pub async fn open(
    app: AppHandle,
    usage: State<'_, Usage>,
    path: String,
    query: String,
    row: usize,
) -> Result<(), String> {
    let setting = raiden_core::system_settings::find(&path).is_some();
    if !setting && (!path.ends_with(".app") || !Path::new(&path).is_dir()) {
        return Err(format!(
            "Raiden opens only apps and settings, and {path} is neither."
        ));
    }
    crate::hide_panel(&app);
    let target = path.clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let target = NSString::from_str(&target);
        let url = if setting {
            NSURL::URLWithString(&target)
        } else {
            Some(NSURL::fileURLWithPath(&target))
        };
        let opened = url.is_some_and(|url| NSWorkspace::sharedWorkspace().openURL(&url));
        let _ = sender.send(opened);
    })
    .map_err(|error| error.to_string())?;
    if receiver.recv_timeout(std::time::Duration::from_secs(5)) != Ok(true) {
        return Err(format!("macOS did not open {path}."));
    }
    record(
        &usage.0,
        &usage::Pick {
            at_seconds: now(),
            row,
            query,
            path,
        },
    )
    .map_err(|error| format!("Raiden opened it but could not note it: {error}"))
}
