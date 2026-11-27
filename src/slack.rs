use std::collections::HashMap;
use std::io::Write as _;
use std::os::unix::process::CommandExt as _;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread::{JoinHandle, sleep};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use raiden_core::slack::{self, Hit, Names, Problem};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::State;

const CURL: &str = "/usr/bin/curl";
const SECURITY: &str = "/usr/bin/security";
const CALL: Duration = Duration::from_secs(15);
const KEYCHAIN: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(25);
const CATALOG_FRESH: u64 = 300;
const MESSAGES: usize = 20;
const MOST: usize = 60;
const PAGE: usize = 200;
const PAGES: usize = 12;

pub struct Slack {
    catalog: Mutex<Option<Catalog>>,
    // The token read from the Keychain, kept so a keystroke does not fork
    // `security` every time. Cleared when it is saved or signed out.
    token: Mutex<Option<String>>,
    // Held while one catalog fetch runs, so a burst of cold searches does not
    // each start their own three-call refresh.
    fetching: Mutex<()>,
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
    pub fn new() -> Self {
        Self {
            catalog: Mutex::new(None),
            token: Mutex::new(None),
            fetching: Mutex::new(()),
        }
    }

    fn cached_token(&self) -> Option<String> {
        {
            let guard = self.token.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(token) = guard.as_ref() {
                return Some(token.clone());
            }
        }
        let token = token()?;
        *self.token.lock().unwrap_or_else(|e| e.into_inner()) = Some(token.clone());
        Some(token)
    }

    fn remember_token(&self, token: Option<String>) {
        *self.token.lock().unwrap_or_else(|e| e.into_inner()) = token;
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

#[tauri::command]
pub async fn slack_manifest() -> Result<String, ()> {
    Ok(slack::MANIFEST.to_owned())
}

#[tauri::command]
pub async fn slack_has_token() -> Result<bool, ()> {
    Ok(token().is_some())
}

#[tauri::command]
pub async fn slack_search(state: State<'_, Slack>, query: String) -> Result<SlackView, ()> {
    Ok(search(&state, &query))
}

#[tauri::command]
pub async fn slack_save_token(state: State<'_, Slack>, token: String) -> Result<SlackView, ()> {
    let token = token.trim().to_owned();
    if !valid_token(&token) {
        return Ok(SlackView::blocked(Problem::signed_out()));
    }
    Ok(match fetch_catalog(&token) {
        Ok(catalog) => {
            if store_token(&token).is_err() {
                SlackView::blocked(Problem::failed("the Keychain would not keep the token"))
            } else {
                state.remember_token(Some(token));
                *state.catalog.lock().unwrap_or_else(|e| e.into_inner()) = Some(catalog);
                search(&state, "")
            }
        }
        // The token never checked out, so keep onboarding open rather than
        // claiming Raiden is signed in.
        Err(problem) => SlackView::blocked(problem),
    })
}

#[tauri::command]
pub async fn slack_sign_out(state: State<'_, Slack>) -> Result<(), ()> {
    forget_token();
    state.remember_token(None);
    *state.catalog.lock().unwrap_or_else(|e| e.into_inner()) = None;
    Ok(())
}

fn search(state: &Slack, query: &str) -> SlackView {
    let Some(token) = state.cached_token() else {
        return SlackView::signed_out();
    };
    if let Err(problem) = ensure_catalog(state, &token) {
        return SlackView::problem(problem);
    }
    let needle = query.trim().to_lowercase();
    // Copy the name matches out while the lock is held, then release it so a
    // slow message search never blocks another keystroke's lookup.
    let mut hits: Vec<Hit> = {
        let guard = state.catalog.lock().unwrap_or_else(|e| e.into_inner());
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
    if slack::wants_messages(query) {
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
    let mut hits = slack::rank(hits);
    hits.truncate(MOST);
    SlackView {
        has_token: true,
        hits,
        problem,
    }
}

fn ensure_catalog(state: &Slack, token: &str) -> Result<(), Problem> {
    let fresh = |state: &Slack| {
        let guard = state.catalog.lock().unwrap_or_else(|e| e.into_inner());
        guard.as_ref().is_some_and(|catalog| {
            catalog.token == token && now().saturating_sub(catalog.fetched) < CATALOG_FRESH
        })
    };
    if fresh(state) {
        return Ok(());
    }
    // One fetch at a time: a burst of cold searches waits here, then finds the
    // catalog the first of them built instead of each hitting Slack.
    let _fetching = state.fetching.lock().unwrap_or_else(|e| e.into_inner());
    if fresh(state) {
        return Ok(());
    }
    let catalog = fetch_catalog(token)?;
    // Only replace a catalog still built for this token; a token change since
    // the fetch began must win, or it would show the old account.
    let mut guard = state.catalog.lock().unwrap_or_else(|e| e.into_inner());
    if guard.as_ref().is_none_or(|kept| kept.token == token) {
        *guard = Some(catalog);
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
// of the Keychain, from ever being run.
fn valid_token(token: &str) -> bool {
    token.starts_with("xox")
        && (20..256).contains(&token.len())
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
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
    (valid_token(&token)).then_some(token)
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

fn forget_token() -> bool {
    matches!(
        run(
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
        ),
        Some((0, _))
    )
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
