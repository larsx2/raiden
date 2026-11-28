use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const FOLDER: &str = "slack";
pub const CACHE: &str = "slack.json";
pub const FRESH_FOR: u64 = 60;
pub const KEYCHAIN_SERVICE: &str = "raiden-slack";
pub const KEYCHAIN_ACCOUNT: &str = "user-token";
pub const API: &str = "https://slack.com/api";
pub const MANIFEST: &str = include_str!("../../assets/slack/manifest.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Person,
    Channel,
    Group,
    Message,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub subtitle: String,
    pub unread: u64,
    pub mention: bool,
    pub updated: u64,
    pub link: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProblemKind {
    NoToken,
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

    pub fn no_token() -> Self {
        Self::new(
            ProblemKind::NoToken,
            "Raiden has no Slack token yet. Open Slack settings to paste your User OAuth Token.",
        )
    }

    pub fn signed_out() -> Self {
        Self::new(
            ProblemKind::SignedOut,
            "Slack rejected the token. Reinstall the Raiden app in Slack and paste a fresh User OAuth Token.",
        )
    }

    pub fn offline() -> Self {
        Self::new(ProblemKind::Offline, "Raiden could not reach Slack.")
    }

    pub fn limited(retry_at: u64) -> Self {
        Self {
            kind: ProblemKind::Limited,
            text: "Slack's limit on requests is used up for now.".to_owned(),
            retry_at: Some(retry_at),
        }
    }

    pub fn failed(detail: &str) -> Self {
        let detail = detail.trim().trim_end_matches('.');
        Self::new(
            ProblemKind::Failed,
            if detail.is_empty() {
                "Slack did not answer as expected.".to_owned()
            } else {
                format!("Slack did not answer as expected: {detail}.")
            },
        )
    }
}

/// Turns a Slack reply's `ok`/`error` into a problem, or passes it through.
///
/// ```
/// use raiden_core::slack::{check, ProblemKind};
/// use serde_json::json;
/// assert!(check(&json!({"ok": true}), 0).is_ok());
/// assert_eq!(check(&json!({"ok": false, "error": "invalid_auth"}), 0).unwrap_err().kind, ProblemKind::SignedOut);
/// ```
pub fn check(value: &Value, now: u64) -> Result<(), Problem> {
    if value.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(());
    }
    let error = value.get("error").and_then(Value::as_str).unwrap_or("");
    Err(match error {
        "invalid_auth" | "not_authed" | "token_revoked" | "account_inactive" | "token_expired" => {
            Problem::signed_out()
        }
        "ratelimited" | "rate_limited" => Problem::limited(now + 60),
        "" => Problem::failed("an empty reply"),
        other => Problem::failed(&other.replace('_', " ")),
    })
}

/// Whether a typed query is long enough to search message text, not just names.
///
/// Mirrors the launcher's file search, which waits for three letters.
///
/// ```
/// assert!(!raiden_core::slack::wants_messages("hi"));
/// assert!(raiden_core::slack::wants_messages("ship"));
/// ```
pub fn wants_messages(query: &str) -> bool {
    query.trim().chars().count() >= 3
}

/// Keeps the people, channels and groups whose name or handle contains the query.
pub fn by_name(hits: Vec<Hit>, query: &str) -> Vec<Hit> {
    let needle = query.trim().to_lowercase();
    hits.into_iter()
        .filter(|hit| matches_name(hit, &needle))
        .collect()
}

/// Whether a hit's name or handle contains the already-lowercased needle.
///
/// An empty needle matches everything, so the whole catalog shows before the
/// person types. The native side calls this on borrowed catalog hits, cloning
/// only the matches, so a keystroke never copies hundreds of unmatched rows.
pub fn matches_name(hit: &Hit, needle: &str) -> bool {
    needle.is_empty()
        || hit.title.to_lowercase().contains(needle)
        || hit.subtitle.to_lowercase().contains(needle)
}

/// The `slack://` deep link that opens a conversation in the desktop app.
///
/// ```
/// use raiden_core::slack::channel_link;
/// assert_eq!(channel_link("T1", "C9"), "slack://channel?team=T1&id=C9");
/// ```
pub fn channel_link(team: &str, id: &str) -> String {
    format!("slack://channel?team={team}&id={id}")
}

/// The `slack://` deep link that opens a person's direct message.
///
/// ```
/// use raiden_core::slack::user_link;
/// assert_eq!(user_link("T1", "U7"), "slack://user?team=T1&id=U7");
/// ```
pub fn user_link(team: &str, id: &str) -> String {
    format!("slack://user?team={team}&id={id}")
}

fn text(node: &Value, pointer: &str) -> String {
    node.pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn seconds(ts: &str) -> u64 {
    ts.split('.').next().unwrap_or(ts).parse().unwrap_or(0)
}

/// People from a `users.list` reply, skipping bots, apps and deactivated accounts.
pub fn people(value: &Value, team: &str) -> Vec<Hit> {
    members(value)
        .iter()
        .filter(|m| {
            !m.get("is_bot").and_then(Value::as_bool).unwrap_or(false)
                && !m.get("deleted").and_then(Value::as_bool).unwrap_or(false)
                && m.get("id").and_then(Value::as_str) != Some("USLACKBOT")
        })
        .filter_map(|m| person(m, team))
        .collect()
}

fn members(value: &Value) -> Vec<Value> {
    value
        .get("members")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn person(m: &Value, team: &str) -> Option<Hit> {
    let id = m.get("id").and_then(Value::as_str)?.to_owned();
    let real = text(m, "/profile/real_name");
    let display = text(m, "/profile/display_name");
    let handle = text(m, "/name");
    let title = first_nonempty([&real, &display, &handle]);
    let at = if handle.is_empty() {
        String::new()
    } else {
        format!("@{handle}")
    };
    Some(Hit {
        link: user_link(team, &id),
        id,
        kind: Kind::Person,
        title,
        subtitle: at,
        unread: 0,
        mention: false,
        updated: 0,
    })
}

/// Channels and groups from a `conversations.list` reply.
pub fn conversations(value: &Value, team: &str, names: &Names) -> Vec<Hit> {
    value
        .get("channels")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|c| conversation(c, team, names))
        .collect()
}

/// Looks a user id up to a display name, for naming direct messages.
#[derive(Debug, Default, Clone)]
pub struct Names(std::collections::HashMap<String, String>);

impl Names {
    pub fn from_people(people: &[Hit]) -> Self {
        Self(
            people
                .iter()
                .map(|p| (p.id.clone(), p.title.clone()))
                .collect(),
        )
    }

    fn name(&self, id: &str) -> String {
        self.0.get(id).cloned().unwrap_or_else(|| id.to_owned())
    }
}

fn conversation(c: &Value, team: &str, names: &Names) -> Option<Hit> {
    let id = c.get("id").and_then(Value::as_str)?.to_owned();
    let is_im = c.get("is_im").and_then(Value::as_bool).unwrap_or(false);
    let is_mpim = c.get("is_mpim").and_then(Value::as_bool).unwrap_or(false);
    let private = c
        .get("is_private")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (kind, title, subtitle) = if is_im {
        let who = names.name(&text(c, "/user"));
        (Kind::Person, who, String::new())
    } else if is_mpim {
        let title = text(c, "/name").replace("mpdm-", "").replace("--", ", ");
        (Kind::Group, title, "group".to_owned())
    } else {
        let hash = format!("#{}", text(c, "/name"));
        let note = if private { "private" } else { "channel" };
        (Kind::Channel, hash, note.to_owned())
    };
    let unread = c
        .get("unread_count_display")
        .and_then(Value::as_u64)
        .or_else(|| c.get("unread_count").and_then(Value::as_u64))
        .unwrap_or(0);
    Some(Hit {
        link: if is_im {
            user_link(team, &text(c, "/user"))
        } else {
            channel_link(team, &id)
        },
        id,
        kind,
        title,
        subtitle,
        unread,
        mention: is_im || is_mpim,
        updated: seconds(&text(c, "/latest/ts")),
    })
}

/// Message matches from a `search.messages` reply.
pub fn messages(value: &Value) -> Vec<Hit> {
    value
        .pointer("/messages/matches")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(message)
        .collect()
}

fn message(m: &Value) -> Option<Hit> {
    let permalink = m.get("permalink").and_then(Value::as_str)?.to_owned();
    let who = first_nonempty([&text(m, "/username"), &text(m, "/user")]);
    let channel = text(m, "/channel/name");
    let where_ = if channel.is_empty() {
        String::new()
    } else {
        format!("#{channel}")
    };
    let body = text(m, "/text").replace('\n', " ");
    Some(Hit {
        id: text(m, "/ts"),
        kind: Kind::Message,
        title: body,
        subtitle: [who, where_]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · "),
        unread: 0,
        mention: false,
        updated: seconds(&text(m, "/ts")),
        link: permalink,
    })
}

fn first_nonempty<'a>(options: impl IntoIterator<Item = &'a String>) -> String {
    options
        .into_iter()
        .find(|s| !s.is_empty())
        .cloned()
        .unwrap_or_default()
}

/// Orders a typed search: a name that is the query, then one that starts with
/// it, then any other name match, all before messages; then unreads, direct
/// mentions and recency.
///
/// Within equal priority the earlier arrival is kept.
pub fn rank(mut hits: Vec<Hit>, query: &str) -> Vec<Hit> {
    let needle = query.trim().trim_start_matches('#').to_lowercase();
    let name_fit = |h: &Hit| {
        if h.kind == Kind::Message || needle.is_empty() {
            return 0;
        }
        let name = h.title.trim_start_matches('#').to_lowercase();
        if name == needle {
            3
        } else if name.starts_with(&needle) {
            2
        } else {
            1
        }
    };
    hits.sort_by_cached_key(|h| {
        std::cmp::Reverse((
            name_fit(h),
            h.kind != Kind::Message,
            h.unread > 0,
            h.mention,
            h.unread,
            h.updated,
        ))
    });
    hits
}

/// When you last wrote in each conversation, by conversation id, from a
/// `search.messages` reply for `from:me`. `conversations.list` carries no
/// latest message, so this is where recency comes from.
pub fn last_written(value: &Value) -> std::collections::HashMap<String, u64> {
    let mut latest = std::collections::HashMap::new();
    for m in value
        .pointer("/messages/matches")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let id = text(m, "/channel/id");
        if id.is_empty() {
            continue;
        }
        let at = seconds(&text(m, "/ts"));
        let kept = latest.entry(id).or_insert(0);
        *kept = (*kept).max(at);
    }
    latest
}

/// Copies those times onto the conversations they belong to.
pub fn with_recency(
    mut hits: Vec<Hit>,
    latest: &std::collections::HashMap<String, u64>,
) -> Vec<Hit> {
    for hit in &mut hits {
        if let Some(&at) = latest.get(&hit.id) {
            hit.updated = hit.updated.max(at);
        }
    }
    hits
}

/// The order with nothing typed: unreads first, then your latest conversations.
pub fn latest_first(mut hits: Vec<Hit>) -> Vec<Hit> {
    hits.sort_by(|a, b| {
        let key = |h: &Hit| (h.unread > 0, h.updated);
        key(b).cmp(&key(a))
    });
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_channel_named_what_you_typed_comes_first_and_messages_come_last() {
        let mut message = hit(0, false, 999, "talk about ifa");
        message.kind = Kind::Message;
        let mut person = hit(4, true, 900, "Clifford");
        person.kind = Kind::Person;
        let ranked = rank(
            vec![
                message,
                person,
                hit(0, false, 0, "#ifa-ops"),
                hit(0, false, 0, "#ifa"),
            ],
            "ifa",
        );
        let titles: Vec<&str> = ranked.iter().map(|h| h.title.as_str()).collect();
        assert_eq!(titles, ["#ifa", "#ifa-ops", "Clifford", "talk about ifa"]);
    }

    #[test]
    fn the_exact_name_beats_unreads_and_a_typed_hash_or_case_does_not_matter() {
        let busy = hit(9, true, 999, "#ifa-alerts");
        let ranked = rank(vec![busy, hit(0, false, 0, "#IFA")], " #ifa ");
        assert_eq!(ranked[0].title, "#IFA");
        // Nothing typed: no name wins, so unreads lead.
        let ranked = rank(vec![hit(0, false, 0, "#a"), hit(1, false, 0, "#b")], "");
        assert_eq!(ranked[0].title, "#b");
    }

    #[test]
    fn a_conversation_you_never_wrote_in_keeps_its_own_time() {
        let latest = last_written(&json!({"ok": false}));
        assert!(latest.is_empty());
        let mut kept = hit(0, false, 500, "kept");
        kept.id = "C9".to_owned();
        let older = std::collections::HashMap::from([("C9".to_owned(), 100)]);
        assert_eq!(with_recency(vec![kept], &older)[0].updated, 500);
    }

    #[test]
    fn with_nothing_typed_your_latest_conversations_come_first() {
        let reply = json!({"messages": {"matches": [
            {"ts": "1700000300.000100", "channel": {"id": "C2"}},
            {"ts": "1700000100.000100", "channel": {"id": "D1"}},
            {"ts": "1700000200.000100", "channel": {"id": "D1"}},
            {"ts": "1700000900.000100", "channel": {}},
        ]}});
        let latest = last_written(&reply);
        assert_eq!(latest.get("D1"), Some(&1_700_000_200));
        assert_eq!(latest.len(), 2);
        let mut dm = hit(0, true, 0, "dm");
        dm.id = "D1".to_owned();
        let mut channel = hit(0, false, 0, "channel");
        channel.id = "C2".to_owned();
        let quiet = hit(0, true, 0, "quiet-dm");
        let unread = hit(2, false, 0, "unread");
        let ordered = latest_first(with_recency(vec![quiet, dm, channel, unread], &latest));
        let titles: Vec<&str> = ordered.iter().map(|h| h.title.as_str()).collect();
        assert_eq!(titles, ["unread", "channel", "dm", "quiet-dm"]);
    }

    fn hit(unread: u64, mention: bool, updated: u64, title: &str) -> Hit {
        Hit {
            id: title.to_owned(),
            kind: Kind::Channel,
            title: title.to_owned(),
            subtitle: String::new(),
            unread,
            mention,
            updated,
            link: String::new(),
        }
    }

    #[test]
    fn unreads_come_before_read_conversations() {
        let ranked = rank(
            vec![
                hit(0, false, 500, "read-recent"),
                hit(3, false, 100, "unread-old"),
            ],
            "",
        );
        assert_eq!(ranked[0].title, "unread-old");
    }

    #[test]
    fn a_mentioned_dm_beats_an_unread_channel() {
        let ranked = rank(
            vec![hit(5, false, 100, "channel"), hit(1, true, 100, "dm")],
            "",
        );
        assert_eq!(ranked[0].title, "dm");
    }

    #[test]
    fn read_conversations_fall_back_to_recency() {
        let ranked = rank(
            vec![hit(0, false, 100, "older"), hit(0, false, 900, "newer")],
            "",
        );
        assert_eq!(ranked[0].title, "newer");
    }

    #[test]
    fn bots_and_slackbot_and_deleted_people_are_dropped() {
        let value = json!({
            "members": [
                {"id": "U1", "name": "ana", "profile": {"real_name": "Ana Lee"}},
                {"id": "U2", "name": "botty", "is_bot": true},
                {"id": "USLACKBOT", "name": "slackbot"},
                {"id": "U3", "name": "gone", "deleted": true},
            ]
        });
        let people = people(&value, "T1");
        assert_eq!(people.len(), 1);
        assert_eq!(people[0].title, "Ana Lee");
        assert_eq!(people[0].subtitle, "@ana");
        assert_eq!(people[0].link, "slack://user?team=T1&id=U1");
    }

    #[test]
    fn a_direct_message_is_named_after_the_person() {
        let names = Names::from_people(&people(
            &json!({"members": [{"id": "U9", "name": "sam", "profile": {"real_name": "Sam Ortiz"}}]}),
            "T1",
        ));
        let value = json!({"channels": [{"id": "D1", "is_im": true, "user": "U9", "unread_count_display": 2, "latest": {"ts": "1700000000.000100"}}]});
        let convos = conversations(&value, "T1", &names);
        assert_eq!(convos[0].title, "Sam Ortiz");
        assert_eq!(convos[0].kind, Kind::Person);
        assert_eq!(convos[0].unread, 2);
        assert!(convos[0].mention);
        assert_eq!(convos[0].link, "slack://user?team=T1&id=U9");
        assert_eq!(convos[0].updated, 1_700_000_000);
    }

    #[test]
    fn a_private_channel_is_marked_private() {
        let value = json!({"channels": [{"id": "C1", "name": "secret", "is_private": true}]});
        let convos = conversations(&value, "T1", &Names::default());
        assert_eq!(convos[0].title, "#secret");
        assert_eq!(convos[0].subtitle, "private");
        assert_eq!(convos[0].link, "slack://channel?team=T1&id=C1");
    }

    #[test]
    fn bad_tokens_read_as_signed_out_and_limits_carry_a_retry() {
        assert_eq!(
            check(&json!({"ok": false, "error": "token_revoked"}), 0)
                .unwrap_err()
                .kind,
            ProblemKind::SignedOut,
        );
        let limited = check(&json!({"ok": false, "error": "ratelimited"}), 100).unwrap_err();
        assert_eq!(limited.kind, ProblemKind::Limited);
        assert_eq!(limited.retry_at, Some(160));
    }

    #[test]
    fn name_search_matches_title_or_handle_case_insensitively() {
        let hits = vec![
            hit(0, false, 0, "General"),
            Hit {
                subtitle: "@ana".to_owned(),
                ..hit(0, false, 0, "Sam")
            },
            hit(0, false, 0, "random"),
        ];
        let found = by_name(hits, "AN");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].title, "Sam");
        assert_eq!(found[1].title, "random");
    }

    #[test]
    fn filtering_and_ranking_a_big_catalog_stays_quick() {
        let catalog: Vec<Hit> = (0..5000)
            .map(|n| hit(n % 3, n % 5 == 0, n, &format!("room {n}")))
            .collect();
        let start = std::time::Instant::now();
        let found = rank(by_name(catalog, "room 123"), "room 123");
        // 5000 rows filtered and sorted well under a frame; guards against O(n^2).
        assert!(
            start.elapsed().as_millis() < 50,
            "took {:?}",
            start.elapsed()
        );
        assert!(found.iter().all(|hit| hit.title.contains("123")));
    }

    #[test]
    fn a_message_match_keeps_its_permalink_and_names_its_channel() {
        let value = json!({"messages": {"matches": [{
            "ts": "1700000000.000200",
            "text": "ship it\nnow",
            "username": "ana",
            "channel": {"name": "general"},
            "permalink": "https://x.slack.com/archives/C1/p1700000000000200"
        }]}});
        let msgs = messages(&value);
        assert_eq!(msgs[0].title, "ship it now");
        assert_eq!(msgs[0].subtitle, "ana · #general");
        assert_eq!(
            msgs[0].link,
            "https://x.slack.com/archives/C1/p1700000000000200"
        );
        assert_eq!(msgs[0].kind, Kind::Message);
    }
}
