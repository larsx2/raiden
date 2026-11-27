use std::path::{Path, PathBuf};

/// ```
/// use std::path::Path;
/// use raiden_core::folders::logs;
/// let home = Path::new("/Users/me");
/// assert_eq!(logs(false, home, None), Path::new("/Users/me/Library/Logs/raiden"));
/// assert_eq!(logs(true, home, None), Path::new("/Users/me/.raiden-dev/logs"));
/// assert_eq!(logs(true, home, Some(Path::new("/tmp/r"))), Path::new("/tmp/r/logs"));
/// ```
pub fn logs(development: bool, home: &Path, raiden_home: Option<&Path>) -> PathBuf {
    if development {
        development_home(home, raiden_home).join("logs")
    } else {
        home.join("Library").join("Logs").join("raiden")
    }
}

/// ```
/// use std::path::Path;
/// use raiden_core::folders::state;
/// let home = Path::new("/Users/me");
/// assert_eq!(state(false, home, None), Path::new("/Users/me/Library/Application Support/raiden"));
/// assert_eq!(state(true, home, None), Path::new("/Users/me/.raiden-dev/state"));
/// ```
pub fn state(development: bool, home: &Path, raiden_home: Option<&Path>) -> PathBuf {
    if development {
        development_home(home, raiden_home).join("state")
    } else {
        home.join("Library")
            .join("Application Support")
            .join("raiden")
    }
}

/// ```
/// use std::path::Path;
/// use raiden_core::folders::settings;
/// let home = Path::new("/Users/me");
/// assert_eq!(settings(false, home, None), Path::new("/Users/me/.config/raiden/raiden.toml"));
/// assert_eq!(settings(true, home, None), Path::new("/Users/me/.raiden-dev/raiden.toml"));
/// ```
pub fn settings(development: bool, home: &Path, raiden_home: Option<&Path>) -> PathBuf {
    let folder = if development {
        development_home(home, raiden_home)
    } else {
        home.join(".config").join("raiden")
    };
    folder.join("raiden.toml")
}

fn development_home(home: &Path, raiden_home: Option<&Path>) -> PathBuf {
    raiden_home.map_or_else(|| home.join(".raiden-dev"), Path::to_path_buf)
}
