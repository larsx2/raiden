use std::time::{Duration, Instant};

pub const LIMIT: Duration = Duration::from_millis(300);
pub const MOST_HELD: usize = 64;

pub const SHIFT: u64 = 0x0002_0000;
pub const CONTROL: u64 = 0x0004_0000;
pub const OPTION: u64 = 0x0008_0000;
pub const COMMAND: u64 = 0x0010_0000;
const MODIFIERS: u64 = SHIFT | CONTROL | OPTION | COMMAND;

const KEY_CODES: &[(&str, u16)] = &[
    ("KeyA", 0),
    ("KeyS", 1),
    ("KeyD", 2),
    ("KeyF", 3),
    ("KeyH", 4),
    ("KeyG", 5),
    ("KeyZ", 6),
    ("KeyX", 7),
    ("KeyC", 8),
    ("KeyV", 9),
    ("KeyB", 11),
    ("KeyQ", 12),
    ("KeyW", 13),
    ("KeyE", 14),
    ("KeyR", 15),
    ("KeyY", 16),
    ("KeyT", 17),
    ("Digit1", 18),
    ("Digit2", 19),
    ("Digit3", 20),
    ("Digit4", 21),
    ("Digit6", 22),
    ("Digit5", 23),
    ("Equal", 24),
    ("Digit9", 25),
    ("Digit7", 26),
    ("Minus", 27),
    ("Digit8", 28),
    ("Digit0", 29),
    ("BracketRight", 30),
    ("KeyO", 31),
    ("KeyU", 32),
    ("BracketLeft", 33),
    ("KeyI", 34),
    ("KeyP", 35),
    ("Enter", 36),
    ("KeyL", 37),
    ("KeyJ", 38),
    ("Quote", 39),
    ("KeyK", 40),
    ("Semicolon", 41),
    ("Backslash", 42),
    ("Comma", 43),
    ("Slash", 44),
    ("KeyN", 45),
    ("KeyM", 46),
    ("Period", 47),
    ("Tab", 48),
    ("Space", 49),
    ("Backquote", 50),
    ("Backspace", 51),
    ("Escape", 53),
    ("F1", 122),
    ("F2", 120),
    ("F3", 99),
    ("F4", 118),
    ("F5", 96),
    ("F6", 97),
    ("F7", 98),
    ("F8", 100),
    ("F9", 101),
    ("F10", 109),
    ("F11", 103),
    ("F12", 111),
    ("F13", 105),
    ("F14", 107),
    ("F15", 113),
    ("F16", 106),
    ("F17", 64),
    ("F18", 79),
    ("F19", 80),
    ("F20", 90),
    ("Delete", 117),
    ("Home", 115),
    ("End", 119),
    ("PageUp", 116),
    ("PageDown", 121),
    ("ArrowLeft", 123),
    ("ArrowRight", 124),
    ("ArrowDown", 125),
    ("ArrowUp", 126),
];

const SCREEN_KEYS: [(u16, &str); 8] = [
    (36, "Enter"),
    (76, "Enter"),
    (53, "Escape"),
    (51, "Backspace"),
    (125, "ArrowDown"),
    (126, "ArrowUp"),
    (116, "PageUp"),
    (121, "PageDown"),
];
const STOPS: [&str; 2] = ["Enter", "Escape"];
const FUNCTION_KEY_TEXT: std::ops::RangeInclusive<char> = '\u{f700}'..='\u{f8ff}';

/// ```
/// use raiden_core::typeahead::key_code;
/// assert_eq!(key_code("Space"), Some(49));
/// assert_eq!(key_code("KeyS"), Some(1));
/// assert_eq!(key_code("Fn"), None);
/// ```
pub fn key_code(name: &str) -> Option<u16> {
    KEY_CODES
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, code)| *code)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    pub code: u16,
    pub modifiers: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub code: u16,
    pub flags: u64,
    pub down: bool,
    pub repeat: bool,
    pub text: String,
}

impl Hotkey {
    fn matches(self, key: &Key) -> bool {
        key.code == self.code && key.flags & MODIFIERS == self.modifiers & MODIFIERS
    }

    fn pressed_by(self, key: &Key) -> bool {
        key.down && !key.repeat && self.matches(key)
    }

    fn belongs(self, key: &Key) -> bool {
        key.code == self.code && (!key.down || self.matches(key))
    }
}

impl Capture {
    fn passes_as_hotkey(&self, key: &Key) -> bool {
        if key.down {
            return self.hotkeys.iter().any(|hotkey| hotkey.matches(key));
        }
        self.hotkeys.iter().any(|hotkey| hotkey.belongs(key))
            && self.in_flight == 0
            && !self.downs.contains(&key.code)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Typed {
    pub key: String,
    pub text: String,
}

/// ```
/// use raiden_core::typeahead::{for_screen, Key, Typed, COMMAND};
/// let key = |code, flags, text: &str| Key { code, flags, down: true, repeat: false, text: text.into() };
/// assert_eq!(for_screen(&key(1, 0, "s")), Some(Typed { key: "s".into(), text: "s".into() }));
/// assert_eq!(for_screen(&key(36, 0, "\r")), Some(Typed { key: "Enter".into(), text: String::new() }));
/// assert_eq!(for_screen(&key(9, COMMAND, "v")), None);
/// assert_eq!(for_screen(&key(48, 0, "\t")), None);
/// ```
pub fn for_screen(key: &Key) -> Option<Typed> {
    if !key.down || key.flags & (COMMAND | CONTROL) != 0 {
        return None;
    }
    if let Some((_, name)) = SCREEN_KEYS.iter().find(|(code, _)| *code == key.code) {
        return (key.flags & OPTION == 0).then(|| Typed {
            key: (*name).to_owned(),
            text: String::new(),
        });
    }
    let printable = !key.text.is_empty()
        && key
            .text
            .chars()
            .all(|c| !c.is_control() && !FUNCTION_KEY_TEXT.contains(&c));
    printable.then(|| Typed {
        key: key.text.clone(),
        text: key.text.clone(),
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Hold,
    Post(Vec<Key>),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Taken {
    Screen(Vec<Typed>),
    Done(Vec<Key>),
}

#[derive(Debug, Default)]
pub struct Capture {
    hotkeys: Vec<Hotkey>,
    until: Option<Instant>,
    held: Vec<Key>,
    downs: Vec<u16>,
    round: u64,
    in_flight: usize,
    sending_since: Option<Instant>,
}

impl Capture {
    pub const fn new() -> Self {
        Self {
            hotkeys: Vec::new(),
            until: None,
            held: Vec::new(),
            downs: Vec::new(),
            round: 0,
            in_flight: 0,
            sending_since: None,
        }
    }

    pub fn follow(&mut self, hotkeys: Vec<Hotkey>) {
        self.hotkeys = hotkeys;
    }

    pub fn holding(&self) -> bool {
        self.until.is_some()
    }

    pub fn notice(&mut self, key: &Key, now: Instant) -> Option<u64> {
        if !self.hotkeys.iter().any(|hotkey| hotkey.pressed_by(key)) {
            return None;
        }
        self.until = Some(now + LIMIT);
        self.round += 1;
        Some(self.round)
    }

    pub fn see(&mut self, key: Key, now: Instant) -> Verdict {
        self.give_up_waiting(now);
        if self.passes_as_hotkey(&key) {
            return Verdict::Pass;
        }
        if let Some(until) = self.until
            && (key.down || self.downs.contains(&key.code))
        {
            if key.down {
                self.downs.push(key.code);
            }
            self.held.push(key);
            if now >= until || self.held.len() >= MOST_HELD {
                return Verdict::Post(self.end(now));
            }
            return Verdict::Hold;
        }
        if self.in_flight > 0 {
            return Verdict::Post(self.sent(vec![key], now));
        }
        Verdict::Pass
    }

    pub fn take(&mut self, now: Instant) -> Taken {
        if !self.holding() {
            return Taken::Done(Vec::new());
        }
        let mut count = 0;
        let mut typed = Vec::new();
        for key in &self.held {
            if key.down {
                let Some(screen) = for_screen(key) else {
                    break;
                };
                let stop = STOPS.contains(&screen.key.as_str());
                typed.push(screen);
                count += 1;
                if stop {
                    break;
                }
            } else {
                count += 1;
            }
        }
        if count == 0 {
            return Taken::Done(self.end(now));
        }
        self.held.drain(..count);
        Taken::Screen(typed)
    }

    pub fn release(&mut self, now: Instant) -> Vec<Key> {
        self.end(now)
    }

    pub fn expire(&mut self, round: u64, now: Instant) -> Vec<Key> {
        if self.round == round && self.holding() {
            self.end(now)
        } else {
            Vec::new()
        }
    }

    pub fn returned(&mut self) {
        self.in_flight = self.in_flight.saturating_sub(1);
        if self.in_flight == 0 {
            self.sending_since = None;
        }
    }

    pub fn lost_track(&mut self) {
        self.in_flight = 0;
        self.sending_since = None;
    }

    fn end(&mut self, now: Instant) -> Vec<Key> {
        self.until = None;
        self.downs.clear();
        let keys = std::mem::take(&mut self.held);
        self.sent(keys, now)
    }

    fn sent(&mut self, keys: Vec<Key>, now: Instant) -> Vec<Key> {
        if !keys.is_empty() {
            self.in_flight += keys.len();
            self.sending_since.get_or_insert(now);
        }
        keys
    }

    fn give_up_waiting(&mut self, now: Instant) {
        if self.sending_since.is_some_and(|since| now >= since + LIMIT) {
            self.lost_track();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOTKEY: Hotkey = Hotkey {
        code: 49,
        modifiers: OPTION,
    };
    const CLIPBOARD_HOTKEY: Hotkey = Hotkey {
        code: 9,
        modifiers: CONTROL | OPTION,
    };

    fn down(code: u16, flags: u64, text: &str) -> Key {
        Key {
            code,
            flags,
            down: true,
            repeat: false,
            text: text.into(),
        }
    }

    fn up(code: u16) -> Key {
        Key {
            down: false,
            ..down(code, 0, "")
        }
    }

    fn following() -> Capture {
        let mut capture = Capture::new();
        capture.follow(vec![HOTKEY, CLIPBOARD_HOTKEY]);
        capture
    }

    fn press_hotkey(capture: &mut Capture, now: Instant) -> Option<u64> {
        let hotkey = down(49, OPTION, " ");
        let round = capture.notice(&hotkey, now);
        assert_eq!(capture.see(hotkey, now), Verdict::Pass);
        round
    }

    fn hold_all(capture: &mut Capture, keys: &[Key], now: Instant) {
        for key in keys {
            assert_eq!(capture.see(key.clone(), now), Verdict::Hold, "{key:?}");
        }
    }

    fn names(taken: Taken) -> Vec<String> {
        match taken {
            Taken::Screen(typed) => typed.into_iter().map(|typed| typed.key).collect(),
            Taken::Done(keys) => panic!("expected keys for the screen, got {keys:?}"),
        }
    }

    fn codes(keys: &[Key]) -> Vec<(u16, bool)> {
        keys.iter().map(|key| (key.code, key.down)).collect()
    }

    fn saf_enter() -> Vec<Key> {
        vec![
            down(1, 0, "s"),
            up(1),
            down(0, 0, "a"),
            up(0),
            down(3, 0, "f"),
            up(3),
            down(36, 0, "\r"),
            up(36),
        ]
    }

    #[test]
    fn keys_pass_until_the_hotkey_is_pressed() {
        let mut capture = following();
        let now = Instant::now();
        for key in [down(1, 0, "s"), down(49, 0, " "), down(49, COMMAND, " ")] {
            assert_eq!(capture.notice(&key, now), None);
            assert_eq!(capture.see(key, now), Verdict::Pass);
        }
        assert!(!capture.holding());
    }

    #[test]
    fn nothing_is_held_once_the_hotkey_is_no_longer_followed() {
        let mut capture = following();
        capture.follow(Vec::new());
        let now = Instant::now();
        assert_eq!(press_hotkey(&mut capture, now), None);
        assert_eq!(capture.see(down(1, 0, "s"), now), Verdict::Pass);
    }

    #[test]
    fn the_screen_gets_the_keys_after_the_hotkey_in_order_then_the_capture_ends() {
        let mut capture = following();
        let now = Instant::now();
        assert_eq!(press_hotkey(&mut capture, now), Some(1));
        assert_eq!(capture.see(up(49), now), Verdict::Pass);
        hold_all(&mut capture, &saf_enter()[..6], now);
        assert_eq!(names(capture.take(now)), ["s", "a", "f"]);
        assert!(capture.holding());
        assert_eq!(capture.take(now), Taken::Done(Vec::new()));
        assert!(!capture.holding());
        assert_eq!(capture.see(down(1, 0, "s"), now), Verdict::Pass);
    }

    #[test]
    fn the_clipboard_hotkey_holds_the_keys_after_it_for_the_screen_in_order() {
        let mut capture = following();
        let now = Instant::now();
        let hotkey = down(9, CONTROL | OPTION, "\u{16}");
        assert_eq!(capture.notice(&hotkey, now), Some(1));
        assert_eq!(capture.see(hotkey, now), Verdict::Pass);
        assert_eq!(capture.see(up(9), now), Verdict::Pass);
        hold_all(
            &mut capture,
            &[down(9, 0, "v"), up(9), down(1, 0, "s"), up(1)],
            now,
        );
        assert_eq!(names(capture.take(now)), ["v", "s"]);
        assert_eq!(capture.take(now), Taken::Done(Vec::new()));
        assert_eq!(capture.see(down(0, 0, "a"), now), Verdict::Pass);
    }

    #[test]
    fn only_the_hotkeys_followed_start_holding() {
        let mut capture = Capture::new();
        capture.follow(vec![HOTKEY]);
        let now = Instant::now();
        for key in [down(9, CONTROL | OPTION, "\u{16}"), down(9, 0, "v")] {
            assert_eq!(capture.notice(&key, now), None, "{key:?}");
            assert_eq!(capture.see(key, now), Verdict::Pass);
        }
        assert!(!capture.holding());
        assert_eq!(press_hotkey(&mut capture, now), Some(1));
    }

    #[test]
    fn the_screen_stops_taking_at_enter_and_hiding_hands_the_rest_back_in_order() {
        let mut capture = following();
        let now = Instant::now();
        press_hotkey(&mut capture, now);
        let mut keys = saf_enter();
        keys.extend([down(7, 0, "x"), up(7)]);
        hold_all(&mut capture, &keys, now);
        assert_eq!(names(capture.take(now)), ["s", "a", "f", "Enter"]);
        assert_eq!(
            codes(&capture.release(now)),
            [(36, false), (7, true), (7, false)]
        );
    }

    #[test]
    fn the_screen_stops_taking_at_escape() {
        let mut capture = following();
        let now = Instant::now();
        press_hotkey(&mut capture, now);
        hold_all(&mut capture, &[down(53, 0, "\u{1b}"), down(1, 0, "s")], now);
        assert_eq!(names(capture.take(now)), ["Escape"]);
        assert_eq!(codes(&capture.release(now)), [(1, true)]);
    }

    #[test]
    fn keys_the_screen_cannot_type_are_handed_back_with_everything_after_them() {
        for native in [
            down(9, COMMAND, "v"),
            down(40, COMMAND, "k"),
            down(14, CONTROL, "\u{5}"),
            down(48, 0, "\t"),
            down(117, 0, "\u{f728}"),
            down(115, 0, "\u{f729}"),
            down(119, 0, "\u{f72b}"),
            down(123, 0, "\u{f702}"),
            down(96, 0, "\u{f708}"),
            down(125, OPTION, "\u{f701}"),
        ] {
            let mut capture = following();
            let now = Instant::now();
            press_hotkey(&mut capture, now);
            let keys = [down(1, 0, "s"), native.clone(), down(0, 0, "a")];
            hold_all(&mut capture, &keys, now);
            assert_eq!(names(capture.take(now)), ["s"], "{native:?}");
            let Taken::Done(back) = capture.take(now) else {
                panic!("{native:?} should end the screen's turn");
            };
            assert_eq!(back, keys[1..], "{native:?}");
            assert!(!capture.holding());
        }
    }

    #[test]
    fn a_typed_space_keeps_its_key_up_behind_it_when_the_hotkey_uses_space() {
        let mut capture = following();
        let now = Instant::now();
        press_hotkey(&mut capture, now);
        assert_eq!(capture.see(up(49), now), Verdict::Pass);
        hold_all(&mut capture, &[down(49, 0, " "), up(49)], now);
        assert_eq!(codes(&capture.release(now)), [(49, true), (49, false)]);
        assert_eq!(capture.see(up(49), now), Verdict::Post(vec![up(49)]));
    }

    #[test]
    fn key_ups_whose_key_downs_were_not_held_pass() {
        let mut capture = following();
        let now = Instant::now();
        press_hotkey(&mut capture, now);
        assert_eq!(capture.see(up(1), now), Verdict::Pass);
        assert_eq!(capture.see(down(0, 0, "a"), now), Verdict::Hold);
        assert_eq!(capture.see(up(0), now), Verdict::Hold);
    }

    #[test]
    fn keys_typed_while_handed_back_keys_are_on_their_way_follow_them() {
        let mut capture = following();
        let now = Instant::now();
        press_hotkey(&mut capture, now);
        hold_all(&mut capture, &[down(1, 0, "s")], now);
        assert_eq!(codes(&capture.release(now)), [(1, true)]);
        assert_eq!(capture.see(up(1), now), Verdict::Post(vec![up(1)]));
        assert_eq!(
            capture.see(down(0, 0, "a"), now),
            Verdict::Post(vec![down(0, 0, "a")])
        );
        for _ in 0..3 {
            capture.returned();
        }
        assert_eq!(capture.see(down(3, 0, "f"), now), Verdict::Pass);
    }

    #[test]
    fn waiting_for_handed_back_keys_gives_up_after_the_time_limit() {
        let mut capture = following();
        let now = Instant::now();
        press_hotkey(&mut capture, now);
        hold_all(&mut capture, &[down(1, 0, "s")], now);
        capture.release(now);
        assert_eq!(capture.see(down(0, 0, "a"), now + LIMIT), Verdict::Pass);
    }

    #[test]
    fn a_handed_back_key_that_never_returns_stops_the_handing_back_at_the_limit() {
        let mut capture = following();
        let start = Instant::now();
        press_hotkey(&mut capture, start);
        hold_all(&mut capture, &[down(1, 0, "s")], start);
        capture.release(start);
        let step = LIMIT / 4;
        for index in 1..4 {
            let key = down(0, 0, "a");
            assert_eq!(
                capture.see(key.clone(), start + step * index),
                Verdict::Post(vec![key])
            );
            capture.returned();
        }
        assert_eq!(capture.see(down(3, 0, "f"), start + LIMIT), Verdict::Pass);
    }

    #[test]
    fn each_new_round_of_handing_back_gets_its_own_limit() {
        let mut capture = following();
        let start = Instant::now();
        press_hotkey(&mut capture, start);
        hold_all(&mut capture, &[down(1, 0, "s")], start);
        capture.release(start);
        capture.returned();
        let later = start + LIMIT * 2;
        press_hotkey(&mut capture, later);
        hold_all(&mut capture, &[down(0, 0, "a")], later);
        capture.release(later);
        let key = down(3, 0, "f");
        assert_eq!(
            capture.see(key.clone(), later + LIMIT / 2),
            Verdict::Post(vec![key])
        );
    }

    #[test]
    fn losing_track_of_handed_back_keys_stops_waiting_for_them() {
        let mut capture = following();
        let now = Instant::now();
        press_hotkey(&mut capture, now);
        hold_all(&mut capture, &[down(1, 0, "s")], now);
        capture.release(now);
        capture.lost_track();
        assert_eq!(capture.see(down(0, 0, "a"), now), Verdict::Pass);
    }

    #[test]
    fn caps_lock_does_not_hide_the_hotkey() {
        let mut capture = following();
        let caps_lock = 0x0001_0000;
        assert_eq!(
            capture.notice(&down(49, OPTION | caps_lock, " "), Instant::now()),
            Some(1)
        );
    }

    #[test]
    fn a_held_down_hotkey_does_not_start_again() {
        let mut capture = following();
        let now = Instant::now();
        press_hotkey(&mut capture, now);
        let repeat = Key {
            repeat: true,
            ..down(49, OPTION, " ")
        };
        assert_eq!(capture.notice(&repeat, now), None);
        assert_eq!(capture.see(repeat, now), Verdict::Pass);
    }

    #[test]
    fn a_key_after_the_time_limit_hands_back_everything_held_in_order() {
        let mut capture = following();
        let start = Instant::now();
        press_hotkey(&mut capture, start);
        hold_all(&mut capture, &[down(1, 0, "s"), up(1)], start);
        let Verdict::Post(keys) = capture.see(down(0, 0, "a"), start + LIMIT) else {
            panic!("the late key should hand back what was held");
        };
        assert_eq!(codes(&keys), [(1, true), (1, false), (0, true)]);
        assert!(!capture.holding());
    }

    #[test]
    fn too_many_keys_hand_back_everything() {
        let mut capture = following();
        let now = Instant::now();
        press_hotkey(&mut capture, now);
        for _ in 1..MOST_HELD {
            assert_eq!(capture.see(down(1, 0, "s"), now), Verdict::Hold);
        }
        assert!(matches!(
            capture.see(down(1, 0, "s"), now),
            Verdict::Post(keys) if keys.len() == MOST_HELD
        ));
        assert!(!capture.holding());
    }

    #[test]
    fn expiring_an_old_round_leaves_a_newer_one_alone() {
        let mut capture = following();
        let now = Instant::now();
        press_hotkey(&mut capture, now);
        hold_all(&mut capture, &[down(1, 0, "s")], now);
        press_hotkey(&mut capture, now);
        assert!(capture.expire(1, now).is_empty());
        assert!(capture.holding());
        assert_eq!(capture.expire(2, now).len(), 1);
        assert!(!capture.holding());
    }

    #[test]
    fn option_letters_are_typed_and_function_keys_are_not() {
        assert_eq!(
            for_screen(&down(31, OPTION, "ø")).map(|typed| typed.text),
            Some("ø".to_owned())
        );
        assert_eq!(for_screen(&down(122, 0, "\u{f704}")), None);
        assert_eq!(for_screen(&up(1)), None);
    }

    #[test]
    fn every_key_name_has_one_code() {
        for (name, code) in KEY_CODES {
            assert_eq!(key_code(name), Some(*code), "{name}");
            assert_eq!(
                KEY_CODES.iter().filter(|(_, other)| other == code).count(),
                1,
                "{name}"
            );
        }
    }
}
