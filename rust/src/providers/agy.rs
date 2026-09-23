// src/providers/agy.rs — Antigravity CLI usage provider
//
// Runs `agy --output-format json --print /quota` as a subprocess.
// Parses Gemini 5h / Weekly 7d buckets from the JSON output.
// Mirrors Python core/providers/agy_provider.py

use super::base::{now_str, percent_text, percentage, Provider, UsageMetrics};
use crate::config::Config;
use chrono::{DateTime, Utc};
use log::{info, warn};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use ureq::OrAnyStatus;

const QUOTA_URL: &str =
    "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary";
const QUOTA_URL_FALLBACK: &str =
    "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary";
const USER_AGENT: &str = "antigravity/1.0";

static AGY_BINARY: OnceLock<Option<String>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgyProfile {
    pub id: String,           // "default", "evanchen940628", "work"
    pub display_name: String, // "預設 (evanchen940628@gmail.com)" or "work (evanchen940628@gmail.com)"
    pub short_name: String,   // "預設", "evanchen", "work"
    pub email: Option<String>,
    pub profile_path: Option<PathBuf>,
    pub is_keyring_active: bool,
    pub last_activity: Option<std::time::SystemTime>,
}

pub fn base64_url_decode(input: &str) -> Option<Vec<u8>> {
    let mut s = input.replace('-', "+").replace('_', "/");
    let missing_padding = (4 - s.len() % 4) % 4;
    for _ in 0..missing_padding {
        s.push('=');
    }

    let mut out = Vec::new();
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;

    for &b in s.as_bytes() {
        let val = match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a' + 26) as u32,
            b'0'..=b'9' => (b - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => return None,
        };
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

pub fn extract_jwt_email(id_token: Option<&str>) -> (Option<String>, Option<String>) {
    let token = match id_token {
        Some(t) if !t.is_empty() => t,
        _ => return (None, None),
    };
    let parts: Vec<&str> = token.trim().split('.').collect();
    if parts.len() < 2 {
        return (None, None);
    }
    let bytes = match base64_url_decode(parts[1]) {
        Some(b) => b,
        None => return (None, None),
    };
    let val: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return (None, None),
    };
    let email = val.get("email").and_then(|v| v.as_str()).map(|s| s.to_string());
    let name = val.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
    (email, name)
}

#[cfg(target_os = "windows")]
pub mod os_cred {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    #[repr(C)]
    struct CREDENTIALW {
        flags: u32,
        r#type: u32,
        target_name: *mut u16,
        comment: *mut u16,
        last_written: [u32; 2],
        credential_blob_size: u32,
        credential_blob: *mut u8,
        persist: u32,
        attribute_count: u32,
        attributes: *mut std::ffi::c_void,
        target_alias: *mut u16,
        user_name: *mut u16,
    }

    #[link(name = "advapi32")]
    extern "system" {
        fn CredReadW(
            target_name: *const u16,
            r#type: u32,
            flags: u32,
            credential: *mut *mut CREDENTIALW,
        ) -> i32;
        fn CredWriteW(credential: *const CREDENTIALW, flags: u32) -> i32;
        fn CredFree(buffer: *mut std::ffi::c_void);
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetLastError() -> u32;
    }

    pub fn get_gemini_raw() -> Option<serde_json::Value> {
        let target: Vec<u16> = OsStr::new("gemini:antigravity\0").encode_wide().collect();
        let mut cred_ptr: *mut CREDENTIALW = std::ptr::null_mut();
        let res = unsafe { CredReadW(target.as_ptr(), 1, 0, &mut cred_ptr) };
        if res == 0 || cred_ptr.is_null() {
            return None;
        }

        let slice = unsafe {
            std::slice::from_raw_parts(
                (*cred_ptr).credential_blob,
                (*cred_ptr).credential_blob_size as usize,
            )
        };
        let text = String::from_utf8_lossy(slice).to_string();
        unsafe {
            CredFree(cred_ptr as *mut std::ffi::c_void);
        }

        serde_json::from_str(&text).ok()
    }

    pub fn get_gemini_token() -> Option<String> {
        get_gemini_raw()?
            .get("token")?
            .get("access_token")?
            .as_str()
            .map(|s| s.to_string())
    }

    pub fn set_gemini_raw(val: &serde_json::Value) -> Result<(), String> {
        let mut text = serde_json::to_string(val).map_err(|e| e.to_string())?;
        // Size Guard: Windows Generic Credential limit is 2560 bytes
        if text.len() > 2560 {
            if let Some(obj) = val.as_object() {
                let mut trimmed = obj.clone();
                trimmed.remove("id_token");
                if let Ok(s) = serde_json::to_string(&trimmed) {
                    text = s;
                }
            }
        }

        let target: Vec<u16> = OsStr::new("gemini:antigravity\0").encode_wide().collect();
        let user: Vec<u16> = OsStr::new("antigravity\0").encode_wide().collect();
        let mut bytes = text.into_bytes();

        let cred = CREDENTIALW {
            flags: 0,
            r#type: 1, // CRED_TYPE_GENERIC
            target_name: target.as_ptr() as *mut u16,
            comment: std::ptr::null_mut(),
            last_written: [0, 0],
            credential_blob_size: bytes.len() as u32,
            credential_blob: bytes.as_mut_ptr(),
            persist: 2, // CRED_PERSIST_LOCAL_MACHINE
            attribute_count: 0,
            attributes: std::ptr::null_mut(),
            target_alias: std::ptr::null_mut(),
            user_name: user.as_ptr() as *mut u16,
        };

        let res = unsafe { CredWriteW(&cred, 0) };
        if res == 0 {
            let err = unsafe { GetLastError() };
            return Err(format!("CredWriteW 失敗 (Win32 Error {})", err));
        }

        super::update_fallback_token_file(val);
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub mod os_cred {
    use std::process::Command;

    pub fn get_gemini_raw() -> Option<serde_json::Value> {
        let output = Command::new("security")
            .args([
                "find-generic-password",
                "-s",
                "gemini",
                "-a",
                "antigravity",
                "-w",
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        serde_json::from_str(&text).ok()
    }

    pub fn get_gemini_token() -> Option<String> {
        get_gemini_raw()?
            .get("token")?
            .get("access_token")?
            .as_str()
            .map(|s| s.to_string())
    }

    pub fn set_gemini_raw(val: &serde_json::Value) -> Result<(), String> {
        let text = serde_json::to_string(val).map_err(|e| e.to_string())?;
        let output = Command::new("security")
            .args([
                "add-generic-password",
                "-s",
                "gemini",
                "-a",
                "antigravity",
                "-w",
                &text,
                "-U",
            ])
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err("security add-generic-password 失敗".to_string());
        }

        super::update_fallback_token_file(val);
        Ok(())
    }
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
pub mod os_cred {
    use std::io::Write;
    use std::process::Command;

    pub fn get_gemini_raw() -> Option<serde_json::Value> {
        let output = Command::new("secret-tool")
            .args(["lookup", "service", "gemini", "account", "antigravity"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        serde_json::from_str(&text).ok()
    }

    pub fn get_gemini_token() -> Option<String> {
        get_gemini_raw()?
            .get("token")?
            .get("access_token")?
            .as_str()
            .map(|s| s.to_string())
    }

    pub fn set_gemini_raw(val: &serde_json::Value) -> Result<(), String> {
        let text = serde_json::to_string(val).map_err(|e| e.to_string())?;
        let mut child = Command::new("secret-tool")
            .args([
                "store",
                "--label=gemini:antigravity",
                "service",
                "gemini",
                "account",
                "antigravity",
            ])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| e.to_string())?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let status = child.wait().map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("secret-tool store 失敗".to_string());
        }

        super::update_fallback_token_file(val);
        Ok(())
    }
}

pub fn update_fallback_token_file(val: &serde_json::Value) {
    let dir = crate::providers::claude::dirs_home().join(".gemini").join("antigravity-cli");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("antigravity-oauth-token");
    if let Ok(text) = serde_json::to_string(val) {
        let _ = std::fs::write(&path, text);
    }
}

pub fn get_profiles_dir() -> PathBuf {
    let dir = crate::providers::claude::dirs_home().join(".gemini").join("profiles");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn auto_capture_active_profile() -> Option<AgyProfile> {
    let raw = os_cred::get_gemini_raw()?;
    let id_tok = raw.get("id_token").and_then(|v| v.as_str());
    let (mut email, mut name) = extract_jwt_email(id_tok);
    if email.is_none() {
        email = raw.get("email").and_then(|v| v.as_str()).map(|s| s.to_string())
            .or_else(|| raw.get("user").and_then(|v| v.as_str()).map(|s| s.to_string()));
    }
    let email = email?;
    let email_clean = email.trim().to_string();
    let short_name = email_clean.split('@').next().unwrap_or(&email_clean).to_string();
    if name.is_none() {
        name = Some(short_name.clone());
    }

    let p_dir = get_profiles_dir();
    let p_path = p_dir.join(format!("{}.json", short_name));

    let mut needs_save = false;
    if !p_path.exists() {
        needs_save = true;
    } else if let Ok(text) = std::fs::read_to_string(&p_path) {
        if let Ok(saved) = serde_json::from_str::<serde_json::Value>(&text) {
            let saved_tok = saved.pointer("/credential/token/access_token")
                .or_else(|| saved.pointer("/token/access_token"))
                .and_then(|v| v.as_str());
            let curr_tok = raw.pointer("/token/access_token").and_then(|v| v.as_str());
            if saved_tok != curr_tok && curr_tok.is_some() {
                needs_save = true;
            }
        } else {
            needs_save = true;
        }
    } else {
        needs_save = true;
    }

    if needs_save {
        let snapshot = serde_json::json!({
            "id": short_name,
            "email": email_clean,
            "name": name,
            "display_name": format!("{} ({})", short_name, email_clean),
            "short_name": short_name,
            "credential": raw
        });
        if let Ok(s) = serde_json::to_string_pretty(&snapshot) {
            let tmp = p_path.with_extension("json.tmp");
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(&tmp, &p_path);
            }
        }
    }

    let last_activity = std::fs::metadata(&p_path).and_then(|m| m.modified()).ok();
    Some(AgyProfile {
        id: short_name.clone(),
        display_name: format!("{} ({})", short_name, email_clean),
        short_name,
        email: Some(email_clean),
        profile_path: Some(p_path),
        is_keyring_active: true,
        last_activity,
    })
}

pub fn discover_profiles() -> Vec<AgyProfile> {
    let mut profiles = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    let active_prof = auto_capture_active_profile();
    let active_id = active_prof.as_ref().map(|p| p.id.to_lowercase());
    if let Some(ap) = active_prof {
        seen_ids.insert(ap.id.to_lowercase());
        profiles.push(ap);
    }

    let p_dir = get_profiles_dir();
    if let Ok(entries) = std::fs::read_dir(&p_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                let pid = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                if pid.is_empty() || seen_ids.contains(&pid.to_lowercase()) {
                    continue;
                }
                if let Ok(text) = std::fs::read_to_string(&path) {
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) {
                        let mut email = data.get("email").and_then(|v| v.as_str()).map(|s| s.to_string());
                        if email.is_none() {
                            let id_tok = data.pointer("/credential/id_token")
                                .or_else(|| data.get("id_token"))
                                .and_then(|v| v.as_str());
                            email = extract_jwt_email(id_tok).0;
                        }
                        let short = data.get("short_name").and_then(|v| v.as_str()).unwrap_or(&pid).to_string();
                        let disp = data.get("display_name").and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| {
                                if let Some(e) = &email {
                                    format!("{} ({})", short, e)
                                } else {
                                    format!("{} (~/{})", short, path.file_name().and_then(|f| f.to_str()).unwrap_or(""))
                                }
                            });
                        let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                        let is_active = active_id.as_deref() == Some(&pid.to_lowercase());
                        seen_ids.insert(pid.to_lowercase());
                        profiles.push(AgyProfile {
                            id: pid,
                            display_name: disp,
                            short_name: short,
                            email,
                            profile_path: Some(path),
                            is_keyring_active: is_active,
                            last_activity: mtime,
                        });
                    }
                }
            }
        }
    }

    if profiles.is_empty() {
        profiles.push(AgyProfile {
            id: "default".to_string(),
            display_name: "預設帳號 (Keyring)".to_string(),
            short_name: "預設".to_string(),
            email: None,
            profile_path: None,
            is_keyring_active: true,
            last_activity: None,
        });
    }

    profiles.sort_by(|a, b| {
        let active_cmp = b.is_keyring_active.cmp(&a.is_keyring_active);
        if active_cmp != std::cmp::Ordering::Equal {
            active_cmp
        } else {
            a.id.to_lowercase().cmp(&b.id.to_lowercase())
        }
    });

    profiles
}

pub fn resolve_active_profile(preference: &str) -> (AgyProfile, bool) {
    let profiles = discover_profiles();
    let pref = preference.trim();

    if pref.is_empty() || pref.eq_ignore_ascii_case("auto") {
        if let Some(active) = profiles.iter().find(|p| p.is_keyring_active) {
            return (active.clone(), true);
        }
        if let Some(first) = profiles.first() {
            return (first.clone(), true);
        }
        return (
            AgyProfile {
                id: "default".to_string(),
                display_name: "預設帳號".to_string(),
                short_name: "預設".to_string(),
                email: None,
                profile_path: None,
                is_keyring_active: true,
                last_activity: None,
            },
            true,
        );
    }

    for p in &profiles {
        if p.id.eq_ignore_ascii_case(pref)
            || p.short_name.eq_ignore_ascii_case(pref)
            || p.email.as_deref().map(|e| e.eq_ignore_ascii_case(pref)).unwrap_or(false)
        {
            return (p.clone(), false);
        }
    }

    let custom_path = get_profiles_dir().join(format!("{}.json", pref));
    (
        AgyProfile {
            id: pref.to_string(),
            display_name: format!("{} (~/.gemini/profiles/{}.json)", pref, pref),
            short_name: pref.to_string(),
            email: None,
            profile_path: Some(custom_path),
            is_keyring_active: false,
            last_activity: None,
        },
        false,
    )
}

pub fn switch_active_profile(profile_id: &str) -> Result<(), String> {
    if profile_id.is_empty() || profile_id.eq_ignore_ascii_case("auto") {
        return Ok(());
    }

    let profiles = discover_profiles();
    let target_prof = profiles
        .iter()
        .find(|p| p.id.eq_ignore_ascii_case(profile_id) || p.short_name.eq_ignore_ascii_case(profile_id))
        .ok_or_else(|| format!("未找到 Profile: {}", profile_id))?;

    let path = target_prof
        .profile_path
        .as_ref()
        .ok_or_else(|| format!("Profile {} 沒有檔案路徑", profile_id))?;

    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("無法讀取 Profile 檔案: {e}"))?;
    let data: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("Profile JSON 解析失敗: {e}"))?;

    let cred = data.get("credential").unwrap_or(&data);
    os_cred::set_gemini_raw(cred)?;
    log::info!("[AgyProvider] 成功切換系統活躍 AGY Profile 為 {}", profile_id);
    Ok(())
}

pub fn save_current_as_profile(alias: &str) -> Result<AgyProfile, String> {
    let raw = os_cred::get_gemini_raw().ok_or_else(|| "無法讀取當前金鑰庫憑證".to_string())?;
    let alias_clean = alias.trim().replace(' ', "_");
    if alias_clean.is_empty() {
        return Err("別名不可為空白".to_string());
    }

    let id_tok = raw.get("id_token").and_then(|v| v.as_str());
    let (mut email, mut name) = extract_jwt_email(id_tok);
    if email.is_none() {
        email = raw.get("email").and_then(|v| v.as_str()).map(|s| s.to_string())
            .or_else(|| raw.get("user").and_then(|v| v.as_str()).map(|s| s.to_string()));
    }

    let p_dir = get_profiles_dir();
    let p_path = p_dir.join(format!("{}.json", alias_clean));

    let snapshot = serde_json::json!({
        "id": alias_clean,
        "email": email,
        "name": name.clone().unwrap_or_else(|| alias_clean.clone()),
        "display_name": if let Some(e) = &email { format!("{} ({})", alias_clean, e) } else { alias_clean.clone() },
        "short_name": alias_clean,
        "credential": raw
    });

    let s = serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?;
    let tmp = p_path.with_extension("json.tmp");
    std::fs::write(&tmp, s).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &p_path).map_err(|e| e.to_string())?;

    let last_activity = std::fs::metadata(&p_path).and_then(|m| m.modified()).ok();
    Ok(AgyProfile {
        id: alias_clean.clone(),
        display_name: if let Some(e) = email.as_ref() { format!("{} ({})", alias_clean, e) } else { alias_clean.clone() },
        short_name: alias_clean,
        email,
        profile_path: Some(p_path),
        is_keyring_active: true,
        last_activity,
    })
}

/// Derive a filesystem-safe alias from an email's local part (native menus have no text input).
pub fn alias_from_email(email: Option<&str>) -> String {
    let local = email.and_then(|e| e.split('@').next()).unwrap_or("");
    let cleaned: String = local
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    if cleaned.is_empty() {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("agy_{}", secs)
    } else {
        cleaned
    }
}

/// Snapshot the current keyring account, naming it after its email (used by the native menu).
pub fn save_current_auto() -> Result<AgyProfile, String> {
    let raw = os_cred::get_gemini_raw().ok_or_else(|| "無法讀取當前金鑰庫憑證".to_string())?;
    let (mut email, _) = extract_jwt_email(raw.get("id_token").and_then(|v| v.as_str()));
    if email.is_none() {
        email = raw.get("email").and_then(|v| v.as_str()).map(|s| s.to_string());
    }
    save_current_as_profile(&alias_from_email(email.as_deref()))
}

pub struct AgyProvider {
    timeout_secs: u64,
    client: ureq::Agent,
    config: Option<Arc<Mutex<Config>>>,
}

impl AgyProvider {
    pub fn new() -> Self {
        Self::with_config(None)
    }

    pub fn with_config(config: Option<Arc<Mutex<Config>>>) -> Self {
        let timeout = Duration::from_secs(8);
        let client = ureq::AgentBuilder::new().timeout(timeout).build();
        Self {
            timeout_secs: 30,
            client,
            config,
        }
    }

    fn find_agy_binary_uncached() -> Option<String> {
        // 1. Windows default AppData directly on filesystem (instant, zero process execution)
        #[cfg(target_os = "windows")]
        {
            let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
            for suffix in &[
                "agy\\bin\\agy.exe",
                "agy\\bin\\agy.cmd",
                "agy\\bin\\agy.bat",
            ] {
                let candidate = format!("{}\\{}", local, suffix);
                if std::path::Path::new(&candidate).is_file() {
                    return Some(candidate);
                }
            }
        }

        // 2. Scan PATH directly using filesystem checks (zero process execution, zero console popups)
        if let Some(path_var) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path_var) {
                #[cfg(target_os = "windows")]
                for ext in &["exe", "cmd", "bat"] {
                    let cand = dir.join(format!("agy.{}", ext));
                    if cand.is_file() {
                        return Some(cand.to_string_lossy().to_string());
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let cand = dir.join("agy");
                    if cand.is_file() {
                        return Some(cand.to_string_lossy().to_string());
                    }
                }
            }
        }

        // 3. macOS / Linux default paths
        #[cfg(not(target_os = "windows"))]
        {
            let home = std::env::var("HOME").unwrap_or_default();
            for candidate in &[
                format!("{}/.local/bin/agy", home),
                "/usr/local/bin/agy".to_owned(),
                format!("{}/bin/agy", home),
            ] {
                if std::path::Path::new(candidate).is_file() {
                    return Some(candidate.clone());
                }
            }
        }

        None
    }

    fn find_agy_binary() -> Option<String> {
        AGY_BINARY
            .get_or_init(Self::find_agy_binary_uncached)
            .clone()
    }

    fn run_agy(bin: &str, timeout_secs: u64) -> Result<String, String> {
        let started = Instant::now();

        #[cfg(target_os = "windows")]
        let mut cmd = {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            if bin.to_lowercase().ends_with(".cmd") || bin.to_lowercase().ends_with(".bat") {
                let mut c = Command::new("cmd.exe");
                c.creation_flags(CREATE_NO_WINDOW);
                c.args([
                    "/d",
                    "/s",
                    "/c",
                    "call",
                    bin,
                    "--output-format",
                    "json",
                    "--print",
                    "/quota",
                ]);
                c
            } else {
                let mut c = Command::new(bin);
                c.creation_flags(CREATE_NO_WINDOW);
                c.args(["--output-format", "json", "--print", "/quota"]);
                c
            }
        };

        #[cfg(not(target_os = "windows"))]
        let mut cmd = {
            let mut c = Command::new(bin);
            c.args(["--output-format", "json", "--print", "/quota"]);
            c
        };

        cmd.current_dir(std::env::temp_dir());
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("無法啟動 agy，請確認安裝與執行權限: {e}"))?;

        // Drain stdout and stderr in background threads to avoid OS pipe buffer deadlock
        let mut stdout_pipe = child.stdout.take().unwrap();
        let mut stderr_pipe = child.stderr.take().unwrap();

        let stdout_thread = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = std::io::Read::read_to_end(&mut stdout_pipe, &mut buf);
            buf
        });

        let stderr_thread = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = std::io::Read::read_to_end(&mut stderr_pipe, &mut buf);
            buf
        });

        let timeout = std::time::Duration::from_secs(timeout_secs.max(5));
        let exit_status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if started.elapsed() >= timeout {
                        let _ = child.kill();
                        let _ = child.wait();
                        let _ = stdout_thread.join();
                        let _ = stderr_thread.join();
                        return Err(format!("agy 執行逾時 (超過 {} 秒)", timeout_secs));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(e) => {
                    let _ = child.kill();
                    let _ = stdout_thread.join();
                    let _ = stderr_thread.join();
                    return Err(format!("agy 等待錯誤: {e}"));
                }
            }
        };

        let stdout_bytes = stdout_thread.join().unwrap_or_default();
        let stderr_bytes = stderr_thread.join().unwrap_or_default();

        let elapsed = started.elapsed().as_secs_f64();
        info!(
            "quota exit={} elapsed={:.2}s",
            exit_status.code().unwrap_or(-1),
            elapsed
        );

        if !exit_status.success() {
            let stderr_msg = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
            if !stderr_msg.is_empty() {
                return Err(format!(
                    "agy 查詢失敗 (exit {}): {}",
                    exit_status.code().unwrap_or(-1),
                    stderr_msg
                ));
            }
            return Err(format!(
                "agy 查詢失敗 (exit {})",
                exit_status.code().unwrap_or(-1)
            ));
        }

        Ok(String::from_utf8_lossy(&stdout_bytes).to_string())
    }

    fn fetch_usage_api(&self, token: &str, now: &str) -> Result<UsageMetrics, String> {
        for url in &[QUOTA_URL, QUOTA_URL_FALLBACK] {
            let res = self
                .client
                .post(url)
                .set("Authorization", &format!("Bearer {}", token))
                .set("Content-Type", "application/json")
                .set("User-Agent", USER_AGENT)
                .send_string("{}")
                .or_any_status();

            match res {
                Ok(resp) => {
                    let status = resp.status();
                    if status == 200 {
                        let json: Value = resp
                            .into_json()
                            .map_err(|e| format!("API JSON 解析失敗: {e}"))?;
                        let metrics = parse_agy_json(json, now);
                        if metrics.metric1_val.is_some() || metrics.metric2_val.is_some() {
                            return Ok(metrics);
                        } else {
                            return Err("API 回傳中未找到有效配額項目".to_string());
                        }
                    } else if status == 401 {
                        return Err("Token 已失效 (HTTP 401 Unauthorized)".to_string());
                    } else {
                        log::debug!("[AgyProvider] {} 回應 HTTP {}", url, status);
                    }
                }
                Err(e) => {
                    log::debug!("[AgyProvider] {} 連線失敗: {}", url, e);
                }
            }
        }
        Err("所有配額 API 連線均未成功".to_string())
    }

    fn fetch_usage_cli(&self, now: &str) -> UsageMetrics {
        let Some(bin) = Self::find_agy_binary() else {
            warn!("[AgyProvider] Antigravity CLI binary not found");
            return UsageMetrics::error_result(
                "agy",
                "Antigravity",
                "未找到 agy 指令\n請確認已安裝 Antigravity CLI",
                "cli_not_found",
            );
        };

        let stdout = match Self::run_agy(&bin, self.timeout_secs) {
            Ok(s) => s,
            Err(msg) => {
                if msg.contains("exit") {
                    return UsageMetrics::error_result("agy", "Antigravity", &msg, "cli_exit");
                }
                return UsageMetrics::error_result("agy", "Antigravity", &msg, "cli_start");
            }
        };

        // Resilient JSON extraction (handle CLI banners/prefixes)
        let raw: Option<Value> = try_parse_json(&stdout);
        let Some(raw) = raw else {
            return UsageMetrics::error_result(
                "agy",
                "Antigravity",
                "agy 配額格式不相容，請查看相容性文件",
                "schema",
            );
        };

        parse_agy_json(raw, now)
    }
}

impl Provider for AgyProvider {
    fn provider_id(&self) -> &str {
        "agy"
    }
    fn display_name(&self) -> &str {
        "Antigravity"
    }

    fn fetch_usage(&self) -> UsageMetrics {
        let now = now_str();

        // Resolve which profile to use from config (mirrors ClaudeProvider::fetch_usage)
        let preference = self
            .config
            .as_ref()
            .and_then(|c| c.lock().ok())
            .map(|c| c.agy_profile.clone())
            .unwrap_or_else(|| "auto".to_string());

        let (active_profile, is_auto) = resolve_active_profile(&preference);
        let display_title = if is_auto {
            if active_profile.id == "default" {
                "ANTIGRAVITY".to_string()
            } else {
                format!("AGY [{}]", active_profile.short_name)
            }
        } else if active_profile.id == "default" {
            "ANTIGRAVITY".to_string()
        } else {
            format!("AGY ({})", active_profile.short_name)
        };

        // 1. Fast path: read token from profile snapshot if available, else keyring
        let token = if let Some(ref path) = active_profile.profile_path {
            if path.exists() {
                std::fs::read_to_string(path)
                    .ok()
                    .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
                    .and_then(|data| {
                        let cred = data.get("credential").unwrap_or(&data);
                        cred.pointer("/token/access_token")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
            } else {
                None
            }
        } else {
            None
        }
        .or_else(|| os_cred::get_gemini_token());

        if let Some(tok) = token {
            match self.fetch_usage_api(&tok, &now) {
                Ok(mut metrics) => {
                    info!("[AgyProvider] Direct API fetch succeeded (fast path)");
                    metrics.provider_name = display_title;
                    return metrics;
                }
                Err(err) => {
                    warn!(
                        "[AgyProvider] Direct API fetch failed ({}), falling back to CLI subprocess",
                        err
                    );
                }
            }
        } else {
            info!("[AgyProvider] No cached OAuth token, using CLI subprocess");
        }

        // 2. Fallback path: CLI subprocess (refreshes tokens and updates keyring)
        let mut metrics = self.fetch_usage_cli(&now);
        metrics.provider_name = display_title;
        metrics
    }
}

fn try_parse_json(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    // Try direct parse first
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return Some(v);
    }
    // Find outermost {...}
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end > start {
        serde_json::from_str::<Value>(&text[start..=end]).ok()
    } else {
        None
    }
}

fn parse_agy_json(raw: Value, now_str: &str) -> UsageMetrics {
    let groups = raw
        .pointer("/command/data/groups")
        .or_else(|| raw.get("groups"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut m1_used_pct: Option<f64> = None;
    let mut m1_reset_dt: Option<DateTime<Utc>> = None;
    let mut m2_used_pct: Option<f64> = None;
    let mut m2_reset_dt: Option<DateTime<Utc>> = None;
    let mut third_party_rem_pct: Option<f64> = None;

    for g in &groups {
        let g_name = g
            .get("name")
            .or_else(|| g.get("displayName"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();

        if g_name.contains("gemini") {
            let buckets = g
                .get("buckets")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for b in &buckets {
                let b_id = b
                    .get("id")
                    .or_else(|| b.get("bucketId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_lowercase();
                let b_window = b
                    .get("window")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_lowercase();
                let rem_frac = percentage(
                    b.get("remaining_fraction")
                        .or_else(|| b.get("remainingFraction"))
                        .and_then(|v| v.as_f64()),
                    1.0,
                );
                let Some(rem_frac) = rem_frac else {
                    continue;
                };
                let used_pct = (1.0 - rem_frac) * 100.0;
                let used_pct = used_pct.clamp(0.0, 100.0);

                let reset_dt: Option<DateTime<Utc>> = b
                    .get("reset_time")
                    .or_else(|| b.get("resetTime"))
                    .and_then(|v| v.as_str())
                    .and_then(|s| DateTime::parse_from_rfc3339(&s.replace('Z', "+00:00")).ok())
                    .map(|dt| dt.with_timezone(&Utc));

                if (b_id.contains("5h") || b_window.contains("5h"))
                    && m1_used_pct.is_none_or(|cur| used_pct > cur)
                {
                    m1_used_pct = Some(used_pct);
                    m1_reset_dt = reset_dt;
                } else if (b_id.contains("week") || b_window.contains("week"))
                    && m2_used_pct.is_none_or(|cur| used_pct > cur)
                {
                    m2_used_pct = Some(used_pct);
                    m2_reset_dt = reset_dt;
                }
            }
        } else if g_name.contains("claude") || g_name.contains("gpt") {
            let buckets = g
                .get("buckets")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for b in &buckets {
                let b_id = b
                    .get("id")
                    .or_else(|| b.get("bucketId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_lowercase();
                if b_id.contains("week") || b_id.contains("third_party") {
                    let rem_frac = percentage(
                        b.get("remaining_fraction")
                            .or_else(|| b.get("remainingFraction"))
                            .and_then(|v| v.as_f64()),
                        1.0,
                    );
                    if let Some(rem_frac) = rem_frac {
                        let remaining = rem_frac * 100.0;
                        third_party_rem_pct = Some(
                            third_party_rem_pct.map_or(remaining, |cur: f64| cur.min(remaining)),
                        );
                    }
                }
            }
        }
    }

    UsageMetrics {
        provider_id: "agy".to_owned(),
        provider_name: "Antigravity".to_owned(),
        metric1_title: "SESSION 5H".to_owned(),
        metric1_val: m1_used_pct,
        metric1_text: percent_text(m1_used_pct),
        metric1_reset: m1_reset_dt,
        metric2_title: "WEEKLY 7D".to_owned(),
        metric2_val: m2_used_pct,
        metric2_text: percent_text(m2_used_pct),
        metric2_reset: m2_reset_dt,
        badge1_text: format!("C/G 剩餘: {}", percent_text(third_party_rem_pct)),
        badge2_text: "Gemini Models".to_owned(),
        last_updated_time: now_str.to_owned(),
        error: if m1_used_pct.is_none() && m2_used_pct.is_none() {
            Some("未取得有效配額資料".to_owned())
        } else {
            None
        },
        error_code: if m1_used_pct.is_none() && m2_used_pct.is_none() {
            "schema".to_owned()
        } else {
            String::new()
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_url_decode_basic() {
        // "hello" in standard base64 is "aGVsbG8="
        // url-safe without padding: "aGVsbG8"
        let result = base64_url_decode("aGVsbG8");
        assert!(result.is_some(), "should decode without padding");
        assert_eq!(result.unwrap(), b"hello");
    }

    #[test]
    fn test_base64_url_decode_with_url_chars() {
        // base64url uses '-' for '+' and '_' for '/'
        let result = base64_url_decode("-_8");
        assert!(result.is_some(), "should handle - and _ characters");
    }

    // Minimal base64url encoder (test-only, no external crate needed)
    fn base64_url_encode(input: &[u8]) -> String {
        const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut out = String::new();
        let mut i = 0;
        while i < input.len() {
            let b0 = input[i] as u32;
            let b1 = if i + 1 < input.len() { input[i + 1] as u32 } else { 0 };
            let b2 = if i + 2 < input.len() { input[i + 2] as u32 } else { 0 };
            out.push(ALPHABET[((b0 >> 2) & 0x3F) as usize] as char);
            out.push(ALPHABET[(((b0 & 0x3) << 4) | (b1 >> 4)) as usize] as char);
            if i + 1 < input.len() {
                out.push(ALPHABET[(((b1 & 0xF) << 2) | (b2 >> 6)) as usize] as char);
            }
            if i + 2 < input.len() {
                out.push(ALPHABET[(b2 & 0x3F) as usize] as char);
            }
            i += 3;
        }
        out
    }

    #[test]
    fn test_extract_jwt_email_valid() {
        // Build a minimal JWT: header.payload.signature (all base64url, no padding)
        let payload_json = r#"{"email":"test@example.com","name":"Test User","sub":"12345"}"#;
        let encoded = base64_url_encode(payload_json.as_bytes());
        let fake_jwt = format!("header.{}.signature", encoded);
        let (email, name) = extract_jwt_email(Some(&fake_jwt));
        assert_eq!(email.as_deref(), Some("test@example.com"));
        assert_eq!(name.as_deref(), Some("Test User"));
    }

    #[test]
    fn test_extract_jwt_email_none_on_invalid() {
        let (email, name) = extract_jwt_email(None);
        assert!(email.is_none());
        assert!(name.is_none());

        // A 3-part token whose payload isn't valid base64url
        let (email2, _) = extract_jwt_email(Some("header.!!!.sig"));
        assert!(email2.is_none());
    }

    #[test]
    fn test_alias_from_email() {
        assert_eq!(alias_from_email(Some("john.doe+x@gmail.com")), "john.doex");
        assert_eq!(alias_from_email(Some("work_1@corp.io")), "work_1");
        assert!(alias_from_email(None).starts_with("agy_"));
        assert!(alias_from_email(Some("@bad.com")).starts_with("agy_"));
    }

    #[test]
    fn test_agy_profile_defaults() {
        let p = AgyProfile {
            id: "alice".to_string(),
            display_name: "Alice (alice@example.com)".to_string(),
            short_name: "alice".to_string(),
            email: Some("alice@example.com".to_string()),
            profile_path: None,
            is_keyring_active: false,
            last_activity: None,
        };
        assert_eq!(p.id, "alice");
        assert!(!p.is_keyring_active);
        assert!(p.profile_path.is_none());
    }

    #[test]
    fn test_resolve_active_profile_auto_fallback() {
        let (prof, is_auto) = resolve_active_profile("auto");
        assert!(is_auto, "auto preference should return is_auto=true");
        assert!(!prof.id.is_empty(), "profile id must not be empty");
    }

    #[test]
    fn test_resolve_active_profile_explicit() {
        let (prof, is_auto) = resolve_active_profile("nonexistent_profile_xyz");
        assert!(!is_auto, "explicit preference must return is_auto=false");
        assert_eq!(prof.id, "nonexistent_profile_xyz");
        assert_eq!(prof.short_name, "nonexistent_profile_xyz");
    }

    #[test]
    fn test_try_parse_json() {
        // Direct parse
        let pure = r#"{"hello": "world"}"#;
        let val = try_parse_json(pure).expect("should parse pure json");
        assert_eq!(val["hello"], "world");

        // With CLI headers/footers
        let banner = "Welcome to AGY CLI v2.0\nInfo: fetching quota\n{\"status\": \"ok\", \"count\": 42}\nDone in 0.2s";
        let val2 = try_parse_json(banner).expect("should parse json inside banner");
        assert_eq!(val2["status"], "ok");
        assert_eq!(val2["count"], 42);

        // Invalid
        assert!(try_parse_json("not json at all").is_none());
        assert!(try_parse_json("").is_none());
    }

    #[test]
    fn test_parse_agy_json() {
        let json_str = r#"{
            "command": {
                "data": {
                    "groups": [
                        {
                            "name": "Gemini Models",
                            "buckets": [
                                {
                                    "id": "session",
                                    "window": "5h",
                                    "remainingFraction": 0.8,
                                    "resetTime": "2030-01-01T00:00:00Z"
                                },
                                {
                                    "id": "weekly",
                                    "window": "7d",
                                    "remainingFraction": 0.45,
                                    "resetTime": "2030-01-07T00:00:00Z"
                                }
                            ]
                        },
                        {
                            "name": "Claude / 3rd Party Models",
                            "buckets": [
                                {
                                    "id": "third_party",
                                    "remainingFraction": 0.92
                                }
                            ]
                        }
                    ]
                }
            }
        }"#;

        let val: Value = serde_json::from_str(json_str).unwrap();
        let m = parse_agy_json(val, "12:00:00");

        assert_eq!(m.provider_id, "agy");
        assert_eq!(m.provider_name, "Antigravity");
        // Used = (1.0 - 0.8) * 100 = 20.0%
        assert!((m.metric1_val.unwrap() - 20.0).abs() < 0.1);
        assert_eq!(m.metric1_text, "20%");
        // Weekly used = (1.0 - 0.45) * 100 = 55.0%
        assert!((m.metric2_val.unwrap() - 55.0).abs() < 0.1);
        assert_eq!(m.metric2_text, "55%");
        // Badge has 92%
        assert!(m.badge1_text.contains("92%"));
        assert!(m.error.is_none());
    }

    #[test]
    fn test_parse_agy_api_json() {
        let api_json = serde_json::json!({
            "groups": [
                {
                    "displayName": "Gemini Models",
                    "buckets": [
                        {
                            "bucketId": "gemini-weekly",
                            "window": "weekly",
                            "remainingFraction": 0.25,
                            "resetTime": "2030-01-07T00:00:00Z"
                        },
                        {
                            "bucketId": "gemini-5h",
                            "window": "5h",
                            "remainingFraction": 0.90,
                            "resetTime": "2030-01-01T05:00:00Z"
                        }
                    ]
                },
                {
                    "displayName": "Claude and GPT models",
                    "buckets": [
                        {
                            "bucketId": "3p-weekly",
                            "window": "weekly",
                            "remainingFraction": 0.70
                        }
                    ]
                }
            ]
        });

        let m = parse_agy_json(api_json, "12:00:00");
        assert_eq!(m.provider_id, "agy");
        assert_eq!(m.provider_name, "Antigravity");
        // 5h used = (1.0 - 0.90) * 100 = 10.0%
        assert!((m.metric1_val.unwrap() - 10.0).abs() < 0.1);
        assert_eq!(m.metric1_text, "10%");
        // Weekly used = (1.0 - 0.25) * 100 = 75.0%
        assert!((m.metric2_val.unwrap() - 75.0).abs() < 0.1);
        assert_eq!(m.metric2_text, "75%");
        // Badge has 70%
        assert!(m.badge1_text.contains("70%"));
        assert!(m.error.is_none());
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_os_cred_token() {
        // Just verify it doesn't crash or panic
        let tok = os_cred::get_gemini_token();
        println!("Gemini token discovered: {}", tok.is_some());
    }

    #[test]
    fn test_fetch_usage_live_benchmark() {
        let provider = AgyProvider::new();
        let start = std::time::Instant::now();
        let metrics = provider.fetch_usage();
        let elapsed = start.elapsed();
        eprintln!("Live agy fetch took: {:.2?}", elapsed);
        if metrics.error_code == "cli_not_found" {
            eprintln!("Skipping live assertion: agy not installed in this environment (e.g. CI)");
            return;
        }
        eprintln!("Provider error: {:?}", metrics.error);
        eprintln!(
            "Metric 1: {} = {}",
            metrics.metric1_title, metrics.metric1_text
        );
        eprintln!(
            "Metric 2: {} = {}",
            metrics.metric2_title, metrics.metric2_text
        );
        eprintln!("Badge 1: {}", metrics.badge1_text);
        assert!(metrics.error.is_none());
        // Correctness check: must finish within the CLI subprocess timeout.
        // (Fast path ≈200ms; CLI fallback ≈5–8s when the cached token is stale.)
        assert!(elapsed < std::time::Duration::from_secs(45), "fetch took too long: {:.2?}", elapsed);
    }
}
