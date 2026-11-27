use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const VIEW_ID: &str = "raiden:pull-requests";
pub const VIEW_TITLE: &str = "Pull Requests";
pub const UNDER: &str = "GitHub";
pub const PAGES: usize = 25;
pub const FRESH_FOR: u64 = 60;
pub const FOLDER: &str = "github";
pub const CACHE: &str = "pulls.json";
const NAMES: usize = 5;
const CONTEXTS: usize = 10;
const SNAPSHOT_LINES: usize = 60;
const SNAPSHOT_BYTES: usize = 8 * 1024;
const RETRY_LATER: u64 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Look {
    Bad,
    Ok,
    Run,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Checks {
    Passed,
    Failed,
    Running,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pull {
    pub owner: String,
    pub repo: String,
    pub shown: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub draft: bool,
    pub updated_at: u64,
    pub branch: String,
    pub base: String,
    pub additions: u64,
    pub deletions: u64,
    pub files: u64,
    pub conflicts: bool,
    pub checks: Checks,
    pub failed: Vec<String>,
    pub running: Vec<String>,
    pub passed: u64,
    pub failing: bool,
    pub ready: bool,
    pub status: String,
    pub look: Look,
}

impl Pull {
    pub fn settle(mut self) -> Self {
        self.failing = self.checks == Checks::Failed;
        self.ready = !self.draft && self.checks == Checks::Passed && !self.conflicts;
        let (status, look) = if self.draft && self.failing {
            ("Draft, and checks are failing".to_owned(), Look::Bad)
        } else if self.failing {
            ("Checks failing".to_owned(), Look::Bad)
        } else if self.conflicts {
            (format!("Conflicts with {}", self.base), Look::Bad)
        } else if self.draft {
            ("Draft".to_owned(), Look::None)
        } else {
            match self.checks {
                Checks::Running => ("Checks running".to_owned(), Look::Run),
                Checks::Passed => (
                    "Ready to merge: checks pass, no conflicts".to_owned(),
                    Look::Ok,
                ),
                Checks::None | Checks::Failed => ("No checks ran".to_owned(), Look::None),
            }
        };
        self.status = status;
        self.look = look;
        self
    }

    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }

    fn within(&self, only: Option<&str>) -> bool {
        match only {
            None => true,
            Some(only) => match only.split_once('/') {
                Some((owner, repo)) => {
                    self.owner.eq_ignore_ascii_case(owner) && self.repo.eq_ignore_ascii_case(repo)
                }
                None => self.owner.eq_ignore_ascii_case(only),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owner {
    pub name: String,
    pub open: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProblemKind {
    Missing,
    SignedOut,
    Offline,
    Limited,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub kind: ProblemKind,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_at: Option<u64>,
}

impl Problem {
    fn new(kind: ProblemKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            retry_at: None,
        }
    }

    pub fn missing() -> Self {
        Self::new(
            ProblemKind::Missing,
            "Raiden could not find gh, GitHub's command-line tool. Install it with brew install gh, then run gh auth login.",
        )
    }

    pub fn signed_out() -> Self {
        Self::new(
            ProblemKind::SignedOut,
            "gh is not signed in to GitHub. Run gh auth login in Terminal.",
        )
    }

    pub fn offline() -> Self {
        Self::new(ProblemKind::Offline, "Raiden could not reach GitHub.")
    }

    pub fn old_gh() -> Self {
        Self::new(
            ProblemKind::Failed,
            "Your gh is too old for Raiden. Update it with brew upgrade gh, then open Pull Requests again.",
        )
    }

    pub fn too_long() -> Self {
        Self::new(
            ProblemKind::Failed,
            "GitHub took too long to send your pull requests, so Raiden stopped and kept the list it had.",
        )
    }

    pub fn not_allowed() -> Self {
        Self::new(
            ProblemKind::Failed,
            "Pull Requests reads nothing until you press Continue.",
        )
    }

    pub fn failed(detail: &str) -> Self {
        let detail = detail.trim().trim_end_matches('.');
        Self::new(
            ProblemKind::Failed,
            if detail.is_empty() {
                "GitHub did not answer as expected.".to_owned()
            } else {
                format!("GitHub did not answer as expected: {detail}.")
            },
        )
    }

    fn limited(retry_at: u64) -> Self {
        Self {
            kind: ProblemKind::Limited,
            text: "GitHub's limit on requests is used up for now.".to_owned(),
            retry_at: Some(retry_at),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullsView {
    pub allowed: bool,
    pub login: Option<String>,
    pub pulls: Vec<Pull>,
    pub fetched_at: Option<u64>,
    pub review_requested: u64,
    pub owners: Vec<Owner>,
    pub pinned: Vec<String>,
    pub lost: Vec<String>,
    pub refreshing: bool,
    pub problem: Option<Problem>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Cache {
    pub login: Option<String>,
    pub fetched_at: Option<u64>,
    pub review_requested: u64,
    pub pulls: Vec<Pull>,
    pub lost: Vec<String>,
    pub problem: Option<Problem>,
}

impl Cache {
    pub fn fresh(&self, now: u64) -> bool {
        self.problem.is_none()
            && self
                .fetched_at
                .is_some_and(|at| now.saturating_sub(at) < FRESH_FOR)
    }

    pub fn view(&self, allowed: bool, pinned: &[String], refreshing: bool) -> PullsView {
        PullsView {
            allowed,
            login: self.login.clone(),
            pulls: self.pulls.clone(),
            fetched_at: self.fetched_at,
            review_requested: self.review_requested,
            owners: owners(&self.pulls),
            pinned: pinned.to_vec(),
            lost: pinned
                .iter()
                .filter(|pin| self.lost.iter().any(|lost| lost.eq_ignore_ascii_case(pin)))
                .cloned()
                .collect(),
            refreshing,
            problem: self.problem.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tab {
    All,
    Failing,
    Ready,
    Drafts,
}

impl Tab {
    fn holds(self, pull: &Pull) -> bool {
        match self {
            Tab::All => true,
            Tab::Failing => pull.failing,
            Tab::Ready => pull.ready,
            Tab::Drafts => pull.draft,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Tab::All => "Open pull requests",
            Tab::Failing => "Failing pull requests",
            Tab::Ready => "Pull requests ready to merge",
            Tab::Drafts => "Draft pull requests",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    pub label: String,
    pub text: String,
}

pub const VIEWER_QUERY_HEAD: &str = "viewer { login organizations(first: 100) { nodes { login } } }\n  reviews: search(query: $reviews, type: ISSUE, first: 1) { issueCount }";

pub const REVIEWS: &str = "is:pr is:open review-requested:@me archived:false";

pub const AUTHORED: &str = "is:pr is:open archived:false author:@me";

pub const SEARCH_QUERY: &str = "query($q: String!, $after: String) {
  search(query: $q, type: ISSUE, first: 40, after: $after) {
    pageInfo { hasNextPage endCursor }
    nodes {
      ... on PullRequest {
        number title url isDraft updatedAt headRefName baseRefName
        additions deletions changedFiles mergeable
        repository { name owner { login } }
        commits(last: 1) { nodes { commit { statusCheckRollup { state
          contexts(first: 10) {
            checkRunCountsByState { state count }
            statusContextCountsByState { state count }
            nodes {
              __typename
              ... on CheckRun { name status conclusion }
              ... on StatusContext { context state }
            }
          }
        } } } }
      }
    }
  }
}";

pub const AUTH_STATUS: [&str; 5] = ["auth", "status", "--active", "--hostname", "github.com"];

pub fn viewer_query(pins: usize) -> String {
    let variables: String = (0..pins)
        .map(|index| format!(", $o{index}: String!, $n{index}: String!"))
        .collect();
    let aliases: String = (0..pins)
        .map(|index| {
            format!(
                "\n  pin{index}: repository(owner: $o{index}, name: $n{index}) {{ nameWithOwner }}"
            )
        })
        .collect();
    format!("query($reviews: String!{variables}) {{\n  {VIEWER_QUERY_HEAD}{aliases}\n}}")
}

pub fn viewer_fields(pinned: &[String]) -> Vec<(String, String)> {
    let mut fields = vec![("reviews".to_owned(), REVIEWS.to_owned())];
    for (index, pin) in pinned.iter().enumerate() {
        let (owner, name) = pin.split_once('/').unwrap_or((pin, ""));
        fields.push((format!("o{index}"), owner.to_owned()));
        fields.push((format!("n{index}"), name.to_owned()));
    }
    fields
}

pub fn search_terms(owner: &str, viewer: &str) -> String {
    let qualifier = if owner.eq_ignore_ascii_case(viewer) {
        "user"
    } else {
        "org"
    };
    format!("is:pr is:open archived:false {qualifier}:{owner}")
}

pub fn arguments(query: &str, fields: &[(String, String)]) -> Result<Vec<String>, String> {
    read_only(query)?;
    let mut arguments: Vec<String> = ["api", "graphql", "--include", "-f"]
        .map(str::to_owned)
        .into();
    arguments.push(format!("query={query}"));
    for (name, value) in fields {
        arguments.push("-f".to_owned());
        arguments.push(format!("{name}={value}"));
    }
    Ok(arguments)
}

/// ```
/// use raiden_core::pulls::read_only;
/// assert!(read_only("query { viewer { login } }").is_ok());
/// assert!(read_only("mutation { closePullRequest(input: {}) { clientMutationId } }").is_err());
/// assert!(read_only(r#"query { search(query: "x") { issueCount } }"#).is_err());
/// ```
pub fn read_only(query: &str) -> Result<(), String> {
    let words = words(query);
    let plain = !query.contains(['"', '#']);
    let starts_well =
        query.trim_start().starts_with('{') || words.first().is_some_and(|word| word == "query");
    let changes = words.iter().any(|word| {
        word.eq_ignore_ascii_case("mutation") || word.eq_ignore_ascii_case("subscription")
    });
    if plain && starts_well && !changes {
        Ok(())
    } else {
        Err(
            "Raiden only reads from GitHub, so it refused a request that could change something"
                .to_owned(),
        )
    }
}

fn words(query: &str) -> Vec<String> {
    query
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub status: Option<u16>,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

/// ```
/// use raiden_core::pulls::reply;
/// let parsed = reply("HTTP/2.0 502 Bad Gateway\nX-Ratelimit-Remaining: 10\n\n{}");
/// assert_eq!(parsed.status, Some(502));
/// assert_eq!(parsed.headers["x-ratelimit-remaining"], "10");
/// assert_eq!(parsed.body, "{}");
/// ```
pub fn reply(stdout: &str) -> Reply {
    if !stdout.starts_with("HTTP/") {
        return Reply {
            status: None,
            headers: BTreeMap::new(),
            body: stdout.to_owned(),
        };
    }
    let text = stdout.replace("\r\n", "\n");
    let (head, body) = text.split_once("\n\n").unwrap_or((&text, ""));
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok());
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    Reply {
        status,
        headers,
        body: body.to_owned(),
    }
}

pub fn retryable(outcome: &Outcome) -> bool {
    let status = reply(&outcome.stdout).status;
    matches!(status, Some(502..=504))
        || ["HTTP 502", "HTTP 503", "HTTP 504"]
            .iter()
            .any(|code| outcome.stderr.contains(code))
}

const OFFLINE: [&str; 9] = [
    "error connecting to",
    "no such host",
    "dial tcp",
    "network is unreachable",
    "i/o timeout",
    "connection refused",
    "connection reset",
    "tls handshake timeout",
    "could not resolve host",
];

pub fn answer(outcome: &Outcome, now: u64) -> Result<Value, Problem> {
    if outcome.timed_out {
        return Err(Problem::too_long());
    }
    let reply = reply(&outcome.stdout);
    let body: Option<Value> = serde_json::from_str(&reply.body).ok();
    let complaint = body
        .as_ref()
        .and_then(|body| {
            body.pointer("/errors/0/message")
                .or_else(|| body.get("message"))
        })
        .and_then(Value::as_str)
        .map(str::to_owned);
    let typed_limit = body
        .as_ref()
        .and_then(|body| body.get("errors"))
        .and_then(Value::as_array)
        .is_some_and(|errors| {
            errors
                .iter()
                .any(|error| error.get("type").and_then(Value::as_str) == Some("RATE_LIMITED"))
        });
    let used_up = reply
        .headers
        .get("x-ratelimit-remaining")
        .is_some_and(|left| left == "0");
    if typed_limit || reply.status == Some(429) || (reply.status == Some(403) && used_up) {
        return Err(Problem::limited(retry_at(&reply, now)));
    }
    if reply.status == Some(401) || outcome.code == Some(4) {
        return Err(Problem::signed_out());
    }
    let data = body
        .as_ref()
        .and_then(|body| body.get("data"))
        .filter(|data| data.is_object());
    if let Some(data) = data
        && reply.status.is_none_or(|status| status == 200)
    {
        return Ok(data.clone());
    }
    let said = format!(
        "{}\n{}",
        outcome.stderr,
        complaint.as_deref().unwrap_or_default()
    )
    .to_lowercase();
    if said.contains("rate limit") {
        return Err(Problem::limited(retry_at(&reply, now)));
    }
    if said.contains("gh auth login") || said.contains("not logged in") {
        return Err(Problem::signed_out());
    }
    if OFFLINE.iter().any(|sign| said.contains(sign)) {
        return Err(Problem::offline());
    }
    let message = complaint
        .or_else(|| {
            outcome
                .stderr
                .lines()
                .map(|line| line.trim().trim_start_matches("gh: "))
                .find(|line| !line.is_empty())
                .map(str::to_owned)
        })
        .or_else(|| reply.status.map(|status| format!("HTTP {status}")))
        .or_else(|| {
            outcome
                .code
                .map(|code| format!("gh stopped with exit status {code}"))
        })
        .unwrap_or_default();
    Err(Problem::failed(&message))
}

fn retry_at(reply: &Reply, now: u64) -> u64 {
    let header = |name: &str| {
        reply
            .headers
            .get(name)
            .and_then(|value| value.parse::<u64>().ok())
    };
    header("retry-after")
        .map(|seconds| now + seconds)
        .or_else(|| header("x-ratelimit-reset").filter(|reset| *reset > now))
        .unwrap_or(now + RETRY_LATER)
}

const LOGGED_IN: &str = "Logged in to github.com account ";

pub fn auth(outcome: &Outcome) -> Result<String, Problem> {
    let login = if outcome.code == Some(0) {
        format!("{}\n{}", outcome.stdout, outcome.stderr)
            .lines()
            .find_map(logged_in)
    } else {
        None
    };
    match login {
        Some(login) => Ok(login),
        None if outcome.timed_out || unreachable(&outcome.stderr) => Err(Problem::offline()),
        None if outcome.stderr.contains("unknown flag") => Err(Problem::old_gh()),
        None => Err(Problem::signed_out()),
    }
}

fn unreachable(said: &str) -> bool {
    let said = said.to_lowercase();
    said.contains("timeout trying to log in") || OFFLINE.iter().any(|sign| said.contains(sign))
}

fn logged_in(line: &str) -> Option<String> {
    let (_, after) = line.split_once(LOGGED_IN)?;
    let login: String = after
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    (!login.is_empty()).then_some(login)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Viewer {
    pub login: String,
    pub organizations: Vec<String>,
    pub review_requested: u64,
    pub lost: Vec<String>,
}

pub fn viewer(data: &Value, pinned: &[String]) -> Result<Viewer, Problem> {
    let login = data
        .pointer("/viewer/login")
        .and_then(Value::as_str)
        .ok_or_else(|| Problem::failed("it did not say who is signed in"))?;
    let organizations = data
        .pointer("/viewer/organizations/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|node| node.get("login").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    let review_requested = data
        .pointer("/reviews/issueCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let lost = pinned
        .iter()
        .enumerate()
        .filter(|(index, _)| data.get(format!("pin{index}")).is_none_or(Value::is_null))
        .map(|(_, pin)| pin.clone())
        .collect();
    Ok(Viewer {
        login: login.to_owned(),
        organizations,
        review_requested,
        lost,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub pulls: Vec<Pull>,
    pub next: Option<String>,
}

pub fn page(data: &Value) -> Result<Page, Problem> {
    let search = data
        .get("search")
        .filter(|search| search.is_object())
        .ok_or_else(|| Problem::failed("it sent no search results"))?;
    let pulls = search
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(pull)
        .collect();
    let more = search
        .pointer("/pageInfo/hasNextPage")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let next = search
        .pointer("/pageInfo/endCursor")
        .and_then(Value::as_str)
        .filter(|_| more)
        .map(str::to_owned);
    Ok(Page { pulls, next })
}

fn text(node: &Value, pointer: &str) -> String {
    node.pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn count(node: &Value, pointer: &str) -> u64 {
    node.pointer(pointer).and_then(Value::as_u64).unwrap_or(0)
}

fn pull(node: &Value) -> Option<Pull> {
    let url = node.get("url")?.as_str()?.to_owned();
    let rollup = node
        .pointer("/commits/nodes/0/commit/statusCheckRollup")
        .filter(|rollup| rollup.is_object());
    let checks = match rollup
        .and_then(|rollup| rollup.get("state"))
        .and_then(Value::as_str)
    {
        Some("SUCCESS") => Checks::Passed,
        Some("FAILURE" | "ERROR") => Checks::Failed,
        Some("PENDING" | "EXPECTED") => Checks::Running,
        _ => Checks::None,
    };
    let contexts: Vec<&Value> = rollup
        .and_then(|rollup| rollup.pointer("/contexts/nodes"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(CONTEXTS)
        .collect();
    let named = |wanted: fn(&Value) -> bool| -> Vec<String> {
        let mut seen = HashSet::new();
        contexts
            .iter()
            .filter(|context| wanted(context))
            .filter_map(|context| {
                context
                    .get("name")
                    .or_else(|| context.get("context"))
                    .and_then(Value::as_str)
            })
            .filter(|name| seen.insert(name.to_owned()))
            .take(NAMES)
            .map(str::to_owned)
            .collect()
    };
    let counted = rollup
        .and_then(|rollup| rollup.get("contexts"))
        .map(|contexts| {
            ["checkRunCountsByState", "statusContextCountsByState"]
                .iter()
                .filter_map(|field| contexts.get(*field).and_then(Value::as_array))
                .flatten()
                .filter(|entry| entry.get("state").and_then(Value::as_str) == Some("SUCCESS"))
                .filter_map(|entry| entry.get("count").and_then(Value::as_u64))
                .sum::<u64>()
        });
    let passed = counted.filter(|sum| *sum > 0).unwrap_or_else(|| {
        contexts
            .iter()
            .filter(|context| passed_context(context))
            .count() as u64
    });
    Some(
        Pull {
            owner: text(node, "/repository/owner/login"),
            repo: text(node, "/repository/name"),
            shown: String::new(),
            number: count(node, "/number"),
            title: text(node, "/title"),
            url,
            draft: node
                .get("isDraft")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            updated_at: unix(&text(node, "/updatedAt")).unwrap_or(0),
            branch: text(node, "/headRefName"),
            base: text(node, "/baseRefName"),
            additions: count(node, "/additions"),
            deletions: count(node, "/deletions"),
            files: count(node, "/changedFiles"),
            conflicts: node.get("mergeable").and_then(Value::as_str) == Some("CONFLICTING"),
            checks,
            failed: named(failed_context),
            running: named(running_context),
            passed,
            failing: false,
            ready: false,
            status: String::new(),
            look: Look::None,
        }
        .settle(),
    )
}

fn field<'a>(context: &'a Value, name: &str) -> &'a str {
    context
        .get(name)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

fn failed_context(context: &Value) -> bool {
    match field(context, "__typename") {
        "CheckRun" => {
            field(context, "status") == "COMPLETED"
                && matches!(
                    field(context, "conclusion"),
                    "FAILURE" | "TIMED_OUT" | "CANCELLED" | "STARTUP_FAILURE" | "ACTION_REQUIRED"
                )
        }
        _ => matches!(field(context, "state"), "FAILURE" | "ERROR"),
    }
}

fn running_context(context: &Value) -> bool {
    match field(context, "__typename") {
        "CheckRun" => field(context, "status") != "COMPLETED",
        _ => matches!(field(context, "state"), "PENDING" | "EXPECTED"),
    }
}

fn passed_context(context: &Value) -> bool {
    match field(context, "__typename") {
        "CheckRun" => field(context, "conclusion") == "SUCCESS",
        _ => field(context, "state") == "SUCCESS",
    }
}

/// ```
/// use raiden_core::pulls::unix;
/// assert_eq!(unix("1970-01-02T00:00:00Z"), Some(86_400));
/// assert_eq!(unix("2024-02-29T12:30:05Z"), Some(1_709_209_805));
/// assert_eq!(unix("yesterday"), None);
/// ```
pub fn unix(text: &str) -> Option<u64> {
    let text = text.trim().trim_end_matches('Z');
    let (date, time) = text.split_once('T')?;
    let mut date = date.splitn(3, '-').map(str::parse::<i64>);
    let (year, month, day) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let mut time = time.splitn(3, ':');
    let hour: i64 = time.next()?.parse().ok()?;
    let minute: i64 = time.next()?.parse().ok()?;
    let second: f64 = time.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let shifted = if month <= 2 { year - 1 } else { year };
    let era = shifted.div_euclid(400);
    let of_era = shifted - era * 400;
    let of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    let days = era * 146_097 + of_cycle - 719_468;
    let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second as i64;
    u64::try_from(seconds).ok()
}

pub fn owners(pulls: &[Pull]) -> Vec<Owner> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for pull in pulls {
        *counts.entry(pull.owner.clone()).or_default() += 1;
    }
    let mut owners: Vec<Owner> = counts
        .into_iter()
        .map(|(name, open)| Owner { name, open })
        .collect();
    owners.sort_by(|a, b| b.open.cmp(&a.open).then_with(|| a.name.cmp(&b.name)));
    owners
}

pub fn finish(pulls: Vec<Pull>, viewer: &str, organizations: &[String]) -> Vec<Pull> {
    let mut seen = HashSet::new();
    let mut pulls: Vec<Pull> = pulls
        .into_iter()
        .filter(|pull| seen.insert(pull.url.clone()))
        .collect();
    pulls.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.url.cmp(&b.url))
    });
    let top = owners(&pulls)
        .into_iter()
        .find(|owner| {
            organizations
                .iter()
                .any(|organization| organization.eq_ignore_ascii_case(&owner.name))
        })
        .map(|owner| owner.name);
    for pull in &mut pulls {
        let plain = pull.owner.eq_ignore_ascii_case(viewer)
            || top
                .as_deref()
                .is_some_and(|top| pull.owner.eq_ignore_ascii_case(top));
        pull.shown = if plain {
            pull.repo.clone()
        } else {
            pull.full_name()
        };
    }
    pulls
}

/// ```
/// use raiden_core::pulls::age;
/// assert_eq!(age(30), "1m");
/// assert_eq!(age(5 * 60), "5m");
/// assert_eq!(age(3 * 3600 + 10), "3h");
/// assert_eq!(age(26 * 3600), "1d");
/// ```
pub fn age(seconds: u64) -> String {
    match seconds {
        0..3_600 => format!("{}m", (seconds / 60).max(1)),
        3_600..86_400 => format!("{}h", seconds / 3_600),
        _ => format!("{}d", seconds / 86_400),
    }
}

/// ```
/// use raiden_core::pulls::accessory;
/// assert_eq!(accessory(0), "");
/// assert_eq!(accessory(1), "1 needs your review");
/// assert_eq!(accessory(3), "3 need your review");
/// ```
pub fn accessory(review_requested: u64) -> String {
    match review_requested {
        0 => String::new(),
        1 => "1 needs your review".to_owned(),
        many => format!("{many} need your review"),
    }
}

pub fn snapshot(
    pulls: &[Pull],
    tab: Tab,
    only: Option<&str>,
    query: Option<&str>,
    urls: Option<&[String]>,
    now: u64,
) -> Snapshot {
    let only = only.map(str::trim).filter(|only| !only.is_empty());
    let mut label = match only {
        None => tab.label().to_owned(),
        Some(only) => format!("{} in {}", tab.label(), narrowed_name(pulls, only)),
    };
    if let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) {
        label = format!("{label} matching “{query}”");
    }
    let mut held: Vec<&Pull> = pulls
        .iter()
        .filter(|pull| {
            pull.within(only) && tab.holds(pull) && urls.is_none_or(|urls| urls.contains(&pull.url))
        })
        .collect();
    held.sort_by_key(|pull| std::cmp::Reverse(pull.updated_at));
    let room = SNAPSHOT_BYTES - 32;
    let mut lines: Vec<String> = Vec::new();
    let mut used = 0;
    for pull in &held {
        let line = snapshot_line(pull, now);
        if lines.len() == SNAPSHOT_LINES || used + line.len() + 1 > room {
            break;
        }
        used += line.len() + 1;
        lines.push(line);
    }
    let left = held.len() - lines.len();
    if held.is_empty() {
        lines.push("None.".to_owned());
    } else if left > 0 {
        lines.push(format!("and {left} more"));
    }
    Snapshot {
        label,
        text: lines.join("\n"),
    }
}

fn narrowed_name(pulls: &[Pull], only: &str) -> String {
    if only.contains('/') {
        pulls
            .iter()
            .find(|pull| pull.within(Some(only)))
            .map_or_else(|| only.to_owned(), |pull| pull.shown.clone())
    } else {
        only.to_owned()
    }
}

fn snapshot_line(pull: &Pull, now: u64) -> String {
    let state = if pull.draft { "draft" } else { "open" };
    let checks = match pull.checks {
        Checks::Passed => "checks passed".to_owned(),
        Checks::Failed if pull.failed.is_empty() => "checks failing".to_owned(),
        Checks::Failed => format!("checks failing: {}", pull.failed.join(", ")),
        Checks::Running => "checks running".to_owned(),
        Checks::None => "no checks".to_owned(),
    };
    let conflicts = if pull.conflicts { ", conflicts" } else { "" };
    format!(
        "{}#{} {} ({state}, {checks}{conflicts}, updated {} ago)",
        pull.full_name(),
        pull.number,
        pull.title,
        age(now.saturating_sub(pull.updated_at))
    )
}

/// ```
/// use raiden_core::pulls::is_repo;
/// assert!(is_repo("fernhill/beacon"));
/// assert!(is_repo("kestrel-io/site.github.io"));
/// assert!(!is_repo("fernhill"));
/// assert!(!is_repo("fern hill/beacon"));
/// ```
pub fn is_repo(text: &str) -> bool {
    let Some((owner, name)) = text.split_once('/') else {
        return false;
    };
    !owner.is_empty()
        && !name.is_empty()
        && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

pub fn pinned_after(pinned: &[String], repo: &str, pin: bool) -> Result<Vec<String>, String> {
    let repo = repo.trim();
    if !is_repo(repo) {
        return Err(format!(
            "{repo:?} is not a repository name such as \"owner/repo\""
        ));
    }
    let mut after: Vec<String> = pinned
        .iter()
        .filter(|kept| !kept.eq_ignore_ascii_case(repo))
        .cloned()
        .collect();
    if pin {
        after.push(repo.to_owned());
    }
    Ok(after)
}

pub fn read_cache(folder: &Path) -> Option<Cache> {
    let text = std::fs::read_to_string(folder.join(FOLDER).join(CACHE)).ok()?;
    serde_json::from_str(&text).ok()
}

#[cfg(unix)]
pub fn write_cache(folder: &Path, cache: &Cache) -> std::io::Result<()> {
    let text = serde_json::to_string(cache).map_err(std::io::Error::other)?;
    write_private(folder, CACHE, text.as_bytes())
}

#[cfg(unix)]
pub fn write_private(folder: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _, PermissionsExt as _};

    let folder = folder.join(FOLDER);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&folder)?;
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700))?;
    let target = folder.join(name);
    let temporary = folder.join(format!("{name}.writing"));
    let _ = std::fs::remove_file(&temporary);
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        })
        .and_then(|()| std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600)))
        .and_then(|()| std::fs::rename(&temporary, &target));
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written
}

pub mod avatar;

#[cfg(test)]
mod tests;
