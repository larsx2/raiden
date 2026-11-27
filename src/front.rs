use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::sleep;
use std::time::Duration;

use tauri::{AppHandle, Manager as _, State};

use crate::access::{self, Seen};

const LONGEST_WAIT: Duration = Duration::from_secs(1);
const HIDE_SETTLES: Duration = Duration::from_millis(120);

struct Capture {
    round: u64,
    seen: Option<Option<Seen>>,
}

impl Default for Capture {
    fn default() -> Self {
        Self {
            round: 0,
            seen: Some(None),
        }
    }
}

#[derive(Default, Clone)]
pub struct Front(Arc<(Mutex<Capture>, Condvar)>);

impl Front {
    pub fn capture(&self) {
        let pid = access::frontmost_pid();
        let round = {
            let mut capture = self.0.0.lock().unwrap_or_else(PoisonError::into_inner);
            capture.round += 1;
            capture.seen = None;
            capture.round
        };
        let shared = self.clone();
        std::thread::spawn(move || {
            let seen = pid.and_then(access::seen);
            let (lock, ready) = &*shared.0;
            let mut capture = lock.lock().unwrap_or_else(PoisonError::into_inner);
            if capture.round == round {
                capture.seen = Some(seen);
                ready.notify_all();
            }
        });
    }

    pub fn wait(&self) -> Option<Seen> {
        let (lock, ready) = &*self.0;
        let capture = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let (capture, _) = ready
            .wait_timeout_while(capture, LONGEST_WAIT, |capture| capture.seen.is_none())
            .unwrap_or_else(PoisonError::into_inner);
        capture.seen.clone().flatten()
    }
}

pub fn capture(app: &AppHandle) {
    app.state::<Front>().capture();
}

#[tauri::command]
pub async fn front_context(front: State<'_, Front>) -> Result<Option<Seen>, ()> {
    let front = front.inner().clone();
    Ok(tauri::async_runtime::spawn_blocking(move || front.wait())
        .await
        .ok()
        .flatten())
}

fn switched(came_from: libc::pid_t, now: Option<(libc::pid_t, String)>) -> Option<String> {
    match now {
        Some((pid, _)) if pid == came_from => None,
        Some((_, name)) if !name.is_empty() => {
            Some(format!("Paste back skipped: you switched to {name}."))
        }
        _ => Some("Paste back skipped: the app you came from is no longer in front.".to_owned()),
    }
}

pub fn front_now() -> Option<(libc::pid_t, String)> {
    let pid = access::frontmost_pid()?;
    Some((pid, access::app_name(pid).unwrap_or_default()))
}

#[tauri::command]
pub async fn paste_back(
    app: AppHandle,
    front: State<'_, Front>,
    text: String,
) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("There is no answer to paste.".to_owned());
    }
    if !access::trusted(false) {
        return Err(access::NEEDS_ACCESS.to_owned());
    }
    let front = front.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let pid = came_from(&front)?;
        paste_into(&app, pid, &text, || Ok(()))
    })
    .await
    .map_err(|error| error.to_string())?
}

pub fn came_from(front: &Front) -> Result<libc::pid_t, String> {
    front.wait().map(|seen| seen.pid).ok_or_else(|| {
        "Raiden could not tell which app you came from, so it pasted nothing.".to_owned()
    })
}

pub fn paste_into(
    app: &AppHandle,
    pid: libc::pid_t,
    text: &str,
    still_right: impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    paste_into_with(app, pid, still_right, || crate::paste::paste_answer(text))
}

pub fn paste_into_with(
    app: &AppHandle,
    pid: libc::pid_t,
    still_right: impl Fn() -> Result<(), String>,
    paste: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if let Some(skipped) = switched(pid, front_now()) {
        return Err(skipped);
    }
    crate::hide_panel(app);
    sleep(HIDE_SETTLES);
    if let Some(skipped) = switched(pid, front_now()) {
        return Err(skipped);
    }
    still_right()?;
    paste()
}

#[cfg(test)]
mod tests {
    use super::switched;

    #[test]
    fn pastes_only_into_the_app_you_came_from() {
        assert_eq!(switched(7, Some((7, "Zed".into()))), None);
        assert_eq!(
            switched(7, Some((9, "Safari".into()))).as_deref(),
            Some("Paste back skipped: you switched to Safari.")
        );
        assert_eq!(
            switched(7, None).as_deref(),
            Some("Paste back skipped: the app you came from is no longer in front.")
        );
    }
}
