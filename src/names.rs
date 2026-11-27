use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_void};
use std::fs::File;
use std::io::{BufReader, BufWriter, Write as _};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::{Duration, Instant, SystemTime};

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFAllocatorRef, CFTypeRef, TCFType};
use core_foundation::runloop::{CFRunLoop, CFRunLoopRef, kCFRunLoopDefaultMode};
use core_foundation::string::{CFString, CFStringRef};
use raiden_core::files;
use raiden_core::names::{Hit, Names, Stamp};

type StreamRef = *mut c_void;

const SINCE_NOW: u64 = u64::MAX;
const LATENCY: f64 = 1.0;
const SAVED_AT_MOST_EVERY: Duration = Duration::from_secs(10 * 60);
const MOST_EXCLUDED: usize = 8;
const WATCH_ROOT: u32 = 0x0000_0004;
const SLACK: Duration = Duration::from_secs(10);
const PROTECTED: [&str; 3] = ["Desktop", "Documents", "Downloads"];

const MUST_SCAN_SUBDIRS: u32 = 0x0000_0001;
const USER_DROPPED: u32 = 0x0000_0002;
const KERNEL_DROPPED: u32 = 0x0000_0004;
const IDS_WRAPPED: u32 = 0x0000_0008;
const HISTORY_DONE: u32 = 0x0000_0010;
const ROOT_CHANGED: u32 = 0x0000_0020;
const LOST_TRACK: u32 = MUST_SCAN_SUBDIRS | USER_DROPPED | KERNEL_DROPPED | IDS_WRAPPED;

#[repr(C)]
struct StreamContext {
    version: isize,
    info: *mut c_void,
    retain: *const c_void,
    release: *const c_void,
    describe: *const c_void,
}

type Callback = extern "C" fn(StreamRef, *mut c_void, usize, *mut c_void, *const u32, *const u64);

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn FSEventStreamCreate(
        allocator: CFAllocatorRef,
        callback: Callback,
        context: *const StreamContext,
        paths: CFArrayRef,
        since: u64,
        latency: f64,
        flags: u32,
    ) -> StreamRef;
    fn FSEventStreamSetExclusionPaths(stream: StreamRef, paths: CFArrayRef) -> u8;
    fn FSEventStreamScheduleWithRunLoop(
        stream: StreamRef,
        run_loop: CFRunLoopRef,
        mode: CFStringRef,
    );
    fn FSEventStreamStart(stream: StreamRef) -> u8;
    fn FSEventsGetCurrentEventId() -> u64;
    fn FSEventsCopyUUIDForDevice(device: libc::dev_t) -> CFTypeRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFUUIDCreateString(allocator: CFAllocatorRef, uuid: CFTypeRef) -> CFStringRef;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Ready,
    Folder(PathBuf),
    Lost(PathBuf),
    Added(PathBuf),
    Removed(PathBuf),
}

#[derive(Default)]
pub struct Index(RwLock<Option<Names>>, Mutex<Vec<Sender<Change>>>);

pub struct Lookup {
    pub hits: Vec<Hit>,
    pub blocked: Vec<String>,
}

impl Index {
    pub fn search(&self, query: &str, most: usize) -> Option<Lookup> {
        let names = self.0.read().unwrap_or_else(PoisonError::into_inner);
        names.as_ref().map(|names| {
            let blocked = names.blocked();
            Lookup {
                hits: names.search(query, most, &blocked),
                blocked: protected_among(names.home(), blocked),
            }
        })
    }

    fn block(&self, path: &Path) {
        let mut names = self.0.write().unwrap_or_else(PoisonError::into_inner);
        if let Some(names) = names.as_mut() {
            names.block(path);
        }
    }

    fn set(&self, names: Option<Names>) {
        let ready = names.is_some();
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = names;
        if ready {
            self.tell(&Change::Ready);
        }
    }

    pub fn listen(&self) -> Receiver<Change> {
        let (sender, changes) = channel();
        self.1
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(sender);
        changes
    }

    fn tell(&self, change: &Change) {
        self.1
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|listener| listener.send(change.clone()).is_ok());
    }

    pub fn ready(&self) -> bool {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }

    pub fn files(&self) -> Vec<String> {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(Names::files)
            .unwrap_or_default()
    }

    fn refresh(&self, path: &Path, deep: Option<SystemTime>) -> bool {
        let changes = {
            let names = self.0.read().unwrap_or_else(PoisonError::into_inner);
            match names.as_ref() {
                Some(names) => names.changes(path, deep),
                None => return false,
            }
        };
        let (changed, added, removed) = {
            let mut names = self.0.write().unwrap_or_else(PoisonError::into_inner);
            match names.as_mut() {
                Some(names) => {
                    let changed = names.apply(changes);
                    (changed, names.take_added(), names.take_removed())
                }
                None => return false,
            }
        };
        for folder in removed {
            self.tell(&Change::Removed(PathBuf::from(folder)));
        }
        for folder in added {
            self.tell(&Change::Added(PathBuf::from(folder)));
        }
        changed
    }

    pub fn files_in(&self, folder: &Path) -> Vec<String> {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|names| names.files_in(folder))
            .unwrap_or_default()
    }

    pub fn files_under(&self, folder: &Path) -> Vec<String> {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|names| names.files_under(folder))
            .unwrap_or_default()
    }

    fn save(&self, stamp: &Stamp, to: &Path) {
        let names = self.0.read().unwrap_or_else(PoisonError::into_inner);
        if let Some(names) = names.as_ref()
            && let Err(error) = write(names, stamp, to)
        {
            eprintln!("Raiden could not save its file index: {error}");
        }
    }
}

fn write(names: &Names, stamp: &Stamp, to: &Path) -> std::io::Result<()> {
    if let Some(folder) = to.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let writing = to.with_extension("writing");
    let mut out = BufWriter::new(
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&writing)?,
    );
    names.write(stamp, &mut out)?;
    out.flush()?;
    out.into_inner()
        .map_err(std::io::IntoInnerError::into_error)?
        .sync_all()?;
    std::fs::rename(writing, to)
}

fn protected(home: &str) -> Vec<String> {
    let mut folders: Vec<String> = PROTECTED
        .iter()
        .map(|folder| format!("{home}/{folder}"))
        .collect();
    folders.push(files::icloud_drive(home));
    folders
}

fn protected_among(home: &str, blocked: Vec<String>) -> Vec<String> {
    let roots = protected(home);
    blocked
        .into_iter()
        .filter(|folder| {
            roots.iter().any(|root| {
                folder == root
                    || folder
                        .strip_prefix(root.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
            })
        })
        .collect()
}

fn read(from: &Path, home: &str, history: &str) -> Option<(Names, Stamp)> {
    let (names, stamp) = Names::read(BufReader::new(File::open(from).ok()?)).ok()?;
    let current = names.home() == home.trim_end_matches('/')
        && !history.is_empty()
        && stamp.history == history
        && stamp.home > 0
        && stamp.drive > 0
        && stamp.at > 0
        && SystemTime::UNIX_EPOCH
            .checked_add(Duration::from_secs(stamp.at))
            .is_some_and(|at| at <= SystemTime::now() + Duration::from_secs(86_400));
    current.then_some((names, stamp))
}

fn history_of(folder: &str) -> String {
    let Ok(device) = std::fs::metadata(folder).map(|found| found.dev()) else {
        return String::new();
    };
    let Ok(device) = libc::dev_t::try_from(device) else {
        return String::new();
    };
    let uuid = unsafe { FSEventsCopyUUIDForDevice(device) };
    if uuid.is_null() {
        return String::new();
    }
    let text = unsafe { CFUUIDCreateString(std::ptr::null(), uuid) };
    unsafe { core_foundation::base::CFRelease(uuid) };
    if text.is_null() {
        return String::new();
    }
    unsafe { CFString::wrap_under_create_rule(text) }.to_string()
}

fn run_in_background() {
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_BACKGROUND, 0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Root {
    Home,
    Drive,
}

struct Watch {
    index: Arc<Index>,
    home: String,
    drive_stream: StreamRef,
    saved_to: PathBuf,
    stamp: Stamp,
    heard_at: [SystemTime; 2],
    replaying: [bool; 2],
    saved_at: Instant,
    changed: bool,
}

impl Root {
    fn at(self) -> usize {
        match self {
            Self::Home => 0,
            Self::Drive => 1,
        }
    }
}

fn seconds(at: SystemTime) -> u64 {
    at.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

fn merged(
    events: &[(String, u32, u64)],
    home: &str,
    root: &str,
    lost_since: SystemTime,
) -> Vec<(PathBuf, Option<SystemTime>)> {
    let mut wanted: HashMap<PathBuf, Option<SystemTime>> = HashMap::new();
    for (path, flags, _) in events {
        if flags & HISTORY_DONE != 0 {
            continue;
        }
        let path = path.trim_end_matches('/');
        let above_root =
            root.starts_with(path) && (path.is_empty() || root[path.len()..].starts_with('/'));
        let deep = if flags & ROOT_CHANGED != 0 {
            Some(SystemTime::UNIX_EPOCH)
        } else {
            (flags & LOST_TRACK != 0).then_some(lost_since)
        };
        let path = match (above_root, deep) {
            (true, Some(_)) => PathBuf::from(root),
            (true, None) => continue,
            (false, _) if !watched(path, home) => continue,
            (false, _) => PathBuf::from(path),
        };
        let known = wanted.entry(path).or_insert(deep);
        *known = match (*known, deep) {
            (Some(known), Some(new)) => Some(known.min(new)),
            (known, new) => known.or(new),
        };
    }
    wanted.into_iter().collect()
}

impl Watch {
    fn took_in(&mut self, root: Root, events: &[(String, u32, u64)]) {
        let heard_at = SystemTime::now();
        let settled = if self.replaying[root.at()] {
            SystemTime::UNIX_EPOCH + Duration::from_secs(self.stamp.at)
        } else {
            self.heard_at[root.at()]
        };
        let lost_since = settled.checked_sub(SLACK).unwrap_or(SystemTime::UNIX_EPOCH);
        for (_, flags, event) in events {
            if flags & HISTORY_DONE != 0 {
                self.replaying[root.at()] = false;
            }
            if flags & HISTORY_DONE != 0 || *event == 0 {
                continue;
            }
            match root {
                Root::Home => self.stamp.home = *event,
                Root::Drive => self.stamp.drive = *event,
            }
        }
        let root_path = match root {
            Root::Home => self.home.clone(),
            Root::Drive => files::icloud_drive(&self.home),
        };
        for (path, deep) in merged(events, &self.home, &root_path, lost_since) {
            self.changed |= self.index.refresh(&path, deep);
            self.index.tell(&match deep {
                Some(_) => Change::Lost(path),
                None => Change::Folder(path),
            });
        }
        self.heard_at[root.at()] = heard_at;
        if self.changed && self.saved_at.elapsed() >= SAVED_AT_MOST_EVERY {
            self.save();
        }
    }

    fn save(&mut self) {
        self.stamp.at = seconds(self.heard_at[0].min(self.heard_at[1]));
        self.index.save(&self.stamp, &self.saved_to);
        self.saved_at = Instant::now();
        self.changed = false;
    }
}

extern "C" fn heard(
    stream: StreamRef,
    info: *mut c_void,
    count: usize,
    paths: *mut c_void,
    flags: *const u32,
    ids: *const u64,
) {
    let watch = unsafe { &mut *info.cast::<Watch>() };
    let paths = unsafe { std::slice::from_raw_parts(paths.cast::<*const c_char>(), count) };
    let flags = unsafe { std::slice::from_raw_parts(flags, count) };
    let ids = unsafe { std::slice::from_raw_parts(ids, count) };
    let events: Vec<(String, u32, u64)> = paths
        .iter()
        .zip(flags)
        .zip(ids)
        .map(|((path, flags), id)| {
            let path = unsafe { CStr::from_ptr(*path) }
                .to_string_lossy()
                .into_owned();
            (path, *flags, *id)
        })
        .collect();
    let root = if stream == watch.drive_stream {
        Root::Drive
    } else {
        Root::Home
    };
    watch.took_in(root, &events);
}

fn under(path: &str, folder: &str) -> bool {
    path.strip_prefix(folder)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

fn watched(path: &str, home: &str) -> bool {
    under(path, &files::icloud_drive(home))
        || path
            .strip_prefix(home)
            .filter(|rest| rest.is_empty() || rest.starts_with('/'))
            .and_then(|rest| rest.trim_start_matches('/').split('/').next())
            .is_some_and(|first| first.is_empty() || files::listed(first, true))
}

fn left_out(home: &str) -> Vec<String> {
    let mut left_out = vec![format!("{home}/Library")];
    let hidden = std::fs::read_dir(home)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !files::listed(name, true) && name != "Library")
        .map(|name| format!("{home}/{name}"));
    left_out.extend(hidden.take(MOST_EXCLUDED - 1));
    left_out
}

fn strings(paths: &[String]) -> CFArray<CFString> {
    let strings: Vec<CFString> = paths.iter().map(|path| CFString::new(path)).collect();
    CFArray::from_CFTypes(&strings)
}

fn stream(info: *mut c_void, root: &str, since: u64, left_out: &[String], flags: u32) -> StreamRef {
    let context = StreamContext {
        version: 0,
        info,
        retain: std::ptr::null(),
        release: std::ptr::null(),
        describe: std::ptr::null(),
    };
    let roots = strings(&[root.to_owned()]);
    let stream = unsafe {
        FSEventStreamCreate(
            std::ptr::null(),
            heard,
            &raw const context,
            roots.as_concrete_TypeRef(),
            if since == 0 { SINCE_NOW } else { since },
            LATENCY,
            flags,
        )
    };
    if !stream.is_null() && !left_out.is_empty() {
        unsafe { FSEventStreamSetExclusionPaths(stream, strings(left_out).as_concrete_TypeRef()) };
    }
    stream
}

fn run(stream: StreamRef) -> bool {
    if stream.is_null() {
        return false;
    }
    unsafe {
        FSEventStreamScheduleWithRunLoop(
            stream,
            CFRunLoop::get_current().as_concrete_TypeRef(),
            kCFRunLoopDefaultMode,
        );
        FSEventStreamStart(stream) != 0
    }
}

fn follow(watch: Watch) {
    let home = watch.home.clone();
    let (home_since, drive_since) = (watch.stamp.home, watch.stamp.drive);
    let watch = Box::into_raw(Box::new(watch));
    let info: *mut c_void = watch.cast();
    let home_stream = stream(info, &home, home_since, &left_out(&home), 0);
    let drive = files::icloud_drive(&home);
    let drive_stream = stream(info, &drive, drive_since, &[], WATCH_ROOT);
    unsafe { (*watch).drive_stream = drive_stream };
    if !run(home_stream) {
        eprintln!("Raiden could not watch the home folder, so file search uses Spotlight.");
        unsafe { (*watch).index.set(None) };
        return;
    }
    if !run(drive_stream) {
        eprintln!("Raiden could not watch iCloud Drive, so Spotlight searches it.");
        unsafe { (*watch).index.block(Path::new(&drive)) };
    }
    CFRunLoop::run_current();
}

fn recheck_access(names: &mut Names, home: &str) {
    for folder in protected(home) {
        names.refresh(Path::new(&folder), None);
    }
}

fn keep_current(index: &Arc<Index>, home: &str, saved_to: PathBuf) {
    run_in_background();
    let history = history_of(home);
    let stamp = match read(&saved_to, home, &history) {
        Some((mut names, stamp)) => {
            recheck_access(&mut names, home);
            index.set(Some(names));
            stamp
        }
        None => {
            let since = unsafe { FSEventsGetCurrentEventId() };
            let started = SystemTime::now();
            index.set(Some(Names::build(home)));
            eprintln!(
                "Raiden indexed the home folder in {:?}.",
                started.elapsed().unwrap_or_default()
            );
            let stamp = Stamp {
                home: since,
                drive: since,
                at: seconds(started),
                history,
            };
            index.save(&stamp, &saved_to);
            stamp
        }
    };
    let heard_at = SystemTime::UNIX_EPOCH
        .checked_add(Duration::from_secs(stamp.at))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    follow(Watch {
        index: Arc::clone(index),
        home: home.to_owned(),
        drive_stream: std::ptr::null_mut(),
        saved_to,
        stamp,
        heard_at: [heard_at; 2],
        replaying: [true; 2],
        saved_at: Instant::now(),
        changed: false,
    });
}

pub fn real_home(home: &str) -> String {
    std::fs::canonicalize(home)
        .ok()
        .and_then(|home| home.to_str().map(str::to_owned))
        .unwrap_or_else(|| home.trim_end_matches('/').to_owned())
}

#[cfg(test)]
pub fn start(home: &str, state: &Path) -> Arc<Index> {
    let index = Arc::new(Index::default());
    begin(&index, home, state);
    index
}

pub fn begin(index: &Arc<Index>, home: &str, state: &Path) {
    let kept = Arc::clone(index);
    let home = real_home(home);
    let saved_to = state.join("names.idx");
    let started = std::thread::Builder::new()
        .name("file index".to_owned())
        .spawn(move || keep_current(&kept, &home, saved_to));
    if let Err(error) = started {
        eprintln!("Raiden could not start its file index: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_changes_the_index_could_hold_are_taken_in() {
        let home = "/Users/me";
        for wanted in [
            "/Users/me",
            "/Users/me/",
            "/Users/me/Documents",
            "/Users/me/Code/raiden/src",
            "/Users/me/Library/Mobile Documents/com~apple~CloudDocs/Work",
        ] {
            assert!(watched(wanted, home), "{wanted}");
        }
        for ignored in [
            "/Users/me/Library/Caches/x",
            "/Users/me/.cache/x",
            "/Users/me/Applications/x",
            "/Users/me/Library/Mobile Documents/com~apple~Pages",
            "/Users/meg/Documents",
            "/Users/me/Library/Mobile Documents/com~apple~CloudDocsOld/x",
        ] {
            assert!(!watched(ignored, home), "{ignored}");
        }
    }

    #[test]
    fn library_and_hidden_folders_are_left_out_of_the_stream() {
        let home = std::env::temp_dir().join(format!("raiden-left-out-{}", std::process::id()));
        for folder in ["Library", ".cache", ".npm", "Documents"] {
            std::fs::create_dir_all(home.join(folder)).unwrap();
        }
        let home_text = home.to_str().unwrap();
        let mut left = left_out(home_text);
        left.sort();
        let _ = std::fs::remove_dir_all(&home);
        assert_eq!(
            left,
            [
                format!("{home_text}/.cache"),
                format!("{home_text}/.npm"),
                format!("{home_text}/Library"),
            ]
        );
    }

    #[test]
    fn a_saved_index_is_used_only_for_the_same_home_and_event_history() {
        let home = std::env::temp_dir().join(format!("raiden-saved-{}", std::process::id()));
        std::fs::create_dir_all(home.join("Documents")).unwrap();
        std::fs::write(home.join("Documents/plan.txt"), "").unwrap();
        let home_text = home.to_str().unwrap().to_owned();
        let saved = home.join("names.idx");
        let stamp = Stamp {
            home: 42,
            drive: 40,
            at: 1_790_000_000,
            history: "A".to_owned(),
        };
        write(&Names::build(&home_text), &stamp, &saved).unwrap();
        assert!(read(&saved, &home_text, "A").is_some_and(|(_, read)| read == stamp));
        assert!(read(&saved, &home_text, "B").is_none());
        assert!(read(&saved, "/Users/someone-else", "A").is_none());
        std::fs::write(&saved, b"broken").unwrap();
        assert!(read(&saved, &home_text, "A").is_none());
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn the_boot_volume_has_an_event_history() {
        assert!(!history_of("/").is_empty());
        assert!(history_of("/no/such/folder").is_empty());
    }

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(label: &str) -> Self {
            let folder =
                std::env::temp_dir().join(format!("raiden-index-{label}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&folder);
            std::fs::create_dir_all(folder.join("home")).unwrap();
            Self(folder.canonicalize().unwrap())
        }

        fn home(&self) -> String {
            self.0.join("home").to_str().unwrap().to_owned()
        }

        fn state(&self) -> PathBuf {
            self.0.join("state")
        }

        fn file(&self, relative: &str) {
            let path = self.0.join("home").join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, relative).unwrap();
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn eventually(index: &Index, query: &str, wanted: bool) -> Duration {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(15) {
            let found = index
                .search(query, 10)
                .is_some_and(|found| !found.hits.is_empty());
            if found == wanted {
                return started.elapsed();
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!(
            "{query:?} was still {} after 15 s",
            if wanted { "missing" } else { "there" }
        );
    }

    #[test]
    fn a_new_file_is_found_moments_after_it_is_written() {
        let scratch = Scratch::new("live");
        scratch.file("Documents/existing.txt");
        let index = start(&scratch.home(), &scratch.state());
        eventually(&index, "existing", true);
        scratch.file("Documents/Projects/brand-new-raiden-file.md");
        let took = eventually(&index, "brand-new-raiden", true);
        eprintln!("a new file was found {took:?} after it was written");
        std::fs::remove_dir_all(scratch.0.join("home/Documents/Projects")).unwrap();
        eventually(&index, "brand-new-raiden", false);
    }

    #[test]
    fn changes_made_while_raiden_was_closed_are_caught_up_without_a_rescan() {
        let scratch = Scratch::new("replay");
        scratch.file("Documents/before.txt");
        scratch.file("Documents/doomed.txt");
        scratch.file("Elsewhere/Deep/only-in-the-saved-index.txt");
        let home = scratch.home();
        let names = Names::build(&home);
        std::fs::remove_file(
            scratch
                .0
                .join("home/Elsewhere/Deep/only-in-the-saved-index.txt"),
        )
        .unwrap();
        std::thread::sleep(Duration::from_secs(1));
        let now = unsafe { FSEventsGetCurrentEventId() };
        let stamp = Stamp {
            home: now,
            drive: now,
            at: seconds(SystemTime::now()),
            history: history_of(&home),
        };
        let saved = scratch.state().join("names.idx");
        write(&names, &stamp, &saved).unwrap();
        scratch.file("Documents/while-closed.txt");
        std::fs::remove_file(scratch.0.join("home/Documents/doomed.txt")).unwrap();
        scratch.file("Unrelated/after.txt");
        std::fs::remove_dir_all(scratch.0.join("home/Unrelated")).unwrap();
        std::thread::sleep(Duration::from_secs(2));
        let index = start(&home, &scratch.state());
        eventually(&index, "before", true);
        eventually(&index, "while-closed", true);
        eventually(&index, "doomed", false);
        assert!(
            index
                .search("only-in-the-saved", 10)
                .is_some_and(|found| found.hits.len() == 1),
            "the saved index was rebuilt instead of caught up"
        );
    }

    #[test]
    fn changes_deep_inside_accented_folders_are_seen() {
        let scratch = Scratch::new("accents");
        scratch.file("Música/Álbumes/Año 2026/viejo.txt");
        let index = start(&scratch.home(), &scratch.state());
        eventually(&index, "viejo", true);
        scratch.file("Música/Álbumes/Año 2026/canción-nueva.txt");
        eventually(&index, "cancion-nueva", true);
        std::fs::remove_file(scratch.0.join("home/Música/Álbumes/Año 2026/viejo.txt")).unwrap();
        eventually(&index, "viejo", false);
    }

    #[test]
    fn icloud_drive_is_watched_although_library_is_left_out() {
        let scratch = Scratch::new("icloud");
        let drive = "Library/Mobile Documents/com~apple~CloudDocs";
        scratch.file(&format!("{drive}/Work/old.key"));
        let index = start(&scratch.home(), &scratch.state());
        eventually(&index, "old.key", true);
        scratch.file(&format!("{drive}/Work/new-in-icloud.key"));
        eventually(&index, "new-in-icloud", true);
        scratch.file("Library/Caches/never-indexed.txt");
        std::thread::sleep(Duration::from_secs(4));
        assert!(
            index
                .search("never-indexed", 10)
                .is_some_and(|found| found.hits.is_empty())
        );
    }

    fn watch(home: &str) -> Watch {
        Watch {
            index: Arc::default(),
            home: home.to_owned(),
            drive_stream: std::ptr::null_mut(),
            saved_to: PathBuf::from("/nonexistent/names.idx"),
            stamp: Stamp {
                home: 100,
                drive: 100,
                at: 1,
                history: String::new(),
            },
            heard_at: [SystemTime::now(); 2],
            replaying: [false; 2],
            saved_at: Instant::now(),
            changed: false,
        }
    }

    #[test]
    fn each_stream_remembers_only_its_own_progress() {
        let mut watch = watch("/Users/me");
        watch.took_in(Root::Home, &[("/Users/me/Documents".to_owned(), 0, 500)]);
        assert_eq!((watch.stamp.home, watch.stamp.drive), (500, 100));
        let drive = files::icloud_drive("/Users/me");
        watch.took_in(Root::Drive, &[(drive, 0, 450)]);
        assert_eq!((watch.stamp.home, watch.stamp.drive), (500, 450));
    }

    #[test]
    fn progress_follows_the_stream_even_when_its_ids_wrap_or_the_change_is_ignored() {
        let mut watch = watch("/Users/me");
        watch.took_in(Root::Home, &[("/Users/me/.cache".to_owned(), 0, 900)]);
        assert_eq!(watch.stamp.home, 900);
        watch.took_in(Root::Home, &[("/Users/me".to_owned(), IDS_WRAPPED, 3)]);
        assert_eq!(watch.stamp.home, 3);
        watch.took_in(Root::Home, &[(String::new(), HISTORY_DONE, 0)]);
        assert_eq!(watch.stamp.home, 3);
    }

    #[test]
    fn a_batch_refreshes_each_folder_once_and_keeps_folders_reported_beside_a_deep_check() {
        let lost = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        let events = [
            ("/Users/me/Downloads".to_owned(), 0, 1),
            ("/Users/me/Downloads/".to_owned(), 0, 2),
            ("/Users/me/Code/app/src".to_owned(), 0, 3),
            ("/Users/me/Code".to_owned(), MUST_SCAN_SUBDIRS, 4),
            ("/Users/me/Code/app".to_owned(), 0, 5),
            ("/Users/me/.cache/x".to_owned(), MUST_SCAN_SUBDIRS, 6),
            (String::new(), HISTORY_DONE, 0),
        ];
        let mut wanted = merged(&events, "/Users/me", "/Users/me", lost);
        wanted.sort();
        assert_eq!(
            wanted,
            [
                (PathBuf::from("/Users/me/Code"), Some(lost)),
                (PathBuf::from("/Users/me/Code/app"), None),
                (PathBuf::from("/Users/me/Code/app/src"), None),
                (PathBuf::from("/Users/me/Downloads"), None),
            ]
        );
    }

    #[test]
    fn a_changed_icloud_root_is_read_again_whole() {
        let drive = files::icloud_drive("/Users/me");
        let wanted = merged(
            &[(drive.clone(), ROOT_CHANGED, 0)],
            "/Users/me",
            &drive,
            SystemTime::now(),
        );
        assert_eq!(
            wanted,
            [(PathBuf::from(&drive), Some(SystemTime::UNIX_EPOCH))]
        );
        let mut watch = watch("/Users/me");
        watch.took_in(Root::Drive, &[(drive, ROOT_CHANGED, 0)]);
        assert_eq!(watch.stamp.drive, 100);
    }

    #[test]
    fn a_saved_index_rechecks_protected_folders_before_it_is_used() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = Scratch::new("recheck");
        scratch.file("Documents/secret.txt");
        let home = scratch.home();
        let mut names = Names::build(&home);
        let documents = scratch.0.join("home/Documents");
        std::fs::set_permissions(&documents, std::fs::Permissions::from_mode(0o000)).unwrap();
        recheck_access(&mut names, &home);
        std::fs::set_permissions(&documents, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(names.blocked(), [documents.to_str().unwrap()]);
    }

    #[test]
    fn at_most_eight_folders_are_left_out_and_library_is_always_one() {
        let home = std::env::temp_dir().join(format!("raiden-many-hidden-{}", std::process::id()));
        std::fs::create_dir_all(home.join("Library")).unwrap();
        for at in 0..12 {
            std::fs::create_dir_all(home.join(format!(".hidden-{at}"))).unwrap();
        }
        let home_text = home.to_str().unwrap();
        let left = left_out(home_text);
        let _ = std::fs::remove_dir_all(&home);
        assert_eq!(left.len(), MOST_EXCLUDED);
        assert_eq!(left[0], format!("{home_text}/Library"));
    }

    #[test]
    fn saving_replaces_the_index_whole_and_leaves_nothing_half_written() {
        let scratch = Scratch::new("save");
        scratch.file("Documents/a.txt");
        let home = scratch.home();
        let saved = scratch.state().join("names.idx");
        let stamp = Stamp {
            home: 1,
            drive: 1,
            at: 1,
            history: "A".to_owned(),
        };
        write(&Names::build(&home), &stamp, &saved).unwrap();
        scratch.file("Documents/b.txt");
        write(&Names::build(&home), &stamp, &saved).unwrap();
        let (names, _) = read(&saved, &home, "A").unwrap();
        assert_eq!(names.search("b.txt", 10, &[]).len(), 1);
        let left: Vec<_> = std::fs::read_dir(scratch.state())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .collect();
        assert_eq!(left, ["names.idx"]);
    }

    #[test]
    fn a_lost_track_event_during_replay_reads_again_everything_since_the_saved_index() {
        let scratch = Scratch::new("replay-lost");
        scratch.file("Code/app/src/old.rs");
        let home = scratch.home();
        let mut watch = watch(&home);
        watch.index.set(Some(Names::build(&home)));
        scratch.file("Code/app/src/added-while-closed.rs");
        watch.stamp.at = seconds(SystemTime::now()) - 3600;
        watch.heard_at = [SystemTime::now() + Duration::from_secs(3600); 2];
        watch.replaying = [true; 2];
        let code = format!("{home}/Code");
        watch.took_in(Root::Home, &[(code, MUST_SCAN_SUBDIRS, 500)]);
        let found = watch.index.search("added-while-closed", 10).unwrap();
        assert_eq!(found.hits.len(), 1);
    }

    #[test]
    fn the_end_of_a_replay_is_remembered_per_stream() {
        let mut watch = watch("/Users/me");
        watch.replaying = [true; 2];
        watch.took_in(Root::Drive, &[(String::new(), HISTORY_DONE, 0)]);
        assert_eq!(watch.replaying, [true, false]);
    }

    #[test]
    fn a_saved_index_from_an_unknown_event_history_is_never_used() {
        let scratch = Scratch::new("no-history");
        let home = scratch.home();
        let saved = scratch.state().join("names.idx");
        let stamp = Stamp {
            home: 1,
            drive: 1,
            at: 1,
            history: String::new(),
        };
        write(&Names::build(&home), &stamp, &saved).unwrap();
        assert!(read(&saved, &home, "").is_none());
    }

    #[test]
    fn protected_folders_count_as_blocked_for_spotlight_but_others_do_not() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = Scratch::new("protected");
        scratch.file("Documents/Taxes/a.pdf");
        scratch.file("Code/pgdata/x");
        let home = scratch.home();
        let mut names = Names::build(&home);
        for folder in ["Documents/Taxes", "Code/pgdata"] {
            let path = scratch.0.join("home").join(folder);
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
            names.refresh(&path, None);
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(
            protected_among(names.home(), names.blocked()),
            [format!("{home}/Documents/Taxes")]
        );
    }

    #[test]
    fn dropped_events_reported_above_a_stream_read_its_whole_root_again() {
        let lost = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        let drive = files::icloud_drive("/Users/me");
        for above in ["/", "/Users", "/Users/me/Library"] {
            let wanted = merged(
                &[(above.to_owned(), KERNEL_DROPPED | MUST_SCAN_SUBDIRS, 9)],
                "/Users/me",
                &drive,
                lost,
            );
            assert_eq!(wanted, [(PathBuf::from(&drive), Some(lost))], "{above}");
        }
        let home = merged(
            &[("/".to_owned(), USER_DROPPED, 9)],
            "/Users/me",
            "/Users/me",
            lost,
        );
        assert_eq!(home, [(PathBuf::from("/Users/me"), Some(lost))]);
        assert!(merged(&[("/".to_owned(), 0, 9)], "/Users/me", "/Users/me", lost).is_empty());
        assert!(
            merged(
                &[("/Users/meg".to_owned(), MUST_SCAN_SUBDIRS, 9)],
                "/Users/me",
                "/Users/me",
                lost
            )
            .is_empty()
        );
    }

    #[test]
    fn twenty_thousand_events_in_one_batch_merge_quickly() {
        let events: Vec<(String, u32, u64)> = (0..20_000)
            .map(|at| (format!("/Users/me/Code/app/node/{}/{at}", at % 50), 0, at))
            .collect();
        let started = Instant::now();
        let wanted = merged(&events, "/Users/me", "/Users/me", SystemTime::now());
        assert_eq!(wanted.len(), 20_000);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn the_saved_index_is_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = Scratch::new("owner-only");
        let saved = scratch.state().join("names.idx");
        let stamp = Stamp {
            home: 1,
            drive: 1,
            at: 1,
            history: "A".to_owned(),
        };
        write(&Names::build(&scratch.home()), &stamp, &saved).unwrap();
        let mode = std::fs::metadata(&saved).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn stale_names_in_any_unreadable_folder_are_left_out_of_answers() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = Scratch::new("stale-unreadable");
        scratch.file("Code/private/old-secret.txt");
        let home = scratch.home();
        let index = Index::default();
        index.set(Some(Names::build(&home)));
        let private = scratch.0.join("home/Code/private");
        std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o000)).unwrap();
        index.refresh(&private, None);
        let found = index.search("old-secret", 10).unwrap();
        std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(found.hits.is_empty());
        assert!(found.blocked.is_empty());
    }

    #[test]
    fn a_folder_restored_with_an_old_date_is_read_when_reported_beside_a_deep_check() {
        let scratch = Scratch::new("restored");
        scratch.file("Code/app/src/old.rs");
        let home = scratch.home();
        let mut watch = watch(&home);
        watch.index.set(Some(Names::build(&home)));
        scratch.file("Code/app/src/restored.rs");
        let src = scratch.0.join("home/Code/app/src");
        let long_ago = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        File::open(&src).unwrap().set_modified(long_ago).unwrap();
        watch.took_in(
            Root::Home,
            &[
                (src.to_str().unwrap().to_owned(), 0, 10),
                (format!("{home}/Code"), MUST_SCAN_SUBDIRS, 11),
            ],
        );
        let found = watch.index.search("restored", 10).unwrap();
        assert_eq!(found.hits.len(), 1);
    }

    #[test]
    fn a_saved_index_dated_in_the_far_future_is_never_used() {
        let scratch = Scratch::new("future");
        let home = scratch.home();
        let saved = scratch.state().join("names.idx");
        let stamp = Stamp {
            home: 1,
            drive: 1,
            at: u64::MAX,
            history: "A".to_owned(),
        };
        write(&Names::build(&home), &stamp, &saved).unwrap();
        assert!(read(&saved, &home, "A").is_none());
    }

    #[test]
    fn listeners_hear_when_the_index_is_ready_and_which_folders_changed() {
        let scratch = Scratch::new("listen");
        scratch.file("Documents/a.txt");
        let home = scratch.home();
        let mut watch = watch(&home);
        let changes = watch.index.listen();
        watch.index.set(Some(Names::build(&home)));
        assert!(watch.index.ready());
        assert_eq!(watch.index.files().len(), 1);
        let documents = format!("{home}/Documents");
        watch.took_in(Root::Home, &[(documents.clone(), 0, 7)]);
        watch.took_in(Root::Home, &[(documents.clone(), MUST_SCAN_SUBDIRS, 8)]);
        let heard: Vec<Change> = changes.try_iter().collect();
        assert_eq!(
            heard,
            [
                Change::Ready,
                Change::Folder(PathBuf::from(&documents)),
                Change::Lost(PathBuf::from(&documents)),
            ]
        );
    }
}
