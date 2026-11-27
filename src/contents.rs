use std::os::darwin::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::{Duration, Instant};

use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::string::{CFString, CFStringRef};
use objc2_foundation::NSProcessInfo;
use raiden_core::contents::{Contents, Found, Index as Built, Mapped, Shape, build_into, readable};
use raiden_core::files::{folder_label, icon_key};
use tauri::{AppHandle, Emitter, State};

use crate::files::Files;
use crate::names::{Change, Index as NameIndex, real_home};

const REBUILD_AFTER: usize = 5000;
const SHOWN: usize = 40;
const DATALESS: u32 = 0x4000_0000;
const RETRY_AFTER: Duration = Duration::from_secs(10 * 60);
const SUBTREES_AT_MOST_EVERY: Duration = Duration::from_secs(5 * 60);
const EVENT: &str = "contents";

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOPSCopyPowerSourcesInfo() -> CFTypeRef;
    fn IOPSGetProvidingPowerSourceType(snapshot: CFTypeRef) -> CFStringRef;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Waiting,
    Building,
    Ready,
    Off,
}

enum Wake {
    Names(Change),
    Asked,
}

pub struct ContentIndex {
    phase: Mutex<Phase>,
    contents: RwLock<Option<Contents<Mapped>>>,
    wake: Mutex<Sender<Wake>>,
    asked: AtomicU64,
    home: String,
    screen: Mutex<Option<AppHandle>>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ContentRow {
    name: String,
    path: String,
    folder: String,
    icon: String,
    line: usize,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ContentAnswer {
    query: String,
    rows: Vec<ContentRow>,
    state: Phase,
}

fn on_power() -> bool {
    let snapshot = unsafe { IOPSCopyPowerSourcesInfo() };
    if snapshot.is_null() {
        return true;
    }
    let source = unsafe { IOPSGetProvidingPowerSourceType(snapshot) };
    let plugged =
        source.is_null() || unsafe { CFString::wrap_under_get_rule(source) } != "Battery Power";
    unsafe { CFRelease(snapshot) };
    plugged
}

fn may_read_everything() -> bool {
    on_power() && !NSProcessInfo::processInfo().isLowPowerModeEnabled()
}

fn readable_files(paths: Vec<String>) -> Vec<(String, u64)> {
    paths
        .into_iter()
        .filter_map(|path| {
            let found = std::fs::symlink_metadata(&path).ok()?;
            let evicted = found.st_flags() & DATALESS != 0;
            (found.is_file() && !evicted && readable(&path, found.len()))
                .then_some((path, found.len()))
        })
        .collect()
}

fn build(names: &NameIndex, saved_to: &Path) -> std::io::Result<Contents<Mapped>> {
    if let Some(folder) = saved_to.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let writing = saved_to.with_extension("writing");
    build_into(Shape::default(), &readable_files(names.files()), &writing)?;
    std::fs::set_permissions(
        &writing,
        std::os::unix::fs::PermissionsExt::from_mode(0o600),
    )?;
    std::fs::rename(&writing, saved_to)?;
    open(saved_to)
}

fn open(saved_to: &Path) -> std::io::Result<Contents<Mapped>> {
    Ok(Contents::new(Some(Built::open(Mapped::open(saved_to)?)?)))
}

impl ContentIndex {
    fn set_phase(&self, phase: Phase) {
        *self.phase.lock().unwrap_or_else(PoisonError::into_inner) = phase;
    }

    fn phase(&self) -> Phase {
        *self.phase.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn keep_current(&self, names: &NameIndex, wakes: &Receiver<Wake>, saved_to: &Path) {
        run_in_background();
        let mut stale = true;
        let mut reconciled = false;
        let mut failed_at: Option<Instant> = None;
        let mut waiting: Vec<(PathBuf, bool)> = Vec::new();
        let mut lost: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
        let mut checked_subtrees_at: Option<Instant> = None;
        if let Ok(contents) = open(saved_to) {
            self.replace(contents);
            self.set_phase(Phase::Ready);
            stale = false;
        }
        loop {
            let Ok(first) = wakes.recv() else {
                return;
            };
            let mut reported: Vec<(PathBuf, bool)> = Vec::new();
            for wake in std::iter::once(first).chain(wakes.try_iter()) {
                match wake {
                    Wake::Names(Change::Folder(folder)) => reported.push((folder, false)),
                    Wake::Names(Change::Added(folder)) => reported.push((folder, true)),
                    Wake::Names(Change::Lost(folder)) => {
                        lost.insert(folder);
                    }
                    Wake::Names(Change::Removed(folder)) => self.reconcile(&folder, true, &[]),
                    Wake::Names(Change::Ready) | Wake::Asked => {}
                }
            }
            let plugged = may_read_everything();
            if plugged
                && !lost.is_empty()
                && checked_subtrees_at.is_none_or(|at| at.elapsed() >= SUBTREES_AT_MOST_EVERY)
            {
                reported.extend(lost.drain().map(|folder| (folder, true)));
                checked_subtrees_at = Some(Instant::now());
            }
            for (folder, below) in merged_folders(reported) {
                if plugged || !self.crowded() {
                    self.catch_up(names, &folder, below);
                } else {
                    waiting.push((folder, below));
                }
            }
            if !names.ready() || !plugged {
                if !plugged && stale && self.phase() != Phase::Ready {
                    self.set_phase(Phase::Waiting);
                }
                continue;
            }
            for (folder, below) in merged_folders(std::mem::take(&mut waiting)) {
                self.catch_up(names, &folder, below);
            }
            if !reconciled && !stale {
                self.reconcile(Path::new(&self.home), true, &names.files());
                reconciled = true;
            }
            let resting = failed_at.is_some_and(|at| at.elapsed() < RETRY_AFTER);
            if !(stale || self.crowded()) || resting {
                continue;
            }
            let had = self.phase();
            if had != Phase::Ready {
                self.set_phase(Phase::Building);
            }
            match build(names, saved_to) {
                Ok(contents) => {
                    self.replace(contents);
                    self.set_phase(Phase::Ready);
                    stale = false;
                    reconciled = true;
                    failed_at = None;
                    self.tell_screen();
                }
                Err(error) => {
                    eprintln!("Raiden could not read your files' text: {error}");
                    failed_at = Some(Instant::now());
                    self.set_phase(if had == Phase::Ready { had } else { Phase::Off });
                }
            }
        }
    }

    fn crowded(&self) -> bool {
        self.contents
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_some_and(|contents| contents.changed_since_built() > REBUILD_AFTER)
    }

    fn catch_up(&self, names: &NameIndex, folder: &Path, below: bool) {
        let listed = if below {
            names.files_under(folder)
        } else {
            names.files_in(folder)
        };
        self.reconcile(folder, below, &listed);
    }

    fn replace(&self, contents: Contents<Mapped>) {
        *self
            .contents
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(contents);
    }

    fn reconcile(&self, folder: &Path, below: bool, listed: &[String]) {
        let update = match self
            .contents
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            Some(contents) => contents.prepare(folder, below, listed),
            None => return,
        };
        if let Some(contents) = self
            .contents
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
        {
            contents.apply(update);
        }
    }

    fn tell_screen(&self) {
        let screen = self.screen.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(app) = screen.as_ref()
            && let Err(error) = app.emit_to(crate::PANEL, EVENT, "ready")
        {
            eprintln!("Raiden could not tell its screen the text index is ready: {error}");
        }
    }

    pub fn attach(&self, app: AppHandle) {
        *self.screen.lock().unwrap_or_else(PoisonError::into_inner) = Some(app);
    }

    fn search(&self, query: &str, asked: u64) -> Vec<Found> {
        let superseded = || self.asked.load(Ordering::SeqCst) > asked;
        self.contents
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|contents| contents.search(query, SHOWN, &superseded))
            .unwrap_or_default()
    }

    fn row(&self, found: Found) -> ContentRow {
        ContentRow {
            name: found.path.rsplit('/').next().unwrap_or_default().to_owned(),
            folder: folder_label(&found.path, &self.home),
            icon: icon_key(&found.path, true),
            line: found.line,
            text: found.text,
            path: found.path,
        }
    }
}

fn merged_folders(reported: Vec<(PathBuf, bool)>) -> Vec<(PathBuf, bool)> {
    let mut merged: std::collections::BTreeMap<PathBuf, bool> = std::collections::BTreeMap::new();
    for (folder, below) in reported {
        let known = merged.entry(folder).or_insert(below);
        *known |= below;
    }
    let whole: Vec<PathBuf> = merged
        .iter()
        .filter(|(_, below)| **below)
        .map(|(folder, _)| folder.clone())
        .collect();
    merged
        .into_iter()
        .filter(|(folder, _)| {
            !folder
                .ancestors()
                .skip(1)
                .any(|above| whole.iter().any(|covering| covering == above))
        })
        .collect()
}

fn run_in_background() {
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_BACKGROUND, 0);
    }
}

pub fn start(names: &Arc<NameIndex>, home: &str, state: &Path) -> Arc<ContentIndex> {
    let (wake, wakes) = channel();
    let index = Arc::new(ContentIndex {
        phase: Mutex::new(Phase::Waiting),
        contents: RwLock::new(None),
        wake: Mutex::new(wake.clone()),
        asked: AtomicU64::new(0),
        home: real_home(home),
        screen: Mutex::new(None),
    });
    let changes = names.listen();
    if names.ready() {
        let _ = wake.send(Wake::Names(Change::Ready));
    }
    let forwarded = std::thread::Builder::new()
        .name("content index events".to_owned())
        .spawn(move || {
            for change in changes {
                if wake.send(Wake::Names(change)).is_err() {
                    return;
                }
            }
        });
    let kept = Arc::clone(&index);
    let names = Arc::clone(names);
    let saved_to: PathBuf = state.join("contents.idx");
    let started = std::thread::Builder::new()
        .name("content index".to_owned())
        .spawn(move || kept.keep_current(&names, &wakes, &saved_to));
    if forwarded.is_err() || started.is_err() {
        eprintln!("Raiden could not start reading your files' text.");
        index.set_phase(Phase::Off);
    }
    index
}

#[tauri::command]
pub async fn contents(
    index: State<'_, Arc<ContentIndex>>,
    files: State<'_, Files>,
    query: String,
    asked: u64,
) -> Result<Option<ContentAnswer>, ()> {
    index.asked.fetch_max(asked, Ordering::SeqCst);
    let _ = index
        .wake
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .send(Wake::Asked);
    let searching = Arc::clone(&index);
    let wanted = query.clone();
    let found = tokio::task::spawn_blocking(move || searching.search(&wanted, asked))
        .await
        .map_err(|_| ())?;
    if index.asked.load(Ordering::SeqCst) > asked {
        return Ok(None);
    }
    let rows: Vec<ContentRow> = found.into_iter().map(|found| index.row(found)).collect();
    files.remember_paths(rows.iter().map(|row| row.path.as_str()));
    Ok(Some(ContentAnswer {
        query,
        rows,
        state: index.phase(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_mac_reports_where_its_power_comes_from() {
        let _ = on_power();
        let _ = may_read_everything();
    }

    #[test]
    fn evicted_binary_and_huge_files_are_never_read() {
        let folder = std::env::temp_dir().join(format!("raiden-readable-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let text = folder.join("notes.md");
        let picture = folder.join("photo.jpg");
        let huge = folder.join("huge.txt");
        std::fs::write(&text, "words").unwrap();
        std::fs::write(&picture, "jpg").unwrap();
        std::fs::write(&huge, vec![b'a'; 2 * 1024 * 1024]).unwrap();
        let kept = readable_files(
            [&text, &picture, &huge, &folder.join("missing.md")]
                .iter()
                .map(|path| path.to_str().unwrap().to_owned())
                .collect(),
        );
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(kept, [(text.to_str().unwrap().to_owned(), 5)]);
    }

    #[test]
    fn the_index_is_built_once_the_names_are_ready_then_follows_edits() {
        let base = std::env::temp_dir().join(format!("raiden-contents-app-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let home = base.join("home");
        std::fs::create_dir_all(home.join("Documents")).unwrap();
        let home = home.canonicalize().unwrap();
        std::fs::write(home.join("Documents/plan.md"), "the quarterly plan").unwrap();
        let home_text = home.to_str().unwrap().to_owned();
        let names = crate::names::start(&home_text, &base.join("state"));
        let index = start(&names, &home_text, &base.join("state"));
        let started = std::time::Instant::now();
        let mut found = Vec::new();
        while found.is_empty() && started.elapsed() < std::time::Duration::from_secs(20) {
            let _ = index
                .wake
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .send(Wake::Asked);
            std::thread::sleep(std::time::Duration::from_millis(100));
            found = index.search("quarterly plan", 0);
        }
        if !may_read_everything() {
            let _ = std::fs::remove_dir_all(&base);
            assert_eq!(index.phase(), Phase::Waiting);
            return;
        }
        assert_eq!(found.len(), 1, "the first build never answered");
        std::fs::write(home.join("Documents/later.md"), "a fresh forecast").unwrap();
        let started = std::time::Instant::now();
        let mut later = Vec::new();
        while later.is_empty() && started.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(100));
            later = index.search("fresh forecast", 0);
        }
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(later.len(), 1, "a new file's text was not seen");
        assert_eq!(index.phase(), Phase::Ready);
    }

    #[test]
    fn queued_folders_are_merged_and_covered_ones_dropped() {
        let merged = merged_folders(vec![
            (PathBuf::from("/h/Code/app/src"), false),
            (PathBuf::from("/h/Code/app/src"), false),
            (PathBuf::from("/h/Code"), true),
            (PathBuf::from("/h/Docs"), false),
            (PathBuf::from("/h/Docs"), true),
            (PathBuf::from("/h/Notes"), false),
        ]);
        assert_eq!(
            merged,
            [
                (PathBuf::from("/h/Code"), true),
                (PathBuf::from("/h/Docs"), true),
                (PathBuf::from("/h/Notes"), false),
            ]
        );
    }
}
