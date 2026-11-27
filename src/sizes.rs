use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use objc2::MainThreadMarker;
use objc2_app_kit::{NSScreen, NSWindow};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use raiden_core::Area;
use raiden_core::window::{self, EXPANDED};
use tauri::{AppHandle, Manager, Monitor, State};

const COMPACT: (f64, f64) = (720.0, 452.0);

pub struct Sizes {
    path: PathBuf,
    expanded: Mutex<bool>,
}

fn saved_choice(state: &Path, path: &Path) -> bool {
    let old = state.join(window::OLD_FILE);
    let Ok(old_text) = std::fs::read_to_string(&old) else {
        return std::fs::read_to_string(path).is_ok_and(|text| window::expanded_in(&text));
    };
    let expanded = match std::fs::read_to_string(path) {
        Ok(text) => window::expanded_in(&text),
        Err(_) => window::any_view_expanded(&old_text),
    };
    if path.exists() || std::fs::write(path, window::size_text(expanded)).is_ok() {
        let _ = std::fs::remove_file(&old);
    }
    expanded
}

impl Sizes {
    pub fn new(state: &Path) -> Self {
        let path = state.join(window::FILE);
        let expanded = saved_choice(state, &path);
        Self {
            path,
            expanded: Mutex::new(expanded),
        }
    }

    pub fn is_expanded(&self) -> bool {
        *self.expanded.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set(&self, expanded: bool) -> Result<(), String> {
        let mut kept = self.expanded.lock().unwrap_or_else(PoisonError::into_inner);
        *kept = expanded;
        self.path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&self.path, window::size_text(expanded)))
            .map_err(|error| format!("Raiden could not remember the window size: {error}"))
    }
}

fn compact(app: &AppHandle) -> (f64, f64) {
    app.config()
        .app
        .windows
        .first()
        .map_or(COMPACT, |window| (window.width, window.height))
}

fn chosen_size(app: &AppHandle) -> (f64, f64) {
    if app.state::<Sizes>().is_expanded() {
        EXPANDED
    } else {
        compact(app)
    }
}

fn work_area(monitor: &Monitor) -> Area {
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    Area {
        x: f64::from(area.position.x) / scale,
        y: f64::from(area.position.y) / scale,
        width: f64::from(area.size.width) / scale,
        height: f64::from(area.size.height) / scale,
    }
}

pub fn frame_on(app: &AppHandle, monitor: &Monitor) -> Area {
    window::panel_frame(
        crate::screen_area(monitor),
        work_area(monitor),
        compact(app),
        chosen_size(app),
    )
}

pub fn apply(app: &AppHandle, frame: Area) {
    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    let Some(window) = app.get_webview_window(crate::PANEL) else {
        return;
    };
    let Ok(pointer) = window.ns_window() else {
        return;
    };
    let Some(ns_window) = (unsafe { pointer.cast::<NSWindow>().as_ref() }) else {
        return;
    };
    let Some(primary) = NSScreen::screens(main_thread).firstObject() else {
        return;
    };
    let cocoa = window::cocoa_frame(frame, primary.frame().size.height);
    let rect = NSRect::new(
        NSPoint::new(cocoa.x, cocoa.y),
        NSSize::new(cocoa.width, cocoa.height),
    );
    if ns_window.frame() != rect {
        ns_window.setFrame_display(rect, ns_window.isVisible());
    }
}

fn fit_chosen(app: &AppHandle) {
    crate::on_main_thread(app, |app| {
        let monitor = app
            .get_webview_window(crate::PANEL)
            .and_then(|window| window.current_monitor().ok().flatten());
        if let Some(monitor) = monitor {
            apply(app, frame_on(app, &monitor));
        }
    });
}

#[tauri::command]
pub fn window_size(sizes: State<'_, Sizes>) -> bool {
    sizes.is_expanded()
}

#[tauri::command]
pub fn expand_window(app: AppHandle, expanded: bool) -> Result<bool, String> {
    let sizes = app.state::<Sizes>();
    let kept = sizes.set(expanded);
    fit_chosen(&app);
    if let Err(error) = &kept {
        eprintln!("{error}");
    }
    kept.map(|()| sizes.is_expanded())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("raiden-sizes-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn the_choice_is_saved_and_read_back_after_a_restart() {
        let state = folder("saved");
        let sizes = Sizes::new(&state);
        assert!(!sizes.is_expanded());
        sizes.set(true).expect("saved");
        assert!(Sizes::new(&state).is_expanded());
        sizes.set(false).expect("saved");
        assert!(!Sizes::new(&state).is_expanded());
        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn an_old_file_with_an_expanded_view_starts_expanded_and_is_removed() {
        let state = folder("old-expanded");
        std::fs::create_dir_all(&state).expect("folder");
        std::fs::write(state.join(window::OLD_FILE), "ai\npulls\n").expect("old file");
        assert!(Sizes::new(&state).is_expanded());
        assert!(!state.join(window::OLD_FILE).exists());
        assert_eq!(
            std::fs::read_to_string(state.join(window::FILE)).expect("new file"),
            "expanded\n"
        );
        assert!(Sizes::new(&state).is_expanded());
        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn an_old_file_with_nothing_expanded_starts_compact_and_is_removed() {
        let state = folder("old-compact");
        std::fs::create_dir_all(&state).expect("folder");
        std::fs::write(state.join(window::OLD_FILE), "").expect("old file");
        assert!(!Sizes::new(&state).is_expanded());
        assert!(!state.join(window::OLD_FILE).exists());
        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn a_new_file_wins_over_an_old_one_left_behind() {
        let state = folder("both");
        std::fs::create_dir_all(&state).expect("folder");
        std::fs::write(state.join(window::OLD_FILE), "pulls\n").expect("old file");
        std::fs::write(state.join(window::FILE), "compact\n").expect("new file");
        assert!(!Sizes::new(&state).is_expanded());
        assert!(!state.join(window::OLD_FILE).exists());
        let _ = std::fs::remove_dir_all(&state);
    }
}
