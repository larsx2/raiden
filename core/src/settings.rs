use std::fmt;
use std::path::Path;

pub use toml_edit::Value;
use toml_edit::{Array, DocumentMut, Item};

use crate::ai::{Effort, Provider};
use crate::terminals::{self, Terminal};

pub const FILE: &str = "raiden.toml";
pub const HOTKEY: &str = "hotkey";
pub const CLIPBOARD_HOTKEY: &str = "clipboard_hotkey";
pub const AI: &str = "ai";
pub const EFFORT: &str = "effort";
pub const PINNED_REPOS: &str = "pinned_repos";
pub const PULL_REQUESTS: &str = "pull_requests";
pub const VIM: &str = "vim";
pub const TERMINAL: &str = "terminal";
pub const FIXER_KEY: &str = "fixer_key";
pub const VIEW_ID: &str = "raiden:settings";
pub const VIEW_TITLE: &str = "Settings";
pub const FILE_ID: &str = "raiden:settings-file";
pub const FILE_TITLE: &str = "Open Settings File";
pub const SPOTLIGHT_HOLDS: &str = "Spotlight holds ⌘Space. Turn off its shortcut in System Settings, Keyboard, Keyboard Shortcuts, Spotlight.";
pub const ONLY_RAIDEN: &str = "⌘Space can open only Raiden, not Clipboard history";
const KNOWN: [&str; 9] = [
    HOTKEY,
    CLIPBOARD_HOTKEY,
    AI,
    EFFORT,
    PINNED_REPOS,
    PULL_REQUESTS,
    VIM,
    TERMINAL,
    FIXER_KEY,
];
const MODIFIER_SYMBOLS: [char; 4] = ['⌃', '⌥', '⇧', '⌘'];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotkey {
    pub control: bool,
    pub option: bool,
    pub shift: bool,
    pub command: bool,
    pub key: String,
}

impl Hotkey {
    /// ```
    /// use raiden_core::settings::Hotkey;
    /// let symbols = Hotkey::parse("⌃⌥Space").unwrap();
    /// assert_eq!(symbols, Hotkey::parse("control+option+space").unwrap());
    /// assert_eq!(symbols.accelerator(), "Control+Alt+Space");
    /// assert_eq!(symbols.to_string(), "⌃⌥Space");
    /// ```
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut hotkey = Self {
            control: false,
            option: false,
            shift: false,
            command: false,
            key: String::new(),
        };
        let parts: String = text
            .chars()
            .flat_map(|c| [Some(c), MODIFIER_SYMBOLS.contains(&c).then_some('+')])
            .flatten()
            .collect();
        let mut tokens: Vec<&str> = parts
            .split('+')
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .collect();
        let Some(key) = tokens.pop() else {
            return Err("the hotkey is empty; write one such as \"⌃⌥Space\"".to_owned());
        };
        for token in tokens {
            let Some(symbol) = modifier(token) else {
                return Err(format!(
                    "\"{token}\" in \"{text}\" is not a modifier key; use Control, Option, Shift or Command, or ⌃ ⌥ ⇧ ⌘"
                ));
            };
            hotkey.hold(symbol);
        }
        if modifier(key).is_some() {
            return Err(format!(
                "\"{text}\" ends with a modifier; add the key to press with it, such as Space"
            ));
        }
        if key.contains(char::is_whitespace) {
            return Err(format!(
                "\"{key}\" in \"{text}\" is not a key name; key names have no spaces, such as Space or PageDown"
            ));
        }
        if !(hotkey.control || hotkey.option || hotkey.command) {
            return Err(format!(
                "\"{text}\" needs Control, Option or Command, or it would take that key from every app"
            ));
        }
        hotkey.key = if key.chars().count() == 1 {
            key.to_uppercase()
        } else {
            capitalized(key)
        };
        Ok(hotkey)
    }

    fn hold(&mut self, symbol: char) {
        match symbol {
            '⌃' => self.control = true,
            '⌥' => self.option = true,
            '⇧' => self.shift = true,
            _ => self.command = true,
        }
    }

    /// ```
    /// use raiden_core::settings::Hotkey;
    /// assert!(Hotkey::parse("⌘Space").unwrap().is_spotlight());
    /// assert!(Hotkey::parse("cmd + space").unwrap().is_spotlight());
    /// assert!(!Hotkey::parse("⌘⇧Space").unwrap().is_spotlight());
    /// ```
    pub fn is_spotlight(&self) -> bool {
        self.command && !self.control && !self.option && !self.shift && self.key == "Space"
    }

    /// ```
    /// use raiden_core::settings::{Hotkey, CLIPBOARD_HOTKEY, HOTKEY};
    /// assert_eq!(Hotkey::chosen("⌃⌥K", HOTKEY).unwrap().to_string(), "⌃⌥K");
    /// assert!(Hotkey::chosen("⌘Space", HOTKEY).is_ok());
    /// assert!(Hotkey::chosen("⌘Space", CLIPBOARD_HOTKEY).is_err());
    /// ```
    pub fn chosen(text: &str, name: &str) -> Result<Self, String> {
        let hotkey = Self::parse(text)?;
        if hotkey.is_spotlight() && name != HOTKEY {
            return Err(ONLY_RAIDEN.to_owned());
        }
        Ok(hotkey)
    }

    pub fn accelerator(&self) -> String {
        let held = [
            (self.control, "Control"),
            (self.option, "Alt"),
            (self.shift, "Shift"),
            (self.command, "Super"),
        ];
        held.iter()
            .filter(|(on, _)| *on)
            .map(|(_, name)| *name)
            .chain([self.key.as_str()])
            .collect::<Vec<_>>()
            .join("+")
    }
}

fn modifier(token: &str) -> Option<char> {
    match token.to_lowercase().as_str() {
        "⌃" | "control" | "ctrl" => Some('⌃'),
        "⌥" | "option" | "alt" => Some('⌥'),
        "⇧" | "shift" => Some('⇧'),
        "⌘" | "command" | "cmd" => Some('⌘'),
        _ => None,
    }
}

fn capitalized(word: &str) -> String {
    let mut letters = word.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

impl fmt::Display for Hotkey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, symbol) in [
            (self.control, '⌃'),
            (self.option, '⌥'),
            (self.shift, '⇧'),
            (self.command, '⌘'),
        ] {
            if on {
                write!(formatter, "{symbol}")?;
            }
        }
        write!(formatter, "{}", self.key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub hotkey: Hotkey,
    pub clipboard_hotkey: Hotkey,
    pub ai: Provider,
    pub effort: Effort,
    pub pinned_repos: Vec<String>,
    pub pull_requests: bool,
    pub vim: bool,
    pub terminal: Option<Terminal>,
    pub fixer_key: Option<String>,
}

impl Settings {
    /// ```
    /// use raiden_core::settings::Settings;
    /// assert_eq!(Settings::defaults(false).hotkey.to_string(), "⌥Space");
    /// assert_eq!(Settings::defaults(true).hotkey.to_string(), "⌥⇧Space");
    /// assert_eq!(Settings::defaults(false).ai.id(), "claude");
    /// assert_eq!(Settings::defaults(false).effort.id(), "low");
    /// assert_eq!(Settings::defaults(false).clipboard_hotkey.to_string(), "⌃⌥V");
    /// assert_eq!(Settings::defaults(true).clipboard_hotkey.to_string(), "⌃⌥⇧V");
    /// ```
    pub fn defaults(development: bool) -> Self {
        Self {
            hotkey: Hotkey {
                control: false,
                option: true,
                shift: development,
                command: false,
                key: "Space".to_owned(),
            },
            clipboard_hotkey: Hotkey {
                control: true,
                option: true,
                shift: development,
                command: false,
                key: "V".to_owned(),
            },
            ai: Provider::DEFAULT,
            effort: Effort::default(),
            pinned_repos: Vec::new(),
            pull_requests: false,
            vim: false,
            terminal: None,
            fixer_key: None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Loaded {
    pub settings: Settings,
    pub problems: Vec<String>,
}

pub fn load(text: Option<&str>, development: bool, previous: Option<&Settings>) -> Loaded {
    let defaults = Settings::defaults(development);
    let kept = previous.unwrap_or(&defaults).clone();
    let Some(text) = text else {
        return Loaded {
            settings: defaults,
            problems: Vec::new(),
        };
    };
    let document = match text.parse::<DocumentMut>() {
        Ok(document) => document,
        Err(error) => {
            return Loaded {
                settings: kept,
                problems: vec![format!(
                    "{FILE} is not valid TOML, so Raiden ignores it until it is fixed: {error}"
                )],
            };
        }
    };
    let mut problems = Vec::new();
    let hotkey = hotkey_setting(
        &document,
        HOTKEY,
        defaults.hotkey,
        kept.hotkey,
        &mut problems,
    );
    let mut clipboard_hotkey = hotkey_setting(
        &document,
        CLIPBOARD_HOTKEY,
        defaults.clipboard_hotkey.clone(),
        kept.clipboard_hotkey.clone(),
        &mut problems,
    );
    if clipboard_hotkey == hotkey {
        let instead = [kept.clipboard_hotkey, defaults.clipboard_hotkey]
            .into_iter()
            .find(|other| *other != hotkey);
        problems.push(match &instead {
            Some(other) => {
                format!("{CLIPBOARD_HOTKEY}: {hotkey} already opens Raiden. Raiden keeps {other}.")
            }
            None => format!(
                "{CLIPBOARD_HOTKEY}: {hotkey} already opens Raiden, so Clipboard history opens only from its command. Set {CLIPBOARD_HOTKEY} to other keys."
            ),
        });
        clipboard_hotkey = instead.unwrap_or(clipboard_hotkey);
    }
    let ai = match document.get(AI) {
        None => defaults.ai,
        Some(item) => match item.as_str().map(Provider::from_id) {
            Some(Some(ai)) => ai,
            _ => {
                problems.push(format!(
                    "ai must be \"claude\" or \"chatgpt\". Raiden keeps \"{}\".",
                    kept.ai.id()
                ));
                kept.ai
            }
        },
    };
    let effort = match document.get(EFFORT) {
        None => defaults.effort,
        Some(item) => match item.as_str().map(Effort::from_id) {
            Some(Some(effort)) => effort,
            _ => {
                problems.push(format!(
                    "effort must be \"low\", \"medium\" or \"high\". Raiden keeps \"{}\".",
                    kept.effort.id()
                ));
                kept.effort
            }
        },
    };
    let pinned_repos = match document.get(PINNED_REPOS) {
        None => defaults.pinned_repos,
        Some(item) => pinned(item, kept.pinned_repos, &mut problems),
    };
    let pull_requests = match document.get(PULL_REQUESTS) {
        None => defaults.pull_requests,
        Some(item) => item.as_bool().unwrap_or_else(|| {
            problems.push(format!(
                "{PULL_REQUESTS} must be true or false. Raiden keeps {}.",
                kept.pull_requests
            ));
            kept.pull_requests
        }),
    };
    let vim = match document.get(VIM) {
        None => defaults.vim,
        Some(item) => item.as_bool().unwrap_or_else(|| {
            problems.push(format!(
                "{VIM} must be true or false. Raiden keeps {}.",
                kept.vim
            ));
            kept.vim
        }),
    };
    let terminal = match document.get(TERMINAL) {
        None => defaults.terminal,
        Some(item) => match item.as_str().map(Terminal::from_id) {
            Some(Some(terminal)) => Some(terminal),
            _ => {
                problems.push(format!(
                    "{TERMINAL} must be {}. Raiden keeps {}.",
                    terminals::ids(),
                    kept.terminal.map_or_else(
                        || "the first one installed".to_owned(),
                        |terminal| format!("\"{}\"", terminal.id())
                    )
                ));
                kept.terminal
            }
        },
    };
    let fixer_key = match document.get(FIXER_KEY) {
        None => defaults.fixer_key,
        Some(item) => match item.as_str().map(str::trim) {
            Some("") => None,
            Some(key) => Some(key.to_owned()),
            None => {
                problems.push(format!(
                    "{FIXER_KEY} must be text. Raiden keeps the key it had."
                ));
                kept.fixer_key.clone()
            }
        },
    };
    problems.extend(
        document
            .iter()
            .filter(|(name, _)| !KNOWN.contains(name))
            .map(|(name, _)| {
                format!("{FILE} has no setting named \"{name}\", so Raiden ignores it.")
            }),
    );
    Loaded {
        settings: Settings {
            hotkey,
            clipboard_hotkey,
            ai,
            effort,
            pinned_repos,
            pull_requests,
            vim,
            terminal,
            fixer_key,
        },
        problems,
    }
}

fn pinned(item: &Item, kept: Vec<String>, problems: &mut Vec<String>) -> Vec<String> {
    let Some(entries) = item.as_array() else {
        problems.push(format!(
            "{PINNED_REPOS} must be a list such as [\"owner/repo\"]. Raiden keeps {}.",
            shown_list(&kept)
        ));
        return kept;
    };
    let mut repos: Vec<String> = Vec::new();
    let mut refused: Vec<String> = Vec::new();
    for entry in entries {
        match entry.as_str().map(str::trim) {
            Some(repo) if crate::pulls::is_repo(repo) => {
                if !repos.iter().any(|kept| kept.eq_ignore_ascii_case(repo)) {
                    repos.push(repo.to_owned());
                }
            }
            _ => refused.push(entry.to_string().trim().to_owned()),
        }
    }
    if !refused.is_empty() {
        problems.push(format!(
            "{PINNED_REPOS} holds {}, which {} not \"owner/repo\", so Raiden leaves {} out.",
            refused.join(", "),
            if refused.len() == 1 { "is" } else { "are" },
            if refused.len() == 1 { "it" } else { "them" }
        ));
    }
    repos
}

fn shown_list(repos: &[String]) -> String {
    list(repos).to_string()
}

/// ```
/// use raiden_core::settings::list;
/// assert_eq!(list(&["fernhill/beacon".to_owned()]).to_string(), "[\"fernhill/beacon\"]");
/// ```
pub fn list(items: &[String]) -> Value {
    Value::Array(items.iter().map(String::as_str).collect::<Array>())
}

fn hotkey_setting(
    document: &DocumentMut,
    name: &str,
    default: Hotkey,
    kept: Hotkey,
    problems: &mut Vec<String>,
) -> Hotkey {
    let Some(item) = document.get(name) else {
        return default;
    };
    match item.as_str().map(|text| Hotkey::chosen(text, name)) {
        Some(Ok(hotkey)) => hotkey,
        Some(Err(why)) => {
            problems.push(format!("{name}: {why}. Raiden keeps {kept}."));
            kept
        }
        None => {
            problems.push(format!(
                "{name} must be text in quotes, such as \"⌃⌥Space\". Raiden keeps {kept}."
            ));
            kept
        }
    }
}

pub fn set(text: &str, name: &str, value: impl Into<Value>) -> Result<String, String> {
    let value = value.into();
    let mut document = text.parse::<DocumentMut>().map_err(|error| {
        format!("{FILE} is not valid TOML, so Raiden left it as it is: {error}")
    })?;
    if document.get(name).is_some_and(|item| !item.is_value()) {
        return Err(format!(
            "{name} in {FILE} is a table, so Raiden left it as it is"
        ));
    }
    match document.get_mut(name).and_then(Item::as_value_mut) {
        Some(Value::Array(old)) if value.is_array() => {
            if let Value::Array(new) = value {
                merge(old, &new);
            }
        }
        Some(existing) => {
            let decor = existing.decor().clone();
            *existing = value;
            *existing.decor_mut() = decor;
        }
        None => {
            let trailing = if document.iter().all(|(_, item)| item.is_value()) {
                document.trailing().as_str().unwrap_or_default().to_owned()
            } else {
                String::new()
            };
            if !trailing.is_empty() {
                document.set_trailing("");
            }
            document.insert(name, toml_edit::value(value));
            if let Some(mut key) = document.key_mut(name) {
                key.leaf_decor_mut().set_prefix(trailing);
            }
        }
    }
    Ok(document.to_string())
}

fn remove_keeping_comments(array: &mut Array, index: usize) {
    let removed = array.remove(index);
    let Some(prefix) = removed.decor().prefix().and_then(|prefix| prefix.as_str()) else {
        return;
    };
    let Some(cut) = prefix.rfind('\n') else {
        return;
    };
    let carried = &prefix[..=cut];
    if !carried.contains('#') {
        return;
    }
    let joined = |rest: &str| format!("{carried}{}", rest.strip_prefix('\n').unwrap_or(rest));
    match array.get_mut(index) {
        Some(next) => {
            let rest = next
                .decor()
                .prefix()
                .and_then(|prefix| prefix.as_str())
                .unwrap_or(" ")
                .to_owned();
            next.decor_mut().set_prefix(joined(&rest));
        }
        None => {
            let rest = array.trailing().as_str().unwrap_or_default().to_owned();
            array.set_trailing(joined(&rest));
        }
    }
}

fn merge(old: &mut Array, new: &Array) {
    let bare = |value: &Value| {
        let mut value = value.clone();
        value.decor_mut().clear();
        value.to_string()
    };
    let wanted: Vec<String> = new.iter().map(bare).collect();
    for index in (0..old.len()).rev() {
        if old
            .get(index)
            .is_some_and(|kept| !wanted.contains(&bare(kept)))
        {
            remove_keeping_comments(old, index);
        }
    }
    let indent = old
        .iter()
        .last()
        .and_then(|last| last.decor().prefix()?.as_str()?.rsplit_once('\n'))
        .map(|(_, indent)| format!("\n{indent}"));
    for value in new {
        if !old.iter().any(|kept| bare(kept) == bare(value)) {
            let mut value = value.clone();
            value.decor_mut().clear();
            old.push_formatted(value);
            if let (Some(indent), Some(added)) = (&indent, old.iter_mut().last()) {
                added.decor_mut().set_prefix(indent.as_str());
            }
        }
    }
}

/// ```
/// use raiden_core::settings::key_name;
/// assert_eq!(key_name("KeyK"), "K");
/// assert_eq!(key_name("Digit5"), "5");
/// assert_eq!(key_name("Space"), "Space");
/// ```
pub fn key_name(code: &str) -> &str {
    ["Key", "Digit"]
        .into_iter()
        .filter_map(|prefix| code.strip_prefix(prefix))
        .find(|rest| {
            rest.len() == 1
                && rest
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        })
        .unwrap_or(code)
}

pub fn shown_path(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(inside) if !home.as_os_str().is_empty() => format!("~/{}", inside.display()),
        _ => path.display().to_string(),
    }
}

pub fn starter(development: bool) -> String {
    format!(
        "# Raiden's settings. A setting that is left out or commented out uses its default.\n\
         \n\
         # The keys that open Raiden, such as \"⌃⌥Space\" or \"Control+Option+Space\".\n\
         # ⌘Space works once Spotlight's shortcut is off, in System Settings, Keyboard,\n\
         # Keyboard Shortcuts, Spotlight.\n\
         # hotkey = \"{}\"\n\
         \n\
         # The keys that open Clipboard history. Not ⌥V: it types √ on a US keyboard.\n\
         # clipboard_hotkey = \"{}\"\n\
         \n\
         # The terminal where anything Raiden opens in a terminal starts: \"ghostty\", \"iterm\", \"rex\",\n\
         # \"tern\", \"terminal\", \"kitty\", \"alacritty\" or \"warp\". Left out, or not installed, Raiden uses the\n\
         # first installed of Ghostty, iTerm2, Rex, Tern, Kitty, Alacritty, Warp and Terminal.\n\
         # terminal = \"ghostty\"\n\
         \n\
         # Vim keys: Esc switches to Normal, where j and k move and letters stop typing.\n\
         # i, a or / goes back to typing.\n\
         # vim = false\n\
         \n\
         # A Fixer API key from fixer.io for currency conversions. Left out, Raiden uses the\n\
         # European Central Bank's daily rates from frankfurter.dev, which need no key.\n\
         # fixer_key = \"\"\n\
         \n\
         # Repositories offered as quick filters in Pull Requests, such as [\"owner/repo\"].\n\
         # pinned_repos = []\n\
         \n\
         # Whether Pull Requests may read your pull requests through gh. Continue sets it.\n\
         # pull_requests = false\n\
         \n\
         # The AI that Ask AI answers with: \"claude\" through Claude Code, or \"chatgpt\" through Codex.\n\
         # Fix grammar always uses Claude Code.\n\
         # ai = \"{}\"\n\
         \n\
         # How hard Ask AI thinks before it answers: \"low\", \"medium\" or \"high\".\n\
         # Higher effort gives better answers but a slower start. Fix grammar always uses low,\n\
         # for speed.\n\
         # effort = \"{}\"\n",
        Settings::defaults(development).hotkey,
        Settings::defaults(development).clipboard_hotkey,
        Provider::DEFAULT.id(),
        Effort::default().id()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMENTED: &str = "# Raiden, my way.\n\
        \n\
        # Open it with both hands.\n\
        hotkey = \"⌥Space\" # was ⌘Space\n\
        \n\
        # More settings come later.\n";

    #[test]
    fn the_starter_names_every_terminal_in_the_order_raiden_tries_them() {
        let text = starter(false);
        for terminal in crate::terminals::Terminal::ALL {
            assert!(text.contains(&format!("\"{}\"", terminal.id())));
        }
        let names: Vec<&str> = crate::terminals::Terminal::PREFERRED
            .iter()
            .map(|terminal| terminal.name())
            .collect();
        let (last, rest) = names.split_last().expect("some terminals");
        assert!(text.contains(&format!("{} and {last}.", rest.join(", "))));
    }

    fn hotkey(text: &str) -> Hotkey {
        Hotkey::parse(text).expect("a valid hotkey")
    }

    #[test]
    fn writing_a_setting_keeps_every_comment() {
        let written = set(COMMENTED, HOTKEY, "⌃⌥Space").expect("valid TOML");
        for comment in COMMENTED
            .lines()
            .filter_map(|line| line.find('#').map(|at| &line[at..]))
        {
            assert!(written.contains(comment), "lost {comment:?} in {written}");
        }
        let loaded = load(Some(&written), true, None);
        assert_eq!(loaded.settings.hotkey, hotkey("⌃⌥Space"));
        assert!(loaded.problems.is_empty());
    }

    #[test]
    fn writing_a_setting_the_file_lacks_adds_it_and_keeps_every_comment() {
        let starter = starter(true);
        let written = set(&starter, HOTKEY, "⌘⇧K").expect("valid TOML");
        for line in starter.lines().filter(|line| line.starts_with('#')) {
            assert!(written.contains(line), "lost {line:?} in {written}");
        }
        assert!(
            written.ends_with("# effort = \"low\"\nhotkey = \"⌘⇧K\"\n"),
            "{written}"
        );
        assert_eq!(
            load(Some(&written), true, None).settings.hotkey,
            hotkey("⌘⇧K")
        );
    }

    #[test]
    fn choosing_an_ai_writes_it_and_keeps_every_comment() {
        let written = set(COMMENTED, AI, "chatgpt").expect("valid TOML");
        for comment in COMMENTED
            .lines()
            .filter_map(|line| line.find('#').map(|at| &line[at..]))
        {
            assert!(written.contains(comment), "lost {comment:?} in {written}");
        }
        let loaded = load(Some(&written), true, None);
        assert_eq!(loaded.settings.ai, Provider::Chatgpt);
        assert_eq!(loaded.settings.hotkey, hotkey("⌥Space"));
        assert!(loaded.problems.is_empty());
        let back = set(&written, AI, "claude").expect("valid TOML");
        assert_eq!(back.matches("ai = ").count(), 1, "{back}");
        assert_eq!(load(Some(&back), true, None).settings.ai, Provider::Claude);
    }

    #[test]
    fn choosing_an_ai_in_the_starter_file_keeps_its_comments() {
        let starter = starter(false);
        let written = set(&starter, AI, "chatgpt").expect("valid TOML");
        for line in starter.lines().filter(|line| line.starts_with('#')) {
            assert!(written.contains(line), "lost {line:?} in {written}");
        }
        assert!(
            written.ends_with("# effort = \"low\"\nai = \"chatgpt\"\n"),
            "{written}"
        );
    }

    #[test]
    fn the_ai_is_read_from_the_file_and_claude_when_left_out() {
        let chosen = load(Some("ai = \"chatgpt\""), false, None);
        assert_eq!(chosen.settings.ai, Provider::Chatgpt);
        assert!(chosen.problems.is_empty());
        assert_eq!(
            load(Some(&starter(false)), false, None).settings.ai,
            Provider::Claude
        );
    }

    #[test]
    fn an_unknown_ai_keeps_the_one_in_use_and_says_so() {
        let previous = Settings {
            ai: Provider::Chatgpt,
            ..Settings::defaults(true)
        };
        for text in ["ai = \"gemini\"", "ai = \"ChatGPT\"", "ai = 5", "ai = \"\""] {
            let loaded = load(Some(text), true, Some(&previous));
            assert_eq!(loaded.settings, previous, "{text}");
            assert_eq!(loaded.problems.len(), 1, "{text}");
            assert!(
                loaded.problems[0].contains("\"claude\" or \"chatgpt\"")
                    && loaded.problems[0].contains("keeps \"chatgpt\""),
                "{}",
                loaded.problems[0]
            );
        }
        let first = load(Some("ai = \"gemini\""), true, None);
        assert_eq!(first.settings.ai, Provider::Claude);
    }

    #[test]
    fn the_effort_is_read_from_the_file_and_low_when_left_out() {
        for effort in Effort::ALL {
            let chosen = load(Some(&format!("effort = \"{}\"", effort.id())), false, None);
            assert_eq!(chosen.settings.effort, effort);
            assert!(chosen.problems.is_empty());
        }
        assert_eq!(
            load(Some(&starter(false)), false, None).settings.effort,
            Effort::Low
        );
        assert_eq!(load(None, false, None).settings.effort, Effort::Low);
    }

    #[test]
    fn an_unknown_effort_keeps_the_one_in_use_and_says_so() {
        let previous = Settings {
            effort: Effort::Medium,
            ..Settings::defaults(true)
        };
        for text in [
            "effort = \"max\"",
            "effort = \"xhigh\"",
            "effort = \"High\"",
            "effort = 2",
            "effort = \"\"",
        ] {
            let loaded = load(Some(text), true, Some(&previous));
            assert_eq!(loaded.settings, previous, "{text}");
            assert_eq!(
                loaded.problems,
                ["effort must be \"low\", \"medium\" or \"high\". Raiden keeps \"medium\"."],
                "{text}"
            );
        }
        let first = load(Some("effort = \"max\""), true, None);
        assert_eq!(first.settings.effort, Effort::Low);
        let beside = load(Some("ai = \"chatgpt\"\neffort = \"max\""), true, None);
        assert_eq!(beside.settings.ai, Provider::Chatgpt);
    }

    #[test]
    fn choosing_an_effort_writes_it_and_keeps_every_comment() {
        let written = set(COMMENTED, EFFORT, "high").expect("valid TOML");
        for comment in COMMENTED
            .lines()
            .filter_map(|line| line.find('#').map(|at| &line[at..]))
        {
            assert!(written.contains(comment), "lost {comment:?} in {written}");
        }
        let loaded = load(Some(&written), true, None);
        assert_eq!(loaded.settings.effort, Effort::High);
        assert_eq!(loaded.settings.hotkey, hotkey("⌥Space"));
        assert!(loaded.problems.is_empty());
        let back = set(&written, EFFORT, "medium").expect("valid TOML");
        assert_eq!(back.matches("effort = ").count(), 1, "{back}");
        assert_eq!(
            load(Some(&back), true, None).settings.effort,
            Effort::Medium
        );
        let starter = starter(false);
        let started = set(&starter, EFFORT, "high").expect("valid TOML");
        for line in starter.lines().filter(|line| line.starts_with('#')) {
            assert!(started.contains(line), "lost {line:?} in {started}");
        }
        assert!(
            started.ends_with("# effort = \"low\"\neffort = \"high\"\n"),
            "{started}"
        );
    }

    #[test]
    fn the_starter_file_says_fix_grammar_stays_at_low_effort() {
        let starter = starter(false);
        assert!(starter.contains("# effort = \"low\""));
        assert!(starter.contains("Fix grammar always uses low"));
    }

    fn pins(repos: &[&str]) -> Value {
        list(
            &repos
                .iter()
                .map(|repo| (*repo).to_owned())
                .collect::<Vec<_>>(),
        )
    }

    fn keeps_comments(before: &str, after: &str) {
        for comment in before
            .lines()
            .filter_map(|line| line.find('#').map(|at| &line[at..]))
        {
            assert!(after.contains(comment), "lost {comment:?} in {after}");
        }
    }

    #[test]
    fn pinning_writes_the_list_and_keeps_every_comment() {
        let written = set(COMMENTED, PINNED_REPOS, pins(&["fernhill/ledger"])).expect("valid TOML");
        keeps_comments(COMMENTED, &written);
        assert!(
            written.ends_with("pinned_repos = [\"fernhill/ledger\"]\n"),
            "{written}"
        );
        let loaded = load(Some(&written), true, None);
        assert_eq!(loaded.settings.pinned_repos, ["fernhill/ledger"]);
        assert_eq!(loaded.settings.hotkey, hotkey("⌥Space"));
        assert!(loaded.problems.is_empty());
        let more = set(
            &written,
            PINNED_REPOS,
            pins(&["fernhill/ledger", "tidewater/lantern"]),
        )
        .expect("valid TOML");
        assert_eq!(more.matches("pinned_repos = ").count(), 1, "{more}");
        assert_eq!(
            load(Some(&more), true, None).settings.pinned_repos,
            ["fernhill/ledger", "tidewater/lantern"]
        );
        let none = set(&more, PINNED_REPOS, pins(&[])).expect("valid TOML");
        assert!(
            load(Some(&none), true, None)
                .settings
                .pinned_repos
                .is_empty()
        );
    }

    #[test]
    fn pinning_into_a_list_written_by_hand_keeps_its_comments() {
        let mine = "# my pins\npinned_repos = [\n  \"fernhill/ledger\", # the money one\n  \"tidewater/lantern\",\n] # done\n";
        let added = set(
            mine,
            PINNED_REPOS,
            pins(&["fernhill/ledger", "tidewater/lantern", "kestrel-io/site"]),
        )
        .expect("valid TOML");
        keeps_comments(mine, &added);
        assert_eq!(
            load(Some(&added), true, None).settings.pinned_repos,
            ["fernhill/ledger", "tidewater/lantern", "kestrel-io/site"]
        );
        let removed = set(
            &added,
            PINNED_REPOS,
            pins(&["fernhill/ledger", "kestrel-io/site"]),
        )
        .expect("valid TOML");
        assert!(removed.contains("# the money one"), "{removed}");
        assert!(
            removed.contains("# my pins") && removed.contains("# done"),
            "{removed}"
        );
        assert_eq!(
            load(Some(&removed), true, None).settings.pinned_repos,
            ["fernhill/ledger", "kestrel-io/site"]
        );
    }

    #[test]
    fn continuing_writes_the_flag_into_the_starter_and_keeps_its_comments() {
        let starter = starter(false);
        assert!(!load(Some(&starter), false, None).settings.pull_requests);
        let written = set(&starter, PULL_REQUESTS, true).expect("valid TOML");
        for line in starter.lines().filter(|line| line.starts_with('#')) {
            assert!(written.contains(line), "lost {line:?} in {written}");
        }
        assert!(
            written.ends_with("# effort = \"low\"\npull_requests = true\n"),
            "{written}"
        );
        let loaded = load(Some(&written), false, None);
        assert!(loaded.settings.pull_requests);
        assert!(loaded.problems.is_empty());
        let mine = set(COMMENTED, PULL_REQUESTS, true).expect("valid TOML");
        keeps_comments(COMMENTED, &mine);
        let off = set(&mine, PULL_REQUESTS, false).expect("valid TOML");
        assert_eq!(off.matches("pull_requests = ").count(), 1, "{off}");
        assert!(!load(Some(&off), false, None).settings.pull_requests);
    }

    #[test]
    fn bad_pins_and_flags_keep_what_was_there_and_say_so() {
        let previous = Settings {
            pinned_repos: vec!["fernhill/ledger".into()],
            pull_requests: true,
            ..Settings::defaults(true)
        };
        for text in ["pinned_repos = \"fernhill/ledger\"", "pinned_repos = 5"] {
            let loaded = load(Some(text), true, Some(&previous));
            assert_eq!(
                loaded.settings.pinned_repos, previous.pinned_repos,
                "{text}"
            );
            assert_eq!(
                loaded.problems,
                [
                    "pinned_repos must be a list such as [\"owner/repo\"]. Raiden keeps [\"fernhill/ledger\"]."
                ],
                "{text}"
            );
        }
        let mixed = load(
            Some("pinned_repos = [\"tidewater/lantern\", \"fernhill\", 3, \"Tidewater/Lantern\"]"),
            true,
            Some(&previous),
        );
        assert_eq!(mixed.settings.pinned_repos, ["tidewater/lantern"]);
        assert_eq!(
            mixed.problems,
            [
                "pinned_repos holds \"fernhill\", 3, which are not \"owner/repo\", so Raiden leaves them out."
            ]
        );
        for text in ["pull_requests = \"yes\"", "pull_requests = 1"] {
            let loaded = load(Some(text), true, Some(&previous));
            assert!(loaded.settings.pull_requests, "{text}");
            assert_eq!(
                loaded.problems,
                ["pull_requests must be true or false. Raiden keeps true."],
                "{text}"
            );
        }
        assert!(
            !load(Some("pull_requests = 1"), true, None)
                .settings
                .pull_requests
        );
    }

    #[test]
    fn the_starter_file_names_the_pull_requests_settings() {
        let starter = starter(false);
        assert!(starter.contains("# pinned_repos = []"));
        assert!(starter.contains("# pull_requests = false"));
        assert!(
            load(Some("pinned_repos = []\npull_requests = true"), false, None)
                .problems
                .is_empty()
        );
    }

    #[test]
    fn vim_keys_are_read_from_the_file_and_off_when_left_out() {
        assert!(load(Some("vim = true"), false, None).settings.vim);
        assert!(!load(Some("vim = false"), false, None).settings.vim);
        assert!(!load(None, false, None).settings.vim);
        assert!(!load(Some(&starter(false)), false, None).settings.vim);
        assert!(load(Some("vim = true"), false, None).problems.is_empty());
    }

    #[test]
    fn the_fixer_key_is_read_from_the_file_and_empty_means_none() {
        let key = "fixer_key = \"0123456789abcdef0123456789abcdef\"";
        assert_eq!(
            load(Some(key), false, None).settings.fixer_key.as_deref(),
            Some("0123456789abcdef0123456789abcdef")
        );
        assert_eq!(
            load(Some("fixer_key = \"\""), false, None)
                .settings
                .fixer_key,
            None
        );
        assert_eq!(
            load(Some(&starter(false)), false, None).settings.fixer_key,
            None
        );
        let previous = Settings {
            fixer_key: Some("kept".to_owned()),
            ..Settings::defaults(false)
        };
        let loaded = load(Some("fixer_key = 5"), false, Some(&previous));
        assert_eq!(loaded.settings.fixer_key.as_deref(), Some("kept"));
        assert_eq!(loaded.problems.len(), 1);
    }

    #[test]
    fn a_vim_value_that_is_not_true_or_false_keeps_the_one_in_use_and_says_so() {
        let off = load(Some("vim = \"yes\""), false, None);
        assert!(!off.settings.vim);
        assert_eq!(
            off.problems,
            ["vim must be true or false. Raiden keeps false."]
        );
        let previous = Settings {
            vim: true,
            ..Settings::defaults(false)
        };
        for text in ["vim = \"yes\"", "vim = 1", "vim = \"\""] {
            let loaded = load(Some(text), false, Some(&previous));
            assert!(loaded.settings.vim, "{text}");
            assert_eq!(
                loaded.problems,
                ["vim must be true or false. Raiden keeps true."],
                "{text}"
            );
        }
    }

    #[test]
    fn turning_vim_keys_on_writes_the_flag_and_keeps_every_comment() {
        let starter = starter(false);
        assert!(starter.contains("# vim = false"));
        let written = set(&starter, VIM, true).expect("valid TOML");
        for line in starter.lines().filter(|line| line.starts_with('#')) {
            assert!(written.contains(line), "lost {line:?} in {written}");
        }
        assert!(
            written.ends_with("# effort = \"low\"\nvim = true\n"),
            "{written}"
        );
        assert!(load(Some(&written), false, None).settings.vim);
        let mine = set(COMMENTED, VIM, true).expect("valid TOML");
        keeps_comments(COMMENTED, &mine);
        let off = set(&mine, VIM, false).expect("valid TOML");
        assert_eq!(off.matches("vim = ").count(), 1, "{off}");
        assert!(!load(Some(&off), false, None).settings.vim);
    }

    #[test]
    fn the_terminal_is_read_from_the_file_and_left_to_what_is_installed_when_out() {
        for terminal in Terminal::ALL {
            let chosen = load(
                Some(&format!("terminal = \"{}\"", terminal.id())),
                false,
                None,
            );
            assert_eq!(chosen.settings.terminal, Some(terminal));
            assert!(chosen.problems.is_empty());
        }
        assert_eq!(load(None, false, None).settings.terminal, None);
        assert_eq!(
            load(Some(&starter(false)), false, None).settings.terminal,
            None
        );
    }

    #[test]
    fn an_unknown_terminal_keeps_the_one_in_use_and_says_so() {
        let previous = Settings {
            terminal: Some(Terminal::Iterm),
            ..Settings::defaults(false)
        };
        for text in [
            "terminal = \"wezterm\"",
            "terminal = \"Ghostty\"",
            "terminal = \"auto\"",
            "terminal = 1",
            "terminal = \"\"",
        ] {
            let loaded = load(Some(text), false, Some(&previous));
            assert_eq!(loaded.settings, previous, "{text}");
            assert_eq!(
                loaded.problems,
                [
                    "terminal must be \"ghostty\", \"iterm\", \"rex\", \"tern\", \"terminal\", \"kitty\", \"alacritty\" or \"warp\". Raiden keeps \"iterm\"."
                ],
                "{text}"
            );
        }
        let first = load(Some("terminal = \"wezterm\""), false, None);
        assert_eq!(first.settings.terminal, None);
        assert!(
            first.problems[0].ends_with("Raiden keeps the first one installed."),
            "{}",
            first.problems[0]
        );
    }

    #[test]
    fn choosing_a_terminal_writes_it_and_keeps_every_comment() {
        let starter = starter(false);
        assert!(starter.contains("# terminal = \"ghostty\""));
        let written = set(&starter, TERMINAL, "rex").expect("valid TOML");
        keeps_comments(&starter, &written);
        assert!(
            written.ends_with("# effort = \"low\"\nterminal = \"rex\"\n"),
            "{written}"
        );
        assert_eq!(
            load(Some(&written), false, None).settings.terminal,
            Some(Terminal::Rex)
        );
        let again = set(&written, TERMINAL, "tern").expect("valid TOML");
        assert_eq!(again.matches("\nterminal = ").count(), 1, "{again}");
        let mine = set(COMMENTED, TERMINAL, "kitty").expect("valid TOML");
        keeps_comments(COMMENTED, &mine);
    }

    #[test]
    fn writing_over_a_table_leaves_it_alone() {
        let table = "# mine\n[hotkey]\nkey = \"Space\" # keep\n";
        assert!(set(table, HOTKEY, "⌥Space").is_err());
    }

    #[test]
    fn writing_into_a_broken_file_leaves_it_alone() {
        assert!(set("hotkey = ", HOTKEY, "⌥Space").is_err());
    }

    #[test]
    fn a_setting_missing_from_the_file_falls_back_to_its_default() {
        let loaded = load(
            Some("# nothing set yet\n"),
            true,
            Some(&Settings::defaults(false)),
        );
        assert_eq!(loaded.settings, Settings::defaults(true));
        assert!(loaded.problems.is_empty());
        assert_eq!(
            load(Some(&starter(false)), false, None).settings,
            Settings::defaults(false)
        );
    }

    #[test]
    fn a_missing_file_means_the_defaults() {
        assert_eq!(
            load(None, false, None),
            Loaded {
                settings: Settings::defaults(false),
                problems: Vec::new()
            }
        );
    }

    #[test]
    fn a_broken_file_keeps_what_was_there_and_says_why() {
        let previous = Settings {
            hotkey: hotkey("⌃⌥Space"),
            ..Settings::defaults(true)
        };
        let loaded = load(Some("hotkey = \"⌥Space"), true, Some(&previous));
        assert_eq!(loaded.settings, previous);
        assert_eq!(loaded.problems.len(), 1);
        let first = load(Some("hotkey = \"⌥Space"), true, None);
        assert_eq!(first.settings, Settings::defaults(true));
    }

    #[test]
    fn a_bad_hotkey_keeps_the_one_in_use() {
        let previous = Settings {
            hotkey: hotkey("⌃⌥Space"),
            ..Settings::defaults(true)
        };
        for text in [
            "hotkey = \"Space\"",
            "hotkey = \"⇧Space\"",
            "hotkey = 5",
            "hotkey = \"⌥\"",
        ] {
            let loaded = load(Some(text), true, Some(&previous));
            assert_eq!(loaded.settings, previous, "{text}");
            assert_eq!(loaded.problems.len(), 1, "{text}");
        }
    }

    #[test]
    fn the_clipboard_hotkey_is_read_from_the_file() {
        let chosen = load(Some("clipboard_hotkey = \"⌥⌘V\""), false, None);
        assert_eq!(chosen.settings.clipboard_hotkey, hotkey("⌥⌘V"));
        assert!(chosen.problems.is_empty());
        let written = set(&starter(false), CLIPBOARD_HOTKEY, "⌃⌘V").expect("valid TOML");
        let loaded = load(Some(&written), false, None);
        assert_eq!(loaded.settings.clipboard_hotkey, hotkey("⌃⌘V"));
        assert_eq!(loaded.settings.hotkey, Settings::defaults(false).hotkey);
    }

    #[test]
    fn a_clipboard_hotkey_that_opens_raiden_keeps_the_one_in_use() {
        let loaded = load(
            Some("hotkey = \"⌃⌥K\"\nclipboard_hotkey = \"Control+Option+K\""),
            false,
            None,
        );
        assert_eq!(loaded.settings.hotkey, hotkey("⌃⌥K"));
        assert_eq!(
            loaded.settings.clipboard_hotkey,
            Settings::defaults(false).clipboard_hotkey
        );
        assert_eq!(
            loaded.problems,
            ["clipboard_hotkey: ⌃⌥K already opens Raiden. Raiden keeps ⌃⌥V."]
        );
        let main_takes_it = load(Some("hotkey = \"⌃⌥V\""), false, None);
        assert_eq!(main_takes_it.settings.hotkey, hotkey("⌃⌥V"));
        assert_eq!(main_takes_it.settings.clipboard_hotkey, hotkey("⌃⌥V"));
        assert!(main_takes_it.problems[0].contains("opens only from its command"));
        let previous = Settings {
            clipboard_hotkey: hotkey("⌃⌘V"),
            ..Settings::defaults(false)
        };
        let moved = load(Some("hotkey = \"⌃⌥V\""), false, Some(&previous));
        assert_eq!(moved.settings.clipboard_hotkey, hotkey("⌃⌘V"));
        let refused = load(Some("clipboard_hotkey = \"⌘Space\""), false, None);
        assert_eq!(
            refused.problems,
            [
                "clipboard_hotkey: ⌘Space can open only Raiden, not Clipboard history. Raiden keeps ⌃⌥V."
            ]
        );
    }

    #[test]
    fn unknown_settings_are_reported() {
        let loaded = load(Some("hotkye = \"⌥Space\""), false, None);
        assert_eq!(loaded.settings, Settings::defaults(false));
        assert!(loaded.problems[0].contains("hotkye"));
    }

    #[test]
    fn command_space_opens_raiden_in_both_copies() {
        for development in [false, true] {
            let loaded = load(Some("hotkey = \"⌘Space\""), development, None);
            assert_eq!(loaded.settings.hotkey, hotkey("⌘Space"));
            assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
        }
        let spelled = load(Some("hotkey = \"Command+Space\""), false, None);
        assert_eq!(spelled.settings.hotkey, hotkey("⌘Space"));
    }

    #[test]
    fn command_space_never_opens_clipboard_history() {
        for development in [false, true] {
            let previous = Settings {
                clipboard_hotkey: hotkey("⌃⌘V"),
                ..Settings::defaults(development)
            };
            let loaded = load(
                Some("clipboard_hotkey = \"⌘Space\""),
                development,
                Some(&previous),
            );
            assert_eq!(loaded.settings.clipboard_hotkey, hotkey("⌃⌘V"));
            assert_eq!(loaded.problems.len(), 1);
            assert!(
                loaded.problems[0].contains(ONLY_RAIDEN),
                "{:?}",
                loaded.problems
            );
        }
        let both = load(
            Some("hotkey = \"⌘Space\"\nclipboard_hotkey = \"⌘Space\""),
            false,
            None,
        );
        assert_eq!(both.settings.hotkey, hotkey("⌘Space"));
        assert_eq!(both.settings.clipboard_hotkey, hotkey("⌃⌥V"));
    }

    #[test]
    fn a_recorded_hotkey_is_refused_for_the_same_reason_the_file_is() {
        assert_eq!(
            Hotkey::chosen("⌘Space", HOTKEY).map(|hotkey| hotkey.to_string()),
            Ok("⌘Space".to_owned())
        );
        let refused = Hotkey::chosen("⌘Space", CLIPBOARD_HOTKEY).expect_err("⌘Space");
        assert_eq!(refused, ONLY_RAIDEN);
        let loaded = load(Some("clipboard_hotkey = \"⌘Space\""), false, None);
        assert!(
            loaded.problems[0].contains(&refused),
            "{}",
            loaded.problems[0]
        );
        assert_eq!(
            Hotkey::chosen("⌥⌘Space", CLIPBOARD_HOTKEY).map(|hotkey| hotkey.to_string()),
            Ok("⌥⌘Space".to_owned())
        );
        assert!(
            Hotkey::chosen("K", HOTKEY)
                .expect_err("no modifier")
                .contains("needs Control, Option or Command")
        );
    }

    #[test]
    fn the_starter_says_command_space_needs_spotlights_shortcut_off() {
        for development in [false, true] {
            let starter = starter(development);
            assert!(
                starter.contains("# ⌘Space works once Spotlight's shortcut is off"),
                "{starter}"
            );
            assert!(!starter.contains("Not ⌘Space"), "{starter}");
        }
    }

    #[test]
    fn recorded_key_codes_become_the_key_names_the_file_uses() {
        for (code, name) in [
            ("KeyA", "A"),
            ("Digit0", "0"),
            ("Space", "Space"),
            ("F5", "F5"),
            ("ArrowUp", "ArrowUp"),
            ("Comma", "Comma"),
            ("Keyboard", "Keyboard"),
            ("Digit", "Digit"),
            ("KeyAB", "KeyAB"),
        ] {
            assert_eq!(key_name(code), name, "{code}");
            if name.len() == 1 || ["Space", "F5", "Comma"].contains(&name) {
                let hotkey = Hotkey::parse(&format!("⌃⌥{name}")).expect(name);
                assert_eq!(hotkey.key, name);
            }
        }
    }

    #[test]
    fn the_settings_file_is_shown_from_home() {
        let home = Path::new("/Users/me");
        assert_eq!(
            shown_path(&home.join(".config/raiden/raiden.toml"), home),
            "~/.config/raiden/raiden.toml"
        );
        assert_eq!(
            shown_path(Path::new("/tmp/dev/raiden.toml"), home),
            "/tmp/dev/raiden.toml"
        );
        assert_eq!(
            shown_path(Path::new("/tmp/raiden.toml"), Path::new("")),
            "/tmp/raiden.toml"
        );
    }

    #[test]
    fn hotkeys_read_as_symbols_or_words() {
        assert_eq!(hotkey("⌥⇧Space"), Settings::defaults(true).hotkey);
        assert_eq!(
            hotkey(" Alt + Shift + space "),
            Settings::defaults(true).hotkey
        );
        assert_eq!(hotkey("cmd+shift+k").to_string(), "⇧⌘K");
        assert_eq!(hotkey("⌘⇧K").accelerator(), "Shift+Super+K");
        assert_eq!(hotkey("⌥F5").accelerator(), "Alt+F5");
        assert_eq!(hotkey("⌥+Space"), Settings::defaults(false).hotkey);
        assert_eq!(hotkey("⌥ + ⇧ + Space"), Settings::defaults(true).hotkey);
        assert_eq!(hotkey("Option+⌘Space").to_string(), "⌥⌘Space");
        assert_eq!(hotkey("⌃ Option+Space").to_string(), "⌃⌥Space");
    }

    #[test]
    fn a_hotkey_that_cannot_be_read_says_what_is_wrong() {
        let why = |text: &str| Hotkey::parse(text).expect_err(text);
        assert!(why("Hyper+Space").contains("\"Hyper\" in \"Hyper+Space\" is not a modifier key"));
        assert!(why("⌥Page Down").contains("\"Page Down\" in \"⌥Page Down\" is not a key name"));
        assert!(why("⌃⌥").contains("ends with a modifier"));
        assert!(why("Option+").contains("ends with a modifier"));
        assert!(why(" ").contains("empty"));
        assert!(why("⇧Space").contains("needs Control, Option or Command"));
    }
}
