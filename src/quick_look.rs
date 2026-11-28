use std::cell::{Cell, OnceCell, RefCell};
use std::ptr::NonNull;
use std::time::{Duration, Instant};

use block2::RcBlock;

use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2::{ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSEvent, NSEventMask, NSResponder, NSWindow, NSWindowWillCloseNotification,
    NSWorkspace,
};
use objc2_foundation::{
    NSInteger, NSNotification, NSNotificationCenter, NSOperationQueue, NSString, NSURL,
};
use objc2_quick_look_ui::{QLPreviewItem, QLPreviewPanel, QLPreviewPanelDataSource};
use tauri::{AppHandle, Emitter};
use tauri_nspanel::ManagerExt;

const LONGEST_CHAIN: usize = 64;
const LOADING_TAKES_FOCUS_FOR: Duration = Duration::from_secs(5);
// ponytail: without Accessibility the key monitor sees nothing, so a shortcut to another panel
// (⌘Space) can't count as leaving; a short window caps how long Raiden may pull the keyboard back.
// A slow preview that takes the keyboard later than this hides Raiden, as before the grace period.
const LOADING_TAKES_FOCUS_UNWATCHED_FOR: Duration = Duration::from_secs(1);
pub const EVENT: &str = "preview";

#[derive(Default)]
pub struct Looked {
    url: RefCell<Option<Retained<NSURL>>>,
}

define_class!(
    #[unsafe(super(NSResponder))]
    #[thread_kind = MainThreadOnly]
    #[name = "RaidenQuickLook"]
    #[ivars = Looked]
    struct Previewer;

    impl Previewer {
        #[unsafe(method(acceptsPreviewPanelControl:))]
        fn accepts_control(&self, _panel: Option<&QLPreviewPanel>) -> bool {
            true
        }

        #[unsafe(method(beginPreviewPanelControl:))]
        fn begin_control(&self, panel: Option<&QLPreviewPanel>) {
            if let Some(panel) = panel {
                self.feed(panel);
            }
        }

        #[unsafe(method(endPreviewPanelControl:))]
        fn end_control(&self, panel: Option<&QLPreviewPanel>) {
            if let Some(panel) = panel {
                unsafe { panel.setDataSource(None) };
                if !panel.isVisible() {
                    closed_by_itself();
                }
            }
        }
    }

    unsafe impl NSObjectProtocol for Previewer {}

    unsafe impl QLPreviewPanelDataSource for Previewer {
        #[unsafe(method(numberOfPreviewItemsInPreviewPanel:))]
        fn item_count(&self, _panel: Option<&QLPreviewPanel>) -> NSInteger {
            NSInteger::from(self.ivars().url.borrow().is_some())
        }

        #[unsafe(method_id(previewPanel:previewItemAtIndex:))]
        fn item(
            &self,
            _panel: Option<&QLPreviewPanel>,
            _index: NSInteger,
        ) -> Option<Retained<ProtocolObject<dyn QLPreviewItem>>> {
            self.ivars()
                .url
                .borrow()
                .clone()
                .map(ProtocolObject::from_retained)
        }
    }
);

impl Previewer {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Looked::default());
        unsafe { msg_send![super(this), init] }
    }

    fn feed(&self, panel: &QLPreviewPanel) {
        unsafe { panel.setDataSource(Some(ProtocolObject::from_ref(self))) };
    }
}

thread_local! {
    static PREVIEWER: OnceCell<Retained<Previewer>> = const { OnceCell::new() };
    static LOADED_AT: Cell<Option<(Instant, i32)>> = const { Cell::new(None) };
    static CLICKED_ELSEWHERE_AT: Cell<Option<Instant>> = const { Cell::new(None) };
    static WATCHING: Cell<bool> = const { Cell::new(false) };
    static SHOWING: Cell<bool> = const { Cell::new(false) };
    static APP: OnceCell<AppHandle> = const { OnceCell::new() };
}

// The screen hears only of closes it did not ask for, so its own close is not echoed back after a
// newer open.
fn closed_by_itself() {
    if SHOWING.with(|showing| showing.replace(false)) {
        tell("closed");
    }
}

pub fn tell(what: &str) {
    APP.with(|app| {
        if let Some(app) = app.get()
            && let Err(error) = app.emit_to(crate::PANEL, EVENT, what)
        {
            eprintln!("Raiden could not tell its screen the preview {what}: {error}");
        }
    });
}

fn frontmost_app() -> i32 {
    NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map_or(0, |app| app.processIdentifier())
}

fn previewer(mtm: MainThreadMarker) -> Retained<Previewer> {
    PREVIEWER.with(|kept| kept.get_or_init(|| Previewer::new(mtm)).clone())
}

fn shared(mtm: MainThreadMarker) -> Option<Retained<QLPreviewPanel>> {
    if unsafe { QLPreviewPanel::sharedPreviewPanelExists(mtm) } {
        unsafe { QLPreviewPanel::sharedPreviewPanel(mtm) }
    } else {
        None
    }
}

fn open_panel(mtm: MainThreadMarker) -> Option<Retained<QLPreviewPanel>> {
    shared(mtm).filter(|panel| panel.isVisible())
}

fn in_chain(launcher: &NSWindow, previewer: &Previewer) -> bool {
    let wanted: *const NSResponder = previewer.as_super();
    let mut next = unsafe { launcher.nextResponder() };
    for _ in 0..LONGEST_CHAIN {
        match next {
            Some(responder) if std::ptr::eq(&*responder, wanted) => return true,
            Some(responder) => next = unsafe { responder.nextResponder() },
            None => return false,
        }
    }
    true
}

fn adopt(launcher: &NSWindow, previewer: &Previewer) {
    if in_chain(launcher, previewer) {
        return;
    }
    unsafe {
        previewer.setNextResponder(launcher.nextResponder().as_deref());
        launcher.setNextResponder(Some(previewer));
    }
}

fn watch_once(panel: &QLPreviewPanel) {
    if WATCHING.with(|watching| watching.replace(true)) {
        return;
    }
    let clicked = RcBlock::new(|_: NonNull<NSEvent>| {
        CLICKED_ELSEWHERE_AT.with(|at| at.set(Some(Instant::now())));
    });
    let clicks = NSEventMask::LeftMouseDown
        | NSEventMask::RightMouseDown
        | NSEventMask::OtherMouseDown
        | NSEventMask::KeyDown;
    std::mem::forget(NSEvent::addGlobalMonitorForEventsMatchingMask_handler(
        clicks, &clicked,
    ));
    let closing = RcBlock::new(|_: NonNull<NSNotification>| closed_by_itself());
    let observer = unsafe {
        NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
            Some(NSWindowWillCloseNotification),
            Some(panel.as_ref()),
            Some(&NSOperationQueue::mainQueue()),
            &closing,
        )
    };
    std::mem::forget(observer);
}

fn show(launcher: &NSWindow, previewer: &Previewer, mtm: MainThreadMarker) -> bool {
    let Some(panel) = (unsafe { QLPreviewPanel::sharedPreviewPanel(mtm) }) else {
        return false;
    };
    watch_once(&panel);
    adopt(launcher, previewer);
    unsafe { panel.updateController() };
    if unsafe { panel.dataSource() }.is_none() {
        previewer.feed(&panel);
    }
    unsafe { panel.reloadData() };
    panel.setHidesOnDeactivate(false);
    panel.setBecomesKeyOnlyIfNeeded(true);
    panel.setCollectionBehavior(launcher.collectionBehavior());
    panel.orderFront(None);
    panel.setLevel(launcher.level() + 1);
    launcher.makeKeyWindow();
    true
}

pub fn look(app: &AppHandle, path: &str, follow: bool) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let Ok(launcher) = app.get_webview_panel(crate::PANEL) else {
        return false;
    };
    APP.with(|kept| {
        let _ = kept.set(app.clone());
    });
    if !launcher.as_panel().isVisible() {
        return false;
    }
    let open = open_panel(mtm);
    if follow && open.is_none() {
        return false;
    }
    if !follow && let Some(panel) = &open {
        SHOWING.with(|showing| showing.set(false));
        panel.orderOut(None);
        return false;
    }
    let previewer = previewer(mtm);
    *previewer.ivars().url.borrow_mut() = Some(NSURL::fileURLWithPath(&NSString::from_str(path)));
    let shown = match open {
        Some(panel) => {
            if unsafe { panel.dataSource() }.is_none() {
                previewer.feed(&panel);
            }
            unsafe { panel.reloadData() };
            true
        }
        None => show(launcher.as_panel(), &previewer, mtm),
    };
    if shown {
        SHOWING.with(|showing| showing.set(true));
        LOADED_AT.with(|at| at.set(Some((Instant::now(), frontmost_app()))));
    }
    shown
}

pub fn close() -> bool {
    SHOWING.with(|showing| showing.set(false));
    let Some(panel) = MainThreadMarker::new().and_then(open_panel) else {
        return false;
    };
    panel.orderOut(None);
    true
}

// Quick Look draws previews in a system process that takes the keyboard once the content loads,
// which looks to Raiden exactly like the user switching to another app. Raiden stays up through
// that for a few seconds after each new file is shown, but only while no other app has come to the
// front, no click or key press has landed outside Raiden, and no mouse button is held. Key presses
// elsewhere are seen only with Accessibility access, so without it the window is shorter.
pub fn keeps_raiden_up(launcher: &NSWindow) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let preview = open_panel(mtm);
    let holding = launcher.isKeyWindow()
        || preview.as_ref().is_some_and(|panel| panel.isKeyWindow())
        || NSApplication::sharedApplication(mtm).keyWindow().is_some();
    let clicked_elsewhere = CLICKED_ELSEWHERE_AT.with(Cell::get);
    let loading = LOADED_AT.with(Cell::get).is_some_and(|(at, frontmost)| {
        let clicked_since = clicked_elsewhere.is_some_and(|clicked| clicked >= at)
            || NSEvent::pressedMouseButtons() != 0;
        let grace = if crate::access::trusted(false) {
            LOADING_TAKES_FOCUS_FOR
        } else {
            LOADING_TAKES_FOCUS_UNWATCHED_FOR
        };
        just_loaded(
            at.elapsed(),
            grace,
            frontmost,
            frontmost_app(),
            clicked_since,
        )
    });
    stays_up(preview.is_some(), holding || loading)
}

fn just_loaded(
    since: Duration,
    grace: Duration,
    frontmost_then: i32,
    frontmost_now: i32,
    clicked_elsewhere: bool,
) -> bool {
    since < grace && frontmost_then == frontmost_now && !clicked_elsewhere
}

fn stays_up(previewing: bool, raiden_has_the_keyboard_or_just_previewed: bool) -> bool {
    previewing && raiden_has_the_keyboard_or_just_previewed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_just_loaded_preview_keeps_raiden_up_until_the_user_goes_elsewhere() {
        let soon = Duration::from_millis(200);
        let grace = LOADING_TAKES_FOCUS_FOR;
        assert!(just_loaded(soon, grace, 42, 42, false));
        assert!(!just_loaded(soon, grace, 42, 77, false));
        assert!(!just_loaded(soon, grace, 42, 42, true));
        assert!(!just_loaded(grace, grace, 42, 42, false));
        let unwatched = LOADING_TAKES_FOCUS_UNWATCHED_FOR;
        assert!(just_loaded(soon, unwatched, 42, 42, false));
        assert!(!just_loaded(unwatched, unwatched, 42, 42, false));
        assert!(unwatched < grace);
    }

    #[test]
    fn raiden_stays_up_only_while_previewing_and_holding_the_keyboard() {
        assert!(stays_up(true, true));
        assert!(!stays_up(true, false));
        assert!(!stays_up(false, true));
        assert!(!stays_up(false, false));
    }
}
