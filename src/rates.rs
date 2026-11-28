use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use raiden_core::rates;
use tokio::process::Command;

const FRESH_FOR: Duration = Duration::from_secs(12 * 60 * 60);
const RETRY_AFTER: Duration = Duration::from_secs(5 * 60);

struct Kept {
    at: Instant,
    rates: HashMap<String, f64>,
}

static KEPT: Mutex<Option<Kept>> = Mutex::new(None);
static FAILED_AT: Mutex<Option<Instant>> = Mutex::new(None);
static FETCHING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn kept(fresh: bool) -> Option<HashMap<String, f64>> {
    let kept = KEPT.lock().ok()?;
    let kept = kept.as_ref()?;
    (!fresh || kept.at.elapsed() < FRESH_FOR).then(|| kept.rates.clone())
}

fn failed_lately() -> bool {
    FAILED_AT
        .lock()
        .is_ok_and(|at| at.is_some_and(|at| at.elapsed() < RETRY_AFTER))
}

/// The ECB's rates, fetched at most twice a day, and the last ones kept when the fetch fails.
#[tauri::command]
pub async fn currency_rates() -> HashMap<String, f64> {
    let _one_at_a_time = FETCHING.lock().await;
    if let Some(rates) = kept(true) {
        return rates;
    }
    if !failed_lately() {
        if let Some(rates) = download().await {
            if let Ok(mut kept) = KEPT.lock() {
                *kept = Some(Kept {
                    at: Instant::now(),
                    rates: rates.clone(),
                });
            }
            return rates;
        }
        if let Ok(mut at) = FAILED_AT.lock() {
            *at = Some(Instant::now());
        }
    }
    kept(false).unwrap_or_default()
}

async fn download() -> Option<HashMap<String, f64>> {
    let output = Command::new("/usr/bin/curl")
        .args([
            "--disable",
            "--silent",
            "--fail",
            "--proto",
            "=https",
            "--max-redirs",
            "0",
            "--connect-timeout",
            "3",
            "--max-time",
            "5",
            "--max-filesize",
            "65536",
            rates::LINK,
        ])
        .env_clear()
        .kill_on_drop(true)
        .output()
        .await
        .ok()?;
    output.status.success().then_some(())?;
    rates::rates_of(std::str::from_utf8(&output.stdout).ok()?)
}
