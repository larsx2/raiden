use std::path::Path;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::FOLDER;

const FILE: &str = "avatar.json";
pub const MAX_BYTES: usize = 512 * 1024;
pub const REFRESH_AFTER: u64 = 7 * 24 * 60 * 60;
pub const DOWNLOAD_SECONDS: u64 = 10;
pub const USER: [&str; 4] = ["api", "--hostname", "github.com", "user"];
const HOST: &str = "avatars.githubusercontent.com";
const SIZE: &str = "s=96";
const OK: &str = "200";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Avatar {
    pub login: String,
    pub fetched_at: u64,
    pub image: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub login: String,
    pub link: String,
}

pub fn account(stdout: &str) -> Option<Account> {
    let user: Value = serde_json::from_str(stdout).ok()?;
    let login = user.get("login")?.as_str()?.to_owned();
    let link = sized(user.get("avatar_url")?.as_str()?)?;
    Some(Account { login, link })
}

pub fn allowed(link: &str) -> bool {
    let Some(rest) = link.strip_prefix("https://") else {
        return false;
    };
    if !link
        .chars()
        .all(|letter| letter.is_ascii_graphic() && letter != '\\')
        || link.contains('#')
    {
        return false;
    }
    let host = rest.split(['/', '?']).next().unwrap_or_default();
    host.eq_ignore_ascii_case(HOST)
}

fn sized(link: &str) -> Option<String> {
    if !allowed(link) {
        return None;
    }
    let joiner = if link.contains('?') { '&' } else { '?' };
    Some(format!("{link}{joiner}{SIZE}"))
}

pub fn download_arguments(link: &str) -> Vec<String> {
    let limit = DOWNLOAD_SECONDS.to_string();
    let most = MAX_BYTES.to_string();
    [
        "--disable",
        "--silent",
        "--globoff",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--max-redirs",
        "0",
        "--connect-timeout",
        "5",
        "--max-time",
        &limit,
        "--max-filesize",
        &most,
        "--user-agent",
        "Raiden",
        "--output",
        "-",
        "--write-out",
        "%{stderr}%{http_code}",
        "--url",
        link,
    ]
    .map(str::to_owned)
    .to_vec()
}

pub fn image(status: &str, body: &[u8]) -> Option<String> {
    if status.trim() != OK || body.len() > MAX_BYTES {
        return None;
    }
    data_url(body)
}

fn data_url(body: &[u8]) -> Option<String> {
    let kind = if body.starts_with(b"\x89PNG\r\n\x1a\n") {
        "png"
    } else if body.starts_with(b"\xff\xd8\xff") {
        "jpeg"
    } else if body.starts_with(b"GIF87a") || body.starts_with(b"GIF89a") {
        "gif"
    } else if body.len() >= 12 && body.starts_with(b"RIFF") && &body[8..12] == b"WEBP" {
        "webp"
    } else {
        return None;
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(body);
    Some(format!("data:image/{kind};base64,{encoded}"))
}

pub fn due(saved: Option<&Avatar>, login: &str, now: u64) -> bool {
    saved.is_none_or(|saved| {
        !saved.login.eq_ignore_ascii_case(login)
            || now.saturating_sub(saved.fetched_at) >= REFRESH_AFTER
    })
}

pub fn shown(saved: Option<&Avatar>, login: &str) -> Option<String> {
    saved
        .filter(|saved| saved.login.eq_ignore_ascii_case(login))
        .map(|saved| saved.image.clone())
}

pub fn read(folder: &Path) -> Option<Avatar> {
    let text = std::fs::read_to_string(folder.join(FOLDER).join(FILE)).ok()?;
    let saved: Avatar = serde_json::from_str(&text).ok()?;
    saved.image.starts_with("data:image/").then_some(saved)
}

#[cfg(unix)]
pub fn write(folder: &Path, avatar: &Avatar) -> std::io::Result<()> {
    let text = serde_json::to_string(avatar).map_err(std::io::Error::other)?;
    super::write_private(folder, FILE, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
    const LINK: &str = "https://avatars.githubusercontent.com/u/4242?v=4";

    #[test]
    fn only_https_links_on_the_avatar_host_are_taken() {
        assert!(allowed(LINK));
        assert!(allowed("https://Avatars.GitHubUserContent.com/u/4242"));
        assert!(!allowed("http://avatars.githubusercontent.com/u/4242"));
        assert!(!allowed(
            "https://avatars.githubusercontent.com.example.net/u/4242"
        ));
        assert!(!allowed(
            "https://example.net/avatars.githubusercontent.com/u/4242"
        ));
        assert!(!allowed(
            "https://example.net/?avatars.githubusercontent.com"
        ));
        assert!(!allowed(
            "https://larsx2@avatars.githubusercontent.com/u/4242"
        ));
        assert!(!allowed(
            "https://avatars.githubusercontent.com:8443/u/4242"
        ));
        assert!(!allowed(
            "https://avatars.githubusercontent.com\\@example.net/"
        ));
        assert!(!allowed(
            "https://avatars.githubusercontent.com/u/4242 -o /tmp/x"
        ));
        assert!(!allowed("https://avatars.githubusercontent.com/u/4242#top"));
        assert!(!allowed("file:///etc/passwd"));
        assert!(!allowed(""));
    }

    #[test]
    fn the_link_asks_for_a_96_pixel_image() {
        assert_eq!(
            sized(LINK).as_deref(),
            Some("https://avatars.githubusercontent.com/u/4242?v=4&s=96")
        );
        assert_eq!(
            sized("https://avatars.githubusercontent.com/u/4242").as_deref(),
            Some("https://avatars.githubusercontent.com/u/4242?s=96")
        );
        assert_eq!(sized("http://avatars.githubusercontent.com/u/4242"), None);
    }

    #[test]
    fn the_account_comes_from_gh_api_user() {
        let user = format!(r#"{{"login":"larsx2","id":4242,"avatar_url":"{LINK}"}}"#);
        assert_eq!(
            account(&user),
            Some(Account {
                login: "larsx2".into(),
                link: format!("{LINK}&s=96"),
            })
        );
        let elsewhere = r#"{"login":"larsx2","avatar_url":"https://example.net/larsx2.png"}"#;
        assert_eq!(account(elsewhere), None);
        assert_eq!(account("not json"), None);
    }

    #[test]
    fn the_download_follows_no_redirect_and_stays_on_https() {
        let arguments = download_arguments(LINK);
        assert_eq!(arguments.first().map(String::as_str), Some("--disable"));
        assert!(
            !arguments
                .iter()
                .any(|one| one == "-L" || one == "--location")
        );
        let after = |flag: &str| {
            arguments
                .iter()
                .position(|one| one == flag)
                .and_then(|at| arguments.get(at + 1))
                .map(String::as_str)
        };
        assert_eq!(after("--max-redirs"), Some("0"));
        assert_eq!(after("--proto"), Some("=https"));
        assert_eq!(after("--proto-redir"), Some("=https"));
        assert_eq!(after("--max-filesize"), Some("524288"));
        assert_eq!(after("--url"), Some(LINK));
        assert!(!arguments.iter().any(|one| one.contains("uthorization")));
    }

    #[test]
    fn a_redirect_or_an_error_is_not_an_image() {
        assert!(image("200", PNG).is_some());
        assert_eq!(image("301", PNG), None);
        assert_eq!(image("302", PNG), None);
        assert_eq!(image("307", PNG), None);
        assert_eq!(image("404", PNG), None);
        assert_eq!(image("000", PNG), None);
        assert_eq!(image("", PNG), None);
    }

    #[test]
    fn an_image_past_the_size_cap_is_dropped() {
        let mut body = PNG.to_vec();
        body.resize(MAX_BYTES, 0);
        assert!(image("200", &body).is_some());
        body.push(0);
        assert_eq!(image("200", &body), None);
    }

    #[test]
    fn the_image_reaches_the_screen_as_a_data_url() {
        assert_eq!(
            data_url(PNG).as_deref(),
            Some("data:image/png;base64,iVBORw0KGgoAAAANSUhEUg==")
        );
        assert!(
            data_url(b"\xff\xd8\xff\xe0")
                .is_some_and(|url| url.starts_with("data:image/jpeg;base64,"))
        );
        assert!(
            data_url(b"GIF89a....").is_some_and(|url| url.starts_with("data:image/gif;base64,"))
        );
        assert!(
            data_url(b"RIFF\0\0\0\0WEBPVP8 ")
                .is_some_and(|url| url.starts_with("data:image/webp;base64,"))
        );
        assert_eq!(data_url(b"<svg xmlns='http://www.w3.org/2000/svg'/>"), None);
        assert_eq!(data_url(b"<html>"), None);
        assert_eq!(data_url(b""), None);
    }

    #[test]
    fn the_avatar_is_fetched_again_after_a_week_or_for_another_account() {
        let now = 2_000_000_000;
        let saved = Avatar {
            login: "larsx2".into(),
            fetched_at: now - REFRESH_AFTER + 1,
            image: "data:image/png;base64,AA==".into(),
        };
        assert!(due(None, "larsx2", now));
        assert!(!due(Some(&saved), "larsx2", now));
        assert!(!due(Some(&saved), "Larsx2", now));
        assert!(due(Some(&saved), "larsx2", now + 1));
        assert!(due(Some(&saved), "hazel", now));
        let ahead = Avatar {
            fetched_at: now + 60,
            ..saved.clone()
        };
        assert!(!due(Some(&ahead), "larsx2", now));
        assert_eq!(shown(Some(&saved), "larsx2"), Some(saved.image.clone()));
        assert_eq!(shown(Some(&saved), "hazel"), None);
    }

    #[cfg(unix)]
    #[test]
    fn the_saved_avatar_is_private_to_you() {
        use std::os::unix::fs::PermissionsExt as _;

        let state = std::env::temp_dir().join(format!("raiden-avatar-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&state);
        let avatar = Avatar {
            login: "larsx2".into(),
            fetched_at: 1,
            image: data_url(PNG).expect("an image"),
        };
        write(&state, &avatar).expect("written");
        let mode = |path: &Path| {
            std::fs::metadata(path)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(mode(&state.join(FOLDER)), 0o700);
        assert_eq!(mode(&state.join(FOLDER).join(FILE)), 0o600);
        assert_eq!(read(&state), Some(avatar.clone()));
        let tampered = Avatar {
            image: "https://example.net/larsx2.png".into(),
            ..avatar
        };
        write(&state, &tampered).expect("written");
        assert_eq!(read(&state), None);
        let _ = std::fs::remove_dir_all(&state);
    }
}
