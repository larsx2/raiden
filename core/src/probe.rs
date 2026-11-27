use std::path::Path;

use crate::control::Reply;
use crate::speed;

pub const HELP: &str = "Measure Raiden on this Mac.

Usage: raiden probe speed

Checks the last 100 opens, keystrokes and scrolling frames, the first opens after the last 10
wakes from sleep, and idle memory, against their limits.
";

pub struct Outcome {
    pub output: String,
    pub code: i32,
}

pub const MEMORY_LIMIT_MB: f64 = 150.0;

pub const IDLE: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Memory {
    pub bytes: u64,
    pub processes: usize,
}

pub struct Host<'a> {
    pub hidden_for: &'a dyn Fn() -> Reply,
    pub memory: &'a dyn Fn() -> Option<Memory>,
}

pub fn run(args: &[String], logs: &Path, host: &Host<'_>) -> Outcome {
    match args.first().map(String::as_str) {
        Some("speed") => speed(logs, idle_memory(host)),
        Some("--help" | "-h" | "help") => Outcome {
            output: HELP.to_owned(),
            code: 0,
        },
        Some(other) => Outcome {
            output: format!("raiden probe has no command {other:?}.\n\n{HELP}"),
            code: 2,
        },
        None => Outcome {
            output: HELP.to_owned(),
            code: 2,
        },
    }
}

/// ```
/// use raiden_core::probe::{memory_line, Memory};
/// let (line, passed) = memory_line(Some(Memory { bytes: 100 * 1024 * 1024, processes: 4 }));
/// assert!(passed);
/// assert!(line.contains("100.0 MB"));
/// assert!(!memory_line(None).1);
/// ```
pub fn memory_line(memory: Option<Memory>) -> (String, bool) {
    let Some(memory) = memory else {
        return (
            "memory     Raiden's processes could not be read, so idle memory was not measured: \
             fail\n"
                .to_owned(),
            false,
        );
    };
    if memory.processes < 2 {
        return (
            "memory     found Raiden but not its web view helpers; measure the installed copy \
             opened from Finder (a copy started from a terminal can't be measured): fail\n"
                .to_owned(),
            false,
        );
    }
    let megabytes = memory.bytes as f64 / (1024.0 * 1024.0);
    let passed = megabytes <= MEMORY_LIMIT_MB;
    let result = if passed { "pass" } else { "too much" };
    (
        format!(
            "memory     {megabytes:.1} MB across {} processes (limit {MEMORY_LIMIT_MB} MB): {result}\n",
            memory.processes
        ),
        passed,
    )
}

fn idle_memory(host: &Host<'_>) -> Result<Option<Memory>, String> {
    let reply = (host.hidden_for)();
    if !reply.ok {
        return Err(format!(
            "memory     not measured: {}: fail\n",
            reply.text.trim_end_matches('.')
        ));
    }
    let seconds: u64 = reply.text.trim().parse().unwrap_or(0);
    if seconds < IDLE.as_secs() {
        return Err(format!(
            "memory     not measured: the panel has been hidden for {seconds} s, and idle memory \
             needs {} s: fail\n",
            IDLE.as_secs()
        ));
    }
    Ok((host.memory)())
}

fn speed(logs: &Path, memory: Result<Option<Memory>, String>) -> Outcome {
    let path = logs.join(speed::LOG);
    let log = std::fs::read_to_string(&path).unwrap_or_default();
    let verdicts = speed::judge(&log);
    let (memory, memory_passed) = match memory {
        Ok(memory) => memory_line(memory),
        Err(line) => (line, false),
    };
    let passed = memory_passed && verdicts.iter().all(speed::Verdict::passed);
    Outcome {
        output: format!("{}\n{}{memory}", path.display(), speed::report(&verdicts)),
        code: i32::from(!passed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speed::{Measure, SAMPLES, line};

    fn folder_with(name: &str, log: &str) -> std::path::PathBuf {
        let folder =
            std::env::temp_dir().join(format!("raiden-probe-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("temporary folder");
        std::fs::write(folder.join(speed::LOG), log).expect("temporary log");
        folder
    }

    fn idle_app() -> Reply {
        Reply::ok("120")
    }

    fn small() -> Option<Memory> {
        Some(Memory {
            bytes: 90 * 1024 * 1024,
            processes: 4,
        })
    }

    fn host<'a>(hidden_for: &'a dyn Fn() -> Reply) -> Host<'a> {
        Host {
            hidden_for,
            memory: &small,
        }
    }

    fn log_with(slow_opens: usize) -> String {
        (0..SAMPLES)
            .map(|index| {
                let open = if index < slow_opens { 60.0 } else { 10.0 };
                line(Measure::Open, open)
                    + &line(Measure::Keystroke, 2.0)
                    + &line(Measure::Frame, 8.3)
            })
            .chain(std::iter::repeat_n(line(Measure::Wake, 20.0), speed::WAKES))
            .collect()
    }

    #[test]
    fn speed_exits_0_with_one_slow_open_and_1_with_ten() {
        let one = folder_with("one", &log_with(1));
        let ten = folder_with("ten", &log_with(10));
        let codes = (
            run(&["speed".into()], &one, &host(&idle_app)).code,
            run(&["speed".into()], &ten, &host(&idle_app)).code,
        );
        let _ = std::fs::remove_dir_all(one);
        let _ = std::fs::remove_dir_all(ten);
        assert_eq!(codes, (0, 1));
    }

    #[test]
    fn too_much_idle_memory_fails_speed() {
        let folder = folder_with("memory", &log_with(0));
        let big = || {
            Some(Memory {
                bytes: 200 * 1024 * 1024,
                processes: 4,
            })
        };
        let outcome = run(
            &["speed".into()],
            &folder,
            &Host {
                hidden_for: &idle_app,
                memory: &big,
            },
        );
        let _ = std::fs::remove_dir_all(folder);
        assert_eq!(outcome.code, 1);
        assert!(outcome.output.contains("too much"));
    }

    #[test]
    fn memory_waits_for_the_panel_to_be_hidden_long_enough() {
        let folder = folder_with("recent", &log_with(0));
        let recent = || Reply::ok("5");
        let outcome = run(&["speed".into()], &folder, &host(&recent));
        let _ = std::fs::remove_dir_all(folder);
        assert_eq!(outcome.code, 1);
        assert!(outcome.output.contains("hidden for 5 s"));
    }

    #[test]
    fn memory_without_helpers_fails() {
        let alone = Memory {
            bytes: 10 * 1024 * 1024,
            processes: 1,
        };
        assert!(!memory_line(Some(alone)).1);
    }

    #[test]
    fn help_lists_speed() {
        let outcome = run(&["--help".into()], Path::new("/nowhere"), &host(&idle_app));
        assert_eq!(outcome.code, 0);
        assert!(outcome.output.contains("speed"));
    }

    #[test]
    fn other_commands_are_refused() {
        let outcome = run(
            &["exercise".into()],
            Path::new("/nowhere"),
            &host(&idle_app),
        );
        assert_eq!(outcome.code, 2);
    }
}
