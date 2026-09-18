// src/autostart.rs — Platform autostart management
// Mirrors Python core/autostart.py

use log::error;

const APP_NAME: &str = "ClaudeHUDMonitor";

/// Check if autostart is enabled for the current platform.
pub fn is_autostart_enabled() -> bool {
    #[cfg(target_os = "windows")]
    return windows_is_enabled();

    #[cfg(target_os = "macos")]
    return macos_plist_path().exists();

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    false
}

/// Enable or disable autostart. Returns true on success.
pub fn set_autostart(enable: bool) -> bool {
    #[cfg(target_os = "windows")]
    return windows_set(enable);

    #[cfg(target_os = "macos")]
    return macos_set(enable);

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    false
}

// ── Windows ───────────────────────────────────────────────────────────────────
#[cfg(target_os = "windows")]
fn windows_is_enabled() -> bool {
    use winreg::enums::*;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run") else {
        return false;
    };
    key.get_value::<String, _>(APP_NAME).is_ok()
}

#[cfg(target_os = "windows")]
fn windows_set(enable: bool) -> bool {
    use winreg::enums::*;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.open_subkey_with_flags(
        r"Software\Microsoft\Windows\CurrentVersion\Run",
        KEY_SET_VALUE,
    ) {
        Ok(key) => {
            if enable {
                let exe = std::env::current_exe()
                    .map(|p| format!("\"{}\"", p.display()))
                    .unwrap_or_default();
                key.set_value(APP_NAME, &exe).map_err(|e| error!("[AutoStart] {e}")).is_ok()
            } else {
                key.delete_value(APP_NAME)
                    .or_else(|_| Ok::<_, ()>(()))
                    .is_ok()
            }
        }
        Err(e) => {
            error!("[AutoStart] Cannot open registry key: {e}");
            false
        }
    }
}

// ── macOS ─────────────────────────────────────────────────────────────────────
#[cfg(target_os = "macos")]
fn macos_plist_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_owned());
    std::path::PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join("com.claudehud.plist")
}

#[cfg(target_os = "macos")]
fn macos_set(enable: bool) -> bool {
    let plist_path = macos_plist_path();
    if enable {
        let exe = std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let plist_content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.claudehud</string>
    <key>ProgramArguments</key>
    <array><string>{}</string></array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>"#,
            exe
        );
        if let Some(dir) = plist_path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        std::fs::write(&plist_path, plist_content).map_err(|e| error!("[AutoStart] {e}")).is_ok()
    } else {
        if plist_path.exists() {
            let _ = std::process::Command::new("launchctl")
                .args(["unload", &plist_path.display().to_string()])
                .output();
            std::fs::remove_file(&plist_path)
                .map_err(|e| error!("[AutoStart] {e}"))
                .is_ok()
        } else {
            true
        }
    }
}
