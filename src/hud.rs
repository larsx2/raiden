use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use tauri::{
    AppHandle, Emitter, LogicalPosition, Manager, State, WebviewUrl, WebviewWindowBuilder,
};
use tauri_nspanel::{CollectionBehavior, ManagerExt, PanelLevel, StyleMask, WebviewWindowExt};

pub const HUD: &str = "hud";
const WIDTH: f64 = 640.0;
const HEIGHT: f64 = 76.0;
const ABOVE_BOTTOM: f64 = 106.0;
const FADE: Duration = Duration::from_millis(200);

mod panel {
    #![allow(clippy::unused_unit)]
    use tauri_nspanel::tauri_panel;

    tauri_panel! {
        panel!(HudPanel {
            config: {
                can_become_key_window: false,
                can_become_main_window: false,
                is_floating_panel: true
            }
        })
    }
}

use panel::HudPanel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Look {
    Working,
    Done,
    Problem,
}

impl Look {
    fn lingers(self, undo: bool) -> Option<Duration> {
        match (self, undo) {
            (Look::Working, _) => None,
            (_, true) => Some(Duration::from_millis(3200)),
            (Look::Problem, false) => Some(Duration::from_millis(3600)),
            (Look::Done, false) => Some(Duration::from_millis(1600)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Message {
    pub look: Look,
    pub text: String,
    pub undo: bool,
    pub shown: bool,
}

#[derive(Default)]
pub struct Hud(Mutex<(u64, Option<Message>)>);

impl Hud {
    fn round(&self) -> u64 {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).0
    }
}

#[tauri::command]
pub fn hud_message(hud: State<'_, Hud>) -> Option<Message> {
    hud.0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .1
        .clone()
}

fn bottom_center(app: &AppHandle) -> Option<LogicalPosition<f64>> {
    let monitor = app
        .get_webview_window(crate::PANEL)
        .and_then(|panel| panel.current_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())?;
    let area = crate::screen_area(&monitor);
    Some(LogicalPosition::new(
        area.x + (area.width - WIDTH) / 2.0,
        area.y + area.height - ABOVE_BOTTOM - HEIGHT,
    ))
}

fn make(app: &AppHandle) -> Result<(), String> {
    let window = WebviewWindowBuilder::new(app, HUD, WebviewUrl::App("hud.html".into()))
        .title("Raiden message")
        .inner_size(WIDTH, HEIGHT)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .build()
        .map_err(|error| error.to_string())?;
    let panel = window
        .to_panel::<HudPanel>()
        .map_err(|error| format!("{error:?}"))?;
    panel.set_level(PanelLevel::Status.value());
    panel
        .add_style_mask(StyleMask::empty().nonactivating_panel().into())
        .map_err(|error| format!("{error:?}"))?;
    panel.set_ignores_mouse_events(true);
    panel.set_collection_behavior(
        CollectionBehavior::new()
            .full_screen_auxiliary()
            .can_join_all_spaces()
            .into(),
    );
    Ok(())
}

fn put_up(app: &AppHandle) {
    if app.get_webview_panel(HUD).is_err()
        && let Err(error) = make(app)
    {
        eprintln!("Raiden could not make its message window: {error}");
        return;
    }
    if let Some(window) = app.get_webview_window(HUD)
        && let Some(at) = bottom_center(app)
        && let Err(error) = window.set_position(at)
    {
        eprintln!("Raiden could not place its message window: {error}");
    }
    if let Ok(panel) = app.get_webview_panel(HUD) {
        panel.show();
    }
}

fn take_down(app: &AppHandle) {
    let window = match app.remove_webview_panel(HUD) {
        Some(panel) => {
            panel.hide();
            panel.to_window()
        }
        None => app.get_webview_window(HUD),
    };
    if let Some(window) = window
        && let Err(error) = window.destroy()
    {
        eprintln!("Raiden could not close its message window: {error}");
    }
}

fn publish(app: &AppHandle, round: Option<u64>, message: &Message) -> Option<u64> {
    let hud = app.state::<Hud>();
    let mut held = hud.0.lock().unwrap_or_else(PoisonError::into_inner);
    if round.is_some_and(|round| round != held.0) {
        return None;
    }
    if round.is_none() {
        held.0 += 1;
    }
    held.1 = Some(message.clone());
    if let Err(error) = app.emit_to(HUD, "hud", message) {
        eprintln!("Raiden could not update its message window: {error}");
    }
    Some(held.0)
}

pub fn show(app: &AppHandle, look: Look, text: impl Into<String>, undo: bool) {
    let message = Message {
        look,
        text: text.into(),
        undo,
        shown: true,
    };
    let Some(round) = publish(app, None, &message) else {
        return;
    };
    crate::on_main_thread(app, put_up);
    let Some(lingers) = look.lingers(undo) else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(lingers).await;
        let hidden = Message {
            shown: false,
            ..message
        };
        if publish(&app, Some(round), &hidden).is_none() {
            return;
        }
        tokio::time::sleep(FADE).await;
        crate::on_main_thread(&app, move |app| {
            if app.state::<Hud>().round() == round {
                take_down(app);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::Look;
    use std::time::Duration;

    #[test]
    fn work_in_progress_stays_and_undo_gives_time_to_read() {
        assert_eq!(Look::Working.lingers(true), None);
        assert_eq!(Look::Done.lingers(true), Some(Duration::from_millis(3200)));
        assert!(Look::Problem.lingers(false) > Look::Done.lingers(false));
    }
}
