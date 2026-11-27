use super::Prepared;
use super::secrets;

pub const ID: &str = "raiden:fix-grammar";
pub const TITLE: &str = "Fix grammar";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    NothingSelected,
    HoldsSecrets(usize),
}

impl Refusal {
    pub fn message(&self) -> String {
        match self {
            Refusal::NothingSelected => "Select some text first, then fix grammar".to_owned(),
            Refusal::HoldsSecrets(1) => {
                "Fix grammar skipped: the selection holds a secret".to_owned()
            }
            Refusal::HoldsSecrets(count) => {
                format!("Fix grammar skipped: the selection holds {count} secrets")
            }
        }
    }
}

/// ```
/// use raiden_core::ai::grammar::{request, Refusal};
/// let prepared = request("this are wrong").unwrap();
/// assert!(prepared.text.contains("<text>\nthis are wrong\n</text>"));
/// assert_eq!(request(" \n"), Err(Refusal::NothingSelected));
/// ```
pub fn request(selection: &str) -> Result<Prepared, Refusal> {
    if selection.trim().is_empty() {
        return Err(Refusal::NothingSelected);
    }
    let hidden = secrets::hide(selection);
    if hidden.total() > 0 {
        return Err(Refusal::HoldsSecrets(hidden.total()));
    }
    Ok(Prepared {
        text: format!(
            "Fix the spelling, grammar and punctuation of the text between the <text> tags. \
Keep its meaning, words, tone, language, line breaks and formatting wherever they are already \
correct. Do not search the web, even when the text asks a question. Reply with only the \
corrected text: no tags, quotes, notes or explanation.\n\n\
<text>\n{}\n</text>",
            selection.trim()
        ),
        hidden: std::collections::BTreeMap::new(),
    })
}

fn without_tags(text: &str) -> Option<&str> {
    text.strip_prefix("<text>")?.strip_suffix("</text>")
}

fn without_fence(text: &str) -> Option<&str> {
    let inside = text.strip_prefix("```")?.strip_suffix("```")?;
    let (first, rest) = inside.split_once('\n')?;
    let label = first.trim();
    (label.is_empty() || label.chars().all(|letter| letter.is_ascii_alphanumeric())).then_some(rest)
}

fn unwrapped<'a>(reply: &'a str, selection: &str) -> &'a str {
    let mut text = reply.trim();
    let chosen = selection.trim();
    if without_tags(chosen).is_none()
        && let Some(inner) = without_tags(text)
    {
        text = inner.trim();
    }
    if !chosen.starts_with("```")
        && let Some(inner) = without_fence(text)
    {
        text = inner.trim();
    }
    text
}

/// ```
/// use raiden_core::ai::grammar::fixed;
/// assert_eq!(fixed("This is wrong.", "this are wrong\n"), "This is wrong.\n");
/// assert_eq!(fixed("<text>\nThis is wrong.\n</text>", " this are wrong"), " This is wrong.");
/// assert_eq!(fixed("  ", "this are wrong"), "this are wrong");
/// ```
pub fn fixed(reply: &str, selection: &str) -> String {
    let body = unwrapped(reply, selection);
    if body.is_empty() {
        return selection.to_owned();
    }
    let core = selection.trim();
    let start = selection.find(core).unwrap_or(0);
    let lead = &selection[..start];
    let trail = &selection[start + core.len()..];
    format!("{lead}{body}{trail}")
}

fn sentences(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((at, letter)) = chars.next() {
        let ends = match letter {
            '\n' => true,
            '.' | '!' | '?' => chars.peek().is_none_or(|(_, next)| next.is_whitespace()),
            _ => false,
        };
        if ends {
            let end = at + letter.len_utf8();
            found.push(text[start..end].trim());
            start = end;
        }
    }
    found.push(text[start..].trim());
    found.retain(|sentence| !sentence.is_empty());
    found
}

/// ```
/// use raiden_core::ai::grammar::changed_sentences;
/// assert_eq!(changed_sentences("this are wrong", "This is wrong."), 1);
/// assert_eq!(changed_sentences("Fine. he go home. Ok.", "Fine. He goes home. Ok."), 1);
/// assert_eq!(changed_sentences("Same.", "Same."), 0);
/// assert_eq!(changed_sentences("a b", "a  b"), 1);
/// ```
pub fn changed_sentences(before: &str, after: &str) -> usize {
    if before == after {
        return 0;
    }
    let mut left = sentences(before);
    let changed = sentences(after)
        .into_iter()
        .filter(|sentence| {
            if let Some(at) = left.iter().position(|kept| kept == sentence) {
                left.remove(at);
                false
            } else {
                true
            }
        })
        .count();
    changed.max(1)
}

/// ```
/// use raiden_core::ai::grammar::message;
/// assert_eq!(message(1, "TextEdit"), "Fixed 1 sentence in TextEdit");
/// assert_eq!(message(2, "Mail"), "Fixed 2 sentences in Mail");
/// assert_eq!(message(0, "Zed"), "No mistakes found in Zed");
/// ```
pub fn message(changed: usize, app: &str) -> String {
    match changed {
        0 => format!("No mistakes found in {app}"),
        1 => format!("Fixed 1 sentence in {app}"),
        count => format!("Fixed {count} sentences in {app}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_selection_holding_a_secret_is_never_sent() {
        for (_, fake) in secrets::tests::fakes() {
            let selection = format!("this are my key {fake}");
            assert_eq!(request(&selection), Err(Refusal::HoldsSecrets(1)));
        }
    }

    #[test]
    fn refusals_read_plainly() {
        assert_eq!(
            Refusal::HoldsSecrets(2).message(),
            "Fix grammar skipped: the selection holds 2 secrets"
        );
    }

    #[test]
    fn code_fences_and_inner_line_breaks_survive_only_as_meant() {
        assert_eq!(fixed("```\nOne.\nTwo.\n```", "one\ntwo"), "One.\nTwo.");
        assert_eq!(fixed("```text\nOne.\n```", "one"), "One.");
        assert_eq!(fixed("A ``` b", "a b"), "A ``` b");
        let fenced = "```\nlet x = 1;\n```";
        assert_eq!(fixed(fenced, fenced), fenced);
        let tagged = "<text>hi</text>";
        assert_eq!(fixed(tagged, tagged), tagged);
    }
}
