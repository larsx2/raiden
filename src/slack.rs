use std::io::Write as _;
use std::os::unix::process::CommandExt as _;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread::{JoinHandle, sleep};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use raiden_core::slack::{self, Hit, Names, Problem};
use serde::Serialize;
use serde_json::Value;
use tauri::State;

const CURL: &str = "/usr/bin/curl";
const SECURITY: &str = "/usr/bin/security";
const CALL: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(25);
const CATALOG_FRESH: u64 = 300;
const MESSAGES: usize = 20;
const MOST: usize = 60;

pub struct Slack {
    catalog: Mutex<Option<Catalog>>,
}

struct Catalog {
    fetched: u64,
    people: Vec<Hit>,
    conversations: Vec<Hit>,
}

impl Slack {
    pub fn new() -> Self {
        Self {
            catalog: Mutex::new(None),
        }
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

    fn problem(problem: Problem) -> Self {
        Self {
            has_token: true,
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
    Ok(match fetch_catalog(&token) {
        Ok(catalog) => {
            if let Err(error) = store_token(&token) {
                SlackView::problem(error)
            } else {
                *state.catalog.lock().unwrap_or_else(|e| e.into_inner()) = Some(catalog);
                search(&state, "")
            }
        }
        Err(problem) => SlackView::problem(problem),
    })
}

#[tauri::command]
pub async fn slack_sign_out(state: State<'_, Slack>) -> Result<(), ()> {
    forget_token();
    *state.catalog.lock().unwrap_or_else(|e| e.into_inner()) = None;
    Ok(())
}

fn search(state: &Slack, query: &str) -> SlackView {
    let Some(token) = token() else {
        return SlackView::signed_out();
    };
    if let Err(problem) = ensure_catalog(state, &token) {
        return SlackView::problem(problem);
    }
    let guard = state.catalog.lock().unwrap_or_else(|e| e.into_inner());
    let Some(catalog) = guard.as_ref() else {
        return SlackView::problem(Problem::offline());
    };
    let mut hits = catalog.people.clone();
    hits.extend(catalog.conversations.clone());
    let mut hits = slack::by_name(hits, query);
    if slack::wants_messages(query)
        && let Ok(found) = get(&token, &messages_url(query))
        && slack::check(&found, now()).is_ok()
    {
        hits.extend(slack::messages(&found));
    }
    let mut hits = slack::rank(hits);
    hits.truncate(MOST);
    SlackView {
        has_token: true,
        hits,
        problem: None,
    }
}

fn ensure_catalog(state: &Slack, token: &str) -> Result<(), Problem> {
    let stale = {
        let guard = state.catalog.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .as_ref()
            .is_none_or(|catalog| now().saturating_sub(catalog.fetched) >= CATALOG_FRESH)
    };
    if stale {
        let catalog = fetch_catalog(token)?;
        *state.catalog.lock().unwrap_or_else(|e| e.into_inner()) = Some(catalog);
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

    let users = get(token, &format!("{}/users.list?limit=1000", slack::API))?;
    slack::check(&users, now())?;
    let people = slack::people(&users, &team);
    let names = Names::from_people(&people);

    let convs = get(
        token,
        &format!(
            "{}/conversations.list?exclude_archived=true&limit=1000&types=public_channel,private_channel,mpim,im",
            slack::API
        ),
    )?;
    slack::check(&convs, now())?;
    let conversations = slack::conversations(&convs, &team, &names);

    Ok(Catalog {
        fetched: now(),
        people,
        conversations,
    })
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

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn token() -> Option<String> {
    let output = Command::new(SECURITY)
        .args([
            "find-generic-password",
            "-s",
            slack::KEYCHAIN_SERVICE,
            "-a",
            slack::KEYCHAIN_ACCOUNT,
            "-w",
        ])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let token = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!token.is_empty()).then_some(token)
}

fn store_token(token: &str) -> Result<(), Problem> {
    // ponytail: the token rides in argv for the length of one `security` exec,
    // briefly visible to `ps`. Switch to the Security framework if that matters.
    let status = Command::new(SECURITY)
        .args([
            "add-generic-password",
            "-U",
            "-s",
            slack::KEYCHAIN_SERVICE,
            "-a",
            slack::KEYCHAIN_ACCOUNT,
            "-w",
            token,
        ])
        .stdin(Stdio::null())
        .status()
        .map_err(|error| Problem::failed(&format!("could not reach the Keychain ({error})")))?;
    if status.success() {
        Ok(())
    } else {
        Err(Problem::failed("the Keychain refused the token"))
    }
}

fn forget_token() {
    let _ = Command::new(SECURITY)
        .args([
            "delete-generic-password",
            "-s",
            slack::KEYCHAIN_SERVICE,
            "-a",
            slack::KEYCHAIN_ACCOUNT,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn get(token: &str, url: &str) -> Result<Value, Problem> {
    let mut child = Command::new(CURL)
        .args([
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
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|error| Problem::failed(&format!("could not start curl ({error})")))?;
    if let Some(mut stdin) = child.stdin.take() {
        // The token goes in on stdin, never argv, so it stays out of `ps`.
        let _ = stdin.write_all(format!("header = \"Authorization: Bearer {token}\"\n").as_bytes());
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

    #[test]
    fn a_query_is_percent_encoded_for_the_search_url() {
        assert_eq!(encode("ship it!"), "ship%20it%21");
        assert_eq!(encode("plain-text_1.0~"), "plain-text_1.0~");
        assert!(messages_url("a b").ends_with("query=a%20b"));
    }
}
