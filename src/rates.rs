use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use raiden_core::rates;
use tauri::AppHandle;
use tokio::io::AsyncWriteExt as _;
use tokio::process::Command;

const FRESH_FOR: Duration = Duration::from_secs(12 * 60 * 60);
const RETRY_AFTER: Duration = Duration::from_secs(5 * 60);

struct Kept {
    at: Instant,
    link: String,
    rates: HashMap<String, f64>,
}

static KEPT: Mutex<Option<Kept>> = Mutex::new(None);
static FAILED: Mutex<Option<Instant>> = Mutex::new(None);

fn kept_rates() -> HashMap<String, f64> {
    KEPT.lock()
        .ok()
        .and_then(|kept| kept.as_ref().map(|kept| kept.rates.clone()))
        .unwrap_or_default()
}

#[tauri::command]
pub async fn currency_rates(app: AppHandle) -> HashMap<String, f64> {
    let link = rates::link(crate::settings::fixer_key(&app).as_deref());
    let fresh = KEPT.lock().ok().is_some_and(|kept| {
        kept.as_ref()
            .is_some_and(|kept| kept.link == link && kept.at.elapsed() < FRESH_FOR)
    });
    let backing_off = FAILED
        .lock()
        .ok()
        .and_then(|failed| *failed)
        .is_some_and(|at| at.elapsed() < RETRY_AFTER);
    if fresh || backing_off {
        return kept_rates();
    }
    let found = match download(&link).await {
        None if link != rates::link(None) => download(&rates::link(None)).await,
        found => found,
    };
    let Some(found) = found else {
        if let Ok(mut failed) = FAILED.lock() {
            *failed = Some(Instant::now());
        }
        return kept_rates();
    };
    if let Ok(mut kept) = KEPT.lock() {
        *kept = Some(Kept {
            at: Instant::now(),
            link,
            rates: found.clone(),
        });
    }
    found
}

async fn download(link: &str) -> Option<HashMap<String, f64>> {
    let mut child = Command::new("/usr/bin/curl")
        .args(["--disable", "--silent", "--fail", "--config", "-"])
        .env_clear()
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    let config = format!(
        "proto = \"=https\"\nmax-redirs = 0\nconnect-timeout = 3\nmax-time = 5\nmax-filesize = 65536\nurl = \"{link}\"\n"
    );
    let mut input = child.stdin.take()?;
    input.write_all(config.as_bytes()).await.ok()?;
    drop(input);
    let output = child.wait_with_output().await.ok()?;
    output.status.success().then_some(())?;
    rates::rates_of(std::str::from_utf8(&output.stdout).ok()?)
}
