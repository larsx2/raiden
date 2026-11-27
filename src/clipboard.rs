use std::path::Path;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::sleep;
use std::time::Duration;

use base64::Engine as _;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardTypeString, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSData, NSString, NSURL};
use raiden_core::clipboard::{
    self, Copied, Entry, Files, IMAGE_TYPES, Kind, Shown, Store, THUMB_PIXELS, Verdict,
};
use tauri::{AppHandle, Emitter as _, State};
use tauri_nspanel::ManagerExt as _;

use crate::access;
use crate::front::{self, Front};

const EVERY: Duration = Duration::from_millis(500);
const TRIM_EVERY: Duration = Duration::from_secs(60 * 60);
const FILE_URL: &str = "public.file-url";
const OPEN_EVENT: &str = "open-clipboard";

static WATCHED: OnceLock<Arc<Store>> = OnceLock::new();
static SEEN: AtomicIsize = AtomicIsize::new(0);

pub struct History(pub Arc<Store>);

impl History {
    pub fn new(state: &Path) -> Self {
        Self(Arc::new(Store::new(state)))
    }
}

struct Snapshot {
    types: Vec<String>,
    text: Option<String>,
    files: Vec<String>,
    image: Option<(String, Vec<u8>)>,
    image_bytes: Option<usize>,
}

fn path_of(link: &NSString) -> Option<String> {
    let url = NSURL::URLWithString(link)?;
    let path = url.filePathURL().unwrap_or(url).path()?;
    Some(path.to_string())
}

fn snapshot(board: &NSPasteboard) -> Snapshot {
    let types: Vec<String> = board
        .types()
        .map(|types| types.iter().map(|kind| kind.to_string()).collect())
        .unwrap_or_default();
    if clipboard::is_private(&types) {
        return Snapshot {
            types,
            text: None,
            files: Vec::new(),
            image: None,
            image_bytes: None,
        };
    }
    let file_url = NSString::from_str(FILE_URL);
    let files: Vec<String> = board
        .pasteboardItems()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.stringForType(&file_url))
                .filter_map(|link| path_of(&link))
                .collect()
        })
        .unwrap_or_default();
    let text = unsafe { board.stringForType(NSPasteboardTypeString) }.map(|text| text.to_string());
    let has_text = text.as_deref().is_some_and(|text| !text.trim().is_empty());
    let image = IMAGE_TYPES
        .iter()
        .find(|kind| !has_text && files.is_empty() && types.iter().any(|offered| offered == *kind))
        .and_then(|kind| {
            board
                .dataForType(&NSString::from_str(kind))
                .map(|data| ((*kind).to_owned(), data))
        });
    let image_bytes = image.as_ref().map(|(_, data)| data.length());
    let image = image
        .filter(|(_, data)| data.length() <= clipboard::LARGEST_IMAGE)
        .map(|(kind, data)| (kind, data.to_vec()));
    Snapshot {
        types,
        text,
        files,
        image,
        image_bytes,
    }
}

fn source_app() -> String {
    access::frontmost_pid()
        .and_then(access::app_name)
        .unwrap_or_default()
}

fn record(store: &Store, snap: &Snapshot, app: String, now: u64) -> Option<Entry> {
    let copied = Copied {
        types: &snap.types,
        text: snap.text.as_deref(),
        files: &snap.files,
        image_bytes: snap.image_bytes,
    };
    let Verdict::Keep { kind, text } = clipboard::judge(&copied) else {
        return None;
    };
    let mut entry = Entry::new(kind, text, app, now);
    let mut image = None;
    let thumb = match (kind, &snap.image) {
        (Kind::Image, Some((uti, data))) if entry.text.is_empty() => {
            let bytes = data.clone();
            entry.id = clipboard::id_of(Kind::Image, &bytes);
            entry.image.clone_from(uti);
            let made = thumbs::from_data(&bytes);
            image = Some(bytes);
            made
        }
        (Kind::Image, _) => thumbs::from_file(Path::new(&entry.text)),
        _ => None,
    };
    if let Some(made) = &thumb {
        entry.width = made.width;
        entry.height = made.height;
    }
    let files = Files {
        image: image.as_deref(),
        thumb: thumb.as_ref().map(|made| made.png.as_slice()),
    };
    match store.add(entry.clone(), &files, now) {
        Ok(_) => Some(entry),
        Err(error) => {
            eprintln!("Raiden could not keep a copied item: {error}");
            None
        }
    }
}

fn changed_while_held(board: &NSPasteboard, seen: &AtomicIsize) -> Option<Snapshot> {
    let count = board.changeCount();
    if count == seen.load(Ordering::SeqCst) {
        return None;
    }
    if crate::paste::own(board, count) {
        seen.store(count, Ordering::SeqCst);
        return None;
    }
    let snap = snapshot(board);
    if board.changeCount() != count {
        return None;
    }
    seen.store(count, Ordering::SeqCst);
    Some(snap)
}

pub fn check(store: &Store, board: &NSPasteboard, seen: &AtomicIsize, now: u64) -> Option<Entry> {
    let snap = {
        let _pasting = crate::paste::pasting();
        changed_while_held(board, seen)?
    };
    record(store, &snap, source_app(), now)
}

pub fn before_raiden_writes(board: &NSPasteboard) -> Option<std::thread::JoinHandle<()>> {
    let store = Arc::clone(WATCHED.get()?);
    let snap = changed_while_held(board, &SEEN)?;
    let app = source_app();
    let now = crate::picks::now();
    Some(std::thread::spawn(move || {
        record(&store, &snap, app, now);
    }))
}

pub fn watch(history: &History) {
    let store = Arc::clone(&history.0);
    std::thread::spawn(move || {
        if let Err(error) = store.entries(crate::picks::now()) {
            eprintln!("Raiden could not read its clipboard history: {error}");
        }
        let board = NSPasteboard::generalPasteboard();
        SEEN.store(board.changeCount(), Ordering::SeqCst);
        let _ = WATCHED.set(Arc::clone(&store));
        let mut since_trim = Duration::ZERO;
        loop {
            sleep(EVERY);
            objc2::rc::autoreleasepool(|_| {
                check(&store, &board, &SEEN, crate::picks::now());
            });
            since_trim += EVERY;
            if since_trim >= TRIM_EVERY {
                since_trim = Duration::ZERO;
                let _ = store.entries(crate::picks::now());
            }
        }
    });
}

pub fn open(app: &AppHandle) {
    crate::on_main_thread(app, |app| {
        let shown = app
            .get_webview_panel(crate::PANEL)
            .is_ok_and(|panel| panel.is_visible());
        if let Err(error) = app.emit_to(crate::PANEL, OPEN_EVENT, shown) {
            eprintln!("Raiden could not open Clipboard history: {error}");
        }
        if !shown {
            crate::show_panel(app);
        }
    });
}

fn file_item(path: &str) -> Retained<NSPasteboardItem> {
    let item = NSPasteboardItem::new();
    let link = NSURL::fileURLWithPath(&NSString::from_str(path)).absoluteString();
    if let Some(link) = link {
        item.setString_forType(&link, &NSString::from_str(FILE_URL));
    }
    unsafe { item.setString_forType(&NSString::from_str(path), NSPasteboardTypeString) };
    item
}

fn items_of(store: &Store, entry: &Entry) -> Result<Vec<Retained<NSPasteboardItem>>, String> {
    let gone = || "That image is no longer in Clipboard history.".to_owned();
    match entry.kind {
        Kind::Text | Kind::Link => {
            let item = NSPasteboardItem::new();
            unsafe {
                item.setString_forType(&NSString::from_str(&entry.text), NSPasteboardTypeString)
            };
            Ok(vec![item])
        }
        Kind::Image if entry.text.is_empty() => {
            let bytes = std::fs::read(store.image_file(&entry.id)).map_err(|_| gone())?;
            let item = NSPasteboardItem::new();
            item.setData_forType(
                &NSData::with_bytes(&bytes),
                &NSString::from_str(&entry.image),
            );
            Ok(vec![item])
        }
        Kind::File | Kind::Image => Ok(entry.text.lines().map(file_item).collect()),
    }
}

fn write(board: &NSPasteboard, items: Vec<Retained<NSPasteboardItem>>) -> bool {
    board.clearContents();
    let objects: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = items
        .into_iter()
        .map(ProtocolObject::from_retained)
        .collect();
    board.writeObjects(&NSArray::from_retained_slice(&objects))
}

fn home() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
}

fn find(history: &History, id: &str) -> Result<Entry, String> {
    history
        .0
        .find(id, crate::picks::now())
        .map_err(|error| format!("Raiden could not read its clipboard history: {error}"))?
        .ok_or_else(|| "That item is no longer in Clipboard history.".to_owned())
}

#[tauri::command]
pub async fn clipboard_items(
    history: State<'_, History>,
    usage: State<'_, crate::picks::Usage>,
    query: Option<String>,
    row: Option<usize>,
) -> Result<Vec<Shown>, String> {
    if let (Some(query), Some(row)) = (query, row) {
        crate::picks::note(&usage.0, clipboard::VIEW_ID, query, row);
    }
    history
        .0
        .entries(crate::picks::now())
        .map(|entries| clipboard::shown(&entries, &home()))
        .map_err(|error| format!("Raiden could not read its clipboard history: {error}"))
}

#[tauri::command]
pub async fn clipboard_thumb(
    history: State<'_, History>,
    id: String,
) -> Result<Option<String>, ()> {
    if !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(None);
    }
    Ok(std::fs::read(history.0.thumb_file(&id)).ok().map(|png| {
        let encoded = base64::engine::general_purpose::STANDARD.encode(png);
        format!("data:image/png;base64,{encoded}")
    }))
}

#[tauri::command]
pub async fn clipboard_paste(
    app: AppHandle,
    front: State<'_, Front>,
    history: State<'_, History>,
    id: String,
) -> Result<(), String> {
    if !access::trusted(false) {
        return Err(access::NEEDS_ACCESS.to_owned());
    }
    let entry = find(&history, &id)?;
    let store = Arc::clone(&history.0);
    let front = front.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let pid = front::came_from(&front)?;
        let items = items_of(&store, &entry)?;
        front::paste_into_with(
            &app,
            pid,
            || Ok(()),
            || crate::paste::put_with(|board| write(board, items)),
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn clipboard_copy(history: State<'_, History>, id: String) -> Result<(), String> {
    let entry = find(&history, &id)?;
    let store = Arc::clone(&history.0);
    tauri::async_runtime::spawn_blocking(move || {
        let items = items_of(&store, &entry)?;
        if let Some(recording) = crate::paste::copy_with(|board| write(board, items))? {
            let _ = recording.join();
        }
        store
            .raise(&entry.id, crate::picks::now())
            .map(|_| ())
            .map_err(|error| format!("Raiden could not move it to the top: {error}"))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn clipboard_delete(history: State<'_, History>, id: String) -> Result<(), String> {
    history
        .0
        .remove(&id, crate::picks::now())
        .map(|_| ())
        .map_err(|error| format!("Raiden could not delete it from Clipboard history: {error}"))
}

mod thumbs {
    use std::ffi::c_void;
    use std::path::Path;

    use core_foundation::base::{CFType, CFTypeRef, TCFType as _};
    use core_foundation::data::{CFData, CFDataRef};
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::number::CFNumber;
    use core_foundation::string::{CFString, CFStringRef};
    use core_foundation::url::{CFURL, CFURLRef};

    use super::THUMB_PIXELS;

    #[link(name = "ImageIO", kind = "framework")]
    unsafe extern "C" {
        fn CGImageSourceCreateWithData(data: CFDataRef, options: CFDictionaryRef) -> CFTypeRef;
        fn CGImageSourceCreateWithURL(url: CFURLRef, options: CFDictionaryRef) -> CFTypeRef;
        fn CGImageSourceCopyPropertiesAtIndex(
            source: CFTypeRef,
            index: usize,
            options: CFDictionaryRef,
        ) -> CFDictionaryRef;
        fn CGImageSourceCreateThumbnailAtIndex(
            source: CFTypeRef,
            index: usize,
            options: CFDictionaryRef,
        ) -> CFTypeRef;
        fn CGImageDestinationCreateWithData(
            data: *mut c_void,
            kind: CFStringRef,
            count: usize,
            options: CFDictionaryRef,
        ) -> CFTypeRef;
        fn CGImageDestinationAddImage(
            destination: CFTypeRef,
            image: CFTypeRef,
            properties: CFDictionaryRef,
        );
        fn CGImageDestinationFinalize(destination: CFTypeRef) -> bool;
        static kCGImageSourceThumbnailMaxPixelSize: CFStringRef;
        static kCGImageSourceCreateThumbnailFromImageAlways: CFStringRef;
        static kCGImageSourceCreateThumbnailWithTransform: CFStringRef;
        static kCGImagePropertyPixelWidth: CFStringRef;
        static kCGImagePropertyPixelHeight: CFStringRef;
        static kCGImagePropertyOrientation: CFStringRef;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFDataCreateMutable(allocator: CFTypeRef, capacity: isize) -> *mut c_void;
    }

    pub struct Thumb {
        pub png: Vec<u8>,
        pub width: u32,
        pub height: u32,
    }

    fn owned(made: CFTypeRef) -> Option<CFType> {
        (!made.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(made) })
    }

    fn key(name: CFStringRef) -> CFString {
        unsafe { CFString::wrap_under_get_rule(name) }
    }

    fn pixels(properties: &CFDictionary<CFString, CFType>, name: CFStringRef) -> u32 {
        properties
            .find(key(name))
            .and_then(|value| value.downcast::<CFNumber>())
            .and_then(|number| number.to_i64())
            .and_then(|number| u32::try_from(number).ok())
            .unwrap_or(0)
    }

    fn from_source(source: &CFType) -> Option<Thumb> {
        let properties = unsafe {
            CGImageSourceCopyPropertiesAtIndex(source.as_CFTypeRef(), 0, std::ptr::null())
        };
        if properties.is_null() {
            return None;
        }
        let properties: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_create_rule(properties) };
        let wide = pixels(&properties, unsafe { kCGImagePropertyPixelWidth });
        let high = pixels(&properties, unsafe { kCGImagePropertyPixelHeight });
        let turned = (5..=8).contains(&pixels(&properties, unsafe { kCGImagePropertyOrientation }));
        let (width, height) = if turned { (high, wide) } else { (wide, high) };
        let yes = core_foundation::boolean::CFBoolean::true_value().as_CFType();
        let options = CFDictionary::from_CFType_pairs(&[
            (
                key(unsafe { kCGImageSourceThumbnailMaxPixelSize }),
                CFNumber::from(i64::from(THUMB_PIXELS)).as_CFType(),
            ),
            (
                key(unsafe { kCGImageSourceCreateThumbnailFromImageAlways }),
                yes.clone(),
            ),
            (
                key(unsafe { kCGImageSourceCreateThumbnailWithTransform }),
                yes,
            ),
        ]);
        let image = owned(unsafe {
            CGImageSourceCreateThumbnailAtIndex(
                source.as_CFTypeRef(),
                0,
                options.as_concrete_TypeRef(),
            )
        })?;
        Some(Thumb {
            png: encode(&image)?,
            width,
            height,
        })
    }

    pub fn encode(image: &CFType) -> Option<Vec<u8>> {
        let buffer = unsafe { CFDataCreateMutable(std::ptr::null(), 0) };
        if buffer.is_null() {
            return None;
        }
        let data = unsafe { CFData::wrap_under_create_rule(buffer.cast_const().cast()) };
        let png = CFString::from_static_string("public.png");
        let destination = owned(unsafe {
            CGImageDestinationCreateWithData(buffer, png.as_concrete_TypeRef(), 1, std::ptr::null())
        })?;
        unsafe {
            CGImageDestinationAddImage(
                destination.as_CFTypeRef(),
                image.as_CFTypeRef(),
                std::ptr::null(),
            );
        }
        if !unsafe { CGImageDestinationFinalize(destination.as_CFTypeRef()) } {
            return None;
        }
        Some(data.bytes().to_vec())
    }

    pub fn from_data(bytes: &[u8]) -> Option<Thumb> {
        let data = CFData::from_buffer(bytes);
        let source = owned(unsafe {
            CGImageSourceCreateWithData(data.as_concrete_TypeRef(), std::ptr::null())
        })?;
        from_source(&source)
    }

    pub fn from_file(path: &Path) -> Option<Thumb> {
        let url = CFURL::from_path(path, false)?;
        let source = owned(unsafe {
            CGImageSourceCreateWithURL(url.as_concrete_TypeRef(), std::ptr::null())
        })?;
        from_source(&source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch {
        board: Retained<NSPasteboard>,
        state: std::path::PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let state = std::env::temp_dir().join(format!(
                "raiden-clipboard-shell-{name}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&state);
            let unique = format!("raiden.tests.{name}.{}", std::process::id());
            Self {
                board: NSPasteboard::pasteboardWithName(&NSString::from_str(&unique)),
                state,
            }
        }

        fn copy(&self, text: &str, marks: &[&str]) {
            let item = NSPasteboardItem::new();
            unsafe { item.setString_forType(&NSString::from_str(text), NSPasteboardTypeString) };
            for mark in marks {
                item.setString_forType(&NSString::from_str(""), &NSString::from_str(mark));
            }
            assert!(write(&self.board, vec![item]));
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let () = unsafe { objc2::msg_send![&*self.board, releaseGlobally] };
            let _ = std::fs::remove_dir_all(&self.state);
        }
    }

    fn drawn_png(width: usize, height: usize) -> Vec<u8> {
        use core_foundation::base::TCFType as _;
        use core_graphics::base::kCGImageAlphaPremultipliedLast;
        use core_graphics::color_space::CGColorSpace;
        use core_graphics::context::CGContext;
        use core_graphics::geometry::{CGPoint, CGRect, CGSize};
        use foreign_types::ForeignType as _;

        let space = CGColorSpace::create_device_rgb();
        let context = CGContext::create_bitmap_context(
            None,
            width,
            height,
            8,
            0,
            &space,
            kCGImageAlphaPremultipliedLast,
        );
        context.set_rgb_fill_color(0.9, 0.3, 0.2, 1.0);
        context.fill_rect(CGRect::new(
            &CGPoint::new(0.0, 0.0),
            &CGSize::new(width as f64, height as f64),
        ));
        let image = context.create_image().expect("an image");
        let image = unsafe {
            core_foundation::base::CFType::wrap_under_get_rule(image.as_ptr().cast_const().cast())
        };
        thumbs::encode(&image).expect("a png")
    }

    fn texts(store: &Store, now: u64) -> Vec<String> {
        store
            .entries(now)
            .expect("read")
            .into_iter()
            .map(|entry| entry.text)
            .collect()
    }

    #[test]
    fn copying_three_plain_items_and_one_concealed_keeps_the_three() {
        let scratch = Scratch::new("concealed");
        let store = Store::new(&scratch.state);
        let seen = AtomicIsize::new(scratch.board.changeCount());
        let now = 1_800_000_000;
        for (at, (text, marks)) in [
            ("git worktree add", &[][..]),
            (
                "correct horse battery",
                &["org.nspasteboard.ConcealedType"][..],
            ),
            ("https://gpui.rs/", &[][..]),
            ("just land", &[][..]),
        ]
        .into_iter()
        .enumerate()
        {
            scratch.copy(text, marks);
            check(&store, &scratch.board, &seen, now + at as u64);
            assert_eq!(check(&store, &scratch.board, &seen, now), None);
        }
        assert_eq!(
            texts(&store, now + 10),
            ["just land", "https://gpui.rs/", "git worktree add"]
        );
        assert_eq!(store.entries(now + 10).expect("read")[1].kind, Kind::Link);
        assert!(texts(&store, now + 8 * 24 * 60 * 60).is_empty());
    }

    #[test]
    fn raidens_own_writes_are_not_kept() {
        let scratch = Scratch::new("own");
        let store = Store::new(&scratch.state);
        let seen = AtomicIsize::new(scratch.board.changeCount());
        scratch.copy("pasted by Raiden", &[]);
        crate::paste::mark_own(&scratch.board);
        assert_eq!(check(&store, &scratch.board, &seen, 1), None);
        scratch.copy("copied by you", &[]);
        assert!(check(&store, &scratch.board, &seen, 2).is_some());
        assert_eq!(texts(&store, 2), ["copied by you"]);
    }

    #[test]
    fn an_image_keeps_its_data_and_a_small_thumbnail() {
        let scratch = Scratch::new("image");
        let store = Store::new(&scratch.state);
        let png = drawn_png(400, 200);
        let item = NSPasteboardItem::new();
        item.setData_forType(&NSData::with_bytes(&png), &NSString::from_str("public.png"));
        assert!(write(&scratch.board, vec![item]));
        let seen = AtomicIsize::new(0);
        let kept = check(&store, &scratch.board, &seen, 1).expect("kept");
        assert_eq!(
            (kept.kind, kept.width, kept.height),
            (Kind::Image, 400, 200)
        );
        assert_eq!(
            std::fs::read(store.image_file(&kept.id)).expect("data"),
            png
        );
        let thumb = std::fs::read(store.thumb_file(&kept.id)).expect("thumb");
        let small = thumbs::from_data(&thumb).expect("a png");
        assert_eq!(
            (small.width, small.height),
            (THUMB_PIXELS, THUMB_PIXELS / 2)
        );
        let back = items_of(&store, &store.entries(1).expect("read")[0]).expect("items");
        assert!(write(&scratch.board, back));
        let pasted = scratch
            .board
            .dataForType(&NSString::from_str("public.png"))
            .expect("png");
        assert_eq!(pasted.to_vec(), png);
    }
}
