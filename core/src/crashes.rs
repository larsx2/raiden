pub const LOG: &str = "crashes.log";
pub const SAME_CRASH_SECONDS: u64 = 60;

const FORCED: &str = "forced";
const CRASH: &str = "crash";
const REPORT: &str = "macOS wrote a crash report: ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crash {
    pub at_seconds: u64,
    pub what: String,
}

/// ```
/// use raiden_core::crashes::{line, parse, Crash};
/// let crash = Crash { at_seconds: 100, what: "panicked at main.rs:1:1: boom".into() };
/// assert_eq!(line(&crash), "100\tcrash\tpanicked at main.rs:1:1: boom\n");
/// assert_eq!(parse(&line(&crash)), vec![crash]);
/// ```
pub fn line(crash: &Crash) -> String {
    format!(
        "{}\t{CRASH}\t{}\n",
        crash.at_seconds,
        crash.what.replace(['\n', '\r', '\t'], " ")
    )
}

pub fn parse(text: &str) -> Vec<Crash> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let at_seconds = fields.next()?.parse().ok()?;
            if !matches!(fields.next()?, CRASH | FORCED) {
                return None;
            }
            Some(Crash {
                at_seconds,
                what: fields.next().unwrap_or_default().to_owned(),
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub name: String,
    pub at_seconds: u64,
    pub contents: String,
}

/// ```
/// use raiden_core::crashes::may_be_ours;
/// assert!(may_be_ours("Raiden", "Raiden-2026-11-24-101500.ips"));
/// assert!(!may_be_ours("Raiden", "Raiden_2026-11-24_cpu_resource.diag"));
/// assert!(!may_be_ours("Raiden", "Safari-2026-11-24-101500.ips"));
/// ```
pub fn may_be_ours(program: &str, name: &str) -> bool {
    name.starts_with(&format!("{program}-")) && name.ends_with(".ips")
}

fn field(contents: &str, key: &str) -> Option<String> {
    let after_key = &contents[contents.find(&format!("\"{key}\""))? + key.len() + 2..];
    let value = after_key.trim_start().strip_prefix(':')?.trim_start();
    let mut chars = value.strip_prefix('"')?.chars();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => out.push(chars.next()?),
            other => out.push(other),
        }
    }
    None
}

/// ```
/// use raiden_core::crashes::from_the_app;
/// let app = r#"{"procPath" : "\/Users\/me\/Applications\/Raiden.app\/Contents\/MacOS\/Raiden", "parentProc" : "launchd"}"#;
/// let cli = r#"{"procPath" : "\/Users\/me\/Applications\/Raiden.app\/Contents\/MacOS\/Raiden", "parentProc" : "zsh"}"#;
/// assert!(from_the_app("Raiden", app));
/// assert!(!from_the_app("Raiden", cli));
/// ```
pub fn from_the_app(program: &str, contents: &str) -> bool {
    let launched_as_app = field(contents, "parentProc").as_deref() == Some("launchd");
    let in_a_bundle = field(contents, "procPath")
        .is_some_and(|path| path.ends_with(&format!(".app/Contents/MacOS/{program}")));
    launched_as_app && in_a_bundle
}

pub fn unnoted_reports(log: &str, program: &str, reports: &[Report]) -> Vec<Crash> {
    let logged: Vec<u64> = parse(log)
        .iter()
        .filter(|crash| !crash.what.starts_with(REPORT))
        .map(|crash| crash.at_seconds)
        .collect();
    let follows_a_logged_crash = |at: u64| {
        logged
            .iter()
            .any(|crashed| (*crashed..=crashed + SAME_CRASH_SECONDS).contains(&at))
    };
    reports
        .iter()
        .filter(|report| {
            may_be_ours(program, &report.name)
                && from_the_app(program, &report.contents)
                && !log.contains(&format!("{REPORT}{}", report.name))
                && !follows_a_logged_crash(report.at_seconds)
        })
        .map(|report| Crash {
            at_seconds: report.at_seconds,
            what: format!("{REPORT}{}", report.name),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broken_lines_are_skipped_and_tabs_in_messages_kept_out() {
        let crash = Crash {
            at_seconds: 3,
            what: "a\tb\nc".into(),
        };
        let text = format!("noise\n5\tmaybe\tx\nsoon\tcrash\tx\n{}", line(&crash));
        let crashes = parse(&text);
        assert_eq!(crashes.len(), 1);
        assert_eq!(crashes[0].what, "a b c");
    }

    const APP_PATH: &str =
        r#""procPath" : "\/Users\/me\/Applications\/Raiden.app\/Contents\/MacOS\/Raiden""#;

    fn report(name: &str, at_seconds: u64, parent: &str) -> Report {
        Report {
            name: name.to_owned(),
            at_seconds,
            contents: format!(
                "{{\"app_name\":\"Raiden\"}}\n{{\n  {APP_PATH},\n  \"parentProc\" : \"{parent}\"\n}}"
            ),
        }
    }

    #[test]
    fn only_new_crash_reports_of_the_app_are_noted() {
        let reports = [
            report("Raiden-2026-11-24-101500.ips", 10, "launchd"),
            report("Raiden-2026-11-23-090000.ips", 5, "launchd"),
            report("Safari-2026-11-24-101500.ips", 11, "launchd"),
            report("Raiden_2026-11-24_cpu_resource.diag", 12, "launchd"),
            report("raiden-2026-11-24-101500.ips", 13, "launchd"),
            report("Raiden-2026-11-24-101600.ips", 14, "zsh"),
        ];
        let log = line(&Crash {
            at_seconds: 5,
            what: format!("{REPORT}Raiden-2026-11-23-090000.ips"),
        });
        let noted = unnoted_reports(&log, "Raiden", &reports);
        assert_eq!(noted.len(), 1);
        assert_eq!(noted[0].at_seconds, 10);
        assert!(noted[0].what.ends_with("Raiden-2026-11-24-101500.ips"));
    }

    #[test]
    fn a_crashing_probe_started_from_a_terminal_is_not_the_app() {
        let probe = report("Raiden-a.ips", 1, "zsh");
        assert!(!from_the_app("Raiden", &probe.contents));
        assert!(from_the_app(
            "Raiden",
            &report("Raiden-b.ips", 1, "launchd").contents
        ));
        let loose = r#"{"procPath" : "\/usr\/local\/bin\/Raiden", "parentProc" : "launchd"}"#;
        assert!(!from_the_app("Raiden", loose));
        assert!(!from_the_app("Raiden", "not a report"));
        assert!(!from_the_app("Raiden", r#"{"parentProc" : "launc"#));
    }

    #[test]
    fn the_report_macos_writes_for_a_logged_panic_is_not_a_second_crash() {
        let log = line(&Crash {
            at_seconds: 1_000,
            what: "panicked at main.rs:1:1: asked".into(),
        });
        let reports = [
            report("Raiden-a.ips", 1_002, "launchd"),
            report("Raiden-b.ips", 1_000 + SAME_CRASH_SECONDS + 1, "launchd"),
            report("Raiden-c.ips", 999, "launchd"),
        ];
        let noted: Vec<String> = unnoted_reports(&log, "Raiden", &reports)
            .into_iter()
            .map(|crash| crash.what)
            .collect();
        assert_eq!(
            noted,
            [
                format!("{REPORT}Raiden-b.ips"),
                format!("{REPORT}Raiden-c.ips")
            ]
        );
    }
}
