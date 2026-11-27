use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread::sleep;
use std::time::Duration;

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardTypeString, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSData, NSString};

const V_KEY: u16 = 9;

static PASTING: Mutex<()> = Mutex::new(());
static OWN: Mutex<Vec<(String, isize)>> = Mutex::new(Vec::new());
const OWN_KEPT: usize = 8;

pub fn pasting() -> MutexGuard<'static, ()> {
    PASTING.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn mark_own(board: &NSPasteboard) {
    let mut own = OWN.lock().unwrap_or_else(PoisonError::into_inner);
    own.push((board.name().to_string(), board.changeCount()));
    let extra = own.len().saturating_sub(OWN_KEPT);
    own.drain(..extra);
}

pub fn own(board: &NSPasteboard, count: isize) -> bool {
    let name = board.name().to_string();
    OWN.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .any(|(board, mark)| *board == name && *mark == count)
}

type SavedItem = Vec<(Retained<NSString>, Retained<NSData>)>;

fn save(board: &NSPasteboard) -> Vec<SavedItem> {
    let Some(items) = board.pasteboardItems() else {
        return Vec::new();
    };
    items
        .iter()
        .map(|item| {
            let types = item.types();
            types
                .iter()
                .filter_map(|kind| item.dataForType(&kind).map(|data| (kind, data)))
                .collect()
        })
        .collect()
}

fn private(saved: &[SavedItem]) -> bool {
    let types: Vec<String> = saved
        .iter()
        .flatten()
        .map(|(kind, _)| kind.to_string())
        .collect();
    raiden_core::clipboard::is_private(&types)
}

fn restore(board: &NSPasteboard, saved: Vec<SavedItem>) {
    board.clearContents();
    if saved.is_empty() {
        return;
    }
    let items: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = saved
        .into_iter()
        .map(|pairs| {
            let item = NSPasteboardItem::new();
            for (kind, data) in pairs {
                item.setData_forType(&data, &kind);
            }
            ProtocolObject::from_retained(item)
        })
        .collect();
    board.writeObjects(&NSArray::from_retained_slice(&items));
}

fn press_command_v() -> Result<(), String> {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|()| "Raiden could not make a keyboard event".to_owned())?;
    for down in [true, false] {
        let event = CGEvent::new_keyboard_event(source.clone(), V_KEY, down)
            .map_err(|()| "Raiden could not make a keyboard event".to_owned())?;
        event.set_flags(CGEventFlags::CGEventFlagCommand);
        event.post(CGEventTapLocation::HID);
    }
    Ok(())
}

pub fn answer_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect()
}

pub fn paste_answer(text: &str) -> Result<(), String> {
    put(&answer_text(text))
}

fn put(text: &str) -> Result<(), String> {
    put_with(|board| unsafe {
        board.clearContents();
        board.setString_forType(&NSString::from_str(text), NSPasteboardTypeString)
    })
}

pub fn copy_with(
    write: impl FnOnce(&NSPasteboard) -> bool,
) -> Result<Option<std::thread::JoinHandle<()>>, String> {
    let _pasting = pasting();
    let board = NSPasteboard::generalPasteboard();
    let recording = crate::clipboard::before_raiden_writes(&board);
    let saved = save(&board);
    let wrote = write(&board);
    mark_own(&board);
    if wrote {
        return Ok(recording);
    }
    restore(&board, saved);
    mark_own(&board);
    Err("macOS did not take the item onto the clipboard.".to_owned())
}

pub fn put_with(write: impl FnOnce(&NSPasteboard) -> bool) -> Result<(), String> {
    let _pasting = pasting();
    let board = NSPasteboard::generalPasteboard();
    let _recording = crate::clipboard::before_raiden_writes(&board);
    let saved = save(&board);
    let keep = !private(&saved);
    let wrote = write(&board);
    mark_own(&board);
    if !wrote {
        restore(&board, saved);
        mark_own(&board);
        return Err("macOS did not take the item onto the clipboard.".to_owned());
    }
    let written = board.changeCount();
    let pressed = press_command_v();
    sleep(Duration::from_millis(500));
    if board.changeCount() == written {
        if keep {
            restore(&board, saved);
        } else {
            board.clearContents();
        }
        mark_own(&board);
    }
    pressed
}

#[cfg(test)]
mod tests {
    use objc2_foundation::NSData;
    use raiden_core::clipboard::{self, PRIVATE_TYPES};

    use super::{NSString, SavedItem, answer_text, private};

    fn saved(kinds: &[&str]) -> Vec<SavedItem> {
        vec![
            kinds
                .iter()
                .map(|kind| (NSString::from_str(kind), NSData::new()))
                .collect(),
        ]
    }

    #[test]
    fn a_paste_back_and_clipboard_history_treat_the_same_types_as_private() {
        for kind in PRIVATE_TYPES
            .iter()
            .copied()
            .chain(["public.utf8-plain-text", "public.png"])
        {
            let types = [String::from("public.utf8-plain-text"), kind.to_owned()];
            assert_eq!(
                private(&saved(&["public.utf8-plain-text", kind])),
                clipboard::is_private(&types),
                "{kind}"
            );
        }
        assert!(private(&saved(&["org.nspasteboard.AutoGeneratedType"])));
        assert!(!private(&saved(&["public.utf8-plain-text"])));
    }

    #[test]
    fn answers_keep_their_lines_and_lose_other_control_characters() {
        let text = "Run this:\r\n\tjust land\u{1b}[31m\u{7}\nDone";
        assert_eq!(answer_text(text), "Run this:\n\tjust land[31m\nDone");
    }
}
