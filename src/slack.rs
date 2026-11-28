use std::collections::HashMap;
use std::io::Write as _;
use std::os::unix::process::CommandExt as _;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{JoinHandle, sleep};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSString, NSURL};
use raiden_core::slack::{self, Hit, Names, Problem};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, State};

const CURL: &str = "/usr/bin/curl";
const SECURITY: &str = "/usr/bin/security";
const CALL: Duration = Duration::from_secs(15);
const KEYCHAIN: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(25);
const CATALOG_FRESH: u64 = 300;
// After a failed fetch, searches reuse its problem this long instead of fetching again on every
// keystroke and keeping Slack's rate limit used up.
const RETRY_AFTER: u64 = 30;
const MESSAGES: usize = 20;
const MOST: usize = 60;
const PAGE: usize = 200;
const PAGES: usize = 50;

#[derive(Default)]
struct Inner {
    catalog: Mutex<Option<Catalog>>,
    // The token read from the Keychain, kept so a keystroke does not fork
    // `security` every time. Cleared when it is saved or signed out.
    token: Mutex<Option<String>>,
    // Held while one catalog fetch runs, so a burst of cold searches does not
    // each start their own refresh.
    fetching: Mutex<()>,
    // When the last catalog fetch failed, for which token and why, for the backoff above.
    failed: Mutex<Option<(u64, String, Problem)>>,
    // Held while a sign-in writes the token and a sign-out clears it, so the
    // two never interleave and leave the Keychain and memory disagreeing.
    account_change: Mutex<()>,
    // Bumped on every sign-in and sign-out, so a slow fetch or save started for
    // an earlier account can tell it has been superseded and drop its result.
    account: AtomicU64,
}

pub struct Slack {
    inner: Arc<Inner>,
}

struct Entry {
    hit: Hit,
    key: String,
}

struct Catalog {
    fetched: u64,
    token: String,
    entries: Vec<Entry>,
}

impl Slack {
    // Fetches the catalog in the background at launch, so the first Tab into Slack has it ready
    // instead of waiting on the whole workspace.
    pub fn new() -> Self {
        let inner = Arc::new(Inner::default());
        let warming = inner.clone();
        std::thread::spawn(move || {
            if let Some(token) = warming.cached_token() {
                let _ = ensure_catalog(&warming, &token);
            }
        });
        Self { inner }
    }
}

impl Inner {
    fn cached_token(&self) -> Option<String> {
        {
            let guard = self.token.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(token) = guard.as_ref() {
                return Some(token.clone());
            }
        }
        // Read the Keychain under the same lock a sign-out holds, so a sign-out
        // in progress cannot have its just-deleted token read back and cached.
        let _change = self
            .account_change
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        {
            // A sign-in may have filled the cache while we waited for the lock.
            let guard = self.token.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(token) = guard.as_ref() {
                return Some(token.clone());
            }
        }
        let token = token()?;
        *self.token.lock().unwrap_or_else(|e| e.into_inner()) = Some(token.clone());
        Some(token)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlackView {
    has_token: bool,
    hits: Vec<Hit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    problem: Option<Problem>,
}

impl SlackView {
    fn signed_out() -> Self {
        Self {
            has_token: false,
            hits: Vec::new(),
            problem: Some(Problem::no_token()),
        }
    }

    // A failure while signed in: the token stays, the trouble shows over the list.
    fn problem(problem: Problem) -> Self {
        Self {
            has_token: true,
            hits: Vec::new(),
            problem: Some(problem),
        }
    }

    // A failure before a token is in hand: onboarding stays open to try again.
    fn blocked(problem: Problem) -> Self {
        Self {
            has_token: false,
            hits: Vec::new(),
            problem: Some(problem),
        }
    }
}

async fn blocking<T, F>(work: F) -> T
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .expect("the Slack worker did not panic")
}

#[tauri::command]
pub async fn slack_manifest() -> Result<String, ()> {
    Ok(slack::MANIFEST.to_owned())
}

#[tauri::command]
pub async fn slack_has_token(state: State<'_, Slack>) -> Result<bool, ()> {
    let inner = state.inner.clone();
    Ok(blocking(move || inner.cached_token().is_some()).await)
}

#[tauri::command]
pub async fn slack_search(state: State<'_, Slack>, query: String) -> Result<SlackView, ()> {
    let inner = state.inner.clone();
    Ok(blocking(move || search(&inner, &query)).await)
}

#[tauri::command]
pub async fn slack_save_token(state: State<'_, Slack>, token: String) -> Result<SlackView, ()> {
    let inner = state.inner.clone();
    Ok(blocking(move || save_token(&inner, token.trim())).await)
}

#[tauri::command]
pub async fn slack_sign_out(state: State<'_, Slack>) -> Result<SlackView, ()> {
    let inner = state.inner.clone();
    Ok(blocking(move || sign_out(&inner)).await)
}

#[tauri::command]
pub async fn slack_open(app: AppHandle, url: String) -> Result<(), String> {
    let link = safe_slack_link(&url)?.to_owned();
    crate::hide_panel(&app);
    // `open` can take seconds while Slack starts, so it runs here, off the main thread. It only
    // ever hands the link to the Slack app, never to a browser.
    if blocking({
        let link = link.clone();
        move || {
            NSURL::URLWithString(&NSString::from_str(&link))
                .is_some_and(|parsed| is_slack_url(&parsed))
                && in_slack_app(&link)
        }
    })
    .await
    {
        return Ok(());
    }
    let (sender, receiver) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        // Re-check the host on the URL macOS actually parsed, so a fragment or
        // query that fooled the string check cannot reach a non-Slack site.
        let opened = NSURL::URLWithString(&NSString::from_str(&link))
            .filter(|parsed| is_slack_url(parsed))
            .is_some_and(|parsed| NSWorkspace::sharedWorkspace().openURL(&parsed));
        let _ = sender.send(opened);
    })
    .map_err(|error| error.to_string())?;
    if receiver.recv_timeout(Duration::from_secs(5)) == Ok(true) {
        Ok(())
    } else {
        Err(format!("macOS did not open {url}."))
    }
}

// A message's permalink is a web link, which would open in the browser and only then hand over
// to Slack; giving it to the Slack app itself opens the message there. The link is passed as one
// argument, never through a shell.
fn in_slack_app(link: &str) -> bool {
    link.starts_with("https://")
        && std::process::Command::new("/usr/bin/open")
            .args(["-b", SLACK_APP_ID, link])
            .status()
            .is_ok_and(|status| status.success())
}

const SLACK_APP_ID: &str = "com.tinyspeck.slackmacgap";

fn save_token(inner: &Arc<Inner>, token: &str) -> SlackView {
    if !valid_token(token) {
        return SlackView::blocked(Problem::signed_out());
    }
    let account = inner.account.fetch_add(1, Ordering::SeqCst) + 1;
    let catalog = match fetch_catalog(token) {
        Ok(catalog) => catalog,
        // The token never checked out, so keep onboarding open rather than
        // claiming Raiden is signed in.
        Err(problem) => return SlackView::blocked(problem),
    };
    {
        // Check, store and publish as one step against any sign-out, so a
        // sign-out that lands first is never undone by this write.
        let _change = inner
            .account_change
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if inner.account.load(Ordering::SeqCst) != account {
            return SlackView::blocked(Problem::signed_out());
        }
        if store_token(token).is_err() {
            return SlackView::blocked(Problem::failed("the Keychain would not keep the token"));
        }
        *inner.token.lock().unwrap_or_else(|e| e.into_inner()) = Some(token.to_owned());
        *inner.catalog.lock().unwrap_or_else(|e| e.into_inner()) = Some(catalog);
    }
    search(inner, "")
}

fn sign_out(inner: &Inner) -> SlackView {
    let gone = {
        let _change = inner
            .account_change
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        inner.account.fetch_add(1, Ordering::SeqCst);
        let gone = forget_token();
        *inner.token.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *inner.catalog.lock().unwrap_or_else(|e| e.into_inner()) = None;
        gone
    };
    if gone {
        SlackView::signed_out()
    } else {
        // The Keychain still holds the token: stay signed in with the trouble
        // shown, so sign-out is still offered to try again, instead of dropping
        // to onboarding while the token lingers.
        SlackView::problem(Problem::failed("the Keychain would not drop the token"))
    }
}

fn search(inner: &Arc<Inner>, query: &str) -> SlackView {
    let Some(token) = inner.cached_token() else {
        return SlackView::signed_out();
    };
    if let Err(problem) = ensure_catalog(inner, &token) {
        return SlackView::problem(problem);
    }
    let needle = query.trim().to_lowercase();
    // Copy the name matches out while the lock is held, then release it so a
    // slow message search never blocks another keystroke's lookup.
    let mut hits: Vec<Hit> = {
        let guard = inner.catalog.lock().unwrap_or_else(|e| e.into_inner());
        let Some(catalog) = guard.as_ref() else {
            return SlackView::problem(Problem::offline());
        };
        if catalog.token != token {
            return SlackView::problem(Problem::offline());
        }
        catalog
            .entries
            .iter()
            .filter(|entry| needle.is_empty() || entry.key.contains(&needle))
            .map(|entry| entry.hit.clone())
            .collect()
    };
    let mut problem = None;
    // Never send token-shaped text to the search endpoint: that would put a
    // pasted token into curl's argv.
    if slack::wants_messages(query) && !looks_like_token(query) {
        match get(&token, &messages_url(query)) {
            Ok(found) => match slack::check(&found, now()) {
                Ok(()) => hits.extend(slack::messages(&found)),
                // Keep the name matches but say the message search did not land,
                // so an outage never reads as "nothing matches".
                Err(trouble) => problem = Some(trouble),
            },
            Err(trouble) => problem = Some(trouble),
        }
    }
    let mut hits = if needle.is_empty() {
        slack::latest_first(hits)
    } else {
        slack::rank(hits, query)
    };
    hits.truncate(MOST);
    SlackView {
        has_token: true,
        hits,
        problem,
    }
}

fn ensure_catalog(inner: &Arc<Inner>, token: &str) -> Result<(), Problem> {
    let age = || {
        let guard = inner.catalog.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .as_ref()
            .filter(|catalog| catalog.token == token)
            .map(|catalog| now().saturating_sub(catalog.fetched))
    };
    match age() {
        Some(age) if age < CATALOG_FRESH => return Ok(()),
        // An old catalog still answers at once; a fresh one is fetched behind it, unless a fetch
        // is already running.
        Some(_) => {
            if recent_failure(inner, token).is_some() {
                return Ok(());
            }
            let refreshing = inner.clone();
            let token = token.to_owned();
            std::thread::spawn(move || {
                if let Ok(_fetching) = refreshing.fetching.try_lock() {
                    let _ = refresh(&refreshing, &token);
                }
            });
            return Ok(());
        }
        None => {}
    }
    // One fetch at a time: a burst of cold searches waits here, then finds the
    // catalog the first of them built instead of each hitting Slack.
    let _fetching = inner.fetching.lock().unwrap_or_else(|e| e.into_inner());
    if age().is_some() {
        return Ok(());
    }
    // The fetch this search waited on failed: answer with its problem rather than fetch again.
    if let Some(problem) = recent_failure(inner, token) {
        return Err(problem);
    }
    refresh(inner, token)
}

fn recent_failure(inner: &Inner, token: &str) -> Option<Problem> {
    let failed = inner.failed.lock().unwrap_or_else(|e| e.into_inner());
    failed
        .as_ref()
        .filter(|(at, failed_for, _)| {
            failed_for == token && now().saturating_sub(*at) < RETRY_AFTER
        })
        .map(|(_, _, problem)| problem.clone())
}

// Fetches the catalog and installs it; the caller holds `fetching`.
fn refresh(inner: &Inner, token: &str) -> Result<(), Problem> {
    let account = inner.account.load(Ordering::SeqCst);
    let fetched = fetch_catalog(token);
    *inner.failed.lock().unwrap_or_else(|e| e.into_inner()) = fetched
        .as_ref()
        .err()
        .map(|problem| (now(), token.to_owned(), problem.clone()));
    let catalog = match fetched {
        Ok(catalog) => catalog,
        Err(problem) => {
            // A revoked token must not keep serving the old workspace: drop its catalog so the
            // next search says so.
            if problem.kind == slack::ProblemKind::SignedOut {
                let mut guard = inner.catalog.lock().unwrap_or_else(|e| e.into_inner());
                if guard.as_ref().is_some_and(|kept| kept.token == token) {
                    *guard = None;
                }
            }
            return Err(problem);
        }
    };
    // Only install if the account has not changed and no newer catalog for a
    // different token has arrived, or it would show the old workspace.
    if inner.account.load(Ordering::SeqCst) == account {
        let mut guard = inner.catalog.lock().unwrap_or_else(|e| e.into_inner());
        if guard.as_ref().is_none_or(|kept| kept.token == token) {
            *guard = Some(catalog);
        }
    }
    Ok(())
}

fn fetch_catalog(token: &str) -> Result<Catalog, Problem> {
    let auth = get(token, &format!("{}/auth.test", slack::API))?;
    slack::check(&auth, now())?;
    let team = auth
        .get("team_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();

    let members = collect(token, "users.list", "", "members")?;
    let people = slack::people(&json!({ "members": members }), &team);
    let names = Names::from_people(&people);

    let channels = collect(
        token,
        "conversations.list",
        "exclude_archived=true&types=public_channel,private_channel,mpim,im",
        "channels",
    )?;
    let conversations = slack::conversations(&json!({ "channels": channels }), &team, &names);
    // ponytail: recency is when you last wrote somewhere, from your 100 latest messages; a DM you
    // only received, or one older than those, sorts by unread state alone. A failed lookup
    // costs only the order.
    let latest = get(token, &latest_url())
        .ok()
        .filter(|found| slack::check(found, now()).is_ok())
        .map(|found| slack::last_written(&found))
        .unwrap_or_default();
    let conversations = slack::with_recency(conversations, &latest);

    Ok(Catalog {
        fetched: now(),
        token: token.to_owned(),
        entries: entries(people, conversations),
    })
}

// Builds the searchable catalog: one entry per destination, so a person and
// their existing direct message do not show as two rows. Shared destinations
// keep the person's name and handle but adopt the message's unread state.
fn entries(people: Vec<Hit>, conversations: Vec<Hit>) -> Vec<Entry> {
    let mut order: Vec<String> = Vec::new();
    let mut by_link: HashMap<String, Hit> = HashMap::new();
    for hit in people.into_iter().chain(conversations) {
        match by_link.get_mut(&hit.link) {
            Some(kept) => {
                kept.unread = kept.unread.max(hit.unread);
                kept.mention = kept.mention || hit.mention;
                kept.updated = kept.updated.max(hit.updated);
                if kept.subtitle.is_empty() {
                    kept.subtitle = hit.subtitle;
                }
            }
            None => {
                order.push(hit.link.clone());
                by_link.insert(hit.link.clone(), hit);
            }
        }
    }
    order
        .into_iter()
        .filter_map(|link| by_link.remove(&link))
        .map(|hit| {
            let key = format!("{}\n{}", hit.title, hit.subtitle).to_lowercase();
            Entry { hit, key }
        })
        .collect()
}

// Reads every page of a Slack list endpoint, following the cursor, so names on
// later pages of a large workspace are still found.
fn collect(token: &str, endpoint: &str, params: &str, key: &str) -> Result<Vec<Value>, Problem> {
    let mut items = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..PAGES {
        let mut url = format!("{}/{endpoint}?limit={PAGE}", slack::API);
        if !params.is_empty() {
            url.push('&');
            url.push_str(params);
        }
        if let Some(cursor) = &cursor {
            url.push_str(&format!("&cursor={}", encode(cursor)));
        }
        let page = get(token, &url)?;
        slack::check(&page, now())?;
        if let Some(array) = page.get(key).and_then(Value::as_array) {
            items.extend(array.iter().cloned());
        }
        cursor = page
            .pointer("/response_metadata/next_cursor")
            .and_then(Value::as_str)
            .filter(|next| !next.is_empty())
            .map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    Ok(items)
}

fn latest_url() -> String {
    format!(
        "{}/search.messages?count=100&sort=timestamp&query={}",
        slack::API,
        encode("from:me")
    )
}

fn messages_url(query: &str) -> String {
    format!(
        "{}/search.messages?count={MESSAGES}&sort=timestamp&query={}",
        slack::API,
        encode(query.trim())
    )
}

// Percent-encodes a query for a Slack URL, leaving the unreserved set alone.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

// A Slack user token is `xox?-` then digits, hyphens and letters. Checking it
// before use keeps anything that could break out of curl's quoted config, or
// of the Keychain, from ever being run. The upper bound stays within what the
// Keychain tool accepts on stdin.
fn valid_token(token: &str) -> bool {
    token.starts_with("xox")
        && (20..=200).contains(&token.len())
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

// True if a token appears anywhere in the text, however it is wrapped — behind
// "Bearer ", quotes, or a NAME= prefix — so a token pasted into search is kept
// out of the search endpoint before it could reach curl's argv.
fn looks_like_token(text: &str) -> bool {
    text.match_indices("xox").any(|(at, _)| {
        let run = text[at..]
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .next()
            .unwrap_or("");
        run.len() >= 12
            && run.as_bytes().get(3).is_some_and(u8::is_ascii_lowercase)
            && run.as_bytes().get(4) == Some(&b'-')
    })
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

fn token() -> Option<String> {
    let (code, out) = run(
        SECURITY,
        &[
            "find-generic-password",
            "-s",
            slack::KEYCHAIN_SERVICE,
            "-a",
            slack::KEYCHAIN_ACCOUNT,
            "-w",
        ],
        None,
        KEYCHAIN,
    )?;
    if code != 0 {
        return None;
    }
    let token = out.trim().to_owned();
    valid_token(&token).then_some(token)
}

fn store_token(token: &str) -> Result<(), Problem> {
    // The token goes in twice on stdin, the way `security -w` prompts for it,
    // so it never rides in argv where `ps` could read it.
    let feed = format!("{token}\n{token}\n");
    match run(
        SECURITY,
        &[
            "add-generic-password",
            "-U",
            "-s",
            slack::KEYCHAIN_SERVICE,
            "-a",
            slack::KEYCHAIN_ACCOUNT,
            "-w",
        ],
        Some(&feed),
        KEYCHAIN,
    ) {
        Some((0, _)) => Ok(()),
        _ => Err(Problem::failed("the Keychain would not keep the token")),
    }
}

// Removes the token, reporting whether it is now gone (success, or already
// absent); only a real deletion failure returns false.
fn forget_token() -> bool {
    match run(
        SECURITY,
        &[
            "delete-generic-password",
            "-s",
            slack::KEYCHAIN_SERVICE,
            "-a",
            slack::KEYCHAIN_ACCOUNT,
        ],
        None,
        KEYCHAIN,
    ) {
        Some((0, _)) => true,
        // Exit 44 is "item not found": nothing to remove is still signed out.
        Some((44, _)) => true,
        // Any other outcome, including a timeout, is a real failure: do not
        // claim the token is gone when it may still be there.
        _ => false,
    }
}

fn get(token: &str, url: &str) -> Result<Value, Problem> {
    if !valid_token(token) {
        return Err(Problem::signed_out());
    }
    // -q first stops curl reading ~/.curlrc, which could log the auth header.
    let feed = format!("header = \"Authorization: Bearer {token}\"\n");
    let mut child = Command::new(CURL)
        .args([
            "-q",
            "--silent",
            "--show-error",
            "--max-time",
            "15",
            "--config",
            "-",
            url,
        ])
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|error| Problem::failed(&format!("could not start curl ({error})")))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(feed.as_bytes());
    }
    let stdout = reader(child.stdout.take());
    let (code, timed_out) = finish(&mut child, Instant::now() + CALL);
    if timed_out {
        return Err(Problem::offline());
    }
    let body = stdout.join().unwrap_or_default();
    if code != Some(0) || body.trim().is_empty() {
        return Err(Problem::offline());
    }
    serde_json::from_str(&body).map_err(|_| Problem::failed("a reply that was not JSON"))
}

// Opens only a Slack destination: a slack:// channel or user link, or an
// https Slack permalink. Anything else, or any whitespace or control
// character, is refused before it reaches the opener.
fn safe_slack_link(url: &str) -> Result<&str, String> {
    let link = url.trim();
    // A backslash reads as a slash to some URL parsers, which would move the host.
    let clean = !link
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || c == '\\');
    let allowed = match link.split_once("://") {
        Some(("slack", rest)) => rest.starts_with("channel?") || rest.starts_with("user?"),
        // The authority ends at the first of / ? #; take only that, drop any
        // userinfo before @ and any :port, so a fragment or query cannot smuggle
        // ".slack.com" past the host check.
        Some(("https", rest)) => {
            let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
            let host = authority
                .rsplit('@')
                .next()
                .unwrap_or("")
                .split(':')
                .next()
                .unwrap_or("");
            host == "slack.com" || host.ends_with(".slack.com")
        }
        _ => false,
    };
    if clean && allowed {
        Ok(link)
    } else {
        Err(format!(
            "Raiden opens only Slack links, and {url} is not one."
        ))
    }
}

// The same check against the host macOS parsed from the URL, as a second gate
// right before it opens.
fn is_slack_url(url: &NSURL) -> bool {
    let scheme = url.scheme().map(|s| s.to_string().to_ascii_lowercase());
    match scheme.as_deref() {
        Some("slack") => true,
        Some("https") => url
            .host()
            .map(|host| host.to_string().to_ascii_lowercase())
            .is_some_and(|host| host == "slack.com" || host.ends_with(".slack.com")),
        _ => false,
    }
}

// Runs a short helper, optionally feeding stdin, and returns its exit code and
// captured stdout, or None if it could not run or outran the deadline.
fn run(program: &str, args: &[&str], feed: Option<&str>, limit: Duration) -> Option<(i32, String)> {
    let mut child = Command::new(program)
        .args(args)
        .env_clear()
        .stdin(if feed.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .ok()?;
    if let (Some(feed), Some(mut stdin)) = (feed, child.stdin.take()) {
        let _ = stdin.write_all(feed.as_bytes());
    }
    let stdout = reader(child.stdout.take());
    let (code, timed_out) = finish(&mut child, Instant::now() + limit);
    if timed_out {
        return None;
    }
    Some((code?, stdout.join().unwrap_or_default()))
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

fn finish(child: &mut Child, deadline: Instant) -> (Option<i32>, bool) {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return (status.code(), false),
            Ok(None) if Instant::now() < deadline => sleep(POLL),
            _ => {
                if let Ok(group) = libc::pid_t::try_from(child.id()) {
                    unsafe {
                        libc::killpg(group, libc::SIGKILL);
                    }
                }
                let _ = child.kill();
                let _ = child.wait();
                return (None, true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raiden_core::slack::Kind;

    fn hit(link: &str, title: &str, unread: u64, mention: bool) -> Hit {
        Hit {
            id: title.to_owned(),
            kind: Kind::Person,
            title: title.to_owned(),
            subtitle: String::new(),
            unread,
            mention,
            updated: 0,
            link: link.to_owned(),
        }
    }

    #[test]
    fn a_failed_fetch_holds_off_retries_for_that_token_only() {
        let inner = Inner::default();
        *inner.failed.lock().unwrap() = Some((now(), "xoxp-a".to_owned(), Problem::offline()));
        assert_eq!(recent_failure(&inner, "xoxp-a"), Some(Problem::offline()));
        assert_eq!(recent_failure(&inner, "xoxp-b"), None);
        *inner.failed.lock().unwrap() =
            Some((now() - RETRY_AFTER, "xoxp-a".to_owned(), Problem::offline()));
        assert_eq!(recent_failure(&inner, "xoxp-a"), None);
    }

    #[test]
    fn a_query_is_percent_encoded_for_the_search_url() {
        assert_eq!(encode("ship it!"), "ship%20it%21");
        assert_eq!(encode("plain-text_1.0~"), "plain-text_1.0~");
        assert!(messages_url("a b").ends_with("query=a%20b"));
    }

    #[test]
    fn only_slack_shaped_tokens_pass() {
        assert!(valid_token(&format!("xoxp-{}", "1".repeat(40))));
        assert!(!valid_token("not-a-token"));
        assert!(!valid_token("xoxp-with spaces and \"quotes\""));
        assert!(!valid_token("xoxp-$(rm -rf)"));
        assert!(!valid_token("xox"));
        assert!(!valid_token(&format!("xoxp-{}", "1".repeat(300))));
    }

    #[test]
    fn a_pasted_token_is_kept_out_of_the_search_endpoint() {
        assert!(looks_like_token("xoxp-123456789"));
        // Even wrapped the way people paste it.
        assert!(looks_like_token("Bearer xoxp-123456789"));
        assert!(looks_like_token("\"xoxp-123456789\""));
        assert!(looks_like_token("SLACK_TOKEN=xoxp-123456789"));
        assert!(!looks_like_token("standup notes"));
        assert!(!looks_like_token("talk about proxywork"));
        assert!(!looks_like_token("xox"));
        assert!(!looks_like_token("xoxox"));
    }

    #[test]
    fn only_slack_destinations_open() {
        assert!(safe_slack_link("slack://channel?team=T&id=C1").is_ok());
        assert!(safe_slack_link("slack://user?team=T&id=U1").is_ok());
        assert!(safe_slack_link("https://x.slack.com/archives/C1/p1").is_ok());
        assert!(safe_slack_link("http://evil.com").is_err());
        assert!(safe_slack_link("https://evil.com/slack").is_err());
        assert!(safe_slack_link("slack://channel?id=C\n evil").is_err());
        assert!(safe_slack_link("file:///etc/passwd").is_err());
        // A fragment or query must not smuggle the suffix past the host check.
        assert!(safe_slack_link("https://evil.com#.slack.com/x").is_err());
        assert!(safe_slack_link("https://evil.com?.slack.com/x").is_err());
        assert!(safe_slack_link("https://slack.com@evil.com/x").is_err());
        assert!(safe_slack_link("https://x.slack.com.evil.com/x").is_err());
        assert!(safe_slack_link("slack://teamsy?evil").is_err());
        assert!(safe_slack_link("https://evil.example\\@x.slack.com/x").is_err());
    }

    #[test]
    fn a_person_and_their_open_dm_become_one_row_that_keeps_the_unread() {
        let people = vec![{
            let mut p = hit("slack://user?team=T&id=U1", "Ana Lee", 0, false);
            p.subtitle = "@ana".to_owned();
            p
        }];
        let dms = vec![hit("slack://user?team=T&id=U1", "Ana Lee", 4, true)];
        let built = entries(people, dms);
        assert_eq!(built.len(), 1);
        assert_eq!(built[0].hit.subtitle, "@ana");
        assert_eq!(built[0].hit.unread, 4);
        assert!(built[0].hit.mention);
        assert!(built[0].key.contains("@ana"));
    }
}
