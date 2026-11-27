use std::collections::VecDeque;
use std::ffi::c_void;
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

type MDQueryRef = *const c_void;

const SYNCHRONOUS: usize = 1;
const LONGEST: Duration = Duration::from_secs(2);
const SLICE: f64 = 0.01;
const REMEMBERED: usize = 400;

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
    fn new(text: &str, folder: &str, most: usize, values: &[CFStringRef]) -> Option<Self> {
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
        let scope = CFArray::from_CFTypes(&[CFString::new(folder)]);
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
        &home(),
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

pub fn search<T>(text: &str, home: &str, mut newer: impl FnMut() -> Option<T>) -> Outcome<T> {
    let answer = |files: Vec<FileRow>, index_off: bool| Answer {
        query: text.to_owned(),
        files,
        index_off,
    };
    let Some(spotlight) = files::spotlight_query(text) else {
        return Outcome::Done(answer(Vec::new(), false));
    };
    let values = unsafe { [kMDItemPath, kMDItemLastUsedDate] };
    let Some(query) = Query::new(&spotlight, home, files::GATHERED, &values) else {
        return Outcome::Done(answer(Vec::new(), false));
    };
    if !query.start(0) {
        return Outcome::Done(answer(Vec::new(), !index_on()));
    }
    let mut replaced = None;
    let finished = wait_for(&query, || {
        replaced = newer();
        replaced.as_ref().map(|_| ())
    });
    if let (false, Some(next)) = (finished, replaced) {
        return Outcome::Replaced(next);
    }
    let found = query.found();
    drop(query);
    let empty = found.is_empty();
    let rows = files::ranked(found, text, home, |path| Path::new(path).is_file());
    Outcome::Done(answer(rows, empty && !index_on()))
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

fn serve(waiting: &Receiver<Request>, home: &str, shown: &Shown) {
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
            search(&request.query, home, || newer_than(waiting, request.asked))
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
    pub fn start() -> Self {
        let (sender, waiting) = channel();
        let home = home();
        let shown = Shown::default();
        let kept = Arc::clone(&shown);
        let started = std::thread::Builder::new()
            .name("file search".to_owned())
            .spawn(move || serve(&waiting, &home, &kept));
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

    impl Scratch {
        fn new(folder: std::path::PathBuf) -> Self {
            std::fs::create_dir_all(&folder).unwrap();
            Self(folder)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
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
        let answer = rows(search("ab", &home(), || None));
        assert!(answer.files.is_empty());
        assert!(!answer.index_off);
    }

    #[test]
    fn a_newer_query_cancels_the_running_one() {
        let outcome = search("raiden-never-named-like-this", &home(), || Some("newer"));
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
        serve(&waiting, &home(), &Shown::default());
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

    #[test]
    fn a_new_file_is_found_within_two_seconds() {
        if !index_on() || std::env::var_os("CI").is_some() {
            eprintln!("Spotlight's index is off or not timely here, so this check cannot run.");
            return;
        }
        let home = home();
        let unique = format!(
            "raiden-file-search-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_nanos())
        );
        let folder = Scratch::new(Path::new(&home).join(&unique));
        let file = folder.0.join(format!("{unique}.txt"));
        std::fs::write(&file, "found").unwrap();
        let started = Instant::now();
        let mut found = false;
        while !found && started.elapsed() < LONGEST {
            found = rows(search(&unique, &home, || None))
                .files
                .iter()
                .any(|row| Path::new(&row.path) == file);
            if !found {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        let took = started.elapsed();
        drop(folder);
        assert!(found, "{} was not found within 2 s", file.display());
        eprintln!("found {} in {took:?}", file.display());
    }
}
