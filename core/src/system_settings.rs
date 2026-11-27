use crate::apps::App;

pub const SCHEME: &str = "x-apple.systempreferences:";
pub const OWNER: &str = "/System/Applications/System Settings.app";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Page {
    pub title: &'static str,
    pub under: &'static str,
    pub pane: &'static str,
    pub anchor: &'static str,
}

const fn page(
    title: &'static str,
    under: &'static str,
    pane: &'static str,
    anchor: &'static str,
) -> Page {
    Page {
        title,
        under,
        pane,
        anchor,
    }
}

pub const PAGES: &[Page] = &[
    page("Wi-Fi", "", "com.apple.wifi-settings-extension", ""),
    page("Bluetooth", "", "com.apple.BluetoothSettings", ""),
    page("Network", "", "com.apple.Network-Settings.extension", ""),
    page(
        "VPN",
        "",
        "com.apple.NetworkExtensionSettingsUI.NESettingsUIExtension",
        "",
    ),
    page(
        "Notifications",
        "",
        "com.apple.Notifications-Settings.extension",
        "",
    ),
    page("Sound", "", "com.apple.Sound-Settings.extension", ""),
    page("Focus", "", "com.apple.Focus-Settings.extension", ""),
    page(
        "Screen Time",
        "",
        "com.apple.Screen-Time-Settings.extension",
        "",
    ),
    page(
        "General",
        "",
        "com.apple.systempreferences.GeneralSettings",
        "",
    ),
    page(
        "Appearance",
        "",
        "com.apple.Appearance-Settings.extension",
        "",
    ),
    page(
        "Accessibility",
        "",
        "com.apple.Accessibility-Settings.extension",
        "",
    ),
    page(
        "Control Center",
        "",
        "com.apple.ControlCenter-Settings.extension",
        "",
    ),
    page(
        "Apple Intelligence & Siri",
        "",
        "com.apple.Siri-Settings.extension",
        "",
    ),
    page(
        "Spotlight",
        "",
        "com.apple.Spotlight-Settings.extension",
        "",
    ),
    page(
        "Privacy & Security",
        "",
        "com.apple.settings.PrivacySecurity.extension",
        "",
    ),
    page(
        "Desktop & Dock",
        "",
        "com.apple.Desktop-Settings.extension",
        "",
    ),
    page("Displays", "", "com.apple.Displays-Settings.extension", ""),
    page(
        "Wallpaper",
        "",
        "com.apple.Wallpaper-Settings.extension",
        "",
    ),
    page(
        "Screen Saver",
        "",
        "com.apple.ScreenSaver-Settings.extension",
        "",
    ),
    page("Energy", "", "com.apple.Battery-Settings.extension", ""),
    page(
        "Lock Screen",
        "",
        "com.apple.Lock-Screen-Settings.extension",
        "",
    ),
    page(
        "Login Password",
        "",
        "com.apple.Touch-ID-Settings.extension",
        "",
    ),
    page(
        "Users & Groups",
        "",
        "com.apple.Users-Groups-Settings.extension",
        "",
    ),
    page(
        "Internet Accounts",
        "",
        "com.apple.Internet-Accounts-Settings.extension",
        "",
    ),
    page(
        "Game Center",
        "",
        "com.apple.Game-Center-Settings.extension",
        "",
    ),
    page(
        "Wallet & Apple Pay",
        "",
        "com.apple.WalletSettingsExtension",
        "",
    ),
    page("Keyboard", "", "com.apple.Keyboard-Settings.extension", ""),
    page("Mouse", "", "com.apple.Mouse-Settings.extension", ""),
    page(
        "Printers & Scanners",
        "",
        "com.apple.Print-Scan-Settings.extension",
        "",
    ),
    page("Family", "", "com.apple.Family-Settings.extension", ""),
    page(
        "About",
        "General",
        "com.apple.SystemProfiler.AboutExtension",
        "",
    ),
    page(
        "Software Update",
        "General",
        "com.apple.Software-Update-Settings.extension",
        "",
    ),
    page("Storage", "General", "com.apple.settings.Storage", ""),
    page(
        "AirDrop & Handoff",
        "General",
        "com.apple.AirDrop-Handoff-Settings.extension",
        "",
    ),
    page(
        "Login Items & Extensions",
        "General",
        "com.apple.LoginItems-Settings.extension",
        "",
    ),
    page(
        "AppleCare & Warranty",
        "General",
        "com.apple.Coverage-Settings.extension",
        "",
    ),
    page(
        "Language & Region",
        "General",
        "com.apple.Localization-Settings.extension",
        "",
    ),
    page(
        "Date & Time",
        "General",
        "com.apple.Date-Time-Settings.extension",
        "",
    ),
    page(
        "Sharing",
        "General",
        "com.apple.Sharing-Settings.extension",
        "",
    ),
    page(
        "Time Machine",
        "General",
        "com.apple.Time-Machine-Settings.extension",
        "",
    ),
    page(
        "Transfer or Reset",
        "General",
        "com.apple.Transfer-Reset-Settings.extension",
        "",
    ),
    page(
        "Startup Disk",
        "General",
        "com.apple.Startup-Disk-Settings.extension",
        "",
    ),
    page(
        "Device Management",
        "General",
        "com.apple.Profiles-Settings.extension",
        "",
    ),
    page(
        "VoiceOver",
        "Accessibility",
        "com.apple.Accessibility-Settings.extension",
        "AX_FEATURE_VOICEOVER",
    ),
    page(
        "Zoom",
        "Accessibility",
        "com.apple.Accessibility-Settings.extension",
        "AX_FEATURE_ZOOM",
    ),
    page(
        "Display",
        "Accessibility",
        "com.apple.Accessibility-Settings.extension",
        "display",
    ),
    page(
        "Spoken Content",
        "Accessibility",
        "com.apple.Accessibility-Settings.extension",
        "AX_FEATURE_SPOKENCONTENT",
    ),
    page(
        "Audio Descriptions",
        "Accessibility",
        "com.apple.Accessibility-Settings.extension",
        "AX_FEATURE_DESCRIPTIONS",
    ),
    page(
        "Hearing Devices",
        "Accessibility",
        "com.apple.Accessibility-Settings.extension",
        "AX_FEATURE_HEARINGAIDS",
    ),
    page(
        "Audio",
        "Accessibility",
        "com.apple.Accessibility-Settings.extension",
        "AX_FEATURE_AUDIO",
    ),
    page(
        "Captions",
        "Accessibility",
        "com.apple.Accessibility-Settings.extension",
        "AX_FEATURE_CAPTIONS",
    ),
    page(
        "Voice Control",
        "Accessibility",
        "com.apple.Accessibility-Settings.extension",
        "AX_VOICE_CONTROL_MIC",
    ),
    page(
        "Keyboard",
        "Accessibility",
        "com.apple.Accessibility-Settings.extension",
        "AX_FEATURE_KEYBOARD",
    ),
    page(
        "Pointer Control",
        "Accessibility",
        "com.apple.Accessibility-Settings.extension",
        "AX_FEATURE_POINTERCONTROL",
    ),
    page(
        "Switch Control",
        "Accessibility",
        "com.apple.Accessibility-Settings.extension",
        "AX_SWITCH_AUTOSCAN",
    ),
    page(
        "Live Speech",
        "Accessibility",
        "com.apple.Accessibility-Settings.extension",
        "AX_LIVE_SPEECH_VOICE_SELECTION",
    ),
    page(
        "Personal Voice",
        "Accessibility",
        "com.apple.Accessibility-Settings.extension",
        "AX_FEATURE_PERSONALVOICE",
    ),
    page(
        "Siri",
        "Accessibility",
        "com.apple.Accessibility-Settings.extension",
        "AX_FEATURE_SIRI",
    ),
    page(
        "Shortcut",
        "Accessibility",
        "com.apple.Accessibility-Settings.extension",
        "AX_FEATURE_SHORTCUT",
    ),
    page(
        "Hover Text",
        "Accessibility",
        "com.apple.Accessibility-Settings.extension",
        "AX_HOVER_TEXT_ENABLE",
    ),
    page(
        "Live Captions",
        "Accessibility",
        "com.apple.Accessibility-Settings.extension",
        "AX_IN_APP",
    ),
    page(
        "Keyboard Shortcuts",
        "Keyboard",
        "com.apple.Keyboard-Settings.extension",
        "Shortcuts",
    ),
    page(
        "Text Replacements",
        "Keyboard",
        "com.apple.Keyboard-Settings.extension",
        "TextReplacements",
    ),
    page(
        "Input Sources",
        "Keyboard",
        "com.apple.Keyboard-Settings.extension",
        "InputSources",
    ),
    page(
        "Accessibility",
        "Privacy & Security",
        "com.apple.settings.PrivacySecurity.extension",
        "Privacy_Accessibility",
    ),
    page(
        "Full Disk Access",
        "Privacy & Security",
        "com.apple.settings.PrivacySecurity.extension",
        "Privacy_AllFiles",
    ),
    page(
        "Screen & System Audio Recording",
        "Privacy & Security",
        "com.apple.settings.PrivacySecurity.extension",
        "Privacy_ScreenCapture",
    ),
    page(
        "Input Monitoring",
        "Privacy & Security",
        "com.apple.settings.PrivacySecurity.extension",
        "Privacy_ListenEvent",
    ),
    page(
        "Automation",
        "Privacy & Security",
        "com.apple.settings.PrivacySecurity.extension",
        "Privacy_Automation",
    ),
    page(
        "Camera",
        "Privacy & Security",
        "com.apple.settings.PrivacySecurity.extension",
        "Privacy_Camera",
    ),
    page(
        "Microphone",
        "Privacy & Security",
        "com.apple.settings.PrivacySecurity.extension",
        "Privacy_Microphone",
    ),
    page(
        "Location Services",
        "Privacy & Security",
        "com.apple.settings.PrivacySecurity.extension",
        "Privacy_LocationServices",
    ),
    page(
        "Files & Folders",
        "Privacy & Security",
        "com.apple.settings.PrivacySecurity.extension",
        "Privacy_FilesAndFolders",
    ),
    page(
        "App Management",
        "Privacy & Security",
        "com.apple.settings.PrivacySecurity.extension",
        "Privacy_AppBundles",
    ),
    page(
        "Developer Tools",
        "Privacy & Security",
        "com.apple.settings.PrivacySecurity.extension",
        "Privacy_DevTools",
    ),
    page(
        "Contacts",
        "Privacy & Security",
        "com.apple.settings.PrivacySecurity.extension",
        "Privacy_Contacts",
    ),
    page(
        "Calendars",
        "Privacy & Security",
        "com.apple.settings.PrivacySecurity.extension",
        "Privacy_Calendars",
    ),
    page(
        "Photos",
        "Privacy & Security",
        "com.apple.settings.PrivacySecurity.extension",
        "Privacy_Photos",
    ),
    page(
        "Analytics & Improvements",
        "Privacy & Security",
        "com.apple.settings.PrivacySecurity.extension",
        "Privacy_Analytics",
    ),
    page(
        "FileVault",
        "Privacy & Security",
        "com.apple.settings.PrivacySecurity.extension",
        "FileVault",
    ),
];

impl Page {
    pub fn link(&self) -> String {
        if self.anchor.is_empty() {
            format!("{SCHEME}{}", self.pane)
        } else {
            format!("{SCHEME}{}?{}", self.pane, self.anchor)
        }
    }

    pub fn subtitle(&self) -> &'static str {
        if self.under.is_empty() {
            "System Settings"
        } else {
            self.under
        }
    }
}

/// ```
/// use raiden_core::system_settings::find;
/// let display = find("x-apple.systempreferences:com.apple.Accessibility-Settings.extension?display").unwrap();
/// assert_eq!((display.title, display.under), ("Display", "Accessibility"));
/// assert!(find("x-apple.systempreferences:com.apple.Accessibility-Settings.extension?evil").is_none());
/// ```
pub fn find(link: &str) -> Option<&'static Page> {
    PAGES.iter().find(|page| page.link() == link)
}

pub fn rows() -> Vec<App> {
    PAGES
        .iter()
        .map(|page| App {
            name: page.title.to_owned(),
            path: page.link(),
            under: page.subtitle().to_owned(),
            accessory: String::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_link_is_distinct_and_well_formed() {
        let links: std::collections::HashSet<String> = PAGES.iter().map(Page::link).collect();
        assert_eq!(links.len(), PAGES.len());
        for page in PAGES {
            assert!(!page.title.is_empty());
            assert!(page.pane.starts_with("com.apple."), "{}", page.title);
            assert!(
                page.pane
                    .chars()
                    .chain(page.anchor.chars())
                    .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)),
                "{}",
                page.title
            );
        }
    }

    #[test]
    fn a_page_under_a_pane_names_a_pane_that_exists() {
        for page in PAGES.iter().filter(|page| !page.under.is_empty()) {
            assert!(
                PAGES
                    .iter()
                    .any(|pane| pane.under.is_empty() && pane.title == page.under),
                "{} is under {}",
                page.title,
                page.under
            );
        }
    }

    #[test]
    fn rows_say_where_each_page_lives() {
        let rows = rows();
        let display = rows.iter().find(|row| row.name == "Display").unwrap();
        assert_eq!(display.under, "Accessibility");
        let sound = rows.iter().find(|row| row.name == "Sound").unwrap();
        assert_eq!(sound.under, "System Settings");
        assert_eq!(
            sound.path,
            "x-apple.systempreferences:com.apple.Sound-Settings.extension"
        );
    }
}
