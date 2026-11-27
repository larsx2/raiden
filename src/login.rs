use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use objc2_foundation::NSString;
use objc2_service_management::{SMAppService, SMAppServiceStatus};
use raiden_core::login::{self, AtLogin, Seen, Status, Step};

static REFUSAL: Mutex<Option<String>> = Mutex::new(None);

fn status(service: &SMAppService) -> Status {
    match unsafe { service.status() } {
        SMAppServiceStatus::Enabled => Status::Enabled,
        SMAppServiceStatus::RequiresApproval => Status::RequiresApproval,
        SMAppServiceStatus::NotRegistered => Status::NotRegistered,
        _ => Status::NotFound,
    }
}

fn bundled_agent() -> objc2::rc::Retained<SMAppService> {
    unsafe { SMAppService::agentServiceWithPlistName(&NSString::from_str(login::AGENT_PLIST)) }
}

fn login_item() -> objc2::rc::Retained<SMAppService> {
    unsafe { SMAppService::mainAppService() }
}

fn old_registrations() -> Status {
    let statuses = [status(&login_item()), status(&bundled_agent())];
    if statuses.contains(&Status::Enabled) {
        Status::Enabled
    } else if statuses.contains(&Status::RequiresApproval) {
        Status::RequiresApproval
    } else {
        Status::NotRegistered
    }
}

fn domain() -> String {
    format!("gui/{}", unsafe { libc::getuid() })
}

fn agent_file() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", login::AGENT_LABEL))
}

fn turned_off() -> bool {
    let listing = std::process::Command::new("/bin/launchctl")
        .args(["print-disabled", &domain()])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default();
    login::disabled(&listing)
}

fn agent_status() -> Status {
    if !agent_file().exists() {
        return Status::NotRegistered;
    }
    if turned_off() {
        return Status::RequiresApproval;
    }
    let loaded = std::process::Command::new("/bin/launchctl")
        .args(["print", &format!("{}/{}", domain(), login::AGENT_LABEL)])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if loaded {
        Status::Enabled
    } else {
        Status::NotRegistered
    }
}

fn marker(development: bool, name: &str) -> PathBuf {
    crate::folders(development).1.join(name)
}

fn remember(development: bool) -> bool {
    let path = marker(development, login::MARKER);
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&path, ""));
    if let Err(error) = &written {
        eprintln!("Raiden could not write {}: {error}", path.display());
    }
    written.is_ok()
}

fn refused(what: &str, error: &objc2_foundation::NSError) {
    let refusal = format!("error {}: {}", error.code(), error.localizedDescription());
    eprintln!("Raiden could not {what}: {refusal}");
    *REFUSAL.lock().unwrap_or_else(PoisonError::into_inner) = Some(refusal);
}

fn leave_login_items(development: bool) -> bool {
    for service in [login_item(), bundled_agent()] {
        if status(&service) != Status::Enabled {
            continue;
        }
        if let Err(error) = unsafe { service.unregisterAndReturnError() } {
            refused(
                "leave its old login registration for its launch agent",
                &error,
            );
            return false;
        }
    }
    for old in [login::LOGIN_ITEM_MARKER, login::BUNDLED_AGENT_MARKER] {
        let _ = std::fs::remove_file(marker(development, old));
    }
    true
}

fn refuse_text(what: &str) {
    eprintln!("Raiden could not {what}");
    *REFUSAL.lock().unwrap_or_else(PoisonError::into_inner) = Some(what.to_owned());
}

fn register() -> bool {
    let Some(program) = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .ok()
        .and_then(|exe| exe.to_str().map(str::to_owned))
    else {
        refuse_text("find its own program to start at login");
        return false;
    };
    let file = agent_file();
    let written = file
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&file, login::agent_plist(&program)));
    if let Err(error) = written {
        refuse_text(&format!("write {}: {error}", file.display()));
        return false;
    }
    let _ = std::process::Command::new("/bin/launchctl")
        .args(["bootstrap", &domain()])
        .arg(&file)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let loaded = std::process::Command::new("/bin/launchctl")
        .args(["print", &format!("{}/{}", domain(), login::AGENT_LABEL)])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !loaded {
        refuse_text("load its launch agent with launchctl bootstrap");
    }
    loaded
}

pub fn start(development: bool) {
    std::thread::spawn(move || {
        let steps = login::plan(Seen {
            development,
            agent_before: marker(development, login::MARKER).exists(),
            login_item_before: marker(development, login::LOGIN_ITEM_MARKER).exists()
                || marker(development, login::BUNDLED_AGENT_MARKER).exists(),
            agent: agent_status(),
            login_item: old_registrations(),
        });
        for step in steps {
            let done = match step {
                Step::RegisterAgent => register(),
                Step::Remember => remember(development),
                Step::LeaveLoginItem => leave_login_items(development),
            };
            if !done {
                return;
            }
        }
    });
}

pub fn at_login(development: bool) -> AtLogin {
    login::at_login(development, agent_file().exists(), turned_off())
}

pub fn set_at_login(development: bool, on: bool) -> Result<AtLogin, String> {
    if development {
        return Err(format!("{}.", login::DEVELOPMENT_NOTE));
    }
    let switched = std::process::Command::new("/bin/launchctl")
        .args(login::switch(unsafe { libc::getuid() }, on))
        .output()
        .map_err(|error| format!("Raiden could not run launchctl: {error}"))?;
    if !switched.status.success() {
        return Err(format!(
            "launchctl refused: {}",
            String::from_utf8_lossy(&switched.stderr).trim()
        ));
    }
    if on && (!agent_file().exists() || agent_status() != Status::Enabled) {
        if !register() {
            let refusal = REFUSAL
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
                .unwrap_or_else(|| "register its launch agent".to_owned());
            return Err(format!("Raiden could not {refusal}."));
        }
        if !remember(development) {
            return Err(
                "Raiden registered its launch agent but could not note that it did.".to_owned(),
            );
        }
    }
    let shown = at_login(development);
    if shown.on != on {
        return Err(format!(
            "macOS kept Open at login {}.",
            if shown.on { "on" } else { "off" }
        ));
    }
    Ok(shown)
}
