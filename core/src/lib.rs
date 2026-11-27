pub mod ai;
pub mod answers;
pub mod apps;
#[cfg(unix)]
pub mod clipboard;
pub mod control;
pub mod crashes;
pub mod files;
pub mod folders;
pub mod login;
pub mod probe;
pub mod pulls;
pub mod rates;
pub mod search;
pub mod settings;
pub mod speed;
pub mod system_settings;
pub mod terminals;
pub mod typeahead;
pub mod usage;
#[cfg(target_arch = "wasm32")]
mod web;
pub mod window;

#[derive(Debug, PartialEq, Eq)]
pub enum Invocation {
    Version,
    Probe(Vec<String>),
    Launcher { rows: Option<usize> },
}

/// ```
/// use raiden_core::{invocation, Invocation};
/// assert_eq!(invocation(["--version"]), Invocation::Version);
/// assert_eq!(invocation(["probe", "speed"]), Invocation::Probe(vec!["speed".into()]));
/// assert_eq!(invocation(Vec::<String>::new()), Invocation::Launcher { rows: None });
/// assert_eq!(invocation(["--rows", "5000"]), Invocation::Launcher { rows: Some(5000) });
/// ```
pub fn invocation<I, S>(args: I) -> Invocation
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut args = args.into_iter();
    match args.next() {
        Some(first) if first.as_ref() == "--version" => Invocation::Version,
        Some(first) if first.as_ref() == "probe" => {
            Invocation::Probe(args.map(|arg| arg.as_ref().to_owned()).collect())
        }
        Some(first) if first.as_ref() == "--rows" => Invocation::Launcher {
            rows: args.next().and_then(|rows| rows.as_ref().parse().ok()),
        },
        _ => Invocation::Launcher { rows: None },
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

const PANEL_TOP: f64 = 0.22;

/// ```
/// use raiden_core::{panel_origin, Area};
/// let screen = Area { x: 0.0, y: 0.0, width: 1440.0, height: 900.0 };
/// assert_eq!(panel_origin(screen, 720.0, 456.0), (360.0, 198.0));
/// ```
pub fn panel_origin(screen: Area, width: f64, height: f64) -> (f64, f64) {
    let x = screen.x + ((screen.width - width) / 2.0).max(0.0);
    let lowest = screen.y + (screen.height - height).max(0.0);
    let y = (screen.y + screen.height * PANEL_TOP).min(lowest);
    (x, y)
}

/// ```
/// assert_eq!(raiden_core::window_title(false), "Raiden");
/// assert_eq!(raiden_core::window_title(true), "Raiden (dev)");
/// ```
pub fn window_title(development: bool) -> &'static str {
    if development {
        "Raiden (dev)"
    } else {
        "Raiden"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anything_but_version_opens_the_launcher() {
        assert_eq!(
            invocation(["-psn_0_12345"]),
            Invocation::Launcher { rows: None }
        );
        assert_eq!(invocation(["--version", "extra"]), Invocation::Version);
    }

    #[test]
    fn the_panel_sits_on_the_screen_it_opens_on() {
        let right = Area {
            x: 1440.0,
            y: 0.0,
            width: 2560.0,
            height: 1440.0,
        };
        assert_eq!(panel_origin(right, 720.0, 456.0), (2360.0, 316.8));
    }

    #[test]
    fn the_panel_never_hangs_off_a_short_screen() {
        let short = Area {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 500.0,
        };
        assert_eq!(panel_origin(short, 720.0, 456.0), (40.0, 44.0));
    }

    #[test]
    fn titles_tell_the_copies_apart() {
        assert_ne!(window_title(true), window_title(false));
    }
}
