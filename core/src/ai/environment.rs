use std::path::{Path, PathBuf};

const PASSED: [&str; 11] = [
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "TMPDIR",
    "LANG",
    "TERM",
    "TZ",
    "CLAUDE_CONFIG_DIR",
    "CODEX_HOME",
];

pub const SYSTEM_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";
pub const PATH_BEGIN: &str = "__RAIDEN_PATH_BEGIN__";
pub const PATH_END: &str = "__RAIDEN_PATH_END__";

/// ```
/// use raiden_core::ai::environment::allowed;
/// let vars = [("HOME", "/Users/me"), ("ANTHROPIC_API_KEY", "x"), ("LC_ALL", "C"), ("PATH", "/bin")]
///     .map(|(name, value)| (name.to_owned(), value.to_owned()));
/// let kept = allowed(vars, Some("/opt/bin:/bin"));
/// assert_eq!(kept, [("HOME".to_owned(), "/Users/me".to_owned()), ("LC_ALL".into(), "C".into()), ("PATH".into(), "/opt/bin:/bin".into())]);
/// ```
pub fn allowed(
    vars: impl IntoIterator<Item = (String, String)>,
    path: Option<&str>,
) -> Vec<(String, String)> {
    let mut kept: Vec<(String, String)> = vars
        .into_iter()
        .filter(|(name, _)| PASSED.contains(&name.as_str()) || name.starts_with("LC_"))
        .filter(|(name, _)| path.is_none() || name != "PATH")
        .collect();
    if let Some(path) = path {
        kept.push(("PATH".to_owned(), path.to_owned()));
    }
    kept
}

pub fn login_path_script() -> String {
    format!("printf '{PATH_BEGIN}%s{PATH_END}' \"$PATH\"")
}

/// ```
/// use raiden_core::ai::environment::login_path;
/// assert_eq!(login_path("motd\n__RAIDEN_PATH_BEGIN__/a:/b__RAIDEN_PATH_END__"), Some("/a:/b".into()));
/// assert_eq!(login_path("nothing"), None);
/// ```
pub fn login_path(output: &str) -> Option<String> {
    let path = output
        .split_once(PATH_BEGIN)?
        .1
        .split_once(PATH_END)?
        .0
        .trim();
    (!path.is_empty() && !path.contains('\n')).then(|| path.to_owned())
}

pub fn which(program: &str, path: &str) -> Option<PathBuf> {
    std::env::split_paths(path)
        .map(|folder| folder.join(program))
        .find(|candidate| is_program(candidate))
}

fn is_program(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_keys_and_unknown_variables_never_pass() {
        let vars = [
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "CLAUDECODE",
            "CLAUDE_CODE_ENTRYPOINT",
            "GITHUB_TOKEN",
            "CODEX_HOME",
            "CLAUDE_CONFIG_DIR",
        ]
        .map(|name| (name.to_owned(), "value".to_owned()));
        let names: Vec<String> = allowed(vars, None).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["CODEX_HOME", "CLAUDE_CONFIG_DIR"]);
    }

    #[test]
    fn which_finds_the_first_program_on_the_path() {
        let folder = std::env::temp_dir().join(format!("raiden-which-{}", std::process::id()));
        let (first, second) = (folder.join("one"), folder.join("two"));
        std::fs::create_dir_all(&first).expect("folder");
        std::fs::create_dir_all(&second).expect("folder");
        std::fs::write(second.join("claude"), "").expect("program");
        let path = format!("{}:{}", first.display(), second.display());
        let found = which("claude", &path);
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(found, Some(second.join("claude")));
    }
}
