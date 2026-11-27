use raiden_core::ai::grammar;
use std::thread::sleep;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager as _, State};

use crate::access;
use crate::front::{self, Front};
use crate::hud::{self, Look};
use crate::picks::Usage;

const LAUNCHER_LIMIT: Duration = Duration::from_secs(30);
const KEYBOARD_RETURNS: Duration = Duration::from_millis(120);
const PASTE_SHOWS: Duration = Duration::from_secs(1);
const CANNOT_TELL: &str =
    "Raiden could not tell whether this text can be edited, so it left it alone";
const IN_A_TERMINAL: &str =
    "Fix grammar skipped: in a terminal, pasting would type onto the command line";
const TERMINALS: [&str; 8] = [
    "com.apple.Terminal",
    "com.googlecode.iterm2",
    "com.mitchellh.ghostty",
    "com.github.wez.wezterm",
    "org.alacritty",
    "net.kovidgoyal.kitty",
    "dev.warp.Warp-Stable",
    "co.zeit.hyper",
];
const NOT_EDITABLE: &str =
    "Fix grammar works on text you can edit, and this selection is read-only";

const NEEDS_ACCESS: &str =
    "Fix grammar needs Accessibility access: System Settings, Privacy & Security, Accessibility";

struct Fixed {
    message: String,
    undo: bool,
}

#[tauri::command]
pub fn fix_grammar(
    app: AppHandle,
    front: State<'_, Front>,
    usage: State<'_, Usage>,
    query: String,
    row: usize,
) {
    crate::hide_panel(&app);
    crate::picks::note(&usage.0, grammar::ID, query, row);
    let front = front.inner().clone();
    tauri::async_runtime::spawn(async move {
        fix_and_tell(&app, front).await;
    });
}

pub async fn fix_and_tell(app: &AppHandle, front: Front) -> Option<String> {
    match fix(app, front).await {
        Ok(fixed) => {
            hud::show(app, Look::Done, fixed.message, fixed.undo);
            None
        }
        Err(problem) => {
            hud::show(app, Look::Problem, problem.clone(), false);
            Some(problem)
        }
    }
}

fn launcher_open(app: &AppHandle) -> bool {
    app.get_webview_window(crate::PANEL)
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(true)
}

fn editable(pid: libc::pid_t) -> Result<(), String> {
    let id = access::bundle_id(pid).ok_or(CANNOT_TELL)?;
    if TERMINALS.contains(&id.as_str()) {
        return Err(IN_A_TERMINAL.to_owned());
    }
    match access::editable_focus(pid) {
        Some(true) => Ok(()),
        Some(false) => Err(NOT_EDITABLE.to_owned()),
        None => Err(CANNOT_TELL.to_owned()),
    }
}

fn still_selected(
    app: &AppHandle,
    pid: libc::pid_t,
    selection: &str,
) -> Result<Option<String>, String> {
    if !until(LAUNCHER_LIMIT, || !launcher_open(app)) {
        return Err("Fix grammar skipped: Raiden's window stayed open".to_owned());
    }
    sleep(KEYBOARD_RETURNS);
    if front::front_now().is_none_or(|(now, _)| now != pid) {
        return Err("Fix grammar skipped: you switched apps".to_owned());
    }
    editable(pid)?;
    let now = access::seen(pid).and_then(|seen| seen.selection);
    if now.as_deref() == Some(selection) {
        Ok(access::focused_text(pid))
    } else {
        Err("Fix grammar skipped: the selection changed while Raiden was fixing it".to_owned())
    }
}

fn arrived(pid: libc::pid_t, before: Option<&str>, app: &str) -> Result<(), String> {
    let Some(before) = before else {
        return Ok(());
    };
    let shown = until(PASTE_SHOWS, || {
        access::focused_text(pid).is_none_or(|text| text != before)
    });
    if shown {
        Ok(())
    } else {
        Err(format!("Raiden pasted the fix, but {app} does not show it"))
    }
}

async fn fix(app: &AppHandle, front: Front) -> Result<Fixed, String> {
    if !access::trusted(false) {
        return Err(NEEDS_ACCESS.to_owned());
    }
    let seen = tauri::async_runtime::spawn_blocking(move || front.wait())
        .await
        .ok()
        .flatten()
        .ok_or("Raiden could not tell which app you came from")?;
    let selection = seen.selection.clone().unwrap_or_default();
    let prepared = grammar::request(&selection).map_err(|refusal| refusal.message())?;
    let pid = seen.pid;
    tauri::async_runtime::spawn_blocking(move || editable(pid))
        .await
        .map_err(|error| error.to_string())??;
    hud::show(
        app,
        Look::Working,
        format!("Fixing grammar in {}", seen.app),
        false,
    );
    let reply = crate::ai::rewrite(app, prepared).await?;
    let fixed = grammar::fixed(&reply, &selection);
    let changed = grammar::changed_sentences(&selection, &fixed);
    if changed > 0 {
        let app = app.clone();
        let name = seen.app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let before = still_selected(&app, pid, &selection)?;
            crate::paste::paste_answer(&fixed)?;
            arrived(pid, before.as_deref(), &name)
        })
        .await
        .map_err(|error| error.to_string())??;
    }
    Ok(Fixed {
        message: grammar::message(changed, &seen.app),
        undo: changed > 0,
    })
}

fn until(limit: Duration, done: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        sleep(Duration::from_millis(50));
    }
    done()
}
