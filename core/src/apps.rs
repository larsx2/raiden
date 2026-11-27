use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct App {
    pub name: String,
    pub path: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub under: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub accessory: String,
}

pub fn folders(home: &Path) -> Vec<PathBuf> {
    vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/Applications/Utilities"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
        PathBuf::from("/System/Library/CoreServices"),
        home.join("Applications"),
    ]
}

pub fn installed(folders: &[PathBuf]) -> Vec<App> {
    let mut apps: Vec<App> = folders
        .iter()
        .filter_map(|folder| std::fs::read_dir(folder).ok())
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "app"))
        .filter(|path| {
            !path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        })
        .filter_map(|path| {
            let name = path.file_stem()?.to_string_lossy().into_owned();
            Some(App {
                name,
                path: path.to_string_lossy().into_owned(),
                under: String::new(),
                accessory: String::new(),
            })
        })
        .filter(|app| !app.path.contains("CoreServices") || app.name == "Finder")
        .collect();
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps.dedup_by(|a, b| a.name == b.name);
    apps
}

/// ```
/// use raiden_core::apps::{pad, App};
/// let one = vec![App { name: "Safari".into(), path: "/Applications/Safari.app".into(), under: String::new(), accessory: String::new() }];
/// let padded = pad(one, 3);
/// assert_eq!(padded.len(), 3);
/// assert_eq!(padded[2].name, "Safari 3");
/// ```
pub fn pad(apps: Vec<App>, rows: usize) -> Vec<App> {
    if apps.is_empty() || apps.len() >= rows {
        return apps;
    }
    (0..rows)
        .map(|index| {
            let app = &apps[index % apps.len()];
            let round = index / apps.len();
            App {
                name: if round == 0 {
                    app.name.clone()
                } else {
                    format!("{} {}", app.name, round + 1)
                },
                path: app.path.clone(),
                under: app.under.clone(),
                accessory: app.accessory.clone(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_apps_sorted_and_skips_other_files() {
        let folder = std::env::temp_dir().join(format!("raiden-apps-{}", std::process::id()));
        for name in [
            "zed.app",
            "Arc.app",
            "notes.txt",
            "Safari.app",
            ".Hidden.app",
        ] {
            std::fs::create_dir_all(folder.join(name)).expect("temporary app");
        }
        let names: Vec<String> = installed(std::slice::from_ref(&folder))
            .into_iter()
            .map(|app| app.name)
            .collect();
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(names, ["Arc", "Safari", "zed"]);
    }

    #[test]
    fn padding_never_shrinks() {
        let apps = vec![
            App {
                name: "A".into(),
                path: "/a".into(),
                under: String::new(),
                accessory: String::new(),
            };
            5
        ];
        assert_eq!(pad(apps, 2).len(), 5);
    }
}
