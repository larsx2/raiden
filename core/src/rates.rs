use std::collections::HashMap;

/// The European Central Bank's daily rates, through frankfurter.dev.
pub const LINK: &str = "https://api.frankfurter.dev/v1/latest?base=USD";

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
    fn anything_else_is_no_rates() {
        assert_eq!(rates_of("<html>"), None);
        assert_eq!(rates_of(r#"{"message":"not found"}"#), None);
    }
}
