#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    pub at_seconds: u64,
    pub row: usize,
    pub query: String,
    pub path: String,
}

const PICK: &str = "pick";

fn flat(text: &str) -> String {
    text.replace(['\n', '\r', '\t'], " ")
}

/// ```
/// use raiden_core::usage::{line, parse, Pick};
/// let pick = Pick { at_seconds: 100, row: 0, query: "saf".into(), path: "/Applications/Safari.app".into() };
/// assert_eq!(parse(&line(&pick)), vec![pick]);
/// ```
pub fn line(pick: &Pick) -> String {
    format!(
        "{PICK}\t{}\t{}\t{}\t{}\n",
        pick.at_seconds,
        pick.row,
        flat(&pick.query),
        flat(&pick.path)
    )
}

pub fn parse(text: &str) -> Vec<Pick> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            if fields.next()? != PICK {
                return None;
            }
            Some(Pick {
                at_seconds: fields.next()?.parse().ok()?,
                row: fields.next()?.parse().ok()?,
                query: fields.next()?.to_owned(),
                path: fields.next()?.to_owned(),
            })
        })
        .collect()
}

pub fn age_days(pick: &Pick, now_seconds: u64) -> f32 {
    now_seconds.saturating_sub(pick.at_seconds) as f32 / 86_400.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broken_and_unknown_lines_are_skipped() {
        let text = "pick\t1\t2\tq\t/a.app\nnoise\npick\tx\t0\tq\t/b.app\npick\t5\t0\t\t/c.app\n";
        let picks = parse(text);
        assert_eq!(picks.len(), 2);
        assert_eq!(picks[1].query, "");
    }

    #[test]
    fn tabs_in_a_query_cannot_break_the_line() {
        let pick = Pick {
            at_seconds: 9,
            row: 3,
            query: "a\tb\nc".into(),
            path: "/x.app".into(),
        };
        assert_eq!(parse(&line(&pick))[0].query, "a b c");
    }

    #[test]
    fn age_is_in_days_and_never_negative() {
        let pick = Pick {
            at_seconds: 86_400,
            row: 0,
            query: String::new(),
            path: String::new(),
        };
        assert!((age_days(&pick, 2 * 86_400) - 1.0).abs() < f32::EPSILON);
        assert!(age_days(&pick, 0).abs() < f32::EPSILON);
    }

    #[test]
    fn lines_left_by_older_versions_are_not_picks() {
        let text = "reach\t1\nreach-held\t2\nwindow\t3\npick\t4\t0\tq\t/a.app\n";
        assert_eq!(parse(text).len(), 1);
    }

    #[test]
    fn picks_left_by_removed_probes_are_skipped() {
        let text = "probe-pick\t1\t0\tsaf\t/Applications/Safari.app\npick\t2\t0\tq\t/a.app\n";
        assert_eq!(parse(text).len(), 1);
    }
}
