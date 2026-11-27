use std::collections::{HashSet, VecDeque};
use std::ffi::{CStr, CString, c_void};
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use core_foundation::ConcreteCFType;
use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::date::CFDate;
use core_foundation::runloop::{CFRunLoopRunInMode, kCFRunLoopDefaultMode, kCFRunLoopRunFinished};
use core_foundation::string::{CFString, CFStringRef};
use raiden_core::files::{self, FileRow, Found};
use tauri::State;
use tokio::sync::oneshot;

use crate::names::Index;
use raiden_core::names::Hit;

type MDQueryRef = *const c_void;

const SYNCHRONOUS: usize = 1;
const LONGEST: Duration = Duration::from_secs(2);
const SLICE: f64 = 0.01;
const REMEMBERED: usize = 400;
const SECONDS_FROM_1970_TO_2001: f64 = 978_307_200.0;
const READS_BETWEEN_LOOKS_FOR_NEWER: usize = 128;
const LAST_USED: &CStr = c"com.apple.lastuseddate#PS";

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn MDQueryCreate(
        allocator: *const c_void,
        query: CFStringRef,
        values: CFArrayRef,
        sorting: CFArrayRef,
    ) -> MDQueryRef;
    fn MDQuerySetSearchScope(query: MDQueryRef, folders: CFArrayRef, options: u32);
    fn MDQuerySetMaxCount(query: MDQueryRef, most: isize);
    fn MDQueryExecute(query: MDQueryRef, options: usize) -> u8;
    fn MDQueryStop(query: MDQueryRef);
    fn MDQueryIsGatheringComplete(query: MDQueryRef) -> u8;
    fn MDQueryGetResultCount(query: MDQueryRef) -> isize;
    fn MDQueryGetAttributeValueOfResultAtIndex(
        query: MDQueryRef,
        name: CFStringRef,
        index: isize,
    ) -> CFTypeRef;
    static kMDItemPath: CFStringRef;
    static kMDItemLastUsedDate: CFStringRef;
}

struct Query(CFType);

impl Query {
    fn new(text: &str, folders: &[String], most: usize, values: &[CFStringRef]) -> Option<Self> {
        let text = CFString::new(text);
        let names: Vec<CFString> = values
            .iter()
            .map(|name| unsafe { CFString::wrap_under_get_rule(*name) })
            .collect();
        let names = CFArray::from_CFTypes(&names);
        let made = unsafe {
            MDQueryCreate(
                std::ptr::null(),
                text.as_concrete_TypeRef(),
                names.as_concrete_TypeRef(),
                std::ptr::null(),
            )
        };
        if made.is_null() {
            return None;
        }
        let query = Self(unsafe { CFType::wrap_under_create_rule(made) });
        let folders: Vec<CFString> = folders.iter().map(|folder| CFString::new(folder)).collect();
        let scope = CFArray::from_CFTypes(&folders);
        unsafe {
            MDQuerySetSearchScope(query.raw(), scope.as_concrete_TypeRef(), 0);
            MDQuerySetMaxCount(query.raw(), isize::try_from(most).unwrap_or(isize::MAX));
        }
        Some(query)
    }

    fn raw(&self) -> MDQueryRef {
        self.0.as_CFTypeRef()
    }

    fn start(&self, options: usize) -> bool {
        unsafe { MDQueryExecute(self.raw(), options) != 0 }
    }

    fn gathered(&self) -> bool {
        unsafe { MDQueryIsGatheringComplete(self.raw()) != 0 }
    }

    fn count(&self) -> isize {
        unsafe { MDQueryGetResultCount(self.raw()) }
    }

    fn value(&self, name: CFStringRef, index: isize) -> CFTypeRef {
        unsafe { MDQueryGetAttributeValueOfResultAtIndex(self.raw(), name, index) }
    }

    fn typed<T: ConcreteCFType>(&self, name: CFStringRef, index: isize) -> Option<T> {
        let value = self.value(name, index);
        if value.is_null() {
            return None;
        }
        unsafe { CFType::wrap_under_get_rule(value) }.downcast_into::<T>()
    }

    fn found(&self) -> Vec<Found> {
        (0..self.count())
            .filter_map(|index| {
                let path = self.typed::<CFString>(unsafe { kMDItemPath }, index)?;
                let used_at = self
                    .typed::<CFDate>(unsafe { kMDItemLastUsedDate }, index)
                    .map(|used| used.abs_time());
                Some(Found {
                    path: path.to_string(),
                    used_at,
                })
            })
            .collect()
    }
}

impl Drop for Query {
    fn drop(&mut self) {
        unsafe { MDQueryStop(self.raw()) };
    }
}

static INDEX_CHECKED: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
const INDEX_KEPT: Duration = Duration::from_secs(60);

fn index_on_now() -> bool {
    Query::new(
        "kMDItemContentTypeTree == \"public.folder\"",
        &[home()],
        1,
        &[],
    )
    .is_some_and(|query| query.start(SYNCHRONOUS) && query.count() > 0)
}

pub fn index_on() -> bool {
    let mut checked = INDEX_CHECKED.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((at, on)) = *checked
        && at.elapsed() < INDEX_KEPT
    {
        return on;
    }
    let on = index_on_now();
    *checked = Some((Instant::now(), on));
    on
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Answer {
    query: String,
    files: Vec<FileRow>,
    index_off: bool,
}

pub enum Outcome<T> {
    Done(Answer),
    Replaced(T),
}

fn wait_for(query: &Query, mut newer: impl FnMut() -> Option<()>) -> bool {
    let started = Instant::now();
    while !query.gathered() && started.elapsed() < LONGEST {
        if newer().is_some() {
            return false;
        }
        let ran = unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, SLICE, 1) };
        if ran == kCFRunLoopRunFinished {
            std::thread::sleep(Duration::from_secs_f64(SLICE));
        }
    }
    true
}

fn last_used(path: &str) -> Option<f64> {
    let path = CString::new(path).ok()?;
    let mut stamp = [0_u8; 16];
    let read = unsafe {
        libc::getxattr(
            path.as_ptr(),
            LAST_USED.as_ptr(),
            stamp.as_mut_ptr().cast(),
            stamp.len(),
            0,
            0,
        )
    };
    if read < 8 {
        return None;
    }
    let seconds = i64::from_le_bytes(stamp[..8].try_into().ok()?);
    let nanoseconds = i64::from_le_bytes(stamp[8..].try_into().ok()?);
    Some(seconds as f64 + nanoseconds as f64 / 1e9 - SECONDS_FROM_1970_TO_2001)
}

fn inside_any(path: &str, folders: &[String]) -> bool {
    folders.iter().any(|folder| {
        path.strip_prefix(folder.as_str())
            .is_some_and(|rest| rest.starts_with('/'))
    })
}

fn tier_that_can_show(hits: &[Hit]) -> u8 {
    hits.get(files::SHOWN - 1).map_or(0, |hit| hit.closeness)
}

struct Gathered {
    found: Vec<Found>,
    files: HashSet<String>,
    ask_spotlight: Vec<String>,
}

fn gather<T>(
    text: &str,
    home: &str,
    index: &Index,
    newer: &mut impl FnMut() -> Option<T>,
) -> Result<Gathered, T> {
    let Some(lookup) = index.search(text, files::GATHERED) else {
        return Ok(Gathered {
            found: Vec::new(),
            files: HashSet::new(),
            ask_spotlight: vec![home.to_owned()],
        });
    };
    let hits: Vec<Hit> = lookup
        .hits
        .into_iter()
        .filter(|hit| !inside_any(&hit.path, &lookup.blocked))
        .collect();
    let lowest = tier_that_can_show(&hits);
    let files = hits
        .iter()
        .filter(|hit| hit.is_file)
        .map(|hit| hit.path.clone())
        .collect();
    let mut found = Vec::with_capacity(hits.len());
    for (at, hit) in hits.into_iter().enumerate() {
        if at % READS_BETWEEN_LOOKS_FOR_NEWER == READS_BETWEEN_LOOKS_FOR_NEWER - 1
            && let Some(next) = newer()
        {
            return Err(next);
        }
        found.push(Found {
            used_at: (hit.closeness >= lowest)
                .then(|| last_used(&hit.path))
                .flatten(),
            path: hit.path,
        });
    }
    Ok(Gathered {
        found,
        files,
        ask_spotlight: lookup.blocked,
    })
}

pub fn search<T>(
    text: &str,
    home: &str,
    index: &Index,
    mut newer: impl FnMut() -> Option<T>,
) -> Outcome<T> {
    let answer = |files: Vec<FileRow>, index_off: bool| Answer {
        query: text.to_owned(),
        files,
        index_off,
    };
    let Some(spotlight) = files::spotlight_query(text) else {
        return Outcome::Done(answer(Vec::new(), false));
    };
    let Gathered {
        mut found,
        files,
        ask_spotlight,
    } = match gather(text, home, index, &mut newer) {
        Ok(gathered) => gathered,
        Err(next) => return Outcome::Replaced(next),
    };
    let from_index = ask_spotlight.first().is_none_or(|first| first != home);
    let is_file = |path: &str| files.contains(path) || Path::new(path).is_file();
    if ask_spotlight.is_empty() {
        return Outcome::Done(answer(files::ranked(found, text, home, is_file), false));
    }
    let values = unsafe { [kMDItemPath, kMDItemLastUsedDate] };
    let Some(query) = Query::new(&spotlight, &ask_spotlight, files::GATHERED, &values) else {
        return Outcome::Done(answer(files::ranked(found, text, home, is_file), false));
    };
    if !query.start(0) {
        let rows = files::ranked(found, text, home, is_file);
        let off = rows.is_empty() && !from_index && !index_on();
        return Outcome::Done(answer(rows, off));
    }
    let mut replaced = None;
    let finished = wait_for(&query, || {
        replaced = newer();
        replaced.as_ref().map(|_| ())
    });
    if let (false, Some(next)) = (finished, replaced) {
        return Outcome::Replaced(next);
    }
    found.extend(query.found());
    drop(query);
    let rows = files::ranked(found, text, home, is_file);
    let off = rows.is_empty() && !from_index && !index_on();
    Outcome::Done(answer(rows, off))
}

struct Request {
    query: String,
    asked: u64,
    reply: oneshot::Sender<Option<Answer>>,
}

fn later(current: Request, next: Request) -> Request {
    let (kept, dropped) = if next.asked > current.asked {
        (next, current)
    } else {
        (current, next)
    };
    let _ = dropped.reply.send(None);
    kept
}

fn newest(waiting: &Receiver<Request>, mut current: Request) -> Request {
    while let Ok(next) = waiting.try_recv() {
        current = later(current, next);
    }
    current
}

fn newer_than(waiting: &Receiver<Request>, asked: u64) -> Option<Request> {
    while let Ok(next) = waiting.try_recv() {
        if next.asked > asked {
            return Some(next);
        }
        let _ = next.reply.send(None);
    }
    None
}

type Shown = Arc<Mutex<VecDeque<String>>>;

fn remember(shown: &Shown, answer: &Answer) {
    let mut paths = shown.lock().unwrap_or_else(PoisonError::into_inner);
    for row in &answer.files {
        paths.retain(|path| *path != row.path);
        paths.push_back(row.path.clone());
    }
    while paths.len() > REMEMBERED {
        paths.pop_front();
    }
}

fn serve(waiting: &Receiver<Request>, home: &str, index: &Index, shown: &Shown) {
    let mut next = None;
    let mut served = 0;
    loop {
        let request = match next.take() {
            Some(request) => request,
            None => match waiting.recv() {
                Ok(request) => request,
                Err(_) => return,
            },
        };
        let request = newest(waiting, request);
        if request.asked <= served {
            let _ = request.reply.send(None);
            continue;
        }
        let outcome = objc2::rc::autoreleasepool(|_| {
            search(&request.query, home, index, || {
                newer_than(waiting, request.asked)
            })
        });
        match outcome {
            Outcome::Done(answer) => {
                served = request.asked;
                remember(shown, &answer);
                let _ = request.reply.send(Some(answer));
            }
            Outcome::Replaced(newer) => {
                let _ = request.reply.send(None);
                next = Some(newer);
            }
        }
    }
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

pub struct Files {
    asking: Mutex<Sender<Request>>,
    shown: Shown,
}

impl Files {
    pub fn start(index: Arc<Index>) -> Self {
        let (sender, waiting) = channel();
        let home = crate::names::real_home(&home());
        let shown = Shown::default();
        let kept = Arc::clone(&shown);
        let started = std::thread::Builder::new()
            .name("file search".to_owned())
            .spawn(move || serve(&waiting, &home, &index, &kept));
        if let Err(error) = started {
            eprintln!("Raiden could not start file search: {error}");
        }
        Self {
            asking: Mutex::new(sender),
            shown,
        }
    }

    pub fn showed(&self, path: &str) -> bool {
        self.shown
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|shown| shown == path)
    }
}

#[tauri::command]
pub async fn files(
    files: State<'_, Files>,
    query: String,
    asked: u64,
) -> Result<Option<Answer>, ()> {
    let (reply, answer) = oneshot::channel();
    files
        .asking
        .lock()
        .map_err(|_| ())?
        .send(Request {
            query,
            asked,
            reply,
        })
        .map_err(|_| ())?;
    Ok(answer.await.ok().flatten())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(std::path::PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            if let Some(folder) = self.0.parent() {
                let _ = std::fs::remove_dir_all(folder);
            }
        }
    }

    fn rows(outcome: Outcome<()>) -> Answer {
        match outcome {
            Outcome::Done(answer) => answer,
            Outcome::Replaced(()) => panic!("nothing newer was asked"),
        }
    }

    #[test]
    fn short_queries_search_nothing() {
        let answer = rows(search("ab", &home(), &Index::default(), || None));
        assert!(answer.files.is_empty());
        assert!(!answer.index_off);
    }

    #[test]
    fn a_newer_query_cancels_the_running_one() {
        let outcome = search(
            "raiden-never-named-like-this",
            &home(),
            &Index::default(),
            || Some("newer"),
        );
        assert!(matches!(outcome, Outcome::Replaced("newer")) || !index_on());
    }

    fn request(query: &str, asked: u64) -> (Request, oneshot::Receiver<Option<Answer>>) {
        let (reply, answer) = oneshot::channel();
        (
            Request {
                query: query.to_owned(),
                asked,
                reply,
            },
            answer,
        )
    }

    #[test]
    fn stale_requests_are_answered_with_nothing() {
        let (sender, waiting) = channel();
        let (one, one_answer) = request("one", 1);
        let (three, three_answer) = request("three", 3);
        let (two, two_answer) = request("two", 2);
        for asked in [one, three, two] {
            sender.send(asked).unwrap();
        }
        let start = waiting.recv().unwrap();
        let kept = newest(&waiting, start);
        assert_eq!(kept.query, "three");
        drop(kept);
        assert_eq!(one_answer.blocking_recv(), Ok(None));
        assert_eq!(two_answer.blocking_recv(), Ok(None));
        assert!(three_answer.blocking_recv().is_err());
    }

    #[test]
    fn only_a_later_request_replaces_the_running_one() {
        let (sender, waiting) = channel();
        let (older, older_answer) = request("older", 4);
        let (later, _) = request("later", 6);
        sender.send(older).unwrap();
        sender.send(later).unwrap();
        assert_eq!(newer_than(&waiting, 5).map(|next| next.asked), Some(6));
        assert_eq!(older_answer.blocking_recv(), Ok(None));
        assert!(newer_than(&waiting, 5).is_none());
    }

    #[test]
    fn the_worker_answers_the_latest_of_what_waits() {
        let (sender, waiting) = channel();
        let (latest, latest_answer) = request("ab", 2);
        let (late, late_answer) = request("a", 1);
        sender.send(latest).unwrap();
        sender.send(late).unwrap();
        drop(sender);
        serve(&waiting, &home(), &Index::default(), &Shown::default());
        assert_eq!(late_answer.blocking_recv(), Ok(None));
        assert_eq!(
            latest_answer
                .blocking_recv()
                .unwrap()
                .map(|answer| answer.query),
            Some("ab".to_owned())
        );
    }

    #[test]
    fn found_paths_are_remembered_for_opening() {
        let shown = Shown::default();
        let row = |path: &str| FileRow {
            name: String::new(),
            path: path.to_owned(),
            folder: String::new(),
            icon: String::new(),
        };
        remember(
            &shown,
            &Answer {
                query: String::new(),
                files: (0..=REMEMBERED)
                    .map(|at| row(&format!("/f/{at}")))
                    .collect(),
                index_off: false,
            },
        );
        let paths = shown.lock().unwrap();
        assert_eq!(paths.len(), REMEMBERED);
        assert!(!paths.contains(&"/f/0".to_owned()));
        assert!(paths.contains(&format!("/f/{REMEMBERED}")));
    }

    struct Closed(std::path::PathBuf);

    impl Closed {
        fn new(folder: &Path) -> Self {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o000)).unwrap();
            Self(folder.to_owned())
        }
    }

    impl Drop for Closed {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
    }

    fn gathered(text: &str, home: &str, index: &Index) -> Gathered {
        match gather(text, home, index, &mut || None::<()>) {
            Ok(gathered) => gathered,
            Err(()) => panic!("nothing newer was asked"),
        }
    }

    fn scratch_home(label: &str) -> (Scratch, String) {
        let folder =
            std::env::temp_dir().join(format!("raiden-file-search-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("home")).unwrap();
        let scratch = Scratch(folder.join("home").canonicalize().unwrap());
        let home = scratch.0.to_str().unwrap().to_owned();
        (scratch, home)
    }

    fn ready_index(home: &str, state: &Path) -> Arc<Index> {
        let index = crate::names::start(home, state);
        let started = Instant::now();
        while index.search("anything", 1).is_none() {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "the index never got ready"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        index
    }

    fn mark_used(path: &Path, seconds: i64) {
        let path = CString::new(path.to_str().unwrap()).unwrap();
        let mut stamp = [0_u8; 16];
        stamp[..8].copy_from_slice(&seconds.to_le_bytes());
        let set = unsafe {
            libc::setxattr(
                path.as_ptr(),
                LAST_USED.as_ptr(),
                stamp.as_ptr().cast(),
                stamp.len(),
                0,
                0,
            )
        };
        assert_eq!(set, 0);
    }

    #[test]
    fn a_new_file_is_found_within_two_seconds() {
        let (scratch, home) = scratch_home("new");
        let index = ready_index(&home, &scratch.0.with_file_name("state"));
        let file = scratch.0.join("Documents/raiden-brand-new.txt");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "found").unwrap();
        let started = Instant::now();
        let mut found = false;
        let allowed = if std::env::var_os("CI").is_some() {
            Duration::from_secs(10)
        } else {
            LONGEST
        };
        while !found && started.elapsed() < allowed {
            found = rows(search("raiden-brand-new", &home, &index, || None))
                .files
                .iter()
                .any(|row| Path::new(&row.path) == file);
            if !found {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        let took = started.elapsed();
        assert!(found, "{} was not found within {allowed:?}", file.display());
        eprintln!("found {} in {took:?}", file.display());
    }

    #[test]
    fn the_index_answers_with_recently_opened_files_first() {
        let (scratch, home) = scratch_home("used");
        for name in ["a/report.txt", "b/report.txt", "c/report.txt"] {
            let path = scratch.0.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "").unwrap();
        }
        mark_used(&scratch.0.join("b/report.txt"), 1_800_000_000);
        mark_used(&scratch.0.join("c/report.txt"), 1_700_000_000);
        let index = ready_index(&home, &scratch.0.with_file_name("state"));
        let answer = rows(search("report", &home, &index, || None));
        let folders: Vec<&str> = answer.files.iter().map(|row| row.folder.as_str()).collect();
        assert!(!answer.index_off);
        assert_eq!(answer.files[0].icon, ".txt");
        assert_eq!(folders.len(), 3);
        assert!(
            folders[0].ends_with("/b") && folders[1].ends_with("/c"),
            "{folders:?}"
        );
    }

    #[test]
    fn a_recently_opened_deep_file_beats_hundreds_of_shallow_namesakes() {
        let (scratch, home) = scratch_home("namesakes");
        for at in 0..700 {
            let path = scratch.0.join(format!("repo-{at}/README.md"));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, "").unwrap();
        }
        let deep = scratch.0.join("work/clients/acme/2026/q4/notes/README.md");
        std::fs::create_dir_all(deep.parent().unwrap()).unwrap();
        std::fs::write(&deep, "").unwrap();
        mark_used(&deep, 1_800_000_000);
        let index = ready_index(&home, &scratch.0.with_file_name("state"));
        let answer = rows(search("readme", &home, &index, || None));
        assert_eq!(Path::new(&answer.files[0].path), deep);
    }

    #[test]
    fn folders_macos_keeps_closed_are_asked_of_spotlight_instead_of_the_index() {
        let (scratch, home) = scratch_home("closed");
        let documents = scratch.0.join("Documents");
        std::fs::create_dir_all(&documents).unwrap();
        std::fs::write(documents.join("contract.pdf"), "").unwrap();
        std::fs::write(scratch.0.join("contract notes.txt"), "").unwrap();
        let closed = Closed::new(&documents);
        let index = ready_index(&home, &scratch.0.with_file_name("state"));
        let gathered = gathered("contract", &home, &index);
        drop(closed);
        assert_eq!(gathered.ask_spotlight, [documents.to_str().unwrap()]);
        let paths: Vec<&str> = gathered
            .found
            .iter()
            .map(|item| item.path.as_str())
            .collect();
        assert_eq!(
            paths,
            [scratch.0.join("contract notes.txt").to_str().unwrap()]
        );
    }

    #[test]
    fn last_used_dates_are_on_spotlights_clock() {
        let (scratch, _) = scratch_home("clock");
        let file = scratch.0.join("x.txt");
        std::fs::write(&file, "").unwrap();
        mark_used(&file, 978_307_200 + 1000);
        assert_eq!(last_used(file.to_str().unwrap()), Some(1000.0));
        assert_eq!(last_used(scratch.0.join("missing").to_str().unwrap()), None);
    }

    #[test]
    fn only_folders_macos_protects_are_handed_to_spotlight() {
        let (scratch, home) = scratch_home("unprotected");
        let private = scratch.0.join("Code/db/pgdata");
        std::fs::create_dir_all(&private).unwrap();
        std::fs::write(scratch.0.join("Code/db/schema.sql"), "").unwrap();
        let closed = Closed::new(&private);
        let index = ready_index(&home, &scratch.0.with_file_name("state"));
        let gathered = gathered("schema", &home, &index);
        drop(closed);
        assert!(gathered.ask_spotlight.is_empty());
        assert_eq!(gathered.found.len(), 1);
    }

    #[test]
    fn typing_on_stops_reading_last_used_dates_for_the_old_query() {
        let (scratch, home) = scratch_home("superseded");
        for at in 0..400 {
            std::fs::write(scratch.0.join(format!("draft {at}.txt")), "").unwrap();
        }
        let index = ready_index(&home, &scratch.0.with_file_name("state"));
        let mut asked = 0;
        let outcome = gather("draft", &home, &index, &mut || {
            asked += 1;
            Some("draft 3")
        });
        assert!(matches!(outcome, Err("draft 3")));
        assert_eq!(asked, 1);
    }

    #[test]
    fn a_home_reached_through_a_link_still_finds_its_files() {
        let (scratch, _) = scratch_home("linked");
        std::fs::write(scratch.0.join("through-link.txt"), "").unwrap();
        let link = scratch.0.with_file_name("home-link");
        std::os::unix::fs::symlink(&scratch.0, &link).unwrap();
        let link = link.to_str().unwrap().to_owned();
        let index = ready_index(&link, &scratch.0.with_file_name("state"));
        let home = crate::names::real_home(&link);
        let answer = rows(search("through-link", &home, &index, || None));
        assert_eq!(answer.files.len(), 1);
        assert_eq!(answer.files[0].folder, "~");
    }
}
