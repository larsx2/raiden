use std::ffi::c_void;

use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::{CFString, CFStringRef};

type AXUIElementRef = *const c_void;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateApplication(pid: libc::pid_t) -> AXUIElementRef;
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, seconds: f32) -> i32;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementIsAttributeSettable(
        element: AXUIElementRef,
        attribute: CFStringRef,
        settable: *mut u8,
    ) -> i32;
    fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
}

pub const NEEDS_ACCESS: &str = "Raiden needs Accessibility access: System Settings, Privacy & Security, Accessibility, then turn on Raiden.";

pub fn trusted(prompt: bool) -> bool {
    let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
    let value = if prompt {
        CFBoolean::true_value()
    } else {
        CFBoolean::false_value()
    };
    let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), value.as_CFType())]);
    unsafe { AXIsProcessTrustedWithOptions(options.as_CFTypeRef()) }
}

fn attribute(element: &CFType, name: &str) -> Option<CFType> {
    let name = CFString::new(name);
    let mut value: CFTypeRef = std::ptr::null();
    let error = unsafe {
        AXUIElementCopyAttributeValue(
            element.as_CFTypeRef(),
            name.as_concrete_TypeRef(),
            &raw mut value,
        )
    };
    (error == 0 && !value.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(value) })
}

fn text(element: &CFType, name: &str) -> Option<String> {
    attribute(element, name)?
        .downcast::<CFString>()
        .map(|text| text.to_string())
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Seen {
    #[serde(skip)]
    pub pid: libc::pid_t,
    pub app: String,
    pub path: Option<String>,
    pub window: Option<String>,
    pub selection: Option<String>,
}

const SLOWEST_ANSWER_SECONDS: f32 = 0.5;

fn application(pid: libc::pid_t) -> CFType {
    let element = unsafe { CFType::wrap_under_create_rule(AXUIElementCreateApplication(pid)) };
    unsafe { AXUIElementSetMessagingTimeout(element.as_CFTypeRef(), SLOWEST_ANSWER_SECONDS) };
    element
}

fn filled(text: Option<String>) -> Option<String> {
    text.filter(|text| !text.trim().is_empty())
}

pub fn seen(pid: libc::pid_t) -> Option<Seen> {
    if pid == unsafe { libc::getpid() } {
        return None;
    }
    let running =
        objc2_app_kit::NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?;
    let app = running.localizedName()?.to_string();
    let path = running
        .bundleURL()
        .and_then(|url| url.path())
        .map(|path| path.to_string());
    let element = application(pid);
    let window = attribute(&element, "AXFocusedWindow").and_then(|window| text(&window, "AXTitle"));
    let selection = attribute(&element, "AXFocusedUIElement")
        .and_then(|focused| text(&focused, "AXSelectedText"));
    Some(Seen {
        pid,
        app,
        path,
        window: filled(window),
        selection: filled(selection),
    })
}

pub fn app_name(pid: libc::pid_t) -> Option<String> {
    objc2_app_kit::NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?
        .localizedName()
        .map(|name| name.to_string())
}

pub fn frontmost_pid() -> Option<libc::pid_t> {
    objc2_app_kit::NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|app| app.processIdentifier())
}

fn settable(element: &CFType, name: &str) -> Option<bool> {
    let name = CFString::new(name);
    let mut answer: u8 = 0;
    let error = unsafe {
        AXUIElementIsAttributeSettable(
            element.as_CFTypeRef(),
            name.as_concrete_TypeRef(),
            &raw mut answer,
        )
    };
    (error == 0).then_some(answer != 0)
}

const EDITING_ATTRIBUTES: [&str; 2] = ["AXSelectedText", "AXValue"];
const TEXT_ROLES: [&str; 5] = [
    "AXTextArea",
    "AXTextField",
    "AXComboBox",
    "AXSearchField",
    "AXWebArea",
];

pub fn editable_focus(pid: libc::pid_t) -> Option<bool> {
    let focused = attribute(&application(pid), "AXFocusedUIElement")?;
    let role = text(&focused, "AXRole")?;
    if !TEXT_ROLES.contains(&role.as_str()) {
        return Some(false);
    }
    let answers: Vec<Option<bool>> = EDITING_ATTRIBUTES
        .iter()
        .map(|name| settable(&focused, name))
        .collect();
    if answers.contains(&Some(true)) {
        Some(true)
    } else if answers.contains(&Some(false)) {
        Some(false)
    } else {
        None
    }
}

pub fn bundle_id(pid: libc::pid_t) -> Option<String> {
    objc2_app_kit::NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?
        .bundleIdentifier()
        .map(|id| id.to_string())
}

pub fn focused_text(pid: libc::pid_t) -> Option<String> {
    let focused = attribute(&application(pid), "AXFocusedUIElement")?;
    text(&focused, "AXValue")
}
