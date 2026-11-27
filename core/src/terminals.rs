use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub const OPEN: &str = "/usr/bin/open";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminal {
    Ghostty,
    Iterm,
    Rex,
    Tern,
    Terminal,
    Kitty,
    Alacritty,
    Warp,
}

impl Terminal {
    pub const ALL: [Terminal; 8] = [
        Terminal::Ghostty,
        Terminal::Iterm,
        Terminal::Rex,
        Terminal::Tern,
        Terminal::Terminal,
        Terminal::Kitty,
        Terminal::Alacritty,
        Terminal::Warp,
    ];
    pub const PREFERRED: [Terminal; 8] = [
        Terminal::Ghostty,
        Terminal::Iterm,
        Terminal::Rex,
        Terminal::Tern,
        Terminal::Kitty,
        Terminal::Alacritty,
        Terminal::Warp,
        Terminal::Terminal,
    ];

    /// ```
    /// use raiden_core::terminals::Terminal;
    /// assert_eq!(Terminal::from_id("iterm"), Some(Terminal::Iterm));
    /// assert_eq!(Terminal::from_id("iTerm2"), None);
    /// ```
    pub fn from_id(id: &str) -> Option<Terminal> {
        Terminal::ALL
            .into_iter()
            .find(|terminal| terminal.id() == id)
    }

    fn names(self) -> [&'static str; 3] {
        match self {
            Terminal::Ghostty => ["ghostty", "Ghostty", "com.mitchellh.ghostty"],
            Terminal::Iterm => ["iterm", "iTerm2", "com.googlecode.iterm2"],
            Terminal::Rex => ["rex", "Rex", "com.superlogical.rex"],
            Terminal::Tern => ["tern", "Tern", "so.stencil.tern"],
            Terminal::Terminal => ["terminal", "Terminal", "com.apple.Terminal"],
            Terminal::Kitty => ["kitty", "Kitty", "net.kovidgoyal.kitty"],
            Terminal::Alacritty => ["alacritty", "Alacritty", "org.alacritty"],
            Terminal::Warp => ["warp", "Warp", "dev.warp.Warp-Stable"],
        }
    }

    pub fn id(self) -> &'static str {
        self.names()[0]
    }

    pub fn name(self) -> &'static str {
        self.names()[1]
    }

    pub fn bundle_id(self) -> &'static str {
        self.names()[2]
    }
}

pub fn ids() -> String {
    let quoted: Vec<String> = Terminal::ALL
        .iter()
        .map(|terminal| format!("\"{}\"", terminal.id()))
        .collect();
    match quoted.split_last() {
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    }
}

/// ```
/// use raiden_core::terminals::{in_use, Terminal};
/// let here = |terminal: Terminal| matches!(terminal, Terminal::Iterm | Terminal::Terminal);
/// assert_eq!(in_use(None, here), Terminal::Iterm);
/// assert_eq!(in_use(Some(Terminal::Terminal), here), Terminal::Terminal);
/// assert_eq!(in_use(Some(Terminal::Kitty), here), Terminal::Iterm);
/// ```
pub fn in_use(chosen: Option<Terminal>, installed: impl Fn(Terminal) -> bool) -> Terminal {
    chosen
        .filter(|terminal| installed(*terminal))
        .or_else(|| {
            Terminal::PREFERRED
                .into_iter()
                .find(|terminal| installed(*terminal))
        })
        .unwrap_or(Terminal::Terminal)
}

/// ```
/// use raiden_core::terminals::{listed, Terminal};
/// let here = |terminal: Terminal| matches!(terminal, Terminal::Terminal | Terminal::Kitty);
/// let order: Vec<&str> = listed(here).iter().map(|terminal| terminal.id()).collect();
/// assert_eq!(order, ["kitty", "terminal", "ghostty", "iterm", "rex", "tern", "alacritty", "warp"]);
/// ```
pub fn listed(installed: impl Fn(Terminal) -> bool) -> Vec<Terminal> {
    let (mut found, missing): (Vec<Terminal>, Vec<Terminal>) = Terminal::PREFERRED
        .into_iter()
        .partition(|terminal| installed(*terminal));
    found.extend(missing);
    found
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ends {
    Soon,
    MayStay,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
    pub ends: Ends,
}

#[derive(Debug, Clone, Copy)]
pub struct Found<'a> {
    pub app: &'a Path,
    pub running: bool,
    pub tern: Option<&'a Path>,
}

pub const NO_TERN: &str = "Raiden could not find the tern command on your login shell's PATH.";

fn open_with(terminal: Terminal, extra: &[&str], target: impl Into<OsString>) -> Run {
    Run {
        program: PathBuf::from(OPEN),
        arguments: ["-b", terminal.bundle_id()]
            .iter()
            .chain(extra)
            .map(OsString::from)
            .chain([target.into()])
            .collect(),
        ends: Ends::Soon,
    }
}

fn inside(app: &Path, program: &str) -> PathBuf {
    app.join("Contents").join("MacOS").join(program)
}

/// ```
/// use raiden_core::terminals::warp_link;
/// use std::path::Path;
/// assert_eq!(
///     warp_link(Path::new("/Users/me/My Code/ü")),
///     "warp://action/new_window?path=/Users/me/My%20Code/%C3%BC"
/// );
/// ```
pub fn warp_link(folder: &Path) -> String {
    let mut link = String::from("warp://action/new_window?path=");
    for byte in folder.as_os_str().as_encoded_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(byte) {
            link.push(char::from(*byte));
        } else {
            link.push_str(&format!("%{byte:02X}"));
        }
    }
    link
}

pub fn plan(terminal: Terminal, folder: &Path, found: &Found<'_>) -> Result<Run, String> {
    if !folder.is_absolute() {
        return Err(format!(
            "Raiden opens only full folder paths in a terminal, and {} is not one.",
            folder.display()
        ));
    }
    Ok(match terminal {
        Terminal::Ghostty | Terminal::Iterm | Terminal::Rex | Terminal::Terminal => {
            open_with(terminal, &[], folder)
        }
        Terminal::Tern => Run {
            program: found.tern.ok_or(NO_TERN)?.to_path_buf(),
            arguments: vec![folder.into()],
            ends: Ends::MayStay,
        },
        Terminal::Kitty if found.running => open_with(terminal, &[], folder),
        Terminal::Kitty => open_with(
            terminal,
            &["--args", "--single-instance", "--directory"],
            folder,
        ),
        Terminal::Alacritty if found.running => Run {
            program: inside(found.app, "alacritty"),
            arguments: vec![
                "msg".into(),
                "create-window".into(),
                "--working-directory".into(),
                folder.into(),
            ],
            ends: Ends::Soon,
        },
        Terminal::Alacritty => open_with(terminal, &["--args", "--working-directory"], folder),
        Terminal::Warp => open_with(terminal, &[], warp_link(folder)),
    })
}

/// ```
/// use raiden_core::terminals::folder_of;
/// use std::path::Path;
/// assert_eq!(folder_of(Path::new("/Users/me/notes.md"), false), Path::new("/Users/me"));
/// assert_eq!(folder_of(Path::new("/Users/me/Code"), true), Path::new("/Users/me/Code"));
/// ```
pub fn folder_of(path: &Path, is_folder: bool) -> &Path {
    if is_folder {
        path
    } else {
        path.parent().unwrap_or(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str = "/Applications/Some.app";
    const FOLDER: &str = "/Users/me/Code/lantern";

    const TERN: &str = "/Users/me/.local/bin/tern";

    fn found(running: bool) -> Found<'static> {
        Found {
            app: Path::new(APP),
            running,
            tern: Some(Path::new(TERN)),
        }
    }

    fn planned(terminal: Terminal, running: bool) -> Run {
        plan(terminal, Path::new(FOLDER), &found(running)).expect("a plan")
    }

    fn words(run: &Run) -> Vec<String> {
        std::iter::once(run.program.as_os_str())
            .chain(run.arguments.iter().map(OsString::as_os_str))
            .map(|word| word.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn every_terminal_has_its_own_id_name_and_bundle() {
        for terminal in Terminal::ALL {
            assert_eq!(Terminal::from_id(terminal.id()), Some(terminal));
            let others = Terminal::ALL.iter().filter(|other| **other != terminal);
            for other in others {
                assert_ne!(other.id(), terminal.id());
                assert_ne!(other.name(), terminal.name());
                assert_ne!(other.bundle_id(), terminal.bundle_id());
            }
            assert!(Terminal::PREFERRED.contains(&terminal));
        }
        assert_eq!(
            ids(),
            "\"ghostty\", \"iterm\", \"rex\", \"tern\", \"terminal\", \"kitty\", \"alacritty\" or \"warp\""
        );
    }

    #[test]
    fn the_first_installed_is_used_in_the_owner_order_with_terminal_last() {
        let order: Vec<&str> = Terminal::PREFERRED.iter().map(|t| t.id()).collect();
        assert_eq!(
            order,
            [
                "ghostty",
                "iterm",
                "rex",
                "tern",
                "kitty",
                "alacritty",
                "warp",
                "terminal"
            ]
        );
        for (at, expected) in Terminal::PREFERRED.iter().enumerate() {
            let here = |terminal: Terminal| Terminal::PREFERRED[at..].contains(&terminal);
            assert_eq!(in_use(None, here), *expected);
        }
        assert_eq!(in_use(None, |_| false), Terminal::Terminal);
    }

    #[test]
    fn the_list_puts_installed_terminals_first_both_in_the_owner_order() {
        let here = |terminal: Terminal| {
            matches!(
                terminal,
                Terminal::Ghostty
                    | Terminal::Iterm
                    | Terminal::Rex
                    | Terminal::Tern
                    | Terminal::Terminal
            )
        };
        let names: Vec<&str> = listed(here).iter().map(|t| t.name()).collect();
        assert_eq!(
            names,
            [
                "Ghostty",
                "iTerm2",
                "Rex",
                "Tern",
                "Terminal",
                "Kitty",
                "Alacritty",
                "Warp"
            ]
        );
        assert_eq!(listed(|_| false), Terminal::PREFERRED);
        assert_eq!(listed(|_| true), Terminal::PREFERRED);
    }

    #[test]
    fn a_chosen_terminal_is_used_only_while_it_is_installed() {
        let all = |_| true;
        for terminal in Terminal::ALL {
            assert_eq!(in_use(Some(terminal), all), terminal);
        }
        let without_ghostty = |terminal: Terminal| terminal != Terminal::Ghostty;
        assert_eq!(
            in_use(Some(Terminal::Ghostty), without_ghostty),
            Terminal::Iterm
        );
        assert_eq!(
            in_use(Some(Terminal::Warp), |terminal| terminal
                == Terminal::Terminal),
            Terminal::Terminal
        );
    }

    #[test]
    fn apps_that_open_folders_get_the_folder_through_open() {
        for (terminal, bundle) in [
            (Terminal::Ghostty, "com.mitchellh.ghostty"),
            (Terminal::Iterm, "com.googlecode.iterm2"),
            (Terminal::Rex, "com.superlogical.rex"),
            (Terminal::Terminal, "com.apple.Terminal"),
        ] {
            for running in [false, true] {
                let run = planned(terminal, running);
                assert_eq!(words(&run), [OPEN, "-b", bundle, FOLDER]);
                assert_eq!(run.ends, Ends::Soon);
            }
        }
    }

    #[test]
    fn tern_runs_its_command_from_the_login_path_and_only_from_there() {
        let run = planned(Terminal::Tern, true);
        assert_eq!(words(&run), [TERN, FOLDER]);
        assert_eq!(run.ends, Ends::MayStay);
        let without = Found {
            tern: None,
            ..found(true)
        };
        assert_eq!(
            plan(Terminal::Tern, Path::new(FOLDER), &without),
            Err(NO_TERN.to_owned())
        );
    }

    #[test]
    fn kitty_gets_the_folder_from_macos_and_starts_as_one_instance() {
        let running = planned(Terminal::Kitty, true);
        assert_eq!(
            words(&running),
            [OPEN, "-b", "net.kovidgoyal.kitty", FOLDER]
        );
        assert_eq!(running.ends, Ends::Soon);
        let starting = planned(Terminal::Kitty, false);
        assert_eq!(
            words(&starting),
            [
                OPEN,
                "-b",
                "net.kovidgoyal.kitty",
                "--args",
                "--single-instance",
                "--directory",
                FOLDER
            ]
        );
        assert_eq!(starting.ends, Ends::Soon);
    }

    #[test]
    fn alacritty_asks_the_running_copy_and_otherwise_starts_through_open() {
        assert_eq!(
            words(&planned(Terminal::Alacritty, true)),
            [
                "/Applications/Some.app/Contents/MacOS/alacritty",
                "msg",
                "create-window",
                "--working-directory",
                FOLDER
            ]
        );
        assert_eq!(
            words(&planned(Terminal::Alacritty, false)),
            [
                OPEN,
                "-b",
                "org.alacritty",
                "--args",
                "--working-directory",
                FOLDER
            ]
        );
    }

    #[test]
    fn warp_gets_a_link_with_the_folder_escaped() {
        for running in [false, true] {
            let run = plan(
                Terminal::Warp,
                Path::new("/Users/me/a b&c=d?#"),
                &found(running),
            )
            .expect("a plan");
            assert_eq!(
                words(&run),
                [
                    OPEN,
                    "-b",
                    "dev.warp.Warp-Stable",
                    "warp://action/new_window?path=/Users/me/a%20b%26c%3Dd%3F%23"
                ]
            );
        }
    }

    #[test]
    fn a_folder_with_odd_characters_stays_one_argument() {
        let odd = "/Users/me/it's; rm -rf ~ $(x) `y`\n";
        for terminal in Terminal::ALL {
            for running in [false, true] {
                let run = plan(terminal, Path::new(odd), &found(running)).expect("a plan");
                assert!(
                    terminal == Terminal::Warp
                        || run.arguments.last().is_some_and(|last| last == odd),
                    "{terminal:?}"
                );
            }
        }
    }

    #[test]
    fn only_full_paths_are_opened() {
        for terminal in Terminal::ALL {
            assert!(plan(terminal, Path::new("Code"), &found(true)).is_err());
            assert!(plan(terminal, Path::new("-n"), &found(false)).is_err());
        }
    }
}
