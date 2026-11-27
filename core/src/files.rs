use std::cmp::Ordering;

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

pub const SHORTEST: usize = 3;
pub const SHOWN: usize = 8;
pub const GATHERED: usize = 5000;

const PACKAGES: [&str; 12] = [
    "app",
    "appex",
    "bundle",
    "framework",
    "kext",
    "musiclibrary",
    "photoslibrary",
    "plugin",
    "xcarchive",
    "xcodeproj",
    "xcworkspace",
    "xpc",
];
const NOISE: [&str; 6] = [
    "node_modules",
    "__pycache__",
    "Caches",
    "DerivedData",
    "Pods",
    "target",
];
const ICLOUD_DRIVE: &str = "Library/Mobile Documents/com~apple~CloudDocs/";
const LEFT_OUT_AT_HOME: [&str; 2] = ["Library", "Applications"];

#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub path: String,
    pub used_at: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FileRow {
    pub name: String,
    pub path: String,
    pub folder: String,
    pub icon: String,
}

fn words(query: &str) -> Vec<&str> {
    query.split_whitespace().collect()
}

/// ```
/// assert!(!raiden_core::files::searchable("ab"));
/// assert!(!raiden_core::files::searchable(" a b "));
/// assert!(raiden_core::files::searchable("abc"));
/// ```
pub fn searchable(query: &str) -> bool {
    words(query)
        .iter()
        .any(|word| word.chars().count() >= SHORTEST)
}

fn escaped(word: &str) -> String {
    word.chars()
        .flat_map(|letter| match letter {
            '\\' | '"' | '*' | '?' => vec!['\\', letter],
            other if other.is_control() => vec![' '],
            other => vec![other],
        })
        .collect()
}

/// ```
/// use raiden_core::files::spotlight_query;
/// assert_eq!(
///     spotlight_query("AGENTS.md").as_deref(),
///     Some(r#"kMDItemFSName == "*AGENTS.md*"cd"#),
/// );
/// assert_eq!(spotlight_query("ab"), None);
/// ```
pub fn spotlight_query(query: &str) -> Option<String> {
    if !searchable(query) {
        return None;
    }
    let parts: Vec<String> = words(query)
        .into_iter()
        .map(|word| format!("kMDItemFSName == \"*{}*\"cd", escaped(word)))
        .collect();
    Some(parts.join(" && "))
}

fn inside_home<'a>(path: &'a str, home: &str) -> Option<&'a str> {
    let home = home.trim_end_matches('/');
    if home.is_empty() {
        return None;
    }
    path.strip_prefix(home)?
        .strip_prefix('/')
        .filter(|rest| !rest.is_empty())
}

fn is_package(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(stem, extension)| !stem.is_empty() && PACKAGES.contains(&extension))
}

/// ```
/// use raiden_core::files::listed;
/// assert!(listed("notes.md", false));
/// assert!(!listed(".git", false));
/// assert!(!listed("Library", true));
/// assert!(listed("Library", false));
/// ```
pub fn listed(name: &str, at_home: bool) -> bool {
    !name.is_empty() && !name.starts_with('.') && !(at_home && LEFT_OUT_AT_HOME.contains(&name))
}

/// ```
/// use raiden_core::files::opened;
/// assert!(opened("src"));
/// assert!(!opened("node_modules"));
/// assert!(!opened("Raiden.app"));
/// ```
pub fn opened(folder: &str) -> bool {
    !NOISE.contains(&folder) && !is_package(folder)
}

pub fn icloud_drive(home: &str) -> String {
    format!(
        "{}/{}",
        home.trim_end_matches('/'),
        ICLOUD_DRIVE.trim_end_matches('/')
    )
}

/// ```
/// use raiden_core::files::kept;
/// assert!(kept("/Users/me/Code/lantern/AGENTS.md", "/Users/me"));
/// assert!(!kept("/Users/me/Library/Caches/x.md", "/Users/me"));
/// assert!(!kept("/Users/me/Code/app/node_modules/x/README.md", "/Users/me"));
/// assert!(!kept("/System/Library/x.md", "/Users/me"));
/// ```
pub fn kept(path: &str, home: &str) -> bool {
    let Some(rest) = inside_home(path, home) else {
        return false;
    };
    let in_icloud = rest.strip_prefix(ICLOUD_DRIVE);
    let parts: Vec<&str> = in_icloud.unwrap_or(rest).split('/').collect();
    if in_icloud.is_none()
        && parts
            .first()
            .is_some_and(|first| LEFT_OUT_AT_HOME.contains(first))
    {
        return false;
    }
    let Some((_, folders)) = parts.split_last() else {
        return false;
    };
    parts
        .iter()
        .all(|part| !part.is_empty() && !part.starts_with('.') && *part != "..")
        && folders.iter().all(|folder| opened(folder))
}

fn name_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// ```
/// use raiden_core::files::folder_label;
/// assert_eq!(folder_label("/Users/me/Code/lantern/AGENTS.md", "/Users/me"), "~/Code/lantern");
/// assert_eq!(folder_label("/Users/me/notes.md", "/Users/me"), "~");
/// ```
pub fn folder_label(path: &str, home: &str) -> String {
    let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
    match inside_home(parent, home) {
        Some(rest) if format!("{rest}/") == ICLOUD_DRIVE => "iCloud Drive".to_owned(),
        Some(rest) => match rest.strip_prefix(ICLOUD_DRIVE) {
            Some(inside) => format!("iCloud Drive/{inside}"),
            None => format!("~/{rest}"),
        },
        None if parent == home.trim_end_matches('/') => "~".to_owned(),
        None if parent.is_empty() => "/".to_owned(),
        None => parent.to_owned(),
    }
}

/// ```
/// use raiden_core::files::icon_key;
/// assert_eq!(icon_key("/Users/me/notes.MD", true), ".md");
/// assert_eq!(icon_key("/Users/me/Projects", false), "/Users/me/Projects");
/// assert_eq!(icon_key("/Users/me/Makefile", true), "/Users/me/Makefile");
/// ```
pub fn icon_key(path: &str, is_file: bool) -> String {
    let extension = name_of(path)
        .rsplit_once('.')
        .filter(|(stem, extension)| !stem.is_empty() && !extension.is_empty());
    match extension {
        Some((_, extension)) if is_file => format!(".{}", extension.to_lowercase()),
        _ => path.to_owned(),
    }
}

fn starts_word(name: &str, at: usize) -> bool {
    name[..at]
        .chars()
        .next_back()
        .is_none_or(|before| !before.is_alphanumeric())
}

/// ```
/// assert_eq!(raiden_core::files::fold("Canción FINAL"), "cancion final");
/// ```
pub fn fold(text: &str) -> String {
    text.to_lowercase()
        .nfd()
        .filter(|letter| !is_combining_mark(*letter))
        .collect()
}

pub fn closeness(name: &str, query: &str) -> u8 {
    closeness_of_folded(&fold(name), &fold(query))
}

pub fn closeness_of_folded(name: &str, query: &str) -> u8 {
    let query = query.trim();
    if name == query {
        return 4;
    }
    if name.starts_with(query) {
        return 3;
    }
    let all_words_start = words(query).iter().all(|word| {
        name.match_indices(word)
            .any(|(at, _)| starts_word(name, at))
    });
    if all_words_start { 2 } else { 1 }
}

fn depth(path: &str) -> usize {
    path.matches('/').count()
}

/// ```
/// use raiden_core::files::{ranked, Found};
/// let found = |path: &str| Found { path: path.into(), used_at: None };
/// let rows = ranked(
///     vec![found("/Users/me/Code/old-AGENTS.md.bak"), found("/Users/me/Code/lantern/AGENTS.md")],
///     "agents.md",
///     "/Users/me",
///     |_| true,
/// );
/// assert_eq!(rows[0].path, "/Users/me/Code/lantern/AGENTS.md");
/// assert_eq!(rows[0].folder, "~/Code/lantern");
/// ```
pub fn ranked(
    found: Vec<Found>,
    query: &str,
    home: &str,
    is_file: impl Fn(&str) -> bool,
) -> Vec<FileRow> {
    let mut scored: Vec<(u8, Found)> = found
        .into_iter()
        .filter(|item| kept(&item.path, home))
        .map(|item| (closeness(name_of(&item.path), query), item))
        .collect();
    scored.sort_by(|(a_close, a), (b_close, b)| {
        b_close
            .cmp(a_close)
            .then_with(|| {
                b.used_at
                    .unwrap_or(f64::NEG_INFINITY)
                    .partial_cmp(&a.used_at.unwrap_or(f64::NEG_INFINITY))
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| depth(&a.path).cmp(&depth(&b.path)))
            .then_with(|| a.path.cmp(&b.path))
    });
    scored.dedup_by(|(_, a), (_, b)| a.path == b.path);
    scored
        .into_iter()
        .take(SHOWN)
        .map(|(_, item)| FileRow {
            name: name_of(&item.path).to_owned(),
            folder: folder_label(&item.path, home),
            icon: icon_key(&item.path, is_file(&item.path)),
            path: item.path,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/Users/me";

    fn found(path: &str, used_at: Option<f64>) -> Found {
        Found {
            path: path.to_owned(),
            used_at,
        }
    }

    fn paths(rows: &[FileRow]) -> Vec<&str> {
        rows.iter().map(|row| row.path.as_str()).collect()
    }

    #[test]
    fn each_word_must_be_in_the_name() {
        assert_eq!(
            spotlight_query("  release   notes ").as_deref(),
            Some(r#"kMDItemFSName == "*release*"cd && kMDItemFSName == "*notes*"cd"#)
        );
    }

    #[test]
    fn quotes_wildcards_and_backslashes_are_taken_literally() {
        assert_eq!(
            spotlight_query(r#"a"b*c?\d'e"#).as_deref(),
            Some(r#"kMDItemFSName == "*a\"b\*c\?\\d'e*"cd"#)
        );
        assert_eq!(
            spotlight_query("ab\u{7}c").as_deref(),
            Some(r#"kMDItemFSName == "*ab c*"cd"#)
        );
    }

    #[test]
    fn one_or_two_letters_search_no_files() {
        for short in ["", "a", "ab", " a ", "a b", "a b c", "ab cd", "\t\n"] {
            assert_eq!(spotlight_query(short), None, "{short:?}");
        }
        assert!(spotlight_query("a bcd").is_some());
        assert!(spotlight_query("éé€").is_some());
    }

    #[test]
    fn noise_is_left_out() {
        for noise in [
            "/Users/me/Library/Mobile Documents/notes.md",
            "/Users/me/Applications/Notes.app",
            "/Users/me/.config/notes.md",
            "/Users/me/Code/lantern/.git/notes.md",
            "/Users/me/Code/lantern/.cache/x/AGENTS.md",
            "/Users/me/Code/web/node_modules/notes/index.js",
            "/Users/me/Code/py/__pycache__/notes.pyc",
            "/Users/me/Code/x/Caches/notes",
            "/Users/me/Code/rust/target/debug/notes.d",
            "/Users/me/Code/ios/Pods/notes.h",
            "/Users/me/Code/App.xcodeproj/project.pbxproj",
            "/Users/me/Library/Mobile Documents/com~apple~Pages/notes.pages",
            "/Users/me/Library/Mobile Documents/com~apple~CloudDocs/.hidden/notes.md",
            "/Users/me/Downloads/Raiden.app/Contents/Info.plist",
            "/Users/me/Pictures/Photos Library.photoslibrary/database/notes",
            "/Users/me/Code/Kit.framework/Headers/notes.h",
            "/Users/me/Code/notes/../secret",
            "/Users/me//notes.md",
            "/Users/me",
            "/Users/me/",
            "/Users/meg/notes.md",
            "/Applications/Notes.app",
            "/private/var/notes.md",
            "relative/notes.md",
        ] {
            assert!(!kept(noise, HOME), "{noise} should be left out");
        }
    }

    #[test]
    fn ordinary_files_and_folders_are_kept() {
        for wanted in [
            "/Users/me/notes.md",
            "/Users/me/Documents/Release notes.pdf",
            "/Users/me/Code/lantern",
            "/Users/me/Downloads/Raiden.app",
            "/Users/me/Code/lantern/src/main.rs",
            "/Users/me/Library notes.txt",
            "/Users/me/Library/Mobile Documents/com~apple~CloudDocs/notes.md",
            "/Users/me/Library/Mobile Documents/com~apple~CloudDocs/Work/notes.md",
        ] {
            assert!(kept(wanted, HOME), "{wanted} should be kept");
            assert!(kept(wanted, "/Users/me/"), "{wanted} with a trailing slash");
        }
    }

    #[test]
    fn exact_names_come_first_then_starts_then_word_starts() {
        let rows = ranked(
            vec![
                found("/Users/me/a/myagents.md", Some(50.0)),
                found("/Users/me/b/old agents.md", Some(40.0)),
                found("/Users/me/c/agents.md.bak", Some(30.0)),
                found("/Users/me/d/AGENTS.md", Some(1.0)),
            ],
            "agents.md",
            HOME,
            |_| true,
        );
        assert_eq!(
            paths(&rows),
            [
                "/Users/me/d/AGENTS.md",
                "/Users/me/c/agents.md.bak",
                "/Users/me/b/old agents.md",
                "/Users/me/a/myagents.md",
            ]
        );
    }

    #[test]
    fn ties_go_to_the_recently_used_then_the_shallow() {
        let rows = ranked(
            vec![
                found("/Users/me/x/y/z/notes.md", None),
                found("/Users/me/x/notes.md", None),
                found("/Users/me/old/notes.md", Some(10.0)),
                found("/Users/me/new/notes.md", Some(20.0)),
            ],
            "notes.md",
            HOME,
            |_| true,
        );
        assert_eq!(
            paths(&rows),
            [
                "/Users/me/new/notes.md",
                "/Users/me/old/notes.md",
                "/Users/me/x/notes.md",
                "/Users/me/x/y/z/notes.md",
            ]
        );
    }

    #[test]
    fn ranking_drops_noise_repeats_and_keeps_a_handful() {
        let mut all: Vec<Found> = (0..20)
            .map(|at| found(&format!("/Users/me/notes {at}.md"), None))
            .collect();
        all.push(found("/Users/me/notes 0.md", None));
        all.push(found("/Users/me/Library/notes.md", None));
        let rows = ranked(all, "notes", HOME, |_| true);
        assert_eq!(rows.len(), SHOWN);
        assert!(rows.iter().all(|row| !row.path.contains("Library")));
        let mut unique = paths(&rows);
        unique.dedup();
        assert_eq!(unique.len(), SHOWN);
    }

    #[test]
    fn rows_carry_name_folder_and_icon() {
        let rows = ranked(
            vec![
                found("/Users/me/Code/lantern/AGENTS.md", None),
                found("/Users/me/Code/agents", None),
            ],
            "agents",
            HOME,
            |path| path.ends_with(".md"),
        );
        assert_eq!(
            rows,
            [
                FileRow {
                    name: "agents".into(),
                    path: "/Users/me/Code/agents".into(),
                    folder: "~/Code".into(),
                    icon: "/Users/me/Code/agents".into(),
                },
                FileRow {
                    name: "AGENTS.md".into(),
                    path: "/Users/me/Code/lantern/AGENTS.md".into(),
                    folder: "~/Code/lantern".into(),
                    icon: ".md".into(),
                },
            ]
        );
    }

    #[test]
    fn accented_names_score_like_their_plain_spelling() {
        assert_eq!(closeness("Résumé.pdf", "resume"), 3);
        assert_eq!(closeness("résumé", "RESUME"), 4);
        assert_eq!(closeness("İstanbul.pdf", "istanbul"), 3);
    }

    #[test]
    fn an_empty_home_keeps_nothing() {
        assert!(!kept("/etc/hosts", ""));
        assert!(!kept("/etc/hosts", "/"));
    }

    #[test]
    fn icloud_drive_is_named_as_finder_names_it() {
        let drive = "/Users/me/Library/Mobile Documents/com~apple~CloudDocs";
        assert_eq!(
            folder_label(&format!("{drive}/notes.md"), HOME),
            "iCloud Drive"
        );
        assert_eq!(
            folder_label(&format!("{drive}/Work/Q4/notes.md"), HOME),
            "iCloud Drive/Work/Q4"
        );
    }

    #[test]
    fn folders_outside_home_keep_their_full_path() {
        assert_eq!(folder_label("/Volumes/USB/notes.md", HOME), "/Volumes/USB");
        assert_eq!(folder_label("/notes.md", HOME), "/");
    }

    #[test]
    fn hidden_files_and_dotted_names_get_no_extension_key() {
        assert_eq!(icon_key("/Users/me/.zshrc", true), "/Users/me/.zshrc");
        assert_eq!(icon_key("/Users/me/notes.", true), "/Users/me/notes.");
        assert_eq!(icon_key("/Users/me/Kit.app", false), "/Users/me/Kit.app");
    }
}
