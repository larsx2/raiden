use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    AnthropicApiKey,
    OpenaiApiKey,
    GithubToken,
    GithubFineGrainedToken,
    AwsAccessKeyId,
    AwsSecretAccessKey,
    SlackBotToken,
    StripeSecretKey,
    PostgresPassword,
    JsonWebToken,
}

impl Kind {
    pub const ALL: [Kind; 10] = [
        Kind::AnthropicApiKey,
        Kind::OpenaiApiKey,
        Kind::GithubToken,
        Kind::GithubFineGrainedToken,
        Kind::AwsAccessKeyId,
        Kind::AwsSecretAccessKey,
        Kind::SlackBotToken,
        Kind::StripeSecretKey,
        Kind::PostgresPassword,
        Kind::JsonWebToken,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Kind::AnthropicApiKey => "anthropic_api_key",
            Kind::OpenaiApiKey => "openai_api_key",
            Kind::GithubToken => "github_token",
            Kind::GithubFineGrainedToken => "github_fine_grained_token",
            Kind::AwsAccessKeyId => "aws_access_key_id",
            Kind::AwsSecretAccessKey => "aws_secret_access_key",
            Kind::SlackBotToken => "slack_bot_token",
            Kind::StripeSecretKey => "stripe_secret_key",
            Kind::PostgresPassword => "postgres_password",
            Kind::JsonWebToken => "json_web_token",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::AnthropicApiKey => "Anthropic API key",
            Kind::OpenaiApiKey => "OpenAI API key",
            Kind::GithubToken => "GitHub token",
            Kind::GithubFineGrainedToken => "GitHub fine-grained token",
            Kind::AwsAccessKeyId => "AWS access key ID",
            Kind::AwsSecretAccessKey => "AWS secret access key",
            Kind::SlackBotToken => "Slack bot token",
            Kind::StripeSecretKey => "Stripe secret key",
            Kind::PostgresPassword => "Postgres password",
            Kind::JsonWebToken => "JSON Web Token",
        }
    }

    fn pattern(self) -> String {
        match self {
            Kind::AnthropicApiKey => standalone(r"sk-ant-[a-z]+\d{2}-[A-Za-z0-9_-]{20,}"),
            Kind::OpenaiApiKey => standalone(r"sk-(?:proj-|svcacct-|admin-)?[A-Za-z0-9_-]{20,}"),
            Kind::GithubToken => standalone(r"gh[pousr]_[A-Za-z0-9]{36,}"),
            Kind::GithubFineGrainedToken => standalone(r"github_pat_[A-Za-z0-9_]{22,}"),
            Kind::AwsAccessKeyId => {
                standalone(r"(?:AKIA|ASIA|ABIA|ACCA)[A-Z0-9]{16}") + r"(?:[^A-Za-z0-9]|$)"
            }
            Kind::AwsSecretAccessKey => r#"(?i)(?:aws[_-]?secret[_-]?access[_-]?key|aws[_-]?secret[_-]?key|secret[_-]?access[_-]?key)["']?\s*(?:[:=]|%3D)\s*["']?(?P<secret>[A-Za-z0-9/+]{40})(?:[^A-Za-z0-9/+]|$)"#.to_owned(),
            Kind::SlackBotToken => standalone(r"xoxb-[0-9]{8,}-[0-9]{8,}-[A-Za-z0-9]{20,}"),
            Kind::StripeSecretKey => standalone(r"[sr]k_(?:live|test)_[A-Za-z0-9]{20,}"),
            Kind::PostgresPassword => {
                r"\bpostgres(?:ql)?(?:\+[a-z0-9]+)?://[^\s:/@]*:(?P<secret>[^\s@]+)@".to_owned()
            }
            Kind::JsonWebToken => standalone(
                r"eyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}",
            ),
        }
    }
}

fn standalone(secret: &str) -> String {
    format!(r"(?:^|[^A-Za-z0-9_]|%3[Dd])(?P<secret>{secret})")
}

static PATTERNS: LazyLock<Vec<(Kind, Regex)>> = LazyLock::new(|| {
    Kind::ALL
        .iter()
        .map(|kind| {
            let regex = Regex::new(&kind.pattern()).expect("every secret pattern compiles");
            (*kind, regex)
        })
        .collect()
});

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Hidden {
    pub text: String,
    pub counts: BTreeMap<Kind, usize>,
}

impl Hidden {
    pub fn total(&self) -> usize {
        self.counts.values().sum()
    }
}

/// ```
/// use raiden_core::ai::secrets::{hide, Kind};
/// let key = format!("AKIA{}", "Q".repeat(16));
/// let hidden = hide(&format!("id = {key}"));
/// assert_eq!(hidden.text, "id = [hidden AWS access key ID]");
/// assert_eq!(hidden.counts[&Kind::AwsAccessKeyId], 1);
/// ```
pub fn hide(text: &str) -> Hidden {
    let mut text = text.to_owned();
    let mut counts = BTreeMap::new();
    for (kind, regex) in PATTERNS.iter() {
        let found = secrets_in(regex, &text);
        if found.is_empty() {
            continue;
        }
        let placeholder = format!("[hidden {}]", kind.label());
        let mut hidden = String::with_capacity(text.len());
        let mut kept = 0;
        for range in &found {
            hidden.push_str(&text[kept..range.start]);
            hidden.push_str(&placeholder);
            kept = range.end;
        }
        hidden.push_str(&text[kept..]);
        text = hidden;
        counts.insert(*kind, found.len());
    }
    Hidden { text, counts }
}

fn secrets_in(regex: &Regex, text: &str) -> Vec<std::ops::Range<usize>> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(captures) = regex.captures_at(text, from) {
        let Some(secret) = captures.name("secret").or_else(|| captures.get(0)) else {
            break;
        };
        if secret.end() <= from {
            break;
        }
        found.push(secret.range());
        from = secret.end();
    }
    found
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn letters(count: usize) -> String {
        "aB3dE5gH7jK9mN1pQ2sT4vW6yZ8"
            .chars()
            .cycle()
            .take(count)
            .collect()
    }

    fn upper(count: usize) -> String {
        "QWERTYUIOP2345678ASDFGHJKL"
            .chars()
            .cycle()
            .take(count)
            .collect()
    }

    fn digits(count: usize) -> String {
        "8271635490".chars().cycle().take(count).collect()
    }

    pub(crate) fn fakes() -> Vec<(Kind, String)> {
        let dot = ".";
        vec![
            (
                Kind::AnthropicApiKey,
                ["sk", "ant", "api03", &letters(90)].join("-"),
            ),
            (Kind::OpenaiApiKey, ["sk", "proj", &letters(48)].join("-")),
            (Kind::GithubToken, ["ghp", &letters(36)].join("_")),
            (
                Kind::GithubFineGrainedToken,
                ["github", "pat", &letters(22), &letters(59)].join("_"),
            ),
            (Kind::AwsAccessKeyId, ["AKIA", &upper(16)].concat()),
            (
                Kind::AwsSecretAccessKey,
                ["aws_secret_access_key = ", &letters(36), "/+Ab"].concat(),
            ),
            (
                Kind::SlackBotToken,
                ["xoxb", &digits(12), &digits(13), &letters(24)].join("-"),
            ),
            (
                Kind::StripeSecretKey,
                ["sk", "live", &letters(24)].join("_"),
            ),
            (
                Kind::PostgresPassword,
                ["postgres", "://app:", &letters(16), "@db.example.com/main"].concat(),
            ),
            (
                Kind::JsonWebToken,
                [
                    "eyJ",
                    &letters(20),
                    dot,
                    "eyJ",
                    &letters(30),
                    dot,
                    &letters(43),
                ]
                .concat(),
            ),
        ]
    }

    pub(crate) fn secret_part(kind: Kind, fake: &str) -> String {
        match kind {
            Kind::AwsSecretAccessKey => fake.rsplit(' ').next().unwrap_or(fake).to_owned(),
            Kind::PostgresPassword => fake
                .split(':')
                .nth(2)
                .and_then(|rest| rest.split('@').next())
                .unwrap_or(fake)
                .to_owned(),
            _ => fake.to_owned(),
        }
    }

    #[test]
    fn every_format_is_hidden_and_counted() {
        for (kind, fake) in fakes() {
            let hidden = hide(&format!("before {fake} after"));
            assert!(
                !hidden.text.contains(&secret_part(kind, &fake)),
                "{kind:?} leaked: {}",
                hidden.text
            );
            assert_eq!(
                hidden.counts,
                BTreeMap::from([(kind, 1)]),
                "{kind:?}: {}",
                hidden.text
            );
        }
    }

    #[test]
    fn all_ten_together_are_counted_once_each() {
        let fakes = fakes();
        let text = fakes
            .iter()
            .map(|(_, fake)| fake.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let hidden = hide(&text);
        assert_eq!(hidden.total(), 10, "{}", hidden.text);
        for (kind, fake) in &fakes {
            assert!(!hidden.text.contains(&secret_part(*kind, fake)), "{kind:?}");
        }
    }

    #[test]
    fn the_rest_of_a_connection_string_stays() {
        let url = ["postgresql", "://me:", &letters(12), "@localhost:5432/app"].concat();
        assert_eq!(
            hide(&url).text,
            "postgresql://me:[hidden Postgres password]@localhost:5432/app"
        );
    }

    #[test]
    fn a_driver_in_the_scheme_still_hides_the_password() {
        let url = ["postgresql+psycopg2", "://me:", &letters(12), "@db/app"].concat();
        assert_eq!(
            hide(&url).text,
            "postgresql+psycopg2://me:[hidden Postgres password]@db/app"
        );
    }

    #[test]
    fn a_password_without_a_user_is_hidden() {
        let url = ["postgres", "://:", &letters(12), "@db/app"].concat();
        assert_eq!(
            hide(&url).text,
            "postgres://:[hidden Postgres password]@db/app"
        );
    }

    #[test]
    fn hyphenated_aws_names_and_encoded_equals_are_covered() {
        let secret = [&letters(36), "/+Ab"].concat();
        for text in [
            format!("aws-secret-access-key: {secret}"),
            format!("AWS_SECRET_ACCESS_KEY%3D{secret}&x=1"),
        ] {
            let hidden = hide(&text);
            assert!(!hidden.text.contains(&secret), "{}", hidden.text);
            assert_eq!(hidden.counts[&Kind::AwsSecretAccessKey], 1);
        }
    }

    #[test]
    fn a_password_with_a_slash_is_hidden_whole() {
        let password = [&letters(6), "/", &letters(6)].concat();
        let url = ["postgres://app:", &password, "@db/main"].concat();
        assert_eq!(
            hide(&url).text,
            "postgres://app:[hidden Postgres password]@db/main"
        );
    }

    #[test]
    fn keys_right_after_an_equals_sign_are_hidden() {
        for (kind, fake) in fakes() {
            if matches!(kind, Kind::AwsSecretAccessKey | Kind::PostgresPassword) {
                continue;
            }
            for text in [format!("key={fake}"), format!("?key%3D{fake}&next=1")] {
                let hidden = hide(&text);
                assert!(!hidden.text.contains(&fake), "{kind:?}: {}", hidden.text);
                assert_eq!(
                    hidden.counts.get(&kind),
                    Some(&1),
                    "{kind:?}: {}",
                    hidden.text
                );
            }
        }
    }

    #[test]
    fn the_character_after_a_key_id_stays() {
        let id = ["AKIA", &upper(16)].concat();
        assert_eq!(
            hide(&format!("{id}, then")).text,
            "[hidden AWS access key ID], then"
        );
    }

    #[test]
    fn two_secrets_one_character_apart_are_both_hidden() {
        for (kind, fake) in fakes() {
            for between in [",", ";", " ", "="] {
                let text = format!("{fake}{between}{fake}");
                let hidden = hide(&text);
                assert!(
                    !hidden.text.contains(&secret_part(kind, &fake)),
                    "{kind:?} with {between:?}: {}",
                    hidden.text
                );
                assert_eq!(
                    hidden.counts.get(&kind),
                    Some(&2),
                    "{kind:?} with {between:?}: {}",
                    hidden.text
                );
            }
        }
    }

    #[test]
    fn ordinary_text_is_left_alone() {
        let text = "Use sk-learn for the task-list and see postgres://localhost/app.";
        let hidden = hide(text);
        assert_eq!(hidden.text, text);
        assert_eq!(hidden.total(), 0);
    }

    #[test]
    fn a_secret_said_twice_counts_twice() {
        let token = ["ghp", &letters(36)].join("_");
        let hidden = hide(&format!("{token} and {token}"));
        assert_eq!(hidden.counts[&Kind::GithubToken], 2);
    }
}
