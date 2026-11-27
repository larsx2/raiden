use std::fmt::Write as _;

pub const SAMPLES: usize = 100;
pub const KEPT: usize = 2 * SAMPLES;
pub const LOG: &str = "speed.log";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Measure {
    Open,
    Keystroke,
    Frame,
    Wake,
}

pub const WAKES: usize = 10;

impl Measure {
    pub const ALL: [Self; 4] = [Self::Open, Self::Keystroke, Self::Frame, Self::Wake];

    pub fn samples(self) -> usize {
        match self {
            Self::Wake => WAKES,
            _ => SAMPLES,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Keystroke => "keystroke",
            Self::Frame => "frame",
            Self::Wake => "wake",
        }
    }

    pub fn limit_ms(self) -> f64 {
        match self {
            Self::Open => 50.0,
            Self::Keystroke => 8.0,
            Self::Frame => 25.0,
            Self::Wake => 50.0,
        }
    }

    fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|measure| measure.name() == name)
    }
}

/// ```
/// use raiden_core::speed::{line, Measure};
/// assert_eq!(line(Measure::Open, 12.5), "open\t12.500\n");
/// ```
pub fn line(measure: Measure, ms: f64) -> String {
    format!("{}\t{ms:.3}\n", measure.name())
}

pub fn parse(log: &str) -> Vec<(Measure, f64)> {
    log.lines()
        .filter_map(|line| {
            let (name, ms) = line.split_once('\t')?;
            let ms: f64 = ms.trim().parse().ok()?;
            (ms.is_finite() && ms >= 0.0).then_some((Measure::named(name)?, ms))
        })
        .collect()
}

/// ```
/// use raiden_core::speed::percentile_95;
/// let values: Vec<f64> = (1..=100).map(f64::from).collect();
/// assert_eq!(percentile_95(&values), Some(95.0));
/// assert_eq!(percentile_95(&[]), None);
/// ```
pub fn percentile_95(values: &[f64]) -> Option<f64> {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = (sorted.len() * 95).div_ceil(100);
    sorted.get(rank.checked_sub(1)?).copied()
}

pub fn trimmed(log: &str) -> Option<String> {
    let entries = parse(log);
    let over = Measure::ALL
        .into_iter()
        .any(|measure| entries.iter().filter(|(kind, _)| *kind == measure).count() > KEPT);
    if !over {
        return None;
    }
    let mut seen = [0; Measure::ALL.len()];
    let mut kept: Vec<(Measure, f64)> = entries
        .into_iter()
        .rev()
        .filter(|(measure, _)| {
            let count = &mut seen[*measure as usize];
            *count += 1;
            *count <= KEPT
        })
        .collect();
    kept.reverse();
    Some(
        kept.into_iter()
            .map(|(measure, ms)| line(measure, ms))
            .collect(),
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub measure: Measure,
    pub samples: usize,
    pub p95: Option<f64>,
}

impl Verdict {
    pub fn passed(&self) -> bool {
        self.samples >= self.measure.samples()
            && self.p95.is_some_and(|p95| p95 < self.measure.limit_ms())
    }
}

pub fn judge(log: &str) -> Vec<Verdict> {
    let entries = parse(log);
    Measure::ALL
        .into_iter()
        .map(|measure| {
            let values: Vec<f64> = entries
                .iter()
                .filter(|(kind, _)| *kind == measure)
                .map(|(_, ms)| *ms)
                .collect();
            let recent = &values[values.len().saturating_sub(measure.samples())..];
            Verdict {
                measure,
                samples: recent.len(),
                p95: percentile_95(recent),
            }
        })
        .collect()
}

pub fn report(verdicts: &[Verdict]) -> String {
    let mut out = String::new();
    for verdict in verdicts {
        let limit = verdict.measure.limit_ms();
        let name = verdict.measure.name();
        let p95 = verdict
            .p95
            .map_or_else(|| "no data".to_owned(), |p95| format!("{p95:.1} ms"));
        let result = if verdict.passed() {
            "pass"
        } else if verdict.samples < verdict.measure.samples() {
            "not enough tries yet"
        } else {
            "too slow"
        };
        let _ = writeln!(
            out,
            "{name:<10} 95% within {p95} (limit {limit} ms), last {} of {} tries: {result}",
            verdict.samples,
            verdict.measure.samples()
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log_with(slow: usize) -> String {
        let mut log = String::new();
        for index in 0..SAMPLES {
            let ms = if index < slow { 60.0 } else { 10.0 };
            log.push_str(&line(Measure::Open, ms));
            log.push_str(&line(Measure::Keystroke, 2.0));
            log.push_str(&line(Measure::Frame, 8.3));
        }
        log.push_str(&line(Measure::Wake, 20.0).repeat(WAKES));
        log
    }

    #[test]
    fn one_slow_open_in_a_hundred_passes() {
        assert!(judge(&log_with(1)).iter().all(Verdict::passed));
    }

    #[test]
    fn ten_slow_opens_in_a_hundred_fail() {
        let verdicts = judge(&log_with(10));
        assert!(!verdicts[0].passed());
        assert!(verdicts[1].passed());
    }

    #[test]
    fn fewer_than_a_hundred_tries_never_pass() {
        let log = line(Measure::Open, 1.0).repeat(SAMPLES - 1);
        assert!(!judge(&log)[0].passed());
    }

    #[test]
    fn only_the_last_hundred_tries_count() {
        let mut log = line(Measure::Open, 90.0).repeat(50);
        log.push_str(&log_with(0));
        assert!(judge(&log)[0].passed());
    }

    #[test]
    fn trimming_keeps_the_newest_tries_of_each_measure() {
        let mut log = line(Measure::Open, 1.0).repeat(KEPT);
        log.push_str(&line(Measure::Keystroke, 3.0).repeat(KEPT * 3));
        log.push_str(&line(Measure::Keystroke, 4.0));
        let kept = parse(&trimmed(&log).expect("over the limit"));
        let keystrokes: Vec<f64> = kept
            .iter()
            .filter(|(measure, _)| *measure == Measure::Keystroke)
            .map(|(_, ms)| *ms)
            .collect();
        assert_eq!(keystrokes.len(), KEPT);
        assert_eq!(keystrokes.last(), Some(&4.0));
        assert_eq!(
            kept.iter().filter(|(m, _)| *m == Measure::Open).count(),
            KEPT
        );
    }

    #[test]
    fn short_logs_are_left_alone() {
        assert_eq!(trimmed(&line(Measure::Open, 1.0)), None);
    }

    #[test]
    fn one_slow_wake_in_ten_fails() {
        let mut log = log_with(0);
        log.push_str(&line(Measure::Wake, 70.0));
        let wake = judge(&log)
            .into_iter()
            .find(|verdict| verdict.measure == Measure::Wake)
            .expect("wake verdict");
        assert!(!wake.passed());
    }

    #[test]
    fn broken_lines_are_skipped() {
        let log = "open\tfast\nopen\t-3\nsomething\t4\nopen\t4.5\n";
        assert_eq!(parse(log), vec![(Measure::Open, 4.5)]);
    }
}
