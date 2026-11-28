mod access;
mod actions;
mod ai;
mod answers;
mod clipboard;
mod control;
mod crashes;
mod files;
mod front;
mod grammar;
mod hud;
mod icons;
mod login;
mod memory;
mod paste;
mod picks;
mod pulls;
mod rates;
mod settings;
mod sizes;
mod spotlight;
mod terminals;
mod typeahead;
mod wake;

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use raiden_core::speed::{self, Measure};
use raiden_core::{Area, Invocation};
use tauri::{AppHandle, Manager, State};
use tauri_nspanel::ManagerExt;
use tauri_plugin_global_shortcut::ShortcutState;

pub const PANEL: &str = "main";

mod panel {
    #![allow(clippy::unused_unit)]
    use tauri::{AppHandle, Manager};
    use tauri_nspanel::{CollectionBehavior, PanelLevel, StyleMask, WebviewWindowExt, tauri_panel};

    use super::PANEL;

    tauri_panel! {
        panel!(LauncherPanel {
            config: {
                can_become_key_window: true,
                is_floating_panel: true
            }
        })

        panel_event!(LauncherEvents {
            window_did_resign_key(notification: &NSNotification) -> ()
        })
    }

    fn draw_as_soon_as_shown(view: *mut objc2::runtime::AnyObject) {
        let Some(view) = (unsafe { view.as_ref() }) else {
            return;
        };
        let selector = objc2::sel!(_setWindowOcclusionDetectionEnabled:);
        let known: bool = unsafe { objc2::msg_send![view, respondsToSelector: selector] };
        if known {
            let () = unsafe { objc2::msg_send![view, _setWindowOcclusionDetectionEnabled: false] };
        }
    }

    pub fn make(
        app: &AppHandle,
        title: &str,
        on_resign: impl Fn() + Send + Sync + 'static,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let window = app
            .get_webview_window(PANEL)
            .ok_or("Raiden's window is missing")?;
        window.set_title(title)?;
        let panel = window
            .to_panel::<LauncherPanel>()
            .map_err(|error| format!("Raiden could not make its panel: {error:?}"))?;
        panel.set_level(PanelLevel::Floating.value());
        panel
            .as_panel()
            .setAnimationBehavior(objc2_app_kit::NSWindowAnimationBehavior::None);
        panel
            .add_style_mask(StyleMask::empty().nonactivating_panel().into())
            .map_err(|error| format!("Raiden could not keep focus on your app: {error:?}"))?;
        panel.set_collection_behavior(
            CollectionBehavior::new()
                .full_screen_auxiliary()
                .can_join_all_spaces()
                .into(),
        );
        window.with_webview(|webview| {
            draw_as_soon_as_shown(webview.inner().cast());
        })?;
        let events = LauncherEvents::new();
        events.window_did_resign_key(move |_| on_resign());
        panel.set_event_handler(Some(events.as_ref()));
        Ok(())
    }
}

pub fn on_main_thread(app: &AppHandle, work: impl FnOnce(&AppHandle) + Send + 'static) {
    let handle = app.clone();
    if let Err(error) = app.run_on_main_thread(move || work(&handle)) {
        eprintln!("Raiden could not reach its window: {error}");
    }
}

pub fn screen_area(monitor: &tauri::Monitor) -> Area {
    let scale = monitor.scale_factor();
    Area {
        x: f64::from(monitor.position().x) / scale,
        y: f64::from(monitor.position().y) / scale,
        width: f64::from(monitor.size().width) / scale,
        height: f64::from(monitor.size().height) / scale,
    }
}

fn contains(area: Area, x: f64, y: f64) -> bool {
    x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height
}

fn place(app: &AppHandle) {
    let monitors = app.available_monitors().unwrap_or_default();
    let primary_scale = app
        .primary_monitor()
        .ok()
        .flatten()
        .map_or(1.0, |monitor| monitor.scale_factor());
    let pointer = app
        .cursor_position()
        .ok()
        .map(|at| (at.x / primary_scale, at.y / primary_scale));
    let monitor = pointer
        .and_then(|(x, y)| {
            monitors
                .iter()
                .find(|monitor| contains(screen_area(monitor), x, y))
        })
        .or_else(|| monitors.first());
    if let Some(monitor) = monitor {
        sizes::apply(app, sizes::frame_on(app, monitor));
    }
}

fn toggle(app: &AppHandle, pressed_at: Instant) {
    on_main_thread(app, move |app| {
        let Ok(panel) = app.get_webview_panel(PANEL) else {
            return;
        };
        if panel.is_visible() {
            panel.hide();
            mark_hidden(app);
        } else {
            app.state::<SpeedLog>().start_open(pressed_at);
            front::capture(app);
            place(app);
            panel.show_and_make_key();
            ai::warm(app);
        }
    });
}

pub fn show_panel(app: &AppHandle) {
    on_main_thread(app, |app| {
        if let Ok(panel) = app.get_webview_panel(PANEL)
            && !panel.is_visible()
        {
            front::capture(app);
            place(app);
            panel.show_and_make_key();
            ai::warm(app);
        }
    });
}

pub fn hide_panel(app: &AppHandle) {
    on_main_thread(app, |app| {
        if let Ok(panel) = app.get_webview_panel(PANEL) {
            panel.hide();
            mark_hidden(app);
        }
    });
}

#[tauri::command]
fn hide(app: AppHandle) {
    hide_panel(&app);
}

pub struct HiddenSince(pub Mutex<Instant>);

pub fn mark_hidden(app: &AppHandle) {
    settings::stop_recording(app);
    typeahead::release();
    ai::rest_later(app);
    if let Ok(mut since) = app.state::<HiddenSince>().0.lock() {
        *since = Instant::now();
    }
}

struct SpeedLog {
    path: PathBuf,
    opened_at: Mutex<Option<Instant>>,
    woke: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl SpeedLog {
    fn new(development: bool) -> Self {
        let (folder, _) = folders(development);
        let log = Self {
            path: folder.join(speed::LOG),
            opened_at: Mutex::new(None),
            woke: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        log.trim();
        log
    }

    fn trim(&self) {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return;
        };
        if let Some(kept) = speed::trimmed(&text)
            && let Err(error) = std::fs::write(&self.path, kept)
        {
            eprintln!("Raiden could not trim {}: {error}", self.path.display());
        }
    }

    fn start_open(&self, pressed_at: Instant) {
        if let Ok(mut opened_at) = self.opened_at.lock() {
            *opened_at = Some(pressed_at);
        }
    }

    fn finish_open(&self) {
        let started = self.opened_at.lock().ok().and_then(|mut at| at.take());
        if let Some(started) = started {
            let ms = started.elapsed().as_secs_f64() * 1000.0;
            self.record(Measure::Open, ms);
            if self.woke.swap(false, std::sync::atomic::Ordering::SeqCst) {
                self.record(Measure::Wake, ms);
            }
        }
    }

    fn record(&self, measure: Measure, ms: f64) {
        let written = self
            .path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.path)
            })
            .and_then(|mut file| file.write_all(speed::line(measure, ms).as_bytes()));
        if let Err(error) = written {
            eprintln!("Raiden could not write {}: {error}", self.path.display());
        }
    }
}

struct Rows(Option<usize>);

#[tauri::command]
async fn apps(
    rows: State<'_, Rows>,
    usage: State<'_, picks::Usage>,
    pulls: State<'_, pulls::Pulls>,
) -> Result<picks::Listing, ()> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let found = raiden_core::apps::installed(&raiden_core::apps::folders(&home));
    let mut apps = match rows.0 {
        Some(rows) => raiden_core::apps::pad(found, rows),
        None => found,
    };
    apps.extend(raiden_core::system_settings::rows());
    apps.extend(
        [
            (
                raiden_core::ai::grammar::TITLE,
                raiden_core::ai::grammar::ID,
                "",
            ),
            (
                raiden_core::clipboard::VIEW_TITLE,
                raiden_core::clipboard::VIEW_ID,
                "Raiden",
            ),
            (
                raiden_core::settings::VIEW_TITLE,
                raiden_core::settings::VIEW_ID,
                "Raiden",
            ),
            (
                raiden_core::settings::FILE_TITLE,
                raiden_core::settings::FILE_ID,
                raiden_core::settings::FILE,
            ),
        ]
        .map(|(name, path, under)| raiden_core::apps::App {
            name: name.to_owned(),
            path: path.to_owned(),
            under: under.to_owned(),
            accessory: String::new(),
        }),
    );
    apps.push(raiden_core::apps::App {
        name: raiden_core::pulls::VIEW_TITLE.to_owned(),
        path: raiden_core::pulls::VIEW_ID.to_owned(),
        under: raiden_core::pulls::UNDER.to_owned(),
        accessory: pulls.accessory(),
    });
    Ok(picks::listing(apps, &usage.0))
}

#[tauri::command]
async fn icon(path: String) -> Result<Option<String>, ()> {
    Ok(
        tauri::async_runtime::spawn_blocking(move || icons::data_url(&path))
            .await
            .ok()
            .flatten(),
    )
}

#[tauri::command]
async fn frame(log: State<'_, SpeedLog>, ms: f64) -> Result<(), ()> {
    if ms.is_finite() && (0.0..10_000.0).contains(&ms) {
        log.record(Measure::Frame, ms);
    }
    Ok(())
}

#[tauri::command]
async fn shown(log: State<'_, SpeedLog>) -> Result<(), ()> {
    log.finish_open();
    Ok(())
}

#[tauri::command]
async fn keystroke(log: State<'_, SpeedLog>, ms: f64) -> Result<(), ()> {
    if ms.is_finite() && (0.0..10_000.0).contains(&ms) {
        log.record(Measure::Keystroke, ms);
    }
    Ok(())
}

fn folders(development: bool) -> (PathBuf, PathBuf) {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let raiden_home = std::env::var_os("RAIDEN_HOME").map(PathBuf::from);
    (
        raiden_core::folders::logs(development, &home, raiden_home.as_deref()),
        raiden_core::folders::state(development, &home, raiden_home.as_deref()),
    )
}

fn probe(args: &[String], development: bool) -> i32 {
    let (logs, state) = folders(development);
    let hidden_for = control::ask_hidden_for(state);
    let app_name = if development { "raiden" } else { "Raiden" };
    let idle = || memory::idle(app_name);
    let outcome = raiden_core::probe::run(
        args,
        &logs,
        &raiden_core::probe::Host {
            hidden_for: &hidden_for,
            memory: &idle,
        },
    );
    print!("{}", outcome.output);
    outcome.code
}

fn main() {
    let development = cfg!(debug_assertions);
    match raiden_core::invocation(std::env::args().skip(1)) {
        Invocation::Version => println!("raiden {}", env!("CARGO_PKG_VERSION")),
        Invocation::Probe(args) => std::process::exit(probe(&args, development)),
        Invocation::Launcher { rows } => launch(rows, development),
    }
}

fn launch(rows: Option<usize>, development: bool) {
    if control::running(&folders(development).1) {
        eprintln!("Raiden is already running, so this copy quits.");
        return;
    }
    crashes::watch(development);
    tauri::Builder::default()
        .manage(SpeedLog::new(development))
        .manage(HiddenSince(Mutex::new(Instant::now())))
        .manage(Rows(rows))
        .manage(ai::Ai::new(development))
        .manage(picks::Usage(folders(development).1.join("usage")))
        .manage(front::Front::default())
        .manage(hud::Hud::default())
        .manage(answers::Answers::new(&folders(development).1))
        .manage(clipboard::History::new(&folders(development).1))
        .manage(files::Files::start())
        .manage(pulls::Pulls::new(&folders(development).1))
        .manage(sizes::Sizes::new(&folders(development).1))
        .on_web_content_process_terminate(crashes::reload)
        .plugin(tauri_nspanel::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    if settings::recording() {
                        settings::heard(app, shortcut);
                    } else if settings::opens_clipboard(app, shortcut) {
                        clipboard::open(app);
                    } else {
                        toggle(app, Instant::now());
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            hide,
            shown,
            keystroke,
            frame,
            apps,
            icon,
            picks::open,
            files::files,
            actions::open_file,
            actions::copy_path,
            actions::show_in_finder,
            actions::copy_text,
            actions::open_link,
            front::front_context,
            front::paste_back,
            clipboard::clipboard_items,
            clipboard::clipboard_thumb,
            clipboard::clipboard_paste,
            clipboard::clipboard_copy,
            clipboard::clipboard_delete,
            answers::conversations,
            answers::save_turn,
            typeahead::typed_ahead,
            ai::ai_accounts,
            ai::ai_sign_in,
            ai::ask,
            ai::ai_stop,
            ai::ai_end,
            settings::chosen_ai,
            settings::choose_ai,
            settings::choose_effort,
            settings::settings_view,
            settings::open_settings_file,
            settings::set_open_at_login,
            settings::record_hotkey,
            settings::set_hotkey,
            settings::vim_keys,
            settings::set_vim_keys,
            settings::choose_terminal,
            terminals::open_in_terminal,
            grammar::fix_grammar,
            hud::hud_message,
            pulls::pull_requests,
            pulls::pull_requests_refresh,
            pulls::pull_requests_allow,
            pulls::pull_requests_avatar,
            pulls::pull_requests_pin,
            pulls::pull_requests_context,
            rates::currency_rates,
            sizes::window_size,
            sizes::expand_window
        ])
        .setup(move |app| {
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            login::start(development);
            wake::watch(std::sync::Arc::clone(&app.state::<SpeedLog>().woke));
            if let Err(error) = control::listen(app.handle(), &folders(development).1) {
                eprintln!("Raiden could not open its probe socket: {error}");
            }
            let handle = app.handle().clone();
            panel::make(
                app.handle(),
                raiden_core::window_title(development),
                move || hide_panel(&handle),
            )?;
            place(app.handle());
            settings::start(app.handle(), development);
            typeahead::watch();
            clipboard::watch(&app.state::<clipboard::History>());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Raiden could not start");
}
