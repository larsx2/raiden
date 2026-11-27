use std::ffi::c_void;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use core_foundation::base::TCFType as _;
use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType, CallbackResult, EventField,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use foreign_types::ForeignType as _;
use raiden_core::typeahead::{self, Capture, Hotkey, Key, Taken, Typed, Verdict};
use tauri_plugin_global_shortcut::{Modifiers, Shortcut};

const REPLAYED: i64 = 0x5241_4944;
const HID: usize = 0;
const SESSION: usize = 1;

static CAPTURE: Mutex<Capture> = Mutex::new(Capture::new());
static TAPS: [AtomicPtr<c_void>; 2] = [
    AtomicPtr::new(std::ptr::null_mut()),
    AtomicPtr::new(std::ptr::null_mut()),
];

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapEnable(tap: *mut c_void, enable: bool);
    fn CGEventKeyboardGetUnicodeString(
        event: *mut c_void,
        most: libc::c_ulong,
        length: *mut libc::c_ulong,
        text: *mut u16,
    );
}

fn capture() -> MutexGuard<'static, Capture> {
    CAPTURE.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn hotkey_of(shortcut: &Shortcut) -> Option<Hotkey> {
    let code = typeahead::key_code(&shortcut.key.to_string())?;
    let modifiers = [
        (Modifiers::SHIFT, typeahead::SHIFT),
        (Modifiers::CONTROL, typeahead::CONTROL),
        (Modifiers::ALT, typeahead::OPTION),
        (Modifiers::SUPER | Modifiers::META, typeahead::COMMAND),
    ]
    .into_iter()
    .filter(|(held, _)| shortcut.mods.intersects(*held))
    .fold(0, |all, (_, flag)| all | flag);
    Some(Hotkey { code, modifiers })
}

pub fn text_of(event: &CGEvent) -> String {
    let mut buffer = [0_u16; 8];
    let mut length: libc::c_ulong = 0;
    unsafe {
        CGEventKeyboardGetUnicodeString(
            event.as_ptr().cast(),
            buffer.len() as libc::c_ulong,
            &raw mut length,
            buffer.as_mut_ptr(),
        );
    }
    let length = usize::try_from(length).unwrap_or(0).min(buffer.len());
    String::from_utf16_lossy(&buffer[..length])
}

fn key_of(event: &CGEvent, down: bool, with_text: bool) -> Key {
    Key {
        code: u16::try_from(event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE))
            .unwrap_or(u16::MAX),
        flags: event.get_flags().bits(),
        down,
        repeat: event.get_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT) != 0,
        text: if down && with_text {
            text_of(event)
        } else {
            String::new()
        },
    }
}

fn post(_held: &MutexGuard<'static, Capture>, keys: &[Key]) {
    if keys.is_empty() {
        return;
    }
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
        eprintln!("Raiden could not hand back {} keys it held.", keys.len());
        return;
    };
    for key in keys {
        let Ok(event) = CGEvent::new_keyboard_event(source.clone(), key.code, key.down) else {
            eprintln!("Raiden could not hand back a key it held.");
            continue;
        };
        event.set_flags(CGEventFlags::from_bits_retain(key.flags));
        if !key.text.is_empty() {
            event.set_string(&key.text);
        }
        event.set_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT, i64::from(key.repeat));
        event.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, REPLAYED);
        event.post(CGEventTapLocation::Session);
    }
}

fn expire_later(round: u64) {
    let spawned = std::thread::Builder::new()
        .name("raiden-typeahead-limit".into())
        .spawn(move || {
            std::thread::sleep(typeahead::LIMIT);
            let mut held = capture();
            let keys = held.expire(round, Instant::now());
            post(&held, &keys);
        });
    if spawned.is_err() {
        release();
    }
}

fn reenable_taps(why: &str) {
    eprintln!("macOS paused Raiden's keyboard watch ({why}); Raiden turned it back on.");
    for tap in &TAPS {
        let tap = tap.load(Ordering::SeqCst);
        if !tap.is_null() {
            unsafe { CGEventTapEnable(tap, true) };
        }
    }
    let mut held = capture();
    held.lost_track();
    let keys = held.release(Instant::now());
    post(&held, &keys);
}

fn paused(kind: CGEventType) -> Option<&'static str> {
    match kind {
        CGEventType::TapDisabledByTimeout => Some("it answered too slowly"),
        CGEventType::TapDisabledByUserInput => Some("user input"),
        _ => None,
    }
}

fn notice(key: &Key) -> bool {
    let round = capture().notice(key, Instant::now());
    if let Some(round) = round {
        expire_later(round);
    }
    round.is_some()
}

fn on_hid_event(kind: CGEventType, event: &CGEvent) -> CallbackResult {
    if let Some(why) = paused(kind) {
        reenable_taps(why);
    } else if matches!(kind, CGEventType::KeyDown) {
        notice(&key_of(event, true, false));
    }
    CallbackResult::Keep
}

fn on_session_event(kind: CGEventType, event: &CGEvent) -> CallbackResult {
    if let Some(why) = paused(kind) {
        reenable_taps(why);
        return CallbackResult::Keep;
    }
    let down = match kind {
        CGEventType::KeyDown => true,
        CGEventType::KeyUp => false,
        _ => return CallbackResult::Keep,
    };
    if event.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA) == REPLAYED {
        capture().returned();
        return CallbackResult::Keep;
    }
    let key = key_of(event, down, true);
    if TAPS[HID].load(Ordering::SeqCst).is_null() && notice(&key) {
        return CallbackResult::Keep;
    }
    let mut held = capture();
    match held.see(key, Instant::now()) {
        Verdict::Pass => CallbackResult::Keep,
        Verdict::Hold => CallbackResult::Drop,
        Verdict::Post(keys) => {
            post(&held, &keys);
            CallbackResult::Drop
        }
    }
}

fn add_tap(
    location: CGEventTapLocation,
    slot: usize,
    callback: fn(CGEventType, &CGEvent) -> CallbackResult,
) -> Option<CGEventTap<'static>> {
    let tap = CGEventTap::new(
        location,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        vec![CGEventType::KeyDown, CGEventType::KeyUp],
        move |_, kind, event| callback(kind, event),
    )
    .ok()?;
    let source = tap.mach_port().create_runloop_source(0).ok()?;
    CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });
    TAPS[slot].store(
        tap.mach_port().as_concrete_TypeRef().cast(),
        Ordering::SeqCst,
    );
    tap.enable();
    Some(tap)
}

fn forget_taps() {
    for tap in &TAPS {
        tap.store(std::ptr::null_mut(), Ordering::SeqCst);
    }
}

fn run_taps() {
    let hid = add_tap(CGEventTapLocation::HID, HID, on_hid_event);
    if hid.is_none() {
        eprintln!(
            "Raiden could not watch the keyboard ahead of macOS's hotkeys, so it watches for its hotkey later in line."
        );
    }
    let Some(session) = add_tap(CGEventTapLocation::Session, SESSION, on_session_event) else {
        forget_taps();
        drop(hid);
        eprintln!(
            "Raiden could not watch the keyboard, so keys typed right after the hotkey may reach the app in front. It needs Accessibility access, then a restart."
        );
        return;
    };
    CFRunLoop::run_current();
    forget_taps();
    drop((hid, session));
    release();
}

pub fn follow(shortcuts: &[Shortcut]) {
    let hotkeys = shortcuts
        .iter()
        .filter_map(|shortcut| {
            let hotkey = hotkey_of(shortcut);
            if hotkey.is_none() {
                eprintln!(
                    "Raiden cannot hold the keys typed right after {shortcut}, because it does not know where that key sits on the keyboard."
                );
            }
            hotkey
        })
        .collect();
    capture().follow(hotkeys);
}

pub fn watch() {
    if let Err(error) = std::thread::Builder::new()
        .name("raiden-typeahead".into())
        .spawn(run_taps)
    {
        eprintln!("Raiden could not start watching the keyboard: {error}");
    }
}

pub fn release() {
    let mut held = capture();
    let keys = held.release(Instant::now());
    post(&held, &keys);
}

#[tauri::command]
pub fn typed_ahead() -> Option<Vec<Typed>> {
    let mut held = capture();
    match held.take(Instant::now()) {
        Taken::Screen(typed) => Some(typed),
        Taken::Done(keys) => {
            post(&held, &keys);
            None
        }
    }
}
