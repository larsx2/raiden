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
static FAILED: Mutex<Vec<(Instant, String)>> = Mutex::new(Vec::new());
static FETCHING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn kept(link: &str) -> Option<HashMap<String, f64>> {
    let kept = KEPT.lock().ok()?;
    let kept = kept.as_ref()?;
    (kept.link == link && kept.at.elapsed() < FRESH_FOR).then(|| kept.rates.clone())
}

fn any_kept() -> HashMap<String, f64> {
    KEPT.lock()
        .ok()
        .and_then(|kept| kept.as_ref().map(|kept| kept.rates.clone()))
        .unwrap_or_default()
}

fn failed_lately(link: &str) -> bool {
    FAILED.lock().is_ok_and(|failed| {
        failed
            .iter()
            .any(|(at, failed)| failed == link && at.elapsed() < RETRY_AFTER)
    })
}

fn note_failure(link: &str) {
    if let Ok(mut failed) = FAILED.lock() {
        failed.retain(|(at, failed)| failed != link && at.elapsed() < RETRY_AFTER);
        failed.push((Instant::now(), link.to_owned()));
    }
}

async fn fetched(link: &str) -> Option<HashMap<String, f64>> {
    if let Some(rates) = kept(link) {
        return Some(rates);
    }
    if failed_lately(link) {
        return None;
    }
    let Some(rates) = download(link).await else {
        note_failure(link);
        return None;
    };
    if let Ok(mut kept) = KEPT.lock() {
        *kept = Some(Kept {
            at: Instant::now(),
            link: link.to_owned(),
            rates: rates.clone(),
        });
    }
    Some(rates)
}

#[tauri::command]
pub async fn currency_rates(app: AppHandle) -> HashMap<String, f64> {
    let _one_at_a_time = FETCHING.lock().await;
    let link = rates::link(crate::settings::fixer_key(&app).as_deref());
    let free = rates::link(None);
    if let Some(rates) = fetched(&link).await {
        return rates;
    }
    if link != free
        && let Some(rates) = fetched(&free).await
    {
        return rates;
    }
    any_kept()
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
