use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

use super::config;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MANAGED_SSH_BEGIN: &str = "# BEGIN IKUN PX MANAGED PROXY";
const MANAGED_SSH_END: &str = "# END IKUN PX MANAGED PROXY";

#[derive(Default, Serialize, Deserialize)]
struct GitSnapshot {
    http_proxy: Option<String>,
    https_proxy: Option<String>,
}

fn run_hidden(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("launch {program}: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn environment_text(http_port: u16, socks_port: u16) -> String {
    format!(
        "set HTTP_PROXY=http://127.0.0.1:{http_port}\r\n\
         set HTTPS_PROXY=http://127.0.0.1:{http_port}\r\n\
         set ALL_PROXY=socks5://127.0.0.1:{socks_port}\r\n\
         set NO_PROXY=localhost,127.0.0.1"
    )
}

pub fn launch_proxy_console(http_port: u16, socks_port: u16) -> Result<(), String> {
    let command = format!(
        "{}&&title IKUN PX Proxy Console&&echo Proxy environment is ready.",
        environment_text(http_port, socks_port).replace("\r\n", "&&")
    );
    Command::new("cmd")
        .args(["/D", "/K", &command])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("open proxy console: {e}"))
}

fn git_snapshot_path() -> PathBuf {
    config::app_dir().join("git-proxy-snapshot.json")
}

fn git_get(key: &str) -> Option<String> {
    run_hidden("git", &["config", "--global", "--get", key])
        .ok()
        .filter(|value| !value.is_empty())
}

fn git_set(key: &str, value: Option<&str>) -> Result<(), String> {
    if let Some(value) = value {
        run_hidden("git", &["config", "--global", key, value]).map(|_| ())
    } else {
        let _ = run_hidden("git", &["config", "--global", "--unset-all", key]);
        Ok(())
    }
}

pub fn set_git_proxy(enabled: bool, http_port: u16) -> Result<(), String> {
    if enabled {
        if !git_snapshot_path().is_file() {
            std::fs::create_dir_all(config::app_dir())
                .map_err(|e| format!("create app data: {e}"))?;
            let snapshot = GitSnapshot {
                http_proxy: git_get("http.proxy"),
                https_proxy: git_get("https.proxy"),
            };
            std::fs::write(
                git_snapshot_path(),
                serde_json::to_vec_pretty(&snapshot)
                    .map_err(|e| format!("encode git proxy snapshot: {e}"))?,
            )
            .map_err(|e| format!("write git proxy snapshot: {e}"))?;
        }
        let proxy = format!("http://127.0.0.1:{http_port}");
        git_set("http.proxy", Some(&proxy))?;
        git_set("https.proxy", Some(&proxy))
    } else {
        let snapshot = if git_snapshot_path().is_file() {
            serde_json::from_slice::<GitSnapshot>(
                &std::fs::read(git_snapshot_path())
                    .map_err(|e| format!("read git proxy snapshot: {e}"))?,
            )
            .map_err(|e| format!("parse git proxy snapshot: {e}"))?
        } else {
            GitSnapshot::default()
        };
        git_set("http.proxy", snapshot.http_proxy.as_deref())?;
        git_set("https.proxy", snapshot.https_proxy.as_deref())?;
        if git_snapshot_path().is_file() {
            std::fs::remove_file(git_snapshot_path())
                .map_err(|e| format!("remove git proxy snapshot: {e}"))?;
        }
        Ok(())
    }
}

fn ssh_config_path() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ssh")
        .join("config")
}

fn find_git_connect() -> Option<PathBuf> {
    [
        Path::new(r"C:\Program Files\Git\mingw64\bin\connect.exe"),
        Path::new(r"C:\Program Files\Git\usr\bin\connect.exe"),
        Path::new(r"C:\Program Files (x86)\Git\mingw32\bin\connect.exe"),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .map(Path::to_path_buf)
}

fn strip_managed_ssh_block(text: &str) -> String {
    let Some(start) = text.find(MANAGED_SSH_BEGIN) else {
        return text.to_string();
    };
    let Some(relative_end) = text[start..].find(MANAGED_SSH_END) else {
        return text[..start].trim_end().to_string();
    };
    let end = start + relative_end + MANAGED_SSH_END.len();
    format!("{}{}", &text[..start], &text[end..])
        .trim()
        .to_string()
}

pub fn set_ssh_proxy(enabled: bool, socks_port: u16) -> Result<(), String> {
    let path = ssh_config_path();
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let base = strip_managed_ssh_block(&existing);
    let output = if enabled {
        let connect = find_git_connect().ok_or_else(|| {
            String::from("Git connect.exe was not found; install Git for Windows first")
        })?;
        format!(
            "{base}\n\n{MANAGED_SSH_BEGIN}\n\
             Host *\n\
             \tProxyCommand \"{}\" -S 127.0.0.1:{socks_port} %h %p\n\
             {MANAGED_SSH_END}\n",
            connect.display()
        )
    } else if base.is_empty() {
        String::new()
    } else {
        format!("{base}\n")
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create .ssh directory: {e}"))?;
    }
    std::fs::write(&path, output).map_err(|e| format!("write {}: {e}", path.display()))
}

pub fn set_auto_start(enabled: bool) -> Result<(), String> {
    let run = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            winreg::enums::KEY_READ | winreg::enums::KEY_WRITE,
        )
        .map_err(|e| format!("open Windows startup registry: {e}"))?;
    if enabled {
        let executable =
            std::env::current_exe().map_err(|e| format!("resolve current executable: {e}"))?;
        run.set_value("IKUN PX", &format!("\"{}\"", executable.display()))
            .map_err(|e| format!("enable startup: {e}"))
    } else {
        match run.delete_value("IKUN PX") {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("disable startup: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_ssh_block_is_removed_without_touching_user_config() {
        let text = "Host github.com\n  User git\n\n# BEGIN IKUN PX MANAGED PROXY\nHost *\n x\n# END IKUN PX MANAGED PROXY\n";
        assert_eq!(strip_managed_ssh_block(text), "Host github.com\n  User git");
    }
}
