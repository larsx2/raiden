use std::collections::HashMap;

const FREE: &str = "https://api.frankfurter.dev/v1/latest?base=USD";
const FIXER: &str = "https://data.fixer.io/api/latest?access_key=";

pub fn link(fixer_key: Option<&str>) -> String {
    fixer_key.map_or_else(|| FREE.to_owned(), |key| format!("{FIXER}{key}"))
}

pub fn fixer_key(text: &str) -> Result<String, String> {
    let key = text.trim();
    if key.len() == 32 && key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(key.to_ascii_lowercase())
    } else {
        Err(
            "Copy a Fixer API key first: 32 letters and digits from fixer.io's dashboard"
                .to_owned(),
        )
    }
}

pub fn rates_of(body: &str) -> Option<HashMap<String, f64>> {
    let reply: serde_json::Value = serde_json::from_str(body).ok()?;
    let base = reply.get("base")?.as_str()?;
    let mut rates: HashMap<String, f64> = reply
        .get("rates")?
        .as_object()?
        .iter()
        .filter_map(|(code, rate)| Some((code.clone(), rate.as_f64().filter(|r| *r > 0.0)?)))
        .collect();
    rates.insert(base.to_owned(), 1.0);
    Some(rates)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_rates_and_counts_the_base_as_one() {
        let rates =
            rates_of(r#"{"amount":1,"base":"USD","date":"2026-11-27","rates":{"EUR":0.9,"MXN":18.2,"BAD":0}}"#)
                .unwrap();
        assert_eq!(rates.get("USD"), Some(&1.0));
        assert_eq!(rates.get("MXN"), Some(&18.2));
        assert_eq!(rates.get("BAD"), None);
    }

    #[test]
    fn fixer_keys_are_32_hex_digits() {
        let key = "0123456789abcdef0123456789ABCDEF";
        assert_eq!(
            fixer_key(&format!(" {key}\n")),
            Ok(key.to_ascii_lowercase())
        );
        for text in ["", "hello", "0123456789abcdef0123456789abcdeg", &key[1..]] {
            assert!(fixer_key(text).is_err(), "{text}");
        }
    }

    #[test]
    fn the_link_uses_fixer_only_with_a_key() {
        assert!(link(None).starts_with("https://api.frankfurter.dev/"));
        assert_eq!(
            link(Some("abc")),
            "https://data.fixer.io/api/latest?access_key=abc"
        );
    }

    #[test]
    fn reads_what_fixer_says_and_its_failures() {
        let rates = rates_of(
            r#"{"success":true,"timestamp":1,"base":"EUR","date":"2026-11-27","rates":{"USD":1.1}}"#,
        )
        .unwrap();
        assert_eq!(rates.get("EUR"), Some(&1.0));
        assert_eq!(
            rates_of(r#"{"success":false,"error":{"code":101,"type":"invalid_access_key"}}"#),
            None
        );
    }

    #[test]
    fn anything_else_is_no_rates() {
        assert_eq!(rates_of("<html>"), None);
        assert_eq!(rates_of(r#"{"message":"not found"}"#), None);
    }
}
