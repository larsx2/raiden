pub const SOCKET: &str = "control.sock";

pub const HIDDEN_FOR: &str = "hidden-for";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub ok: bool,
    pub text: String,
}

impl Reply {
    pub fn ok(text: impl Into<String>) -> Self {
        Self {
            ok: true,
            text: text.into(),
        }
    }

    pub fn fail(text: impl Into<String>) -> Self {
        Self {
            ok: false,
            text: text.into(),
        }
    }

    pub fn encode(&self) -> String {
        format!(
            "{} {}\n",
            if self.ok { "ok" } else { "fail" },
            self.text.replace(['\n', '\r'], " ")
        )
    }

    pub fn decode(line: &str) -> Self {
        let line = line.trim_end_matches(['\r', '\n']);
        match line.split_once(' ') {
            Some(("ok", text)) => Self::ok(text),
            Some(("fail", text)) => Self::fail(text),
            _ if line == "ok" => Self::ok(""),
            _ => Self::fail(format!("Raiden replied {line:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_survive_the_round_trip() {
        for reply in [Reply::ok("120"), Reply::fail("Raiden is not running")] {
            assert_eq!(Reply::decode(&reply.encode()), reply);
        }
    }

    #[test]
    fn a_reply_stays_on_one_line() {
        assert_eq!(Reply::ok("one\ntwo").encode(), "ok one two\n");
    }
}
