use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use base64::Engine as _;
use objc2::AnyThread as _;
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSWorkspace};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

const SIZE: f64 = 64.0;

static CACHE: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn render(path: &str) -> Option<String> {
    objc2::rc::autoreleasepool(|_| render_in_pool(path))
}

fn render_in_pool(path: &str) -> Option<String> {
    let image = NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(path));
    let mut rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(SIZE, SIZE));
    let picture = unsafe { image.CGImageForProposedRect_context_hints(&raw mut rect, None, None) }?;
    let bitmap = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &picture);
    let png = unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(png.to_vec());
    Some(format!("data:image/png;base64,{encoded}"))
}

fn cached(path: &str) -> Option<String> {
    CACHE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()?
        .get(path)
        .cloned()
}

pub fn data_url(path: &str) -> Option<String> {
    let key = raiden_core::files::icon_key(path, std::path::Path::new(path).is_file());
    if let Some(url) = cached(&key) {
        return Some(url);
    }
    let url = render(path)?;
    CACHE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get_or_insert_with(HashMap::new)
        .insert(key, url.clone());
    Some(url)
}
