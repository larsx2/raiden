use std::io::Write as _;
use std::path::PathBuf;
use std::str::FromStr as _;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use raiden_core::ai::{Effort, Provider};
use raiden_core::login::AtLogin;
use raiden_core::settings::{self, Hotkey, Settings};
use raiden_core::terminals::Terminal;
use tauri::{AppHandle, Emitter as _, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt as _, Modifiers, Shortcut};

const LOG: &str = "settings.log";
const CHECK_EVERY: Duration = Duration::from_millis(500);

static RECORDING: AtomicBool = AtomicBool::new(false);
static WRITING: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Panel,
    Clipboard,
}

impl Slot {
    fn name(self) -> &'static str {
        match self {
            Slot::Panel => settings::HOTKEY,
            Slot::Clipboard => settings::CLIPBOARD_HOTKEY,
        }
    }

    fn other(self) -> Self {
        match self {
            Slot::Panel => Slot::Clipboard,
            Slot::Clipboard => Slot::Panel,
        }
    }

    fn opens(self) -> &'static str {
        match self {
            Slot::Panel => "Raiden",
            Slot::Clipboard => "Clipboard history",
        }
    }

    fn of(which: Option<&str>) -> Result<Self, String> {
        match which {
            None | Some("hotkey") => Ok(Slot::Panel),
            Some("clipboard") => Ok(Slot::Clipboard),
            Some(other) => Err(format!("Raiden has no hotkey called {other:?}")),
        }
    }
}

#[derive(Default)]
struct Held {
    panel: Mutex<Option<Shortcut>>,
    clipboard: Mutex<Option<Shortcut>>,
}

impl Held {
    fn slot(&self, slot: Slot) -> &Mutex<Option<Shortcut>> {
        match slot {
            Slot::Panel => &self.panel,
            Slot::Clipboard => &self.clipboard,
        }
    }
}

fn held(app: &AppHandle, slot: Slot) -> Option<Shortcut> {
    app.try_state::<Held>()?
        .slot(slot)
        .lock()
        .ok()
        .and_then(|held| *held)
}

pub fn hotkey(app: &AppHandle) -> Option<Shortcut> {
    held(app, Slot::Panel)
}

pub fn clipboard_hotkey(app: &AppHandle) -> Option<Shortcut> {
    held(app, Slot::Clipboard)
}

fn follow_hotkeys(app: &AppHandle) {
    let held: Vec<Shortcut> = [hotkey(app), clipboard_hotkey(app)]
        .into_iter()
        .flatten()
        .collect();
    crate::typeahead::follow(&held);
}

pub fn opens_clipboard(app: &AppHandle, shortcut: &Shortcut) -> bool {
    held(app, Slot::Clipboard) == Some(*shortcut)
}

type Stamp = Option<(SystemTime, u64)>;

struct Live {
    files: Files,
    ai: Mutex<Provider>,
    effort: Mutex<Effort>,
    hotkey: Mutex<Hotkey>,
    clipboard_hotkey: Mutex<Hotkey>,
    pinned_repos: Mutex<Vec<String>>,
    pull_requests: Mutex<bool>,
    vim: Mutex<bool>,
    terminal: Mutex<Option<Terminal>>,
    fixer_key: Mutex<Option<String>>,
    seen: Mutex<Stamp>,
}

impl Live {
    fn current(&self) -> Option<Settings> {
        Some(Settings {
            hotkey: self.hotkey.lock().ok()?.clone(),
            clipboard_hotkey: self.clipboard_hotkey.lock().ok()?.clone(),
            ai: *self.ai.lock().ok()?,
            effort: *self.effort.lock().ok()?,
            pinned_repos: self.pinned_repos.lock().ok()?.clone(),
            pull_requests: *self.pull_requests.lock().ok()?,
            vim: *self.vim.lock().ok()?,
            terminal: *self.terminal.lock().ok()?,
            fixer_key: self.fixer_key.lock().ok()?.clone(),
        })
    }

    fn hotkey_in(&self, slot: Slot) -> &Mutex<Hotkey> {
        match slot {
            Slot::Panel => &self.hotkey,
            Slot::Clipboard => &self.clipboard_hotkey,
        }
    }
}

fn live(app: &AppHandle) -> Result<State<'_, Live>, String> {
    app.try_state::<Live>()
        .ok_or_else(|| "Raiden has not read its settings yet".to_owned())
}

pub fn fixer_key(app: &AppHandle) -> Option<String> {
    app.try_state::<Live>()?.fixer_key.lock().ok()?.clone()
}

#[tauri::command]
pub fn set_fixer_key(app: AppHandle, remove: bool) -> Result<Option<String>, String> {
    let key = if remove {
        None
    } else {
        Some(raiden_core::rates::fixer_key(
            &crate::actions::copied_text(),
        )?)
    };
    let live = live(&app)?;
    let mut kept = live
        .fixer_key
        .lock()
        .map_err(|_| "Raiden lost track of its settings")?;
    live.files
        .write(settings::FIXER_KEY, key.clone().unwrap_or_default())?;
    *kept = key;
    Ok(kept.as_deref().map(key_end))
}

fn key_end(key: &str) -> String {
    format!("…{}", &key[key.len().saturating_sub(4)..])
}

pub fn ai(app: &AppHandle) -> Provider {
    app.try_state::<Live>()
        .and_then(|live| live.ai.lock().ok().map(|ai| *ai))
        .unwrap_or(Provider::DEFAULT)
}

#[tauri::command]
pub fn chosen_ai(app: AppHandle) -> Provider {
    ai(&app)
}

#[tauri::command]
pub fn choose_ai(app: AppHandle, id: String) -> Result<(), String> {
    let wanted = Provider::from_id(&id).ok_or_else(|| format!("Raiden has no AI called {id:?}"))?;
    let live = live(&app)?;
    {
        let mut ai = live.ai.lock().map_err(|_| "Raiden lost track of its AI")?;
        live.files.write(settings::AI, wanted.id())?;
        *ai = wanted;
    }
    crate::ai::warm_up(&app, wanted);
    Ok(())
}

#[tauri::command]
pub fn choose_effort(app: AppHandle, id: String) -> Result<(), String> {
    let wanted =
        Effort::from_id(&id).ok_or_else(|| format!("Raiden has no effort called {id:?}"))?;
    let live = live(&app)?;
    let changed = {
        let mut effort = live
            .effort
            .lock()
            .map_err(|_| "Raiden lost track of its effort")?;
        live.files.write(settings::EFFORT, wanted.id())?;
        *effort = wanted;
        crate::ai::set_effort(&app, wanted)
    };
    if changed {
        crate::ai::warm_up(&app, ai(&app));
    }
    Ok(())
}

pub fn pull_requests(app: &AppHandle) -> (bool, Vec<String>) {
    let Some(live) = app.try_state::<Live>() else {
        return (false, Vec::new());
    };
    catch_up(app, &live);
    let allowed = live.pull_requests.lock().is_ok_and(|allowed| *allowed);
    let pinned = live
        .pinned_repos
        .lock()
        .map(|pinned| pinned.clone())
        .unwrap_or_default();
    (allowed, pinned)
}

pub fn allow_pull_requests(app: &AppHandle) -> Result<(), String> {
    let live = live(app)?;
    let mut allowed = live
        .pull_requests
        .lock()
        .map_err(|_| "Raiden lost track of its settings")?;
    live.files.write(settings::PULL_REQUESTS, true)?;
    *allowed = true;
    Ok(())
}

#[tauri::command]
pub async fn vim_keys(app: AppHandle) -> bool {
    let Some(live) = app.try_state::<Live>() else {
        return false;
    };
    catch_up(&app, &live);
    live.vim.lock().is_ok_and(|on| *on)
}

#[tauri::command]
pub async fn set_vim_keys(app: AppHandle, on: bool) -> Result<bool, String> {
    let live = live(&app)?;
    let mut vim = live
        .vim
        .lock()
        .map_err(|_| "Raiden lost track of its settings")?;
    live.files.write(settings::VIM, on)?;
    *vim = on;
    Ok(on)
}

pub fn terminal(app: &AppHandle) -> Option<Terminal> {
    let live = app.try_state::<Live>()?;
    catch_up(app, &live);
    *live.terminal.lock().ok()?
}

#[tauri::command]
pub async fn choose_terminal(app: AppHandle, id: String) -> Result<&'static str, String> {
    let wanted =
        Terminal::from_id(&id).ok_or_else(|| format!("Raiden has no terminal called {id:?}"))?;
    if crate::terminals::app_of(wanted).is_none() {
        return Err(format!("{} is not installed on this Mac.", wanted.name()));
    }
    let live = live(&app)?;
    let mut terminal = live
        .terminal
        .lock()
        .map_err(|_| "Raiden lost track of its settings")?;
    live.files.write(settings::TERMINAL, wanted.id())?;
    *terminal = Some(wanted);
    Ok(wanted.id())
}

pub fn pin_repo(app: &AppHandle, repo: &str, pin: bool) -> Result<Vec<String>, String> {
    let live = live(app)?;
    catch_up(app, &live);
    let mut pinned = live
        .pinned_repos
        .lock()
        .map_err(|_| "Raiden lost track of its settings")?;
    let after = raiden_core::pulls::pinned_after(&pinned, repo, pin)?;
    live.files
        .write(settings::PINNED_REPOS, settings::list(&after))?;
    pinned.clone_from(&after);
    Ok(after)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    hotkey: String,
    clipboard_hotkey: String,
    ai: &'static str,
    effort: Effort,
    vim: bool,
    terminal: &'static str,
    terminals: Vec<crate::terminals::Listed>,
    file: String,
    at_login: AtLogin,
    fixer_key: Option<String>,
}

#[tauri::command]
pub async fn settings_view(
    app: AppHandle,
    usage: State<'_, crate::picks::Usage>,
    query: Option<String>,
    row: Option<usize>,
) -> Result<View, String> {
    if let (Some(query), Some(row)) = (query, row) {
        crate::picks::note(&usage.0, settings::VIEW_ID, query, row);
    }
    let live = live(&app)?;
    catch_up(&app, &live);
    let current = live.current().ok_or("Raiden lost track of its settings")?;
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    Ok(View {
        hotkey: current.hotkey.to_string(),
        clipboard_hotkey: current.clipboard_hotkey.to_string(),
        ai: current.ai.id(),
        effort: current.effort,
        vim: current.vim,
        terminal: crate::terminals::in_use(current.terminal).id(),
        terminals: crate::terminals::listed(),
        file: settings::shown_path(&live.files.settings, &home),
        at_login: crate::login::at_login(live.files.development),
        fixer_key: current.fixer_key.as_deref().map(key_end),
    })
}

#[tauri::command]
pub async fn open_settings_file(
    app: AppHandle,
    usage: State<'_, crate::picks::Usage>,
    query: Option<String>,
    row: Option<usize>,
) -> Result<(), String> {
    if let (Some(query), Some(row)) = (query, row) {
        crate::picks::note(&usage.0, settings::FILE_ID, query, row);
    }
    let live = live(&app)?;
    live.files
        .write_starter()
        .and_then(|()| {
            crate::hide_panel(&app);
            crate::actions::open_in_editor(&live.files.settings)
        })
        .inspect_err(|why| live.files.report(why))
}

#[tauri::command]
pub async fn set_open_at_login(app: AppHandle, on: bool) -> Result<AtLogin, String> {
    crate::login::set_at_login(live(&app)?.files.development, on)
}

pub fn recording() -> bool {
    RECORDING.load(Ordering::SeqCst)
}

fn record(app: &AppHandle, on: bool) {
    RECORDING.store(on, Ordering::SeqCst);
    if on {
        crate::typeahead::follow(&[]);
    } else {
        follow_hotkeys(app);
    }
}

pub fn stop_recording(app: &AppHandle) {
    if recording() {
        record(app, false);
    }
}

#[tauri::command]
pub fn record_hotkey(app: AppHandle, on: bool) {
    record(&app, on);
}

fn spoken(shortcut: &Shortcut) -> String {
    let held: String = [
        (Modifiers::CONTROL, '⌃'),
        (Modifiers::ALT, '⌥'),
        (Modifiers::SHIFT, '⇧'),
        (Modifiers::SUPER | Modifiers::META, '⌘'),
    ]
    .into_iter()
    .filter(|(modifier, _)| shortcut.mods.intersects(*modifier))
    .map(|(_, symbol)| symbol)
    .collect();
    format!("{held}{}", settings::key_name(&shortcut.key.to_string()))
}

pub fn heard(app: &AppHandle, shortcut: &Shortcut) {
    if let Err(error) = app.emit_to(crate::PANEL, "hotkey-pressed", spoken(shortcut)) {
        eprintln!("Raiden could not pass on the keys it heard: {error}");
    }
}

#[tauri::command]
pub async fn set_hotkey(
    app: AppHandle,
    hotkey: String,
    which: Option<String>,
) -> Result<String, String> {
    let slot = Slot::of(which.as_deref())?;
    let live = live(&app)?;
    let wanted = Hotkey::chosen(&hotkey, slot.name())?;
    let shortcut = shortcut(&wanted)?;
    let _changing = live
        .seen
        .lock()
        .map_err(|_| "Raiden lost track of its settings")?;
    let fresh = !hotkey_is(&app, slot, shortcut);
    take(&app, slot, &wanted)?;
    if let Err(why) = live.files.write(slot.name(), wanted.to_string()) {
        if fresh {
            let _ = let_go(&app, shortcut);
        }
        return Err(why);
    }
    switch_to(&app, slot, shortcut);
    if let Ok(mut current) = live.hotkey_in(slot).lock() {
        current.clone_from(&wanted);
    }
    RECORDING.store(false, Ordering::SeqCst);
    follow_hotkeys(&app);
    eprintln!("{} now opens with {wanted}.", slot.opens());
    Ok(wanted.to_string())
}

#[derive(Clone)]
struct Files {
    settings: PathBuf,
    log: PathBuf,
    development: bool,
}

impl Files {
    fn new(development: bool) -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let raiden_home = std::env::var_os("RAIDEN_HOME").map(PathBuf::from);
        let (logs, _) = crate::folders(development);
        Self {
            settings: raiden_core::folders::settings(development, &home, raiden_home.as_deref()),
            log: logs.join(LOG),
            development,
        }
    }

    fn report(&self, problem: &str) {
        eprintln!("{problem}");
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        let written = self
            .log
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.log)
            })
            .and_then(|mut file| file.write_all(format!("{seconds}\t{problem}\n").as_bytes()));
        if let Err(error) = written {
            eprintln!("Raiden could not write {}: {error}", self.log.display());
        }
    }

    fn write(&self, name: &str, value: impl Into<settings::Value>) -> Result<(), String> {
        let _writing = WRITING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let shown = self.settings.display();
        let text = match std::fs::read_to_string(&self.settings) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                settings::starter(self.development)
            }
            Err(error) => return Err(format!("Raiden could not read {shown}: {error}")),
        };
        let written = settings::set(&text, name, value)?;
        let target = match std::fs::canonicalize(&self.settings) {
            Ok(target) => target,
            Err(_) if std::fs::symlink_metadata(&self.settings).is_ok() => {
                return Err(format!("{shown} links to a file that is missing"));
            }
            Err(_) => self.settings.clone(),
        };
        let temporary = target.with_extension("toml.writing");
        let replaced = target
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&temporary, written))
            .and_then(|()| match std::fs::metadata(&target) {
                Ok(old) => std::fs::set_permissions(&temporary, old.permissions()),
                Err(_) => Ok(()),
            })
            .and_then(|()| std::fs::rename(&temporary, &target));
        if let Err(error) = replaced {
            let _ = std::fs::remove_file(&temporary);
            return Err(format!("Raiden could not write {shown}: {error}"));
        }
        Ok(())
    }

    fn write_starter(&self) -> Result<(), String> {
        if self.settings.exists() {
            return Ok(());
        }
        let written = self
            .settings
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| {
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&self.settings)
            })
            .and_then(|mut file| file.write_all(settings::starter(self.development).as_bytes()));
        match written {
            Err(error) if error.kind() != std::io::ErrorKind::AlreadyExists => Err(format!(
                "Raiden could not create {}: {error}",
                self.settings.display()
            )),
            _ => Ok(()),
        }
    }

    fn read(&self, previous: Option<&Settings>) -> Settings {
        let text = match std::fs::read_to_string(&self.settings) {
            Ok(text) => Some(text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                self.report(&format!(
                    "Raiden could not read {}, so it uses its defaults: {error}",
                    self.settings.display()
                ));
                None
            }
        };
        let loaded = settings::load(text.as_deref(), self.development, previous);
        for problem in &loaded.problems {
            self.report(&format!("{}: {problem}", self.settings.display()));
        }
        loaded.settings
    }

    fn stamp(&self) -> Stamp {
        let metadata = std::fs::metadata(&self.settings).ok()?;
        Some((metadata.modified().ok()?, metadata.len()))
    }
}

fn shortcut(hotkey: &Hotkey) -> Result<Shortcut, String> {
    Shortcut::from_str(&hotkey.accelerator())
        .map_err(|error| format!("Raiden does not know the key in {hotkey} ({error})"))
}

fn take(app: &AppHandle, slot: Slot, hotkey: &Hotkey) -> Result<Shortcut, String> {
    let shortcut = shortcut(hotkey)?;
    if hotkey_is(app, slot, shortcut) {
        return Ok(shortcut);
    }
    if hotkey_is(app, slot.other(), shortcut) {
        return Err(format!("{hotkey} already opens {}", slot.other().opens()));
    }
    if shortcut == crate::spotlight::command_space() && crate::spotlight::holds_command_space() {
        return Err(settings::SPOTLIGHT_HOLDS.to_owned());
    }
    app.global_shortcut()
        .register(shortcut)
        .map_err(|error| format!("Raiden could not take {hotkey} ({error})"))?;
    Ok(shortcut)
}

fn let_go(app: &AppHandle, shortcut: Shortcut) -> Result<(), String> {
    app.global_shortcut()
        .unregister(shortcut)
        .map_err(|error| error.to_string())
}

fn so(why: &str, then: &str) -> String {
    if !why.ends_with('.') {
        return format!("{why}, so {then}.");
    }
    let mut letters = then.chars();
    let first: String = letters
        .next()
        .map(char::to_uppercase)
        .into_iter()
        .flatten()
        .collect();
    format!("{why} {first}{}.", letters.as_str())
}

fn hotkey_is(app: &AppHandle, slot: Slot, shortcut: Shortcut) -> bool {
    held(app, slot) == Some(shortcut)
}

fn hold(app: &AppHandle, slot: Slot, shortcut: Shortcut) -> Option<Shortcut> {
    let held = app.try_state::<Held>()?;
    let mut held = held.slot(slot).lock().ok()?;
    held.replace(shortcut).filter(|old| *old != shortcut)
}

fn switch_to(app: &AppHandle, slot: Slot, shortcut: Shortcut) {
    if let Some(old) = hold(app, slot, shortcut)
        && !hotkey_is(app, slot.other(), old)
        && let Err(error) = let_go(app, old)
    {
        eprintln!("Raiden could not let go of its old hotkey: {error}");
    }
}

fn release(app: &AppHandle, slot: Slot) {
    let Some(held) = app.try_state::<Held>() else {
        return;
    };
    let Some(old) = held.slot(slot).lock().ok().and_then(|mut held| held.take()) else {
        return;
    };
    if let Err(error) = let_go(app, old) {
        eprintln!(
            "Raiden could not let go of {}'s hotkey: {error}",
            slot.opens()
        );
    }
    eprintln!(
        "{} gave up its hotkey to {}.",
        slot.opens(),
        slot.other().opens()
    );
}

fn start_slot(app: &AppHandle, files: &Files, slot: Slot, wanted: &mut Hotkey, default: Hotkey) {
    let mut taken = take(app, slot, wanted);
    if let Err(why) = &taken
        && *wanted != default
    {
        files.report(&so(why, &format!("Raiden uses {default}")));
        *wanted = default;
        taken = take(app, slot, wanted);
    }
    match taken {
        Ok(shortcut) => {
            hold(app, slot, shortcut);
        }
        Err(why) => files.report(&so(
            &why,
            &format!(
                "{} opens only {}",
                slot.opens(),
                match slot {
                    Slot::Panel => "from Finder",
                    Slot::Clipboard => "from its command in search",
                }
            ),
        )),
    }
}

pub fn start(app: &AppHandle, development: bool) {
    let files = Files::new(development);
    if let Err(why) = files.write_starter() {
        files.report(&why);
    }
    let seen = files.stamp();
    let mut current = files.read(None);
    let defaults = Settings::defaults(development);
    app.manage(Held::default());
    start_slot(
        app,
        &files,
        Slot::Panel,
        &mut current.hotkey,
        defaults.hotkey,
    );
    start_slot(
        app,
        &files,
        Slot::Clipboard,
        &mut current.clipboard_hotkey,
        defaults.clipboard_hotkey,
    );
    follow_hotkeys(app);
    crate::ai::set_effort(app, current.effort);
    app.manage(Live {
        files,
        ai: Mutex::new(current.ai),
        effort: Mutex::new(current.effort),
        hotkey: Mutex::new(current.hotkey),
        clipboard_hotkey: Mutex::new(current.clipboard_hotkey),
        pinned_repos: Mutex::new(current.pinned_repos),
        pull_requests: Mutex::new(current.pull_requests),
        vim: Mutex::new(current.vim),
        terminal: Mutex::new(current.terminal),
        fixer_key: Mutex::new(current.fixer_key),
        seen: Mutex::new(seen),
    });
    let app = app.clone();
    std::thread::spawn(move || {
        loop {
            sleep(CHECK_EVERY);
            if let Some(live) = app.try_state::<Live>() {
                catch_up(&app, &live);
            }
        }
    });
}

fn catch_up(app: &AppHandle, live: &Live) {
    let Ok(mut seen) = live.seen.lock() else {
        return;
    };
    let stamp = live.files.stamp();
    if stamp == *seen {
        return;
    }
    *seen = stamp;
    let Some(current) = live.current() else {
        return;
    };
    let wanted = live.files.read(Some(&current));
    if wanted.ai != current.ai
        && let Ok(mut ai) = live.ai.lock()
    {
        *ai = wanted.ai;
        eprintln!("Ask AI now answers through {}.", wanted.ai.through());
    }
    if wanted.effort != current.effort
        && let Ok(mut effort) = live.effort.lock()
    {
        *effort = wanted.effort;
        crate::ai::set_effort(app, wanted.effort);
        eprintln!("Ask AI now answers at {} effort.", wanted.effort.id());
    }
    if let Ok(mut pinned) = live.pinned_repos.lock() {
        pinned.clone_from(&wanted.pinned_repos);
    }
    if let Ok(mut allowed) = live.pull_requests.lock() {
        *allowed = wanted.pull_requests;
    }
    if let Ok(mut vim) = live.vim.lock() {
        *vim = wanted.vim;
    }
    if let Ok(mut terminal) = live.terminal.lock() {
        *terminal = wanted.terminal;
    }
    if let Ok(mut fixer_key) = live.fixer_key.lock() {
        fixer_key.clone_from(&wanted.fixer_key);
    }
    let changes: Vec<Change> = [
        Change {
            slot: Slot::Panel,
            now: &current.hotkey,
            next: &wanted.hotkey,
            other_next: &wanted.clipboard_hotkey,
        },
        Change {
            slot: Slot::Clipboard,
            now: &current.clipboard_hotkey,
            next: &wanted.clipboard_hotkey,
            other_next: &wanted.hotkey,
        },
    ]
    .into_iter()
    .filter(|change| {
        change.next != change.now
            || shortcut(change.next).is_ok_and(|keys| !hotkey_is(app, change.slot, keys))
    })
    .collect();
    let waiting: Vec<Change> = changes
        .into_iter()
        .filter(|change| take_changed(app, live, change).is_err())
        .collect();
    for change in waiting {
        if let Err(why) = take_changed(app, live, &change) {
            let left = if held(app, change.slot).is_some() {
                format!("Raiden keeps {}", change.now)
            } else {
                format!("{} has no hotkey", change.slot.opens())
            };
            live.files.report(&so(&why, &left));
        }
    }
}

#[derive(Clone, Copy)]
struct Change<'a> {
    slot: Slot,
    now: &'a Hotkey,
    next: &'a Hotkey,
    other_next: &'a Hotkey,
}

fn gives_way(app: &AppHandle, change: &Change<'_>) -> Result<bool, String> {
    let wanted = shortcut(change.next)?;
    if !hotkey_is(app, change.slot.other(), wanted) {
        return Ok(false);
    }
    let both_want_it = change.other_next == change.next && change.slot == Slot::Panel;
    let swapping =
        shortcut(change.other_next).is_ok_and(|other| hotkey_is(app, change.slot, other));
    Ok(both_want_it || swapping)
}

fn take_changed(app: &AppHandle, live: &Live, change: &Change<'_>) -> Result<(), String> {
    let (slot, next) = (change.slot, change.next);
    if gives_way(app, change)? {
        release(app, slot.other());
    }
    let taken = take(app, slot, next).map(|shortcut| switch_to(app, slot, shortcut));
    if !recording() {
        follow_hotkeys(app);
    }
    taken?;
    eprintln!("{} now opens with {next}.", slot.opens());
    if let Ok(mut hotkey) = live.hotkey_in(slot).lock() {
        hotkey.clone_from(next);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tauri_plugin_global_shortcut::{Code, Modifiers};

    use super::*;

    fn scratch(name: &str) -> Files {
        let folder = std::env::temp_dir().join(format!("raiden-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        Files {
            settings: folder.join("settings").join(settings::FILE),
            log: folder.join(LOG),
            development: false,
        }
    }

    #[test]
    fn choosing_an_ai_with_no_file_writes_the_starter_with_it() {
        let files = scratch("missing");
        files.write(settings::AI, "chatgpt").expect("written");
        let written = std::fs::read_to_string(&files.settings).expect("a file");
        assert!(written.starts_with(&settings::starter(false)), "{written}");
        assert_eq!(files.read(None).ai, Provider::Chatgpt);
        files.write(settings::AI, "claude").expect("written again");
        assert_eq!(files.read(None).ai, Provider::Claude);
        let folder = files.settings.parent().expect("a folder");
        let names: Vec<_> = std::fs::read_dir(folder)
            .expect("the folder")
            .map(|entry| entry.expect("an entry").file_name())
            .collect();
        assert_eq!(names, [settings::FILE]);
        let _ = std::fs::remove_dir_all(folder.parent().expect("the scratch folder"));
    }

    #[test]
    fn choosing_an_effort_with_no_file_writes_the_starter_with_it() {
        let files = scratch("effort");
        files.write(settings::EFFORT, "high").expect("written");
        let written = std::fs::read_to_string(&files.settings).expect("a file");
        assert!(written.starts_with(&settings::starter(false)), "{written}");
        assert_eq!(files.read(None).effort, Effort::High);
        files
            .write(settings::EFFORT, "medium")
            .expect("written again");
        let again = std::fs::read_to_string(&files.settings).expect("a file");
        assert_eq!(again.matches("\neffort = ").count(), 1, "{again}");
        assert_eq!(files.read(None).effort, Effort::Medium);
        let folder = files.settings.parent().expect("a folder");
        let _ = std::fs::remove_dir_all(folder.parent().expect("the scratch folder"));
    }

    #[test]
    fn turning_vim_keys_on_keeps_a_comment_placed_above_it() {
        let files = scratch("vim");
        let folder = files.settings.parent().expect("a folder");
        std::fs::create_dir_all(folder).expect("the folder");
        let mine = "# Home row, please.\nvim = false # for now\n";
        std::fs::write(&files.settings, mine).expect("written");
        files.write(settings::VIM, true).expect("written");
        let written = std::fs::read_to_string(&files.settings).expect("a file");
        assert!(
            written.starts_with("# Home row, please.\nvim = true"),
            "{written}"
        );
        assert!(written.contains("# for now"), "{written}");
        assert!(files.read(None).vim);
        files.write(settings::VIM, false).expect("written again");
        assert!(!files.read(None).vim);
        let _ = std::fs::remove_dir_all(folder.parent().expect("the scratch folder"));
    }

    #[test]
    fn pins_and_the_first_use_choice_are_written_into_the_starter() {
        let files = scratch("pulls");
        files.write(settings::PULL_REQUESTS, true).expect("written");
        let pins = vec!["fernhill/ledger".to_owned(), "tidewater/lantern".to_owned()];
        files
            .write(settings::PINNED_REPOS, settings::list(&pins))
            .expect("written");
        let written = std::fs::read_to_string(&files.settings).expect("a file");
        assert!(written.starts_with(&settings::starter(false)), "{written}");
        let read = files.read(None);
        assert!(read.pull_requests);
        assert_eq!(read.pinned_repos, pins);
        let folder = files.settings.parent().expect("a folder");
        let _ = std::fs::remove_dir_all(folder.parent().expect("the scratch folder"));
    }

    #[test]
    fn choosing_an_ai_through_a_link_changes_the_linked_file() {
        let files = scratch("linked");
        let folder = files.settings.parent().expect("a folder").to_owned();
        std::fs::create_dir_all(&folder).expect("a folder");
        let kept = folder.join("dotfiles.toml");
        std::fs::write(&kept, "# mine\n").expect("written");
        std::os::unix::fs::symlink(&kept, &files.settings).expect("linked");
        files.write(settings::AI, "chatgpt").expect("written");
        assert!(
            std::fs::symlink_metadata(&files.settings)
                .expect("the link")
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read_to_string(&kept).expect("the file"),
            "# mine\nai = \"chatgpt\"\n"
        );
        std::fs::remove_file(&kept).expect("removed");
        assert!(files.write(settings::AI, "claude").is_err());
        let _ = std::fs::remove_dir_all(folder.parent().expect("the scratch folder"));
    }

    #[test]
    fn choosing_an_ai_leaves_a_broken_file_as_it_is() {
        let files = scratch("broken");
        files.write(settings::AI, "chatgpt").expect("written");
        std::fs::write(&files.settings, "hotkey = ").expect("broken");
        assert!(files.write(settings::AI, "claude").is_err());
        assert_eq!(
            std::fs::read_to_string(&files.settings).expect("a file"),
            "hotkey = "
        );
        let folder = files.settings.parent().expect("a folder");
        let _ = std::fs::remove_dir_all(folder.parent().expect("the scratch folder"));
    }

    fn parsed(text: &str) -> Result<Shortcut, String> {
        shortcut(&Hotkey::parse(text)?)
    }

    #[test]
    fn the_defaults_are_the_hotkeys_raiden_always_had() {
        let alt = Modifiers::ALT;
        assert_eq!(
            shortcut(&Settings::defaults(false).hotkey),
            Ok(Shortcut::new(Some(alt), Code::Space))
        );
        assert_eq!(
            shortcut(&Settings::defaults(true).hotkey),
            Ok(Shortcut::new(Some(alt | Modifiers::SHIFT), Code::Space))
        );
        assert_eq!(
            shortcut(&Settings::defaults(false).clipboard_hotkey),
            Ok(Shortcut::new(Some(Modifiers::CONTROL | alt), Code::KeyV))
        );
    }

    #[test]
    fn command_space_is_the_shortcut_spotlight_uses() {
        assert_eq!(parsed("⌘Space"), Ok(crate::spotlight::command_space()));
        assert_eq!(
            parsed("Command+Space"),
            Ok(crate::spotlight::command_space())
        );
        assert_ne!(parsed("⌥⌘Space"), Ok(crate::spotlight::command_space()));
    }

    #[test]
    fn a_problem_reads_as_one_sentence_or_two() {
        assert_eq!(
            so("Raiden could not take ⌃⌥K (taken)", "Raiden keeps ⌥Space"),
            "Raiden could not take ⌃⌥K (taken), so Raiden keeps ⌥Space."
        );
        assert_eq!(
            so(settings::SPOTLIGHT_HOLDS, "raiden uses ⌥Space"),
            format!("{} Raiden uses ⌥Space.", settings::SPOTLIGHT_HOLDS)
        );
    }

    #[test]
    fn written_hotkeys_become_the_shortcuts_they_name() {
        let control_option = Modifiers::CONTROL | Modifiers::ALT;
        assert_eq!(
            parsed("⌃⌥Space"),
            Ok(Shortcut::new(Some(control_option), Code::Space))
        );
        assert_eq!(
            parsed("Command+Shift+k"),
            Ok(Shortcut::new(
                Some(Modifiers::SUPER | Modifiers::SHIFT),
                Code::KeyK
            ))
        );
        assert_eq!(
            parsed("⌥F5"),
            Ok(Shortcut::new(Some(Modifiers::ALT), Code::F5))
        );
        assert!(parsed("⌥Nowhere").is_err());
    }
}
