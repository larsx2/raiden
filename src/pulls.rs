use std::io::Read as _;
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::process::CommandExt as _;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError, TryLockError};
use std::thread::{JoinHandle, sleep};
use std::time::{Duration, Instant};

use raiden_core::pulls::avatar::{self, Avatar};
use raiden_core::pulls::{
    self, AUTH_STATUS, Cache, Outcome, Problem, ProblemKind, Pull, PullsView, Snapshot, Tab, Viewer,
};
use serde_json::Value;
use tauri::{AppHandle, Manager, State};

const CALL_LIMIT: Duration = Duration::from_secs(30);
const REFRESH_LIMIT: Duration = Duration::from_secs(90);
const AUTH_LIMIT: Duration = Duration::from_secs(5);
const RETRIES: usize = 2;
const RETRY_WAIT: Duration = Duration::from_secs(2);
const POLL: Duration = Duration::from_millis(25);
const DRAIN_LIMIT: Duration = Duration::from_secs(1);
const SEARCHES_AT_ONCE: usize = 4;
const PLACES: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];
const CURL: &str = "/usr/bin/curl";
const USER_LIMIT: Duration = Duration::from_secs(10);
const DOWNLOAD_LIMIT: Duration = Duration::from_secs(avatar::DOWNLOAD_SECONDS + 5);
const AVATAR_RETRY: u64 = 60 * 60;

pub struct Pulls {
    state: PathBuf,
    cache: Mutex<Option<Cache>>,
    login: Mutex<Option<String>>,
    refreshing: AtomicBool,
    running: Mutex<()>,
    tried_at: Mutex<Option<u64>>,
    avatar_tried: Mutex<Option<(String, u64)>>,
}

impl Pulls {
    pub fn new(state: &Path) -> Self {
        Self {
            state: state.to_owned(),
            cache: Mutex::new(None),
            login: Mutex::new(None),
            refreshing: AtomicBool::new(false),
            running: Mutex::new(()),
            tried_at: Mutex::new(None),
            avatar_tried: Mutex::new(None),
        }
    }

    fn cached(&self) -> Cache {
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache
            .get_or_insert_with(|| pulls::read_cache(&self.state).unwrap_or_default())
            .clone()
    }

    fn keep(&self, cache: Cache) {
        if let Err(error) = pulls::write_cache(&self.state, &cache) {
            eprintln!("Raiden could not save the pull requests it read: {error}");
        }
        *self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(cache);
    }

    fn held_back(&self, now: u64) -> bool {
        self.cached().problem.as_ref().is_some_and(|problem| {
            problem.kind == ProblemKind::Limited && problem.retry_at.is_some_and(|at| at > now)
        })
    }

    fn rests(&self, now: u64) -> bool {
        self.cached().fresh(now) || self.tried_lately(now)
    }

    fn skips(&self, force: bool, now: u64) -> bool {
        self.held_back(now) || (!force && self.rests(now))
    }

    fn tried_lately(&self, now: u64) -> bool {
        self.tried_at
            .lock()
            .ok()
            .and_then(|at| *at)
            .is_some_and(|at| now.saturating_sub(at) < pulls::FRESH_FOR)
    }

    fn known_login(&self) -> Option<String> {
        self.cached()
            .login
            .or_else(|| self.login.lock().ok().and_then(|login| login.clone()))
    }

    fn may_fetch_avatar(&self, login: &str, now: u64) -> bool {
        let mut tried = self
            .avatar_tried
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let again = tried.as_ref().is_none_or(|(before, at)| {
            !before.eq_ignore_ascii_case(login) || now.saturating_sub(*at) >= AVATAR_RETRY
        });
        if again {
            *tried = Some((login.to_owned(), now));
        }
        again
    }

    pub fn accessory(&self) -> String {
        pulls::accessory(self.cached().review_requested)
    }

    fn view(&self, allowed: bool, pinned: &[String]) -> PullsView {
        let mut view = self
            .cached()
            .view(allowed, pinned, self.refreshing.load(Ordering::SeqCst));
        if view.login.is_none() {
            view.login = self.login.lock().ok().and_then(|login| login.clone());
        }
        view
    }
}

async fn background<T: Send + 'static>(
    app: AppHandle,
    work: impl FnOnce(&AppHandle) -> T + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || work(&app))
        .await
        .map_err(|error| format!("Raiden could not read your pull requests: {error}"))
}

#[tauri::command]
pub async fn pull_requests(
    app: AppHandle,
    usage: State<'_, crate::picks::Usage>,
    query: Option<String>,
    row: Option<usize>,
) -> Result<PullsView, String> {
    if let (Some(query), Some(row)) = (query, row) {
        crate::picks::note(&usage.0, pulls::VIEW_ID, query, row);
    }
    background(app, opened).await
}

#[tauri::command]
pub async fn pull_requests_refresh(app: AppHandle, force: bool) -> Result<PullsView, String> {
    background(app, move |app| refresh(app, force)).await
}

#[tauri::command]
pub async fn pull_requests_allow(app: AppHandle) -> Result<PullsView, String> {
    crate::settings::allow_pull_requests(&app)?;
    background(app, opened).await
}

#[tauri::command]
pub async fn pull_requests_avatar(app: AppHandle) -> Result<Option<String>, String> {
    background(app, |app| {
        avatar_for(&app.state::<Pulls>(), crate::picks::now(), fetch_avatar)
    })
    .await
}

#[tauri::command]
pub async fn pull_requests_pin(
    app: AppHandle,
    repo: String,
    pin: bool,
) -> Result<Vec<String>, String> {
    crate::settings::pin_repo(&app, &repo, pin)
}

#[tauri::command]
pub async fn pull_requests_context(
    app: AppHandle,
    tab: Tab,
    only: Option<String>,
    query: Option<String>,
    urls: Option<Vec<String>>,
) -> Result<Snapshot, String> {
    background(app, move |app| {
        let cache = app.state::<Pulls>().cached();
        pulls::snapshot(
            &cache.pulls,
            tab,
            only.as_deref(),
            query.as_deref(),
            urls.as_deref(),
            crate::picks::now(),
        )
    })
    .await
}

fn opened(app: &AppHandle) -> PullsView {
    let state = app.state::<Pulls>();
    let (allowed, pinned) = crate::settings::pull_requests(app);
    let mut view = state.view(allowed, &pinned);
    if !allowed && view.login.is_none() {
        match signed_in() {
            Ok(login) => {
                if let Ok(mut known) = state.login.lock() {
                    *known = Some(login.clone());
                }
                view.login = Some(login);
            }
            Err(problem) => view.problem = Some(problem),
        }
    }
    view
}

type Fetched = Result<(Viewer, Vec<Pull>), Problem>;

fn refresh(app: &AppHandle, force: bool) -> PullsView {
    refreshed(
        &app.state::<Pulls>(),
        force,
        || crate::settings::pull_requests(app),
        fetch,
    )
}

fn refreshed(
    state: &Pulls,
    force: bool,
    settings: impl Fn() -> (bool, Vec<String>),
    fetch: impl FnOnce(&[String]) -> Fetched,
) -> PullsView {
    let (allowed, pinned) = settings();
    if !allowed {
        let mut view = state.view(false, &pinned);
        view.problem = Some(Problem::not_allowed());
        return view;
    }
    let now = crate::picks::now();
    let busy = state.refreshing.load(Ordering::SeqCst);
    if state.held_back(now) || (!force && !busy && state.rests(now)) {
        return state.view(true, &pinned);
    }
    let running = match state.running.try_lock() {
        Ok(running) => Some(running),
        Err(TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    };
    match running {
        Some(_running) if state.skips(force, crate::picks::now()) => {}
        Some(_running) => {
            let _busy = Busy::start(&state.refreshing);
            if let Ok(mut tried_at) = state.tried_at.lock() {
                *tried_at = Some(now);
            }
            let fetched = fetch(&pinned);
            state.keep(settled(state.cached(), fetched, crate::picks::now()));
        }
        None => drop(state.running.lock()),
    }
    let (_, pinned) = settings();
    state.view(true, &pinned)
}

struct Busy<'a>(&'a AtomicBool);

impl<'a> Busy<'a> {
    fn start(flag: &'a AtomicBool) -> Self {
        flag.store(true, Ordering::SeqCst);
        Self(flag)
    }
}

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

fn settled(mut cache: Cache, fetched: Fetched, now: u64) -> Cache {
    match fetched {
        Ok((viewer, pulls)) => Cache {
            login: Some(viewer.login),
            fetched_at: Some(now),
            review_requested: viewer.review_requested,
            pulls,
            lost: viewer.lost,
            problem: None,
        },
        Err(problem) => {
            cache.problem = Some(problem);
            cache
        }
    }
}

fn fetch(pinned: &[String]) -> Fetched {
    let gh = gh().ok_or_else(Problem::missing)?;
    let deadline = Instant::now() + REFRESH_LIMIT;
    let data = ask(
        &gh,
        &pulls::viewer_query(pinned.len()),
        &pulls::viewer_fields(pinned),
        deadline,
    )?;
    let viewer = pulls::viewer(&data, pinned)?;
    let mut owners = vec![viewer.login.clone()];
    for organization in &viewer.organizations {
        if !owners
            .iter()
            .any(|owner| owner.eq_ignore_ascii_case(organization))
        {
            owners.push(organization.clone());
        }
    }
    let searches: Vec<String> = owners
        .iter()
        .map(|owner| pulls::search_terms(owner, &viewer.login))
        .chain([pulls::AUTHORED.to_owned()])
        .collect();
    let found = at_most(&searches, SEARCHES_AT_ONCE, |terms| {
        searched(&gh, terms, deadline)
    });
    let mut all = Vec::new();
    for pulls in found {
        all.extend(
            pulls.unwrap_or_else(|_| Err(Problem::failed("Raiden stopped reading a page")))?,
        );
    }
    let pulls = pulls::finish(all, &viewer.login, &viewer.organizations);
    Ok((viewer, pulls))
}

fn at_most<T: Sync, R: Send>(
    items: &[T],
    limit: usize,
    work: impl Fn(&T) -> R + Sync,
) -> Vec<std::thread::Result<R>> {
    let next = AtomicUsize::new(0);
    let done = Mutex::new(Vec::with_capacity(items.len()));
    std::thread::scope(|scope| {
        for _ in 0..limit.min(items.len()) {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::SeqCst);
                    let Some(item) = items.get(index) else {
                        break;
                    };
                    let result = std::panic::catch_unwind(AssertUnwindSafe(|| work(item)));
                    done.lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push((index, result));
                }
            });
        }
    });
    let mut done = done.into_inner().unwrap_or_else(PoisonError::into_inner);
    done.sort_by_key(|(index, _)| *index);
    done.into_iter().map(|(_, result)| result).collect()
}

fn searched(gh: &Path, terms: &str, deadline: Instant) -> Result<Vec<Pull>, Problem> {
    let mut found = Vec::new();
    let mut after: Option<String> = None;
    for _ in 0..pulls::PAGES {
        if Instant::now() >= deadline {
            return Err(Problem::too_long());
        }
        let mut fields = vec![("q".to_owned(), terms.to_owned())];
        if let Some(cursor) = &after {
            fields.push(("after".to_owned(), cursor.clone()));
        }
        let data = ask(gh, pulls::SEARCH_QUERY, &fields, deadline)?;
        let page = pulls::page(&data)?;
        found.extend(page.pulls);
        match page.next {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    Ok(found)
}

fn ask(
    gh: &Path,
    query: &str,
    fields: &[(String, String)],
    deadline: Instant,
) -> Result<Value, Problem> {
    let arguments = pulls::arguments(query, fields).map_err(|why| Problem::failed(&why))?;
    let mut tries = 0;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(Problem::too_long());
        }
        let outcome = run(gh, &arguments, CALL_LIMIT.min(left))?;
        if outcome.timed_out && Instant::now() >= deadline {
            return Err(Problem::too_long());
        }
        if tries < RETRIES && pulls::retryable(&outcome) {
            tries += 1;
            sleep(RETRY_WAIT.min(deadline.saturating_duration_since(Instant::now())));
            continue;
        }
        return pulls::answer(&outcome, crate::picks::now());
    }
}

fn signed_in() -> Result<String, Problem> {
    let gh = gh().ok_or_else(Problem::missing)?;
    pulls::auth(&run(&gh, &AUTH_STATUS.map(str::to_owned), AUTH_LIMIT)?)
}

fn avatar_for(
    state: &Pulls,
    now: u64,
    fetch: impl FnOnce(u64) -> Option<Avatar>,
) -> Option<String> {
    let login = state.known_login()?;
    let saved = avatar::read(&state.state);
    if !avatar::due(saved.as_ref(), &login, now) || !state.may_fetch_avatar(&login, now) {
        return avatar::shown(saved.as_ref(), &login);
    }
    let Some(fresh) = fetch(now).filter(|fresh| fresh.login.eq_ignore_ascii_case(&login)) else {
        return avatar::shown(saved.as_ref(), &login);
    };
    if let Err(error) = avatar::write(&state.state, &fresh) {
        eprintln!("Raiden could not save your GitHub avatar: {error}");
    }
    Some(fresh.image)
}

fn fetch_avatar(now: u64) -> Option<Avatar> {
    let gh = gh()?;
    let outcome = run(&gh, &avatar::USER.map(str::to_owned), USER_LIMIT).ok()?;
    if outcome.code != Some(0) || outcome.timed_out {
        return None;
    }
    let account = avatar::account(&outcome.stdout)?;
    let image = download(&account.link)?;
    Some(Avatar {
        login: account.login,
        fetched_at: now,
        image,
    })
}

fn download(link: &str) -> Option<String> {
    download_with(Path::new(CURL), link)
}

fn download_with(curl: &Path, link: &str) -> Option<String> {
    if !avatar::allowed(link) {
        return None;
    }
    let mut child = Command::new(curl)
        .args(avatar::download_arguments(link))
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .ok()?;
    let body = capped(child.stdout.take());
    let status = reader(child.stderr.take());
    let (code, timed_out) = finish(&mut child, Instant::now() + DOWNLOAD_LIMIT);
    let until = Instant::now() + DRAIN_LIMIT;
    if !drained(&[&body], until) || !drained(&[&status], until) {
        return None;
    }
    let body = body.join().ok()?;
    let status = status.join().ok()?;
    if code != Some(0) || timed_out {
        return None;
    }
    avatar::image(&status, &body)
}

fn capped(stream: Option<ChildStdout>) -> JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(stream) = stream {
            let _ = stream
                .take(avatar::MAX_BYTES as u64 + 1)
                .read_to_end(&mut bytes);
        }
        bytes
    })
}

fn search_path() -> Vec<PathBuf> {
    folders_in(std::env::var_os("PATH").as_deref())
}

fn folders_in(path: Option<&std::ffi::OsStr>) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = path
        .map(|path| {
            std::env::split_paths(path)
                .filter(|folder| folder.is_absolute())
                .collect()
        })
        .unwrap_or_default();
    for place in PLACES {
        if !folders.iter().any(|folder| folder == Path::new(place)) {
            folders.push(PathBuf::from(place));
        }
    }
    folders
}

fn gh() -> Option<PathBuf> {
    search_path()
        .into_iter()
        .map(|folder| folder.join("gh"))
        .find(|path| {
            std::fs::metadata(path)
                .is_ok_and(|found| found.is_file() && found.permissions().mode() & 0o111 != 0)
        })
}

fn reader(stream: Option<impl std::io::Read + Send + 'static>) -> JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut stream) = stream {
            let _ = stream.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn run(gh: &Path, arguments: &[String], limit: Duration) -> Result<Outcome, Problem> {
    let deadline = Instant::now() + limit;
    let path = std::env::join_paths(search_path()).unwrap_or_default();
    let mut child = Command::new(gh)
        .args(arguments)
        .env("PATH", path)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => Problem::missing(),
            _ => Problem::failed(&format!("Raiden could not start gh ({error})")),
        })?;
    let stdout = reader(child.stdout.take());
    let stderr = reader(child.stderr.take());
    let (code, mut timed_out) = finish(&mut child, deadline);
    let readers = [&stdout, &stderr];
    if !drained(&readers, deadline.max(Instant::now() + DRAIN_LIMIT)) {
        stop_group(&child);
        timed_out = true;
        drained(&readers, Instant::now() + DRAIN_LIMIT);
    }
    Ok(Outcome {
        code,
        stdout: collected(stdout),
        stderr: collected(stderr),
        timed_out,
    })
}

fn finish(child: &mut Child, deadline: Instant) -> (Option<i32>, bool) {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return (status.code(), false),
            Ok(None) if Instant::now() < deadline => sleep(POLL),
            outcome => {
                stop_group(child);
                let _ = child.kill();
                let _ = child.wait();
                return (None, outcome.is_ok());
            }
        }
    }
}

fn stop_group(child: &Child) {
    if let Ok(group) = libc::pid_t::try_from(child.id()) {
        unsafe {
            libc::killpg(group, libc::SIGKILL);
        }
    }
}

fn drained<T>(readers: &[&JoinHandle<T>], until: Instant) -> bool {
    loop {
        if readers.iter().all(|reader| reader.is_finished()) {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        sleep(POLL);
    }
}

fn collected(reader: JoinHandle<String>) -> String {
    if reader.is_finished() {
        reader.join().unwrap_or_default()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_that_runs_too_long_is_stopped() {
        let started = Instant::now();
        let outcome = run(
            Path::new("/bin/sleep"),
            &["5".to_owned()],
            Duration::from_millis(200),
        )
        .expect("started");
        assert!(outcome.timed_out);
        assert_eq!(outcome.code, None);
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(pulls::answer(&outcome, 0), Err(Problem::too_long()));
    }

    #[test]
    fn a_child_left_holding_the_output_is_stopped_with_gh() {
        let started = Instant::now();
        let outcome = run(
            Path::new("/bin/sh"),
            &["-c".to_owned(), "sleep 30 & sleep 30".to_owned()],
            Duration::from_millis(200),
        )
        .expect("started");
        assert!(outcome.timed_out);
        assert!(started.elapsed() < Duration::from_secs(3));
        let started = Instant::now();
        let outcome = run(
            Path::new("/bin/sh"),
            &["-c".to_owned(), "echo early; sleep 30 &".to_owned()],
            Duration::from_millis(200),
        )
        .expect("started");
        assert!(outcome.timed_out);
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn a_call_hands_back_what_it_printed() {
        let outcome = run(
            Path::new("/bin/sh"),
            &[
                "-c".to_owned(),
                "printf 'HTTP/2.0 200 OK\\n\\n{\"data\":{}}'; echo oops >&2; exit 3".to_owned(),
            ],
            CALL_LIMIT,
        )
        .expect("started");
        assert_eq!(outcome.code, Some(3));
        assert!(!outcome.timed_out);
        assert_eq!(outcome.stdout, "HTTP/2.0 200 OK\n\n{\"data\":{}}");
        assert_eq!(outcome.stderr, "oops\n");
    }

    #[test]
    fn a_missing_gh_says_so() {
        assert_eq!(
            run(Path::new("/nowhere/gh"), &[], CALL_LIMIT),
            Err(Problem::missing())
        );
    }

    #[test]
    fn gh_is_looked_for_where_homebrew_puts_it() {
        let folders = search_path();
        for place in PLACES {
            assert!(folders.iter().any(|folder| folder == Path::new(place)));
        }
    }

    #[test]
    fn at_most_four_searches_run_at_once_and_each_answer_keeps_its_place() {
        let running = AtomicUsize::new(0);
        let most = AtomicUsize::new(0);
        let owners: Vec<usize> = (0..11).collect();
        let found = at_most(&owners, SEARCHES_AT_ONCE, |owner| {
            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            most.fetch_max(now, Ordering::SeqCst);
            sleep(Duration::from_millis(20));
            running.fetch_sub(1, Ordering::SeqCst);
            if *owner == 5 {
                std::panic::resume_unwind(Box::new("a search that panics"));
            }
            owner * 10
        });
        let most = most.load(Ordering::SeqCst);
        assert!((2..=SEARCHES_AT_ONCE).contains(&most), "{most}");
        assert_eq!(found.len(), 11);
        assert!(found[5].is_err());
        let answers: Vec<usize> = found.into_iter().filter_map(Result::ok).collect();
        assert_eq!(answers, [0, 10, 20, 30, 40, 60, 70, 80, 90, 100]);
        assert!(at_most(&[] as &[usize], SEARCHES_AT_ONCE, |owner| *owner).is_empty());
    }

    #[test]
    fn a_refresh_past_its_deadline_stops_and_says_it_took_too_long() {
        let passed = Instant::now();
        let sh = Path::new("/bin/sh");
        assert_eq!(
            ask(sh, "{ viewer { login } }", &[], passed),
            Err(Problem::too_long())
        );
        assert_eq!(searched(sh, "is:pr", passed), Err(Problem::too_long()));
        let slow = std::env::temp_dir().join(format!("raiden-slow-gh-{}", std::process::id()));
        std::fs::write(&slow, "#!/bin/sh\nexec sleep 5\n").expect("written");
        std::fs::set_permissions(&slow, std::fs::Permissions::from_mode(0o755)).expect("runnable");
        let started = Instant::now();
        let stopped = ask(
            &slow,
            "{ viewer { login } }",
            &[],
            started + Duration::from_millis(200),
        );
        let _ = std::fs::remove_file(&slow);
        assert_eq!(stopped, Err(Problem::too_long()));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    fn fake_curl(name: &str, script: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("raiden-{name}-{}", std::process::id()));
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).expect("written");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("runnable");
        path
    }

    const AVATAR_LINK: &str = "https://avatars.githubusercontent.com/u/4242?v=4&s=96";

    #[test]
    fn the_avatar_download_starts_with_no_environment_so_no_token_reaches_it() {
        let curl = fake_curl(
            "bare-curl",
            r#"[ -z "$HOME$USER$GH_TOKEN$GITHUB_TOKEN" ] || exit 9; printf '\211PNG\r\n\032\n'; printf 200 >&2"#,
        );
        let image = download_with(&curl, AVATAR_LINK);
        let elsewhere = download_with(&curl, "https://example.net/larsx2.png");
        let _ = std::fs::remove_file(&curl);
        assert_eq!(image.as_deref(), Some("data:image/png;base64,iVBORw0KGgo="));
        assert_eq!(elsewhere, None);
    }

    #[test]
    fn an_avatar_download_that_is_too_big_or_redirected_is_dropped() {
        let big = fake_curl(
            "big-curl",
            "printf '\\211PNG\\r\\n\\032\\n'; /bin/dd if=/dev/zero bs=1024 count=600 2>/dev/null; printf 200 >&2",
        );
        let moved = fake_curl(
            "moved-curl",
            "printf '\\211PNG\\r\\n\\032\\n'; printf 302 >&2",
        );
        let too_big = download_with(&big, AVATAR_LINK);
        let redirected = download_with(&moved, AVATAR_LINK);
        let _ = std::fs::remove_file(&big);
        let _ = std::fs::remove_file(&moved);
        assert_eq!(too_big, None);
        assert_eq!(redirected, None);
    }

    #[test]
    fn the_avatar_is_fetched_once_and_kept_until_it_is_a_week_old() {
        let folder =
            std::env::temp_dir().join(format!("raiden-pulls-avatar-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let state = Pulls::new(&folder);
        let now = 2_000_000_000;
        assert_eq!(avatar_for(&state, now, |_| panic!("no login yet")), None);
        *state.login.lock().expect("login") = Some("larsx2".into());
        let image = "data:image/png;base64,AA==".to_owned();
        let fetched = |at: u64| {
            Some(Avatar {
                login: "larsx2".into(),
                fetched_at: at,
                image: image.clone(),
            })
        };
        let other = avatar_for(&state, now, |at| {
            Some(Avatar {
                login: "hazel".into(),
                fetched_at: at,
                image: "data:image/png;base64,Ag==".into(),
            })
        });
        assert_eq!(other, None);
        assert_eq!(avatar::read(&folder), None);
        *state.avatar_tried.lock().expect("tried") = None;
        assert_eq!(avatar_for(&state, now, fetched), Some(image.clone()));
        assert_eq!(
            avatar_for(&state, now + 60, |_| panic!("still fresh")),
            Some(image.clone())
        );
        let week = now + avatar::REFRESH_AFTER;
        assert_eq!(avatar_for(&state, week, |_| None), Some(image.clone()));
        assert_eq!(
            avatar_for(&state, week + 60, |_| panic!("tried within the hour")),
            Some(image.clone())
        );
        let later = week + AVATAR_RETRY;
        let newer = "data:image/png;base64,AQ==".to_owned();
        let replaced = avatar_for(&state, later, |at| {
            Some(Avatar {
                login: "larsx2".into(),
                fetched_at: at,
                image: newer.clone(),
            })
        });
        let kept = avatar::read(&folder);
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(replaced, Some(newer));
        assert_eq!(kept.map(|kept| kept.fetched_at), Some(later));
    }

    #[test]
    fn a_refresh_that_panics_still_stops_refreshing() {
        let refreshing = AtomicBool::new(false);
        let crashed = std::panic::catch_unwind(|| {
            let _busy = Busy::start(&refreshing);
            assert!(refreshing.load(Ordering::SeqCst));
            std::panic::resume_unwind(Box::new("a fetch that panics"));
        });
        assert!(crashed.is_err());
        assert!(!refreshing.load(Ordering::SeqCst));
    }

    #[test]
    fn gh_is_never_looked_for_in_a_folder_relative_to_where_raiden_runs() {
        let folders = folders_in(Some(std::ffi::OsStr::new("bin::./tools:/usr/bin:.")));
        assert_eq!(
            folders,
            ["/usr/bin", "/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from)
        );
        assert_eq!(folders_in(None), PLACES.map(PathBuf::from));
    }

    #[test]
    fn a_repo_pinned_during_a_refresh_stays_pinned_and_is_not_lost() {
        let folder = std::env::temp_dir().join(format!("raiden-pulls-pins-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let state = Pulls::new(&folder);
        let asked = AtomicUsize::new(0);
        let settings = || {
            let pinned = if asked.fetch_add(1, Ordering::SeqCst) == 0 {
                vec!["fernhill/gone".to_owned()]
            } else {
                vec!["fernhill/gone".to_owned(), "tidewater/lantern".to_owned()]
            };
            (true, pinned)
        };
        let view = refreshed(&state, true, settings, |pinned| {
            assert_eq!(pinned, ["fernhill/gone"]);
            Ok((
                Viewer {
                    login: "larsx2".into(),
                    organizations: Vec::new(),
                    review_requested: 0,
                    lost: vec!["fernhill/gone".into()],
                },
                Vec::new(),
            ))
        });
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(view.pinned, ["fernhill/gone", "tidewater/lantern"]);
        assert_eq!(view.lost, ["fernhill/gone"]);
        assert!(!view.refreshing);
    }

    #[test]
    fn a_refresh_asked_for_while_one_runs_waits_for_its_result() {
        let folder =
            std::env::temp_dir().join(format!("raiden-pulls-waits-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let state = Pulls::new(&folder);
        let settings = || (true, Vec::new());
        let started = std::sync::Barrier::new(2);
        let second = std::thread::scope(|scope| {
            scope.spawn(|| {
                refreshed(&state, true, settings, |_| {
                    started.wait();
                    std::thread::sleep(Duration::from_millis(300));
                    Ok((
                        Viewer {
                            login: "larsx2".into(),
                            organizations: Vec::new(),
                            review_requested: 1,
                            lost: Vec::new(),
                        },
                        Vec::new(),
                    ))
                })
            });
            started.wait();
            refreshed(&state, false, settings, |_| {
                panic!("a second read must not start while one runs")
            })
        });
        let _ = std::fs::remove_dir_all(&folder);
        assert!(second.fetched_at.is_some());
        assert!(!second.refreshing);
        assert_eq!(second.login.as_deref(), Some("larsx2"));
    }

    #[test]
    fn a_failed_refresh_keeps_the_list_and_says_why() {
        let kept = Cache {
            login: Some("larsx2".into()),
            fetched_at: Some(100),
            review_requested: 2,
            ..Cache::default()
        };
        let failed = settled(kept.clone(), Err(Problem::offline()), 500);
        assert_eq!(failed.fetched_at, Some(100));
        assert_eq!(failed.review_requested, 2);
        assert_eq!(failed.problem, Some(Problem::offline()));
        let viewer = Viewer {
            login: "larsx2".into(),
            organizations: Vec::new(),
            review_requested: 1,
            lost: vec!["fernhill/gone".into()],
        };
        let fresh = settled(failed, Ok((viewer, Vec::new())), 500);
        assert_eq!(fresh.fetched_at, Some(500));
        assert_eq!(fresh.review_requested, 1);
        assert_eq!(fresh.lost, ["fernhill/gone"]);
        assert_eq!(fresh.problem, None);
    }
}
