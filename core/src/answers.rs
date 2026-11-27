use std::collections::HashMap;

use serde::Serialize;

use crate::ai::{Web, secrets};

pub const FILE: &str = "answers";
pub const KEPT: usize = 50;
pub const TURNS_KEPT: usize = 40;
pub const FILE_LONGEST: usize = 4_000_000;
pub const LONGEST: usize = 20_000;

const ANSWER: &str = "answer";
const TURN: &str = "turn";
const NO_WEB: &str = "no-web";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub conversation: String,
    pub at: u64,
    pub question: String,
    pub model: String,
    pub answer: String,
    pub web: Web,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Turn {
    pub question: String,
    pub answer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conversation {
    pub id: String,
    pub at: u64,
    pub model: String,
    pub turns: Vec<Turn>,
    #[serde(skip)]
    pub web: Web,
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for letter in text.chars() {
        match letter {
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

fn unescape(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut letters = text.chars();
    while let Some(letter) = letters.next() {
        if letter != '\\' {
            plain.push(letter);
            continue;
        }
        match letters.next() {
            Some('n') => plain.push('\n'),
            Some('r') => plain.push('\r'),
            Some('t') => plain.push('\t'),
            Some(other) => plain.push(other),
            None => plain.push('\\'),
        }
    }
    plain
}

/// ```
/// use raiden_core::answers::{hidden, Saved};
/// let key = format!("AKIA{}", "Q".repeat(16));
/// let saved = hidden(Saved { conversation: "c".into(), at: 1, question: format!("is {key} live?"), model: "haiku".into(), answer: format!("{key} is a key"), web: raiden_core::ai::Web::Searchable });
/// assert!(!saved.question.contains(&key) && !saved.answer.contains(&key));
/// ```
pub fn hidden(saved: Saved) -> Saved {
    Saved {
        question: secrets::hide(&saved.question).text,
        answer: secrets::hide(&saved.answer).text,
        ..saved
    }
}

fn clipped(text: &str) -> &str {
    text.char_indices()
        .nth(LONGEST)
        .map_or(text, |(at, _)| &text[..at])
}

/// ```
/// use raiden_core::ai::Web;
/// use raiden_core::answers::{line, parse, Saved};
/// let saved = Saved { conversation: "a1-c1".into(), at: 5, question: "Why?".into(), model: "claude-haiku".into(), answer: "Because.\n\n`x`".into(), web: Web::Off };
/// assert_eq!(parse(&line(&saved)), vec![saved]);
/// ```
pub fn line(saved: &Saved) -> String {
    let question = secrets::hide(&saved.question).text;
    let answer = secrets::hide(&saved.answer).text;
    let mark = match saved.web {
        Web::Searchable => String::new(),
        Web::Off => format!("\t{NO_WEB}"),
    };
    format!(
        "{TURN}\t{}\t{}\t{}\t{}\t{}{mark}\n",
        saved.at,
        escape(&saved.conversation),
        escape(clipped(&question)),
        escape(&saved.model),
        escape(clipped(&answer))
    )
}

fn parse_line(line: &str, older: &mut Vec<u64>) -> Option<Saved> {
    let mut fields = line.split('\t');
    let kind = fields.next()?;
    let at: u64 = fields.next()?.parse().ok()?;
    let conversation = match kind {
        TURN => unescape(fields.next()?),
        ANSWER => {
            let same = older.iter().filter(|seen| **seen == at).count();
            older.push(at);
            format!("answer-{at}-{same}")
        }
        _ => return None,
    };
    Some(Saved {
        conversation,
        at,
        question: unescape(fields.next()?),
        model: unescape(fields.next()?),
        answer: unescape(fields.next()?),
        web: if fields.next() == Some(NO_WEB) {
            Web::Off
        } else {
            Web::Searchable
        },
    })
}

pub fn parse(text: &str) -> Vec<Saved> {
    let mut older = Vec::new();
    text.lines()
        .filter_map(|line| parse_line(line, &mut older))
        .filter(|saved| !saved.question.trim().is_empty() && !saved.conversation.is_empty())
        .collect()
}

fn grouped(lines: Vec<Saved>) -> Vec<Conversation> {
    let mut conversations: Vec<Conversation> = Vec::new();
    for saved in lines {
        let turn = Turn {
            question: saved.question,
            answer: saved.answer,
        };
        let found = conversations
            .iter()
            .position(|conversation| conversation.id == saved.conversation);
        let mut conversation = match found {
            Some(at) => conversations.remove(at),
            None => Conversation {
                id: saved.conversation,
                at: saved.at,
                model: String::new(),
                turns: Vec::new(),
                web: Web::Searchable,
            },
        };
        if saved.web == Web::Off {
            conversation.web = Web::Off;
        }
        conversation.at = conversation.at.max(saved.at);
        conversation.model = saved.model;
        conversation.turns.push(turn);
        if conversation.turns.len() > TURNS_KEPT {
            conversation.turns.remove(0);
        }
        conversations.push(conversation);
    }
    conversations.reverse();
    conversations
}

pub fn recent(text: &str) -> Vec<Conversation> {
    let mut conversations = grouped(parse(text));
    conversations.truncate(KEPT);
    conversations
}

pub fn find(text: &str, id: &str) -> Option<Conversation> {
    grouped(parse(text))
        .into_iter()
        .find(|conversation| conversation.id == id)
}

pub fn kept(text: &str) -> Option<String> {
    let lines = parse(text);
    let conversations = grouped(lines.clone());
    let mut later: HashMap<&str, usize> = HashMap::new();
    let mut sizes: HashMap<&str, usize> = HashMap::new();
    let mut written: Vec<(&str, &Saved)> = Vec::new();
    for saved in lines.iter().rev() {
        let id = saved.conversation.as_str();
        let seen = later.entry(id).or_default();
        *seen += 1;
        if *seen <= TURNS_KEPT {
            *sizes.entry(id).or_default() += size(saved);
            written.push((id, saved));
        }
    }
    let mut room = FILE_LONGEST;
    let newest: Vec<&str> = conversations
        .iter()
        .take(KEPT)
        .map(|conversation| conversation.id.as_str())
        .enumerate()
        .take_while(|(at, id)| {
            let size = sizes.get(id).copied().unwrap_or(0);
            let fits = *at == 0 || size <= room;
            room = room.saturating_sub(size);
            fits
        })
        .map(|(_, id)| id)
        .collect();
    written.retain(|(id, _)| newest.contains(id));
    if written.len() == lines.len() {
        return None;
    }
    written.reverse();
    Some(written.into_iter().map(|(_, saved)| line(saved)).collect())
}

fn size(saved: &Saved) -> usize {
    [
        &saved.conversation,
        &saved.question,
        &saved.model,
        &saved.answer,
    ]
    .iter()
    .map(|text| text.len() + text.matches(['\\', '\n', '\r', '\t']).count())
    .sum::<usize>()
        + 32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(conversation: &str, at: u64, question: &str) -> Saved {
        Saved {
            conversation: conversation.into(),
            at,
            question: question.into(),
            model: "claude-haiku-4-5".into(),
            answer: format!("About {question}"),
            web: Web::Searchable,
        }
    }

    fn questions(conversation: &Conversation) -> Vec<&str> {
        conversation
            .turns
            .iter()
            .map(|turn| turn.question.as_str())
            .collect()
    }

    #[test]
    fn newlines_tabs_and_backslashes_survive() {
        let tricky = Saved {
            answer: "a\tb\nc\\nd\r\\".into(),
            ..saved("c\t1", 1, "q\\t")
        };
        let text = line(&tricky);
        assert_eq!(text.matches('\n').count(), 1);
        assert_eq!(parse(&text), vec![tricky]);
    }

    #[test]
    fn broken_and_other_lines_are_skipped() {
        let text = format!(
            "pick\t1\t0\tq\t/a.app\nturn\tsoon\tc\tq\tm\ta\n{}turn\t2\tc\t \tm\ta\nturn\t3\tc\tq\n",
            line(&saved("c", 1, "kept"))
        );
        assert_eq!(parse(&text), vec![saved("c", 1, "kept")]);
    }

    #[test]
    fn turns_of_one_conversation_come_back_together_in_order() {
        let text: String = [
            saved("a", 1, "first"),
            saved("b", 2, "other"),
            saved("a", 3, "and then?"),
        ]
        .iter()
        .map(line)
        .collect();
        let conversations = recent(&text);
        let ids: Vec<&str> = conversations.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
        assert_eq!(questions(&conversations[0]), ["first", "and then?"]);
        assert_eq!(conversations[0].at, 3);
        assert_eq!(conversations[0].turns[1].answer, "About and then?");
        assert_eq!(find(&text, "b").map(|c| c.turns.len()), Some(1));
        assert_eq!(find(&text, "z"), None);
    }

    #[test]
    fn answers_saved_before_conversations_read_as_one_turn_each() {
        let text = "answer\t7\tWhy?\thaiku\tBecause.\nanswer\t7\tHow?\thaiku\tLike so.\n";
        let older = recent(text);
        let ids: Vec<&str> = older.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["answer-7-1", "answer-7-0"]);
        assert_eq!(questions(&older[1]), ["Why?"]);
        assert_eq!(older[1].turns[0].answer, "Because.");
        let continued = format!("{text}{}", line(&saved("answer-7-0", 9, "And?")));
        let first = find(&continued, "answer-7-0").map(|c| c.turns.len());
        assert_eq!(first, Some(2));
        assert_eq!(recent(&continued)[0].id, "answer-7-0");
    }

    #[test]
    fn only_the_newest_conversations_are_kept_whole() {
        let mut all: Vec<Saved> = (0..60)
            .map(|at| saved(&format!("c{at}"), at, &format!("q{at}")))
            .collect();
        all.push(saved("c5", 61, "follow-up"));
        let text: String = all.iter().map(line).collect();
        let trimmed = kept(&text).unwrap_or_default();
        let left = recent(&trimmed);
        assert_eq!(left.len(), KEPT);
        assert_eq!(left[0].id, "c5");
        assert_eq!(left[0].turns.len(), 2);
        assert!(left.iter().all(|c| c.id != "c10"));
        assert!(left.iter().any(|c| c.id == "c11"));
        assert_eq!(kept(&trimmed), None);
    }

    #[test]
    fn a_long_conversation_keeps_its_newest_turns() {
        let text: String = (0..(TURNS_KEPT as u64 + 3))
            .map(|at| line(&saved("long", at, &format!("q{at}"))))
            .collect();
        let read = recent(&text);
        assert_eq!(read[0].turns.len(), TURNS_KEPT);
        assert_eq!(read[0].turns[0].question, "q3");
        let trimmed = kept(&text).unwrap_or_default();
        assert_eq!(parse(&trimmed).len(), TURNS_KEPT);
        assert_eq!(recent(&trimmed), read);
        assert_eq!(kept(&trimmed), None);
    }

    #[test]
    fn the_file_keeps_only_the_newest_conversations_that_fit() {
        let mut text = String::new();
        for conversation in 0..30_u64 {
            for turn in 0..10 {
                text.push_str(&line(&Saved {
                    answer: "a\n".repeat(LONGEST / 2),
                    ..saved(&format!("c{conversation}"), conversation * 10 + turn, "q")
                }));
            }
        }
        let trimmed = kept(&text).unwrap_or_default();
        assert!(trimmed.len() <= FILE_LONGEST);
        let left = recent(&trimmed);
        assert_eq!(left[0].id, "c29");
        assert!((10..30).contains(&left.len()), "{}", left.len());
        assert!(
            left.iter()
                .all(|conversation| conversation.turns.len() == 10)
        );
        assert_eq!(kept(&trimmed), None);
    }

    #[test]
    fn the_newest_conversation_is_kept_even_when_it_alone_is_too_big() {
        let mut text = line(&saved("old", 0, "q"));
        for turn in 0..TURNS_KEPT as u64 {
            text.push_str(&line(&Saved {
                question: "€".repeat(LONGEST),
                answer: "€".repeat(LONGEST),
                ..saved("huge", turn + 1, "q")
            }));
        }
        let trimmed = kept(&text).unwrap_or_default();
        let left = recent(&trimmed);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, "huge");
        assert_eq!(left[0].turns.len(), TURNS_KEPT);
    }

    #[test]
    fn no_secret_is_ever_written_or_returned() {
        for (kind, fake) in secrets::tests::fakes() {
            let secret = secrets::tests::secret_part(kind, &fake);
            let raw = Saved {
                answer: format!("Use {fake} here"),
                ..saved("c", 1, &format!("why does {fake} fail?"))
            };
            let text = line(&raw);
            assert!(!text.contains(&secret), "{kind:?} was written");
            let back = hidden(raw);
            assert!(!back.question.contains(&secret) && !back.answer.contains(&secret));
            assert!(back.question.contains("[hidden "));
        }
    }

    #[test]
    fn a_conversation_kept_off_the_web_stays_off_and_old_lines_still_read() {
        let old = "turn\t1\tc\tWhy?\thaiku\tBecause.\nanswer\t2\tHow?\thaiku\tSo.\n";
        let read = parse(old);
        assert_eq!(read.len(), 2);
        assert!(read.iter().all(|saved| saved.web == Web::Searchable));
        assert_eq!(find(old, "c").map(|c| c.web), Some(Web::Searchable));
        let off = Saved {
            web: Web::Off,
            ..saved("c", 3, "pulls")
        };
        let text = format!("{old}{}{}", line(&off), line(&saved("c", 4, "later")));
        assert!(line(&off).ends_with("\tno-web\n"));
        assert_eq!(parse(&text)[2], off);
        assert_eq!(find(&text, "c").map(|c| c.web), Some(Web::Off));
        assert_eq!(
            find(&text, "answer-2-0").map(|c| c.web),
            Some(Web::Searchable)
        );
    }

    #[test]
    fn long_answers_are_clipped() {
        let long = Saved {
            answer: "é".repeat(LONGEST + 10),
            ..saved("c", 1, "long")
        };
        let back = parse(&line(&long));
        assert_eq!(back[0].answer.chars().count(), LONGEST);
    }
}
