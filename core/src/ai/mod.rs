pub mod environment;
pub mod grammar;
pub mod secrets;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const INSTRUCTIONS: &str = "You are the assistant inside Raiden, a launcher on the user's Mac. \
Answer the question directly and keep the answer short. Light Markdown is fine: \
links, lists, bold and code, but no tables or HTML. Use the attached \
context when it helps. Search the web when the question needs current or outside facts, \
without saying that you are searching, and name the sites you used. You cannot run other tools \
or open files.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Claude,
    Chatgpt,
}

impl Provider {
    pub const ALL: [Provider; 2] = [Provider::Claude, Provider::Chatgpt];
    pub const DEFAULT: Provider = Provider::Claude;

    /// ```
    /// use raiden_core::ai::Provider;
    /// assert_eq!(Provider::from_id("chatgpt"), Some(Provider::Chatgpt));
    /// assert_eq!(Provider::from_id("gemini"), None);
    /// ```
    pub fn from_id(id: &str) -> Option<Provider> {
        Provider::ALL
            .into_iter()
            .find(|provider| provider.id() == id)
    }

    pub fn id(self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::Chatgpt => "chatgpt",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Provider::Claude => "Claude",
            Provider::Chatgpt => "ChatGPT",
        }
    }

    pub fn through(self) -> &'static str {
        match self {
            Provider::Claude => "Claude Code",
            Provider::Chatgpt => "Codex",
        }
    }

    pub fn program(self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::Chatgpt => "codex",
        }
    }

    pub fn adapter(self) -> &'static str {
        match self {
            Provider::Claude => "claude-agent-acp",
            Provider::Chatgpt => "codex-acp",
        }
    }

    pub fn executable_variable(self) -> &'static str {
        match self {
            Provider::Claude => "CLAUDE_CODE_EXECUTABLE",
            Provider::Chatgpt => "CODEX_PATH",
        }
    }

    pub fn status_args(self) -> &'static [&'static str] {
        match self {
            Provider::Claude => &["auth", "status", "--json"],
            Provider::Chatgpt => &["login", "status"],
        }
    }

    pub fn sign_in_args(self) -> &'static [&'static str] {
        match self {
            Provider::Claude => &["auth", "login", "--claudeai"],
            Provider::Chatgpt => &["login"],
        }
    }

    pub fn effort_per_process(self) -> bool {
        matches!(self, Provider::Chatgpt)
    }

    pub fn model(self) -> &'static str {
        match self {
            Provider::Claude => "haiku",
            Provider::Chatgpt => CODEX_MODEL,
        }
    }

    /// ```
    /// use raiden_core::ai::Provider;
    /// assert!(Provider::Claude.signed_in(false, r#"{"loggedIn": true, "authMethod": "claude.ai"}"#));
    /// assert!(!Provider::Claude.signed_in(true, r#"{"loggedIn": false, "authMethod": "none"}"#));
    /// assert!(!Provider::Claude.signed_in(true, r#"{"loggedIn": true, "authMethod": "console"}"#));
    /// assert!(Provider::Chatgpt.signed_in(true, "Logged in using ChatGPT"));
    /// assert!(!Provider::Chatgpt.signed_in(true, "Logged in using an API key - sk-***"));
    /// assert!(!Provider::Chatgpt.signed_in(false, "Not logged in"));
    /// ```
    pub fn signed_in(self, succeeded: bool, output: &str) -> bool {
        match self {
            Provider::Claude => serde_json::from_str::<serde_json::Value>(output)
                .ok()
                .is_some_and(|status| {
                    status.get("loggedIn").and_then(serde_json::Value::as_bool) == Some(true)
                        && status.get("authMethod").and_then(serde_json::Value::as_str)
                            == Some("claude.ai")
                }),
            Provider::Chatgpt => succeeded && output.contains("Logged in using ChatGPT"),
        }
    }

    pub fn signed_out_message(self) -> String {
        format!(
            "{} is signed out. Sign in to {} to ask.",
            self.through(),
            self.through()
        )
    }
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    #[default]
    Low,
    Medium,
    High,
}

impl Effort {
    pub const ALL: [Effort; 3] = [Effort::Low, Effort::Medium, Effort::High];

    /// ```
    /// use raiden_core::ai::Effort;
    /// assert_eq!(Effort::from_id("high"), Some(Effort::High));
    /// assert_eq!(Effort::from_id("High"), None);
    /// assert_eq!(Effort::from_id("max"), None);
    /// ```
    pub fn from_id(id: &str) -> Option<Effort> {
        Effort::ALL.into_iter().find(|effort| effort.id() == id)
    }

    pub fn id(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
        }
    }

    pub fn claude(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
        }
    }

    pub fn codex(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: Provider,
    pub name: String,
    pub through: String,
    pub signed_in: bool,
}

impl Account {
    pub fn new(provider: Provider, signed_in: bool) -> Self {
        Self {
            id: provider,
            name: provider.name().to_owned(),
            through: provider.through().to_owned(),
            signed_in,
        }
    }

    /// ```
    /// use raiden_core::ai::{Account, Provider};
    /// assert_eq!(Account::new(Provider::Claude, true).line(), "Claude, through Claude Code: signed in");
    /// assert_eq!(Account::new(Provider::Chatgpt, false).line(), "ChatGPT, through Codex: signed out");
    /// ```
    pub fn line(&self) -> String {
        let state = if self.signed_in {
            "signed in"
        } else {
            "signed out"
        };
        format!("{}, through {}: {state}", self.name, self.through)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tag {
    pub kind: String,
    pub label: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    pub text: String,
    pub hidden: BTreeMap<String, usize>,
}

/// ```
/// use raiden_core::ai::{prepare, Tag};
/// let tag = Tag { kind: "selection".into(), label: "Selection from Zed".into(), text: "fn main() {}".into() };
/// let prepared = prepare("What does this do?", &[tag]);
/// assert!(prepared.text.contains("fn main() {}"));
/// assert!(prepared.text.ends_with("What does this do?"));
/// assert!(prepared.hidden.is_empty());
/// ```
pub fn prepare(question: &str, context: &[Tag]) -> Prepared {
    let mut counts: BTreeMap<secrets::Kind, usize> = BTreeMap::new();
    let mut hide = |text: &str| {
        let hidden = secrets::hide(text);
        for (kind, count) in hidden.counts {
            *counts.entry(kind).or_default() += count;
        }
        hidden.text
    };
    let mut parts: Vec<String> = context
        .iter()
        .map(|tag| {
            let kind = attribute(&hide(&tag.kind));
            let label = attribute(&hide(&tag.label));
            format!(
                "<context kind=\"{kind}\" label=\"{label}\">\n{}\n</context>",
                hide(&tag.text)
            )
        })
        .collect();
    parts.push(hide(question));
    Prepared {
        text: parts.join("\n\n"),
        hidden: counts
            .into_iter()
            .map(|(kind, count)| (kind.id().to_owned(), count))
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exchange {
    pub asked: String,
    pub answer: String,
}

/// ```
/// use raiden_core::ai::{prepare, replay, Exchange};
/// let earlier = [Exchange { asked: "Pick a number.".into(), answer: "7".into() }];
/// let sent = replay(&earlier, prepare("Double it.", &[]));
/// assert!(sent.text.find("Pick a number.") < sent.text.find("Double it."));
/// assert!(sent.text.ends_with("Double it."));
/// assert_eq!(replay(&[], prepare("Hi", &[])).text, "Hi");
/// ```
pub fn replay(earlier: &[Exchange], next: Prepared) -> Prepared {
    if earlier.is_empty() {
        return next;
    }
    let mut counts: BTreeMap<String, usize> = next.hidden;
    let mut hide = |text: &str| {
        let hidden = secrets::hide(text);
        for (kind, count) in hidden.counts {
            *counts.entry(kind.id().to_owned()).or_default() += count;
        }
        hidden.text
    };
    let mut room = REPLAY_LONGEST;
    let mut turns: Vec<String> = Vec::new();
    for exchange in earlier.iter().rev() {
        let turn = format!(
            "<turn>\n<question>\n{}\n</question>\n<answer>\n{}\n</answer>\n</turn>",
            clipped_start(&hide(&exchange.asked)),
            clipped(&hide(&exchange.answer))
        );
        let size = turn.chars().count();
        if size > room {
            break;
        }
        room -= size;
        turns.push(turn);
    }
    turns.reverse();
    let text = format!(
        "{REPLAY_LEAD}\n<earlier>\n{}\n</earlier>\n\n{}",
        turns.join("\n"),
        next.text
    );
    Prepared {
        text,
        hidden: counts,
    }
}

pub const REPLAY_LONGEST: usize = 60_000;
const TURN_LONGEST: usize = 20_000;

fn clipped(text: &str) -> &str {
    text.char_indices()
        .nth(TURN_LONGEST)
        .map_or(text, |(at, _)| &text[..at])
}

fn clipped_start(text: &str) -> &str {
    let count = text.chars().count();
    text.char_indices()
        .nth(count.saturating_sub(TURN_LONGEST))
        .map_or(text, |(at, _)| &text[at..])
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    pub asked: Prepared,
    pub sent: Prepared,
}

/// ```
/// use raiden_core::ai::{outgoing, Exchange, Tag};
/// let tag = Tag { kind: "selection".into(), label: "Selection from Zed".into(), text: "SELECT 1".into() };
/// let first = outgoing(&[], true, "Why?", &[tag.clone()]);
/// assert!(first.sent.text.contains("SELECT 1"));
/// let earlier = [Exchange { asked: first.asked.text.clone(), answer: "Because.".into() }];
/// let held = outgoing(&earlier, false, "And then?", &[tag.clone()]);
/// assert_eq!(held.sent.text, "And then?");
/// let replayed = outgoing(&earlier, true, "And then?", &[tag]);
/// assert_eq!(replayed.sent.text.matches("SELECT 1").count(), 1);
/// assert!(replayed.sent.text.contains("Because.") && replayed.sent.text.ends_with("And then?"));
/// assert_eq!(replayed.asked.text, "And then?");
/// ```
pub fn outgoing(earlier: &[Exchange], fresh: bool, question: &str, context: &[Tag]) -> Outgoing {
    let first = earlier.is_empty();
    let asked = prepare(question, if first { context } else { &[] });
    let sent = if fresh {
        replay(earlier, asked.clone())
    } else {
        asked.clone()
    };
    Outgoing { asked, sent }
}

const REPLAY_LEAD: &str = "This continues a conversation whose earlier turns are below. \
Answer only the last question, using them as context.";

fn attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnswerKind {
    Delta,
    Searching,
    Done,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    pub id: String,
    pub kind: AnswerKind,
    pub text: String,
    pub model: String,
    pub hidden: BTreeMap<String, usize>,
}

const WEB_SEARCH: &str = "WebSearch";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Web {
    Searchable,
    Off,
}

pub const PULLS_TAG: &str = "pulls";

/// ```
/// use raiden_core::ai::{web_for, Tag, Web};
/// let tag = |kind: &str| Tag { kind: kind.into(), label: "Open pull requests".into(), text: "fernhill/beacon#1".into() };
/// assert_eq!(web_for(&[]), Web::Searchable);
/// assert_eq!(web_for(&[tag("selection")]), Web::Searchable);
/// assert_eq!(web_for(&[tag("selection"), tag("pulls")]), Web::Off);
/// ```
pub fn web_for(context: &[Tag]) -> Web {
    if context.iter().any(|tag| tag.kind == PULLS_TAG) {
        Web::Off
    } else {
        Web::Searchable
    }
}

/// ```
/// use raiden_core::ai::{claude_options, Effort, Web};
/// let options = claude_options("haiku", Web::Searchable, Effort::Low);
/// assert_eq!(options["claudeCode"]["options"]["tools"], serde_json::json!(["WebSearch"]));
/// assert_eq!(options["claudeCode"]["options"]["allowedTools"], serde_json::json!(["WebSearch"]));
/// assert_eq!(options["claudeCode"]["options"]["settingSources"], serde_json::json!([]));
/// let closed = claude_options("haiku", Web::Off, Effort::Low);
/// assert_eq!(closed["claudeCode"]["options"]["tools"], serde_json::json!([]));
/// assert_eq!(closed["claudeCode"]["options"]["allowedTools"], serde_json::json!([]));
/// ```
pub fn claude_options(model: &str, web: Web, effort: Effort) -> serde_json::Value {
    let tools: &[&str] = match web {
        Web::Searchable => &[WEB_SEARCH],
        Web::Off => &[],
    };
    let options = serde_json::json!({
        "tools": tools,
        "allowedTools": tools,
        "settingSources": [],
        "strictMcpConfig": true,
        "mcpServers": {},
        "persistSession": false,
        "promptSuggestions": false,
        "effort": effort.claude(),
        "extraArgs": { "disable-slash-commands": null },
        "model": model,
    });
    serde_json::json!({ "claudeCode": { "options": options } })
}

pub const CODEX_MODEL: &str = "gpt-6-luna";

const CODEX_FEATURES_OFF: [&str; 15] = [
    "multi_agent",
    "code_mode_host",
    "plugins",
    "apps",
    "skill_search",
    "shell_tool",
    "unified_exec",
    "sleep_tool",
    "goals",
    "image_generation",
    "browser_use",
    "computer_use",
    "tool_suggest",
    "send_message_to_user_async",
    "default_mode_request_user_input",
];

/// ```
/// use raiden_core::ai::{codex_config, Effort, Web};
/// let config = codex_config("/tmp/instructions.md", Effort::Low, Web::Searchable);
/// assert_eq!(config["model_instructions_file"], "/tmp/instructions.md");
/// assert_eq!(config["features"]["shell_tool"], false);
/// assert_eq!(config["web_search"], "cached");
/// assert_eq!(config["model"], "gpt-6-luna");
/// let closed = codex_config("/tmp/instructions.md", Effort::Low, Web::Off);
/// assert_eq!(closed["web_search"], "disabled");
/// ```
pub fn codex_config(instructions_file: &str, effort: Effort, web: Web) -> serde_json::Value {
    let features: serde_json::Map<String, serde_json::Value> = CODEX_FEATURES_OFF
        .iter()
        .map(|feature| ((*feature).to_owned(), false.into()))
        .collect();
    serde_json::json!({
        "model_instructions_file": instructions_file,
        "model": CODEX_MODEL,
        "model_reasoning_effort": effort.codex(),
        "features": features,
        "web_search": match web {
            Web::Searchable => "cached",
            Web::Off => "disabled",
        },
        "agents": { "enabled": false },
        "project_doc_max_bytes": 0,
        "include_permissions_instructions": false,
        "include_apps_instructions": false,
        "include_collaboration_mode_instructions": false,
        "include_environment_context": false,
        "skills": { "include_instructions": false },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_secrets_never_reach_the_prepared_text() {
        let fakes = secrets::tests::fakes();
        let selection = fakes
            .iter()
            .map(|(_, fake)| fake.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let tag = Tag {
            kind: "selection".into(),
            label: "Selection from Zed".into(),
            text: selection,
        };
        let prepared = prepare("What are these?", &[tag]);
        for (kind, fake) in &fakes {
            let secret = secrets::tests::secret_part(*kind, fake);
            assert!(!prepared.text.contains(&secret), "{fake} leaked");
        }
        assert_eq!(prepared.hidden.values().sum::<usize>(), 10);
        assert_eq!(prepared.hidden.len(), 10);
    }

    #[test]
    fn secrets_in_labels_are_hidden_too() {
        let (kind, fake) = secrets::tests::fakes().swap_remove(1);
        let tag = Tag {
            kind: "window".into(),
            label: format!("export OPENAI_API_KEY={fake}"),
            text: "echo".into(),
        };
        let prepared = prepare("q", &[tag]);
        assert!(!prepared.text.contains(&fake));
        assert_eq!(prepared.hidden.get(kind.id()), Some(&1));
    }

    #[test]
    fn a_removed_tag_is_absent_and_order_is_kept() {
        let tag = |label: &str, text: &str| Tag {
            kind: "window".into(),
            label: label.into(),
            text: text.into(),
        };
        let prepared = prepare("q", &[tag("First", "one"), tag("Second", "two")]);
        assert!(prepared.text.find("one") < prepared.text.find("two"));
        let without = prepare("q", &[tag("Second", "two")]);
        assert!(!without.text.contains("one"));
    }

    #[test]
    fn a_replayed_conversation_keeps_its_turns_in_order_with_secrets_hidden() {
        let fakes = secrets::tests::fakes();
        let (asked_kind, asked_fake) = &fakes[0];
        let (answer_kind, answer_fake) = &fakes[1];
        let earlier = [
            Exchange {
                asked: format!("Is {asked_fake} valid?"),
                answer: format!("It looks like {answer_fake}."),
            },
            Exchange {
                asked: "Where do keys go?".into(),
                answer: "In the environment.".into(),
            },
        ];
        let tag = Tag {
            kind: "selection".into(),
            label: "Selection from Zed".into(),
            text: "fn main() {}".into(),
        };
        let sent = replay(&earlier, prepare("And in CI?", &[tag]));
        for (kind, fake) in [(asked_kind, asked_fake), (answer_kind, answer_fake)] {
            let secret = secrets::tests::secret_part(*kind, fake);
            assert!(!sent.text.contains(&secret), "{kind:?} leaked");
            assert_eq!(sent.hidden.get(kind.id()), Some(&1));
        }
        let at = |part: &str| sent.text.find(part).unwrap_or(usize::MAX);
        assert!(at("Is [hidden ") < at("Where do keys go?"));
        assert!(at("In the environment.") < at("fn main() {}"));
        assert!(sent.text.ends_with("And in CI?"));
        assert_eq!(sent.text.matches("<turn>").count(), 2);
    }

    #[test]
    fn a_long_conversation_replays_only_its_newest_turns_that_fit() {
        let earlier: Vec<Exchange> = (0..10)
            .map(|at| Exchange {
                asked: format!("question {at}"),
                answer: "é".repeat(25_000),
            })
            .collect();
        let sent = replay(&earlier, prepare("last?", &[]));
        assert!(sent.text.chars().count() < REPLAY_LONGEST + 1_000);
        assert!(sent.text.contains("question 9") && sent.text.contains("question 8"));
        assert!(!sent.text.contains("question 7"));
        assert!(sent.text.ends_with("last?"));
    }

    #[test]
    fn a_huge_selection_never_pushes_the_question_out_of_a_replay() {
        let tag = Tag {
            kind: "selection".into(),
            label: "Selection from Zed".into(),
            text: "x".repeat(50_000),
        };
        let first = prepare("What is this?", &[tag]);
        let earlier = [Exchange {
            asked: first.text,
            answer: "A lot of x.".into(),
        }];
        let sent = replay(&earlier, prepare("Why?", &[]));
        assert!(sent.text.contains("What is this?\n</question>"));
        assert!(sent.text.contains("A lot of x."));
    }

    #[test]
    fn tags_go_only_with_the_question_the_tool_sees_first() {
        let tag = Tag {
            kind: "window".into(),
            label: "AGENTS.md in Zed".into(),
            text: "AGENTS.md in Zed".into(),
        };
        let earlier = [Exchange {
            asked: "Hi".into(),
            answer: "Hello".into(),
        }];
        for fresh in [true, false] {
            assert!(
                outgoing(&[], fresh, "q", std::slice::from_ref(&tag))
                    .sent
                    .text
                    .contains("AGENTS.md")
            );
            let later = outgoing(&earlier, fresh, "q", std::slice::from_ref(&tag));
            assert!(!later.sent.text.contains("AGENTS.md"));
            assert_eq!(later.asked.text, "q");
        }
    }

    #[test]
    fn labels_cannot_break_out_of_their_tag() {
        let tag = Tag {
            kind: "selection".into(),
            label: "a\" kind=\"x".into(),
            text: String::new(),
        };
        assert!(prepare("q", &[tag]).text.contains("a&quot; kind=&quot;x"));
    }

    #[test]
    fn each_engine_gets_the_effort_it_names_the_same_way() {
        let expected = [
            (Effort::Low, "low"),
            (Effort::Medium, "medium"),
            (Effort::High, "high"),
        ];
        for (effort, level) in expected {
            for web in [Web::Searchable, Web::Off] {
                let options = claude_options("haiku", web, effort);
                assert_eq!(
                    options["claudeCode"]["options"]["effort"], level,
                    "{effort:?}"
                );
                assert_eq!(options["claudeCode"]["options"]["model"], "haiku");
            }
            let config = codex_config("/tmp/instructions.md", effort, Web::Off);
            assert_eq!(config["model_reasoning_effort"], level, "{effort:?}");
            assert_eq!(config["model"], CODEX_MODEL);
            assert_eq!(serde_json::to_value(effort).ok(), Some(level.into()));
            assert_eq!(Effort::from_id(level), Some(effort));
        }
        assert_eq!(Effort::default(), Effort::Low);
    }

    #[test]
    fn only_codex_fixes_its_effort_when_it_starts() {
        assert!(Provider::Chatgpt.effort_per_process());
        assert!(!Provider::Claude.effort_per_process());
    }

    #[test]
    fn answers_serialize_as_the_screen_reads_them() {
        let answer = Answer {
            id: "1".into(),
            kind: AnswerKind::Delta,
            text: "Hi".into(),
            model: "claude-haiku".into(),
            hidden: BTreeMap::from([("openai_api_key".into(), 1)]),
        };
        assert_eq!(
            serde_json::to_value(answer).ok(),
            Some(serde_json::json!({
                "id": "1", "kind": "delta", "text": "Hi", "model": "claude-haiku",
                "hidden": { "openai_api_key": 1 }
            }))
        );
        let account = serde_json::to_value(Account::new(Provider::Chatgpt, true)).ok();
        assert_eq!(
            account,
            Some(serde_json::json!({
                "id": "chatgpt", "name": "ChatGPT", "through": "Codex", "signedIn": true
            }))
        );
    }
}
