use std::path::{Path, PathBuf};
use std::time::Duration;

use raiden_core::ai::Provider;
use sha2::{Digest, Sha256};

use super::process;

const STAGING: &str = ".installing-";
const REGISTRY: &str = "https://registry.npmjs.org/";
const INSTALL_LIMIT: Duration = Duration::from_secs(300);

pub struct Package {
    pub name: &'static str,
    manifest: &'static str,
    lock: &'static str,
}

const CLAUDE_CODE: Package = Package {
    name: "claude-agent-acp",
    manifest: include_str!("../../adapters/claude-agent-acp/package.json"),
    lock: include_str!("../../adapters/claude-agent-acp/package-lock.json"),
};

const CODEX: Package = Package {
    name: "codex-acp",
    manifest: include_str!("../../adapters/codex-acp/package.json"),
    lock: include_str!("../../adapters/codex-acp/package-lock.json"),
};

pub fn package(provider: Provider) -> &'static Package {
    match provider {
        Provider::Claude => &CLAUDE_CODE,
        Provider::Chatgpt => &CODEX,
    }
}

impl Package {
    pub fn folder(&self, root: &Path) -> PathBuf {
        let mut digest = Sha256::new();
        digest.update(self.manifest);
        digest.update([0]);
        digest.update(self.lock);
        let digest = digest.finalize();
        let hex: String = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
        root.join(format!("{}-{hex}", self.name))
    }

    pub fn program(&self, root: &Path) -> PathBuf {
        self.folder(root)
            .join("node_modules")
            .join(".bin")
            .join(self.name)
    }
}

pub async fn ensure(
    package: &Package,
    root: &Path,
    env: &[(String, String)],
    path: &str,
) -> Result<PathBuf, String> {
    let program = package.program(root);
    if program.is_file() {
        return Ok(program);
    }
    let failed = |detail: String| format!("Raiden could not install {}: {detail}", package.name);
    let npm = raiden_core::ai::environment::which("npm", path)
        .ok_or_else(|| failed("npm was not found on your login shell's PATH".into()))?;
    let io = |error: std::io::Error| failed(error.to_string());
    tokio::fs::create_dir_all(root).await.map_err(io)?;
    let (user, global) = (root.join("user.npmrc"), root.join("global.npmrc"));
    tokio::fs::write(&user, "").await.map_err(io)?;
    tokio::fs::write(&global, "").await.map_err(io)?;
    remove_abandoned(root).await;
    let staging = root.join(format!("{STAGING}{}-{}", package.name, std::process::id()));
    let _ = tokio::fs::remove_dir_all(&staging).await;
    tokio::fs::create_dir_all(&staging).await.map_err(io)?;
    tokio::fs::write(staging.join("package.json"), package.manifest)
        .await
        .map_err(io)?;
    tokio::fs::write(staging.join("package-lock.json"), package.lock)
        .await
        .map_err(io)?;
    let mut npm_env: Vec<(String, String)> = env
        .iter()
        .filter(|(name, _)| !name.to_ascii_lowercase().starts_with("npm_config_"))
        .cloned()
        .collect();
    npm_env.extend([
        ("npm_config_registry".to_owned(), REGISTRY.to_owned()),
        (
            "npm_config_userconfig".to_owned(),
            user.display().to_string(),
        ),
        (
            "npm_config_globalconfig".to_owned(),
            global.display().to_string(),
        ),
        ("npm_config_update_notifier".to_owned(), "false".to_owned()),
    ]);
    let mut command = process::command(&npm, &npm_env, &staging);
    command.args([
        "ci",
        "--ignore-scripts",
        "--omit=dev",
        "--omit=optional",
        "--no-audit",
        "--no-fund",
        "--loglevel=error",
    ]);
    let ran = process::run(command, INSTALL_LIMIT).await.map_err(io)?;
    if !ran.succeeded {
        let _ = tokio::fs::remove_dir_all(&staging).await;
        return Err(failed(format!("npm failed: {}", ran.output.trim())));
    }
    let folder = package.folder(root);
    if !program.is_file() {
        let _ = tokio::fs::remove_dir_all(&folder).await;
    }
    let renamed = tokio::fs::rename(&staging, &folder).await;
    let _ = tokio::fs::remove_dir_all(&staging).await;
    if let Err(error) = renamed
        && !program.is_file()
    {
        return Err(io(error));
    }
    program
        .is_file()
        .then_some(program.clone())
        .ok_or_else(|| failed(format!("npm did not place {}", program.display())))
}

async fn remove_abandoned(root: &Path) {
    let Ok(mut entries) = tokio::fs::read_dir(root).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        if abandoned(&name, alive) {
            let _ = tokio::fs::remove_dir_all(entry.path()).await;
        }
    }
}

fn alive(pid: libc::pid_t) -> bool {
    let signalled = unsafe { libc::kill(pid, 0) };
    signalled == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

fn abandoned(name: &str, alive: impl Fn(libc::pid_t) -> bool) -> bool {
    name.strip_prefix(STAGING)
        .and_then(|rest| rest.rsplit('-').next())
        .and_then(|pid| pid.parse::<libc::pid_t>().ok())
        .is_some_and(|pid| !alive(pid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_lockfile_pins_its_adapter() {
        for provider in Provider::ALL {
            let package = package(provider);
            let lock: serde_json::Value =
                serde_json::from_str(package.lock).expect("lockfile is JSON");
            let pinned = &lock["packages"]
                [format!("node_modules/@agentclientprotocol/{}", provider.adapter())];
            assert!(pinned["integrity"].is_string(), "{}", package.name);
            assert_eq!(package.name, provider.adapter());
        }
    }

    #[test]
    fn only_installs_of_ended_processes_are_abandoned() {
        let running = |pid| pid == 42;
        assert!(abandoned(".installing-codex-acp-7", running));
        assert!(!abandoned(".installing-codex-acp-42", running));
        assert!(!abandoned("codex-acp-0123abcd", running));
        assert!(!abandoned(".installing-codex-acp-x", running));
    }

    #[test]
    fn a_new_lockfile_installs_into_a_new_folder() {
        let root = Path::new("/state/adapters");
        let changed = Package {
            name: CODEX.name,
            manifest: CODEX.manifest,
            lock: "{}",
        };
        assert_ne!(CODEX.folder(root), changed.folder(root));
        assert!(CODEX.program(root).ends_with("node_modules/.bin/codex-acp"));
    }
}
