use std::path::{Path, PathBuf};
use std::process::Command;

use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString, NSWorkspace};
use objc2_foundation::{NSArray, NSString, NSURL};
use tauri::{AppHandle, State};

use crate::files::Files;

fn copy(text: &str) -> bool {
    let board = NSPasteboard::generalPasteboard();
    board.clearContents();
    unsafe { board.setString_forType(&NSString::from_str(text), NSPasteboardTypeString) }
}

#[tauri::command]
pub fn copy_path(path: String) -> Result<(), String> {
    if copy(&path) {
        Ok(())
    } else {
        Err(format!("macOS did not take {path} onto the clipboard."))
    }
}

#[tauri::command]
pub fn copy_text(text: String) -> Result<(), String> {
    if copy(&crate::paste::answer_text(&text)) {
        Ok(())
    } else {
        Err("macOS did not take the answer onto the clipboard.".to_owned())
    }
}

pub fn web_link(url: &str) -> Result<&str, String> {
    let link = url.trim();
    let web = link.split_once("://").is_some_and(|(scheme, rest)| {
        matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") && !rest.is_empty()
    });
    if web && !link.chars().any(|c| c.is_whitespace() || c.is_control()) {
        Ok(link)
    } else {
        Err(format!(
            "Raiden opens only web links, and {url} is not one."
        ))
    }
}

fn is_web(url: &NSURL) -> bool {
    url.scheme().is_some_and(|scheme| {
        matches!(
            scheme.to_string().to_ascii_lowercase().as_str(),
            "http" | "https"
        )
    })
}

#[tauri::command]
pub async fn open_link(app: AppHandle, url: String) -> Result<(), String> {
    let link = web_link(&url)?.to_owned();
    crate::hide_panel(&app);
    let (sender, receiver) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let opened = NSURL::URLWithString(&NSString::from_str(&link))
            .filter(|parsed| is_web(parsed))
            .is_some_and(|parsed| NSWorkspace::sharedWorkspace().openURL(&parsed));
        let _ = sender.send(opened);
    })
    .map_err(|error| error.to_string())?;
    if receiver.recv_timeout(std::time::Duration::from_secs(5)) == Ok(true) {
        Ok(())
    } else {
        Err(format!("macOS did not open {url}."))
    }
}

fn is_app_folder(path: &str) -> bool {
    let target = Path::new(path);
    target.is_absolute() && path.ends_with(".app") && target.is_dir()
}

fn is_found_file(path: &str) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    Path::new(path).is_absolute()
        && raiden_core::files::kept(path, &home)
        && std::fs::symlink_metadata(path).is_ok()
}

pub fn was_found(files: &Files, path: &str) -> bool {
    files.showed(path) && is_found_file(path)
}

#[tauri::command]
pub async fn open_file(
    app: AppHandle,
    files: State<'_, Files>,
    path: String,
) -> Result<(), String> {
    if !was_found(&files, &path) {
        return Err(format!(
            "Raiden opens only files that file search found, and {path} is not one."
        ));
    }
    crate::hide_panel(&app);
    let target = path.clone();
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&target));
        let _ = sender.send(NSWorkspace::sharedWorkspace().openURL(&url));
    })
    .map_err(|error| error.to_string())?;
    match tokio::time::timeout(std::time::Duration::from_secs(5), receiver).await {
        Ok(Ok(true)) => Ok(()),
        _ => Err(format!("macOS did not open {path}.")),
    }
}

fn shown_by_raiden(path: &str, showed: impl FnOnce(&str) -> bool) -> bool {
    is_app_folder(path) || (showed(path) && is_found_file(path))
}

#[tauri::command]
pub fn show_in_finder(app: AppHandle, files: State<'_, Files>, path: String) -> Result<(), String> {
    if !shown_by_raiden(&path, |shown| files.showed(shown)) {
        return Err(format!(
            "Raiden shows only apps and the files it finds in Finder, and {path} is neither."
        ));
    }
    crate::hide_panel(&app);
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path));
    NSWorkspace::sharedWorkspace()
        .activateFileViewerSelectingURLs(&NSArray::from_retained_slice(&[url]));
    Ok(())
}

async fn on_main<T: Send + 'static>(
    app: &AppHandle,
    work: impl FnOnce(&AppHandle) -> T + Send + 'static,
) -> Result<T, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = sender.send(work(&handle));
    })
    .map_err(|error| error.to_string())?;
    receiver
        .await
        .map_err(|_| "Raiden's window did not answer.".to_owned())
}

#[tauri::command]
pub async fn quick_look(
    app: AppHandle,
    files: State<'_, Files>,
    path: String,
    follow: bool,
) -> Result<bool, String> {
    if !shown_by_raiden(&path, |shown| files.showed(shown)) {
        return Err(format!(
            "Raiden previews only apps and the files it finds, and {path} is neither."
        ));
    }
    on_main(&app, move |app| crate::quick_look::look(app, &path, follow)).await
}

#[tauri::command]
pub async fn close_quick_look(app: AppHandle) -> Result<bool, String> {
    on_main(&app, |_| crate::quick_look::close()).await
}

fn editor_for(file: &Path) -> Option<PathBuf> {
    let url = NSURL::fileURLWithPath(&NSString::from_str(file.to_str()?));
    let app = NSWorkspace::sharedWorkspace().URLForApplicationToOpenURL(&url)?;
    Some(PathBuf::from(app.path()?.to_string()))
}

pub fn open_in_editor(file: &Path) -> Result<(), String> {
    let mut open = Command::new("/usr/bin/open");
    match editor_for(file) {
        Some(editor) => open.arg("-a").arg(editor),
        None => open.arg("-e"),
    };
    let opened = open
        .arg(file)
        .output()
        .map_err(|error| format!("Raiden could not run open: {error}"))?;
    if opened.status.success() {
        return Ok(());
    }
    Err(format!(
        "macOS did not open {}: {}",
        file.display(),
        String::from_utf8_lossy(&opened.stderr).trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::{is_app_folder, is_found_file, shown_by_raiden, web_link};

    #[test]
    fn opens_only_web_links() {
        assert_eq!(
            web_link(" https://a.example/x_(y) "),
            Ok("https://a.example/x_(y)")
        );
        assert_eq!(web_link("HTTP://a.example"), Ok("HTTP://a.example"));
        for refused in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,hi",
            "x-apple.systempreferences:com.apple.Displays-Settings.extension",
            "ftp://a.example",
            "/Applications/Safari.app",
            "https://",
            "https://a.example/\nfile:///x",
            "https://a.example/ x",
        ] {
            assert!(web_link(refused).is_err(), "{refused} should be refused");
        }
    }

    #[test]
    fn shows_only_absolute_app_folders() {
        let folder = std::env::temp_dir().join(format!("raiden-{}.app", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.with_extension("txt");
        std::fs::write(&file, "").unwrap();
        let shown = is_app_folder(folder.to_str().unwrap());
        let plain_file = is_app_folder(file.to_str().unwrap());
        std::fs::remove_dir(&folder).unwrap();
        std::fs::remove_file(&file).unwrap();
        assert!(shown);
        assert!(!plain_file);
        assert!(!is_app_folder("Safari.app"));
        assert!(!is_app_folder("/Applications"));
        assert!(!is_app_folder("/nowhere/Missing.app"));
    }

    #[test]
    fn shows_and_opens_only_files_that_search_would_find() {
        let home = std::env::var("HOME").unwrap();
        let folder =
            std::path::Path::new(&home).join(format!("raiden-actions-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("found.txt");
        std::fs::write(&file, "").unwrap();
        let found = is_found_file(file.to_str().unwrap());
        let missing = is_found_file(folder.join("missing.txt").to_str().unwrap());
        let climbing = is_found_file(&format!(
            "{}/../{}/found.txt",
            folder.display(),
            folder.file_name().unwrap().to_str().unwrap()
        ));
        std::fs::remove_dir_all(&folder).unwrap();
        assert!(found);
        assert!(!missing);
        assert!(!climbing);
        assert!(!is_found_file(&format!("{home}/Library/Preferences")));
        assert!(!is_found_file("/etc/hosts"));
        assert!(!is_found_file("found.txt"));
    }

    #[test]
    fn previews_and_reveals_only_apps_and_files_search_showed() {
        let home = std::env::var("HOME").unwrap();
        let folder =
            std::path::Path::new(&home).join(format!("raiden-shown-{}", std::process::id()));
        let app = folder.join("Shown.app");
        std::fs::create_dir_all(&app).unwrap();
        let file = folder.join("shown.txt");
        std::fs::write(&file, "").unwrap();
        let app = app.to_str().unwrap();
        let file = file.to_str().unwrap();
        let app_unshown = shown_by_raiden(app, |_| false);
        let file_unshown = shown_by_raiden(file, |_| false);
        let file_shown = shown_by_raiden(file, |shown| shown == file);
        let other_shown = shown_by_raiden(file, |shown| shown != file);
        std::fs::remove_dir_all(&folder).unwrap();
        assert!(app_unshown);
        assert!(!file_unshown);
        assert!(file_shown);
        assert!(!other_shown);
        assert!(!shown_by_raiden("/etc/hosts", |_| true));
        assert!(!shown_by_raiden(
            &format!("{home}/Library/Preferences"),
            |_| true
        ));
        assert!(!shown_by_raiden("shown.txt", |_| true));
    }
}
