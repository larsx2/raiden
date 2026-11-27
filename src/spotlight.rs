use plist::Value;
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

const SPOTLIGHT_SHORTCUT: &str = "64";
const SPACE_KEY: u64 = 49;
const COMMAND_MASK: u64 = 1 << 20;

pub fn command_space() -> Shortcut {
    Shortcut::new(Some(Modifiers::SUPER), Code::Space)
}

pub fn holds_command_space() -> bool {
    std::process::Command::new("/usr/bin/defaults")
        .args(["export", "com.apple.symbolichotkeys", "-"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| holds_in(&output.stdout))
}

fn holds_in(exported: &[u8]) -> bool {
    let Ok(Value::Dictionary(root)) = Value::from_reader(std::io::Cursor::new(exported)) else {
        return false;
    };
    let Some(shortcut) = root
        .get("AppleSymbolicHotKeys")
        .and_then(Value::as_dictionary)
        .and_then(|shortcuts| shortcuts.get(SPOTLIGHT_SHORTCUT))
        .and_then(Value::as_dictionary)
    else {
        return true;
    };
    let on = match shortcut.get("enabled") {
        Some(Value::Boolean(on)) => *on,
        Some(Value::Integer(number)) => number.as_signed() != Some(0),
        _ => true,
    };
    let keys = shortcut
        .get("value")
        .and_then(Value::as_dictionary)
        .and_then(|value| value.get("parameters"))
        .and_then(Value::as_array);
    let on_command_space = keys.is_none_or(|keys| {
        keys.get(1).and_then(Value::as_unsigned_integer) == Some(SPACE_KEY)
            && keys.get(2).and_then(Value::as_unsigned_integer) == Some(COMMAND_MASK)
    });
    on && on_command_space
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exported(entry: &str) -> Vec<u8> {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>\
             <key>AppleSymbolicHotKeys</key><dict>{entry}</dict></dict></plist>\n"
        )
        .into_bytes()
    }

    fn spotlight_entry(enabled: &str, key: u64, mask: u64) -> String {
        format!(
            "<key>64</key><dict><key>enabled</key>{enabled}<key>value</key><dict>\
             <key>parameters</key><array><integer>32</integer><integer>{key}</integer>\
             <integer>{mask}</integer></array><key>type</key><string>standard</string>\
             </dict></dict>"
        )
    }

    #[test]
    fn spotlight_holds_command_space_only_while_its_shortcut_is_on() {
        assert!(!holds_in(&exported(&spotlight_entry(
            "<false/>", 49, 1_048_576
        ))));
        assert!(holds_in(&exported(&spotlight_entry(
            "<true/>", 49, 1_048_576
        ))));
        assert!(!holds_in(&exported(&spotlight_entry(
            "<integer>0</integer>",
            49,
            1_048_576
        ))));
        assert!(holds_in(&exported(&spotlight_entry(
            "<integer>1</integer>",
            49,
            1_048_576
        ))));
    }

    #[test]
    fn spotlight_moved_to_other_keys_leaves_command_space_free() {
        assert!(!holds_in(&exported(&spotlight_entry(
            "<true/>", 49, 1_179_648
        ))));
        assert!(!holds_in(&exported(&spotlight_entry(
            "<true/>", 50, 1_048_576
        ))));
    }

    #[test]
    fn a_mac_that_never_changed_the_shortcut_has_spotlight_on_command_space() {
        assert!(holds_in(&exported("")));
        assert!(holds_in(
            b"<?xml version=\"1.0\"?><plist version=\"1.0\"><dict/></plist>"
        ));
        assert!(holds_in(&exported("<key>64</key><dict/>")));
    }

    #[test]
    fn an_unreadable_answer_leaves_the_choice_to_registering() {
        assert!(!holds_in(b"not a plist"));
    }
}
