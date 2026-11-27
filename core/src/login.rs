pub const MARKER: &str = "registered-launch-agent";
pub const LOGIN_ITEM_MARKER: &str = "registered-at-login";
pub const BUNDLED_AGENT_MARKER: &str = "registered-agent-at-login";
pub const AGENT_PLIST: &str = "com.larsx2.raiden.plist";
pub const AGENT_LABEL: &str = "com.larsx2.raiden.agent";

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// ```
/// let plist = raiden_core::login::agent_plist("/Users/me/Applications/Raiden.app/Contents/MacOS/Raiden");
/// assert!(plist.contains("<string>com.larsx2.raiden.agent</string>"));
/// assert!(plist.contains("<key>Crashed</key>"));
/// ```
pub fn agent_plist(program: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{AGENT_LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{}</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<dict>
		<key>Crashed</key>
		<true/>
	</dict>
	<key>LimitLoadToSessionType</key>
	<string>Aqua</string>
	<key>ProcessType</key>
	<string>Interactive</string>
</dict>
</plist>
"#,
        xml(program)
    )
}

/// ```
/// use raiden_core::login::{disabled, AGENT_LABEL};
/// let listing = format!("disabled services = {{\n\t\"{AGENT_LABEL}\" => disabled\n}}");
/// assert!(disabled(&listing));
/// assert!(!disabled(&listing.replace("=> disabled", "=> enabled")));
/// ```
pub fn disabled(listing: &str) -> bool {
    let name = format!("\"{AGENT_LABEL}\"");
    listing.lines().any(|line| {
        let line = line.trim();
        line.starts_with(&name) && (line.ends_with("=> disabled") || line.ends_with("=> true"))
    })
}

pub const DEVELOPMENT_NOTE: &str = "Only the installed copy opens at login";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AtLogin {
    pub on: bool,
    pub can_change: bool,
    pub note: Option<String>,
}

/// ```
/// use raiden_core::login::at_login;
/// assert!(at_login(false, true, false).on);
/// assert!(!at_login(false, true, true).on);
/// assert!(!at_login(true, true, false).can_change);
/// ```
pub fn at_login(development: bool, written: bool, disabled: bool) -> AtLogin {
    if development {
        return AtLogin {
            on: false,
            can_change: false,
            note: Some(DEVELOPMENT_NOTE.to_owned()),
        };
    }
    AtLogin {
        on: written && !disabled,
        can_change: true,
        note: None,
    }
}

/// ```
/// use raiden_core::login::switch;
/// assert_eq!(switch(501, false), ["disable", "gui/501/com.larsx2.raiden.agent"]);
/// ```
pub fn switch(user: u32, on: bool) -> [String; 2] {
    [
        if on { "enable" } else { "disable" }.to_owned(),
        format!("gui/{user}/{AGENT_LABEL}"),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    NotRegistered,
    Enabled,
    RequiresApproval,
    NotFound,
}

impl Status {
    fn registered(self) -> bool {
        matches!(self, Self::Enabled | Self::RequiresApproval)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seen {
    pub development: bool,
    pub agent_before: bool,
    pub login_item_before: bool,
    pub agent: Status,
    pub login_item: Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    RegisterAgent,
    Remember,
    LeaveLoginItem,
}

/// ```
/// use raiden_core::login::{plan, Seen, Status, Step};
/// let moving = Seen {
///     development: false,
///     agent_before: false,
///     login_item_before: true,
///     agent: Status::NotRegistered,
///     login_item: Status::Enabled,
/// };
/// assert_eq!(plan(moving), [Step::RegisterAgent, Step::Remember, Step::LeaveLoginItem]);
/// assert!(plan(Seen { development: true, ..moving }).is_empty());
/// ```
pub fn plan(seen: Seen) -> Vec<Step> {
    if seen.development {
        return Vec::new();
    }
    let leave = seen.login_item == Status::Enabled;
    let mut steps = Vec::new();
    if seen.agent_before {
        if leave && seen.agent.registered() {
            steps.push(Step::LeaveLoginItem);
        }
        return steps;
    }
    if seen.agent.registered() {
        steps.push(Step::Remember);
    } else if seen.login_item_before && !leave {
        return steps;
    } else {
        steps.extend([Step::RegisterAgent, Step::Remember]);
    }
    if leave {
        steps.push(Step::LeaveLoginItem);
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Status; 4] = [
        Status::NotRegistered,
        Status::Enabled,
        Status::RequiresApproval,
        Status::NotFound,
    ];

    fn installed(agent: Status, login_item: Status) -> Seen {
        Seen {
            development: false,
            agent_before: false,
            login_item_before: false,
            agent,
            login_item,
        }
    }

    fn every_seen() -> impl Iterator<Item = Seen> {
        ALL.into_iter().flat_map(|agent| {
            ALL.into_iter().flat_map(move |login_item| {
                [false, true].into_iter().flat_map(move |agent_before| {
                    [false, true].map(move |login_item_before| Seen {
                        development: false,
                        agent_before,
                        login_item_before,
                        agent,
                        login_item,
                    })
                })
            })
        })
    }

    #[test]
    fn open_at_login_is_on_only_while_the_agent_is_written_and_not_turned_off() {
        let cases = [
            (true, false, true),
            (true, true, false),
            (false, false, false),
            (false, true, false),
        ];
        for (written, disabled, on) in cases {
            let shown = at_login(false, written, disabled);
            assert_eq!(shown.on, on, "{written} {disabled}");
            assert!(shown.can_change);
            assert_eq!(shown.note, None);
        }
        for (written, disabled, _) in cases {
            let shown = at_login(true, written, disabled);
            assert!(!shown.on && !shown.can_change);
            assert_eq!(shown.note.as_deref(), Some(DEVELOPMENT_NOTE));
        }
    }

    #[test]
    fn turning_open_at_login_off_and_on_uses_launchctl_on_the_agent_alone() {
        assert_eq!(
            switch(501, true),
            ["enable", "gui/501/com.larsx2.raiden.agent"]
        );
        assert_eq!(
            switch(501, false),
            ["disable", "gui/501/com.larsx2.raiden.agent"]
        );
        let listing = format!("disabled services = {{\n\t\"{AGENT_LABEL}\" => disabled\n}}");
        assert!(!at_login(false, true, disabled(&listing)).on);
    }

    #[test]
    fn once_the_agent_is_remembered_it_never_registers_again() {
        for seen in every_seen().filter(|seen| seen.agent_before) {
            assert!(!plan(seen).contains(&Step::RegisterAgent), "{seen:?}");
        }
    }

    #[test]
    fn the_development_copy_never_registers() {
        for seen in every_seen() {
            let seen = Seen {
                development: true,
                ..seen
            };
            assert!(plan(seen).is_empty());
        }
    }

    #[test]
    fn the_login_item_is_left_only_once_the_agent_is_in_place() {
        for seen in every_seen() {
            let steps = plan(seen);
            let Some(leave) = steps.iter().position(|step| *step == Step::LeaveLoginItem) else {
                continue;
            };
            let registers = steps.contains(&Step::RegisterAgent);
            assert!(registers || seen.agent.registered(), "{seen:?} {steps:?}");
            if let Some(register) = steps.iter().position(|step| *step == Step::RegisterAgent) {
                assert!(register < leave, "{seen:?} {steps:?}");
            }
        }
    }

    #[test]
    fn an_enabled_login_item_is_never_kept_without_reason_and_never_left_alone() {
        for seen in every_seen().filter(|seen| seen.login_item == Status::Enabled) {
            let steps = plan(seen);
            let agent_coming = seen.agent.registered() || steps.contains(&Step::RegisterAgent);
            assert_eq!(
                steps.contains(&Step::LeaveLoginItem),
                agent_coming,
                "{seen:?} {steps:?}"
            );
        }
    }

    #[test]
    fn the_installed_copy_registers_the_agent_only_when_missing_and_remembers_otherwise() {
        let plans: Vec<Vec<Step>> = ALL
            .into_iter()
            .map(|agent| plan(installed(agent, Status::NotRegistered)))
            .collect();
        assert_eq!(
            plans,
            [
                vec![Step::RegisterAgent, Step::Remember],
                vec![Step::Remember],
                vec![Step::Remember],
                vec![Step::RegisterAgent, Step::Remember],
            ]
        );
    }

    #[test]
    fn an_enabled_login_item_moves_to_the_agent_even_after_it_was_remembered() {
        let seen = Seen {
            login_item_before: true,
            ..installed(Status::NotRegistered, Status::Enabled)
        };
        assert_eq!(
            plan(seen),
            [Step::RegisterAgent, Step::Remember, Step::LeaveLoginItem]
        );
    }

    #[test]
    fn a_login_item_left_over_after_the_move_is_left_on_a_later_start() {
        let seen = Seen {
            agent_before: true,
            ..installed(Status::Enabled, Status::Enabled)
        };
        assert_eq!(plan(seen), [Step::LeaveLoginItem]);
        let agent_removed = Seen {
            agent_before: true,
            ..installed(Status::NotRegistered, Status::Enabled)
        };
        assert!(plan(agent_removed).is_empty());
    }

    #[test]
    fn a_login_item_the_owner_removed_or_turned_off_stays_off() {
        for login_item in [
            Status::NotRegistered,
            Status::RequiresApproval,
            Status::NotFound,
        ] {
            let seen = Seen {
                login_item_before: true,
                ..installed(Status::NotRegistered, login_item)
            };
            assert!(plan(seen).is_empty(), "{login_item:?}");
        }
    }
}
