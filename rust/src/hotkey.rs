// src/hotkey.rs — Global hotkeys (mirrors Python system/hotkey.py)
//
//   Alt+C  (id 9527) → toggle visibility
//   Alt+Shift+C (id 9528) → toggle click-through
//
// Registration and the worker thread belong to ferrokit's `GlobalHotkeyManager` (Windows only;
// elsewhere it reports `Unsupported`). A press runs the handler on that worker thread, which only
// forwards it to the UI thread (`run_on_main_thread` in main.rs).

use ferrokit_platform::{GlobalHotkeyManager, HotkeyError, HotkeyModifiers};
use ferrokit_widgets::KeySequence;
use log::{info, warn};

const HOTKEY_ID_TOGGLE: u32 = 9527;
const HOTKEY_ID_CLICKTHROUGH: u32 = 9528;

/// Which hotkey was pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    Toggle,
    ClickThrough,
}

/// Outcome of one global-hotkey registration (`GlobalHotkeyManager.toggle_registered` /
/// `clickthrough_registered` in Python).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyStatus {
    Registered,
    /// The system refused the registration (`HotkeyError::Os { code }`: `GetLastError()` taken
    /// right after `RegisterHotKey`, 1409 = `ERROR_HOTKEY_ALREADY_REGISTERED`).
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    Failed(u32),
    /// This platform has no global-hotkey backend.
    #[cfg_attr(target_os = "windows", allow(dead_code))]
    Unsupported,
}

/// What `HotkeyManager::start` achieved: one status per hotkey, with the label used in messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyRegistration {
    pub toggle: HotkeyStatus,
    pub toggle_label: String,
    pub clickthrough: HotkeyStatus,
    pub clickthrough_label: String,
}

impl HotkeyRegistration {
    /// No registration confirmed and none failed: no backend, or the worker did not report in time.
    pub fn unconfirmed() -> Self {
        Self {
            toggle: HotkeyStatus::Unsupported,
            toggle_label: String::new(),
            clickthrough: HotkeyStatus::Unsupported,
            clickthrough_label: String::new(),
        }
    }

    /// Python `clickthrough_registered`.
    pub fn clickthrough_registered(&self) -> bool {
        self.clickthrough == HotkeyStatus::Registered
    }

    /// The tray messages Python shows for failed registrations, in its order: for each failed
    /// hotkey `hotkey_failed` (with the Win32 error) and then `unavailable`
    /// (`python/system/hotkey.py:70-86`). The caller appends the "use the tray" hint
    /// (`python/main.py:77-84`).
    pub fn failure_notices(&self) -> Vec<String> {
        let mut notices = Vec::new();
        if let HotkeyStatus::Failed(err) = self.toggle {
            notices.push(format!(
                "{} 全域快捷鍵註冊失敗 (Win32 Error: {err})，可能已被其他程式佔用",
                self.toggle_label
            ));
            notices.push(format!("{} 已被占用，請使用系統匣操作", self.toggle_label));
        }
        if let HotkeyStatus::Failed(err) = self.clickthrough {
            notices.push(format!(
                "{} 穿透模式快捷鍵註冊失敗 (Win32 Error: {err})，可能已被其他程式佔用",
                self.clickthrough_label
            ));
            notices.push(format!(
                "{} 已被占用，請使用系統匣操作",
                self.clickthrough_label
            ));
        }
        notices
    }
}

/// Whether the startup click-through mode may stay on (`_safe_init_click_through`,
/// `python/ui/hud_window.py:426-437`). Click-through makes the window ignore the mouse, and the
/// click-through hotkey is the way back out, so it is only allowed when that hotkey is registered.
/// No manager (hotkeys disabled or failed to start) means not registered.
pub fn click_through_startup_allowed(registration: Option<&HotkeyRegistration>) -> bool {
    registration.is_some_and(HotkeyRegistration::clickthrough_registered)
}
/// The warning body shown when click-through mode is disabled on startup to prevent lockout
/// (mirrors Python `hud_window.py:432`).
pub const LOCKOUT_WARNING_MESSAGE: &str =
    "全域快捷鍵註冊失敗，已自動停用啟動時穿透模式以防視窗鎖死。\n您仍可由系統匣右鍵選單手動開啟。";

pub struct HotkeyManager {
    registration: HotkeyRegistration,
    /// Dropping it unregisters both hotkeys and joins the worker thread.
    _manager: Option<GlobalHotkeyManager>,
}

/// The modifier set and virtual key of a portable hotkey string like "Alt+C" or "Ctrl+Shift+H"
/// (`QKeySequence::fromString`). Anything that is not a single registrable chord falls back to
/// Alt+C.
fn resolve_hotkey(text: &str) -> (HotkeyModifiers, u32) {
    let parse = |s: &str| KeySequence::from_portable_string(s).to_hotkey();
    parse(text)
        .or_else(|| parse("Alt+C"))
        .expect("Alt+C is a registrable hotkey")
}

/// "Alt+Shift+C"-style text of a hotkey, for messages (`QKeySequence::toString(PortableText)`).
fn label(modifiers: HotkeyModifiers, vk: u32) -> String {
    KeySequence::from_hotkey(modifiers, vk)
        .map(|keys| keys.to_portable_string())
        .unwrap_or_else(|| format!("VK {vk:#04X}"))
}

/// Registers `id` and turns the outcome into the status Python keeps. Errors that are not about
/// this combination (a stopped worker, a duplicate id) are real failures of the manager.
fn register(
    manager: &GlobalHotkeyManager,
    id: u32,
    modifiers: HotkeyModifiers,
    vk: u32,
) -> Result<HotkeyStatus, String> {
    // `NO_REPEAT` is part of what conflicts with another registration of the combination.
    match manager.register(id, modifiers | HotkeyModifiers::NO_REPEAT, vk) {
        Ok(()) => Ok(HotkeyStatus::Registered),
        Err(HotkeyError::Os { code }) => Ok(HotkeyStatus::Failed(code)),
        Err(HotkeyError::Unsupported) => Ok(HotkeyStatus::Unsupported),
        Err(e) => Err(format!("hotkey registration failed: {e}")),
    }
}

impl HotkeyManager {
    /// Registration status of the global hotkeys. `start` returns once the registrations have
    /// been attempted, so this is final; a refused registration does not make `start` fail, since
    /// the other hotkey may still work (Python keeps it too).
    pub fn registration(&self) -> &HotkeyRegistration {
        &self.registration
    }

    /// Registers the toggle hotkey (`hotkey_str`) and the click-through hotkey (the same key with
    /// one more modifier). `on_press` runs on the manager's worker thread, never on the UI thread.
    pub fn start(
        hotkey_str: &str,
        on_press: impl Fn(HotkeyAction) + Send + Sync + 'static,
    ) -> Result<Self, String> {
        let manager = match GlobalHotkeyManager::new() {
            Ok(manager) => manager,
            Err(HotkeyError::Unsupported) => {
                warn!("[Hotkey] Global hotkeys not implemented on this platform");
                return Ok(Self {
                    registration: HotkeyRegistration::unconfirmed(),
                    _manager: None,
                });
            }
            Err(e) => return Err(format!("Failed to start hotkey thread: {e}")),
        };
        manager.set_handler(move |id| match id {
            HOTKEY_ID_TOGGLE => on_press(HotkeyAction::Toggle),
            HOTKEY_ID_CLICKTHROUGH => on_press(HotkeyAction::ClickThrough),
            _ => {}
        });

        let (mods, vk) = resolve_hotkey(hotkey_str);
        let toggle = register(&manager, HOTKEY_ID_TOGGLE, mods, vk)?;
        let toggle_label = label(mods, vk);
        match toggle {
            HotkeyStatus::Failed(_) => {
                warn!("[Hotkey] Main hotkey registration failed ({toggle_label}, {toggle:?})")
            }
            _ => info!("[Hotkey] Main hotkey registered ({toggle_label})"),
        }

        // Secondary hotkey: toggle click-through (distinct non-conflicting modifier)
        let ct_mods = compute_ct_mods(mods);
        let clickthrough = register(&manager, HOTKEY_ID_CLICKTHROUGH, ct_mods, vk)?;
        let clickthrough_label = label(ct_mods, vk);
        match clickthrough {
            HotkeyStatus::Failed(_) => warn!(
                "[Hotkey] Click-through hotkey registration failed ({clickthrough_label}, {clickthrough:?})"
            ),
            _ => info!("[Hotkey] Click-through hotkey registered ({clickthrough_label})"),
        }

        Ok(Self {
            registration: HotkeyRegistration {
                toggle,
                toggle_label,
                clickthrough,
                clickthrough_label,
            },
            _manager: Some(manager),
        })
    }
}

/// Computes modifiers for the secondary click-through hotkey such that it never conflicts
/// with the primary hotkey even when Shift, Ctrl, or Alt are already present.
pub fn compute_ct_mods(mods: HotkeyModifiers) -> HotkeyModifiers {
    if !mods.contains(HotkeyModifiers::SHIFT) {
        mods | HotkeyModifiers::SHIFT
    } else if !mods.contains(HotkeyModifiers::CONTROL) {
        mods | HotkeyModifiers::CONTROL
    } else if !mods.contains(HotkeyModifiers::ALT) {
        mods | HotkeyModifiers::ALT
    } else {
        mods ^ HotkeyModifiers::SHIFT // If all are selected, invert Shift
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALT: HotkeyModifiers = HotkeyModifiers::ALT;
    const CONTROL: HotkeyModifiers = HotkeyModifiers::CONTROL;
    const SHIFT: HotkeyModifiers = HotkeyModifiers::SHIFT;

    #[test]
    fn test_resolve_hotkey() {
        // Default "Alt+C"
        assert_eq!(resolve_hotkey("Alt+C"), (ALT, b'C' as u32));
        assert_eq!(
            resolve_hotkey("Ctrl+Shift+H"),
            (CONTROL | SHIFT, b'H' as u32)
        );
        // Function key "F12"
        assert_eq!(resolve_hotkey("F12"), (HotkeyModifiers::empty(), 0x70 + 11));
        // Not a registrable chord: falls back to Alt+C
        assert_eq!(resolve_hotkey("InvalidKeyString"), (ALT, b'C' as u32));
        assert_eq!(resolve_hotkey("Alt+C, Alt+D"), (ALT, b'C' as u32));
    }

    #[test]
    fn test_compute_ct_mods() {
        // 1. When Shift is absent, Shift is added
        assert_eq!(compute_ct_mods(ALT), ALT | SHIFT);
        assert_eq!(compute_ct_mods(CONTROL), CONTROL | SHIFT);

        // 2. When Shift is present, Control is added
        assert_eq!(compute_ct_mods(SHIFT | ALT), SHIFT | ALT | CONTROL);

        // 3. When Shift and Control are present, Alt is added (crucial fix for Ctrl+Shift+C)
        assert_eq!(compute_ct_mods(CONTROL | SHIFT), CONTROL | SHIFT | ALT);
        assert_ne!(compute_ct_mods(CONTROL | SHIFT), CONTROL | SHIFT);

        // 4. When all three are present, Shift is toggled off
        let all = ALT | CONTROL | SHIFT;
        assert_eq!(compute_ct_mods(all), ALT | CONTROL);
        assert_ne!(compute_ct_mods(all), all);
    }

    #[test]
    fn hotkey_labels_name_modifiers_then_key() {
        // Python shows "Alt+C" / "Alt+Shift+C" (python/system/hotkey.py:74,83).
        assert_eq!(label(ALT, b'C' as u32), "Alt+C");
        assert_eq!(label(ALT | SHIFT, b'C' as u32), "Alt+Shift+C");
        assert_eq!(
            label(CONTROL | SHIFT | ALT, 0x70 + 11),
            "Ctrl+Alt+Shift+F12"
        );
        assert_eq!(label(HotkeyModifiers::empty(), 0x20), "Space");
    }

    #[test]
    fn failure_notices_follow_python_order_and_text() {
        // python/system/hotkey.py:74,77,83,86: per failed hotkey, `hotkey_failed` then `unavailable`.
        let r = HotkeyRegistration {
            toggle: HotkeyStatus::Failed(1409),
            toggle_label: "Alt+C".into(),
            clickthrough: HotkeyStatus::Failed(5),
            clickthrough_label: "Alt+Shift+C".into(),
        };
        assert_eq!(
            r.failure_notices(),
            vec![
                "Alt+C 全域快捷鍵註冊失敗 (Win32 Error: 1409)，可能已被其他程式佔用",
                "Alt+C 已被占用，請使用系統匣操作",
                "Alt+Shift+C 穿透模式快捷鍵註冊失敗 (Win32 Error: 5)，可能已被其他程式佔用",
                "Alt+Shift+C 已被占用，請使用系統匣操作",
            ]
        );
    }

    #[test]
    fn click_through_needs_a_registered_click_through_hotkey() {
        let reg = |ct| HotkeyRegistration {
            toggle: HotkeyStatus::Registered,
            toggle_label: String::new(),
            clickthrough: ct,
            clickthrough_label: String::new(),
        };
        assert!(click_through_startup_allowed(Some(&reg(
            HotkeyStatus::Registered
        ))));
        assert!(!click_through_startup_allowed(Some(&reg(
            HotkeyStatus::Failed(1409)
        ))));
        assert!(!click_through_startup_allowed(Some(&reg(
            HotkeyStatus::Unsupported
        ))));
        assert!(
            !click_through_startup_allowed(None),
            "hotkeys disabled or not started"
        );
    }

    #[test]
    fn test_lockout_warning_message_matches_python() {
        // Python hud_window.py:432:
        // "全域快捷鍵註冊失敗，已自動停用啟動時穿透模式以防視窗鎖死。\n您仍可由系統匣右鍵選單手動開啟。"
        assert_eq!(
            LOCKOUT_WARNING_MESSAGE,
            "全域快捷鍵註冊失敗，已自動停用啟動時穿透模式以防視窗鎖死。\n您仍可由系統匣右鍵選單手動開啟。"
        );
    }

    #[test]
    fn test_unconfirmed_registration_is_degraded_not_failed() {
        let r = HotkeyRegistration::unconfirmed();
        assert!(!r.clickthrough_registered());
        assert!(r.failure_notices().is_empty());
    }

    // ---- registration result (RC-12), against the real `RegisterHotKey` ------------------------

    #[cfg(target_os = "windows")]
    mod registration {
        use super::*;

        const VK_F21: u32 = 0x70 + 20;
        const VK_F22: u32 = 0x70 + 21;
        const ERROR_HOTKEY_ALREADY_REGISTERED: u32 = HotkeyError::ERROR_HOTKEY_ALREADY_REGISTERED;

        /// Another registration of `mods + vk` in this process, held for the guard's lifetime: a
        /// second `GlobalHotkeyManager` holding the combination is what "the hotkey is taken by
        /// another program" looks like to `RegisterHotKey`.
        struct Squatter(GlobalHotkeyManager);

        impl Squatter {
            fn new(mods: HotkeyModifiers, vk: u32) -> Self {
                let manager = GlobalHotkeyManager::new().expect("test setup: worker thread");
                // Same flags as the HUD uses: `MOD_NOREPEAT` is part of what conflicts.
                manager
                    .register(1, mods | HotkeyModifiers::NO_REPEAT, vk)
                    .expect("test setup: combo is free");
                Squatter(manager)
            }
        }

        fn start(text: &str) -> HotkeyManager {
            HotkeyManager::start(text, |_| {}).expect("start still returns the manager")
        }

        #[test]
        fn start_reports_that_both_hotkeys_registered() {
            let hk = start("Ctrl+Alt+Shift+F23");
            let r = hk.registration();
            assert_eq!(r.toggle, HotkeyStatus::Registered);
            assert_eq!(r.clickthrough, HotkeyStatus::Registered);
            assert!(r.failure_notices().is_empty());
            assert!(click_through_startup_allowed(Some(r)));
        }

        #[test]
        fn start_reports_a_taken_toggle_hotkey_and_keeps_the_other() {
            // Toggle = Ctrl+Alt+Shift+F22 is taken; click-through = Ctrl+Alt+F22 is free.
            let _squat = Squatter::new(CONTROL | ALT | SHIFT, VK_F22);
            let hk = start("Ctrl+Alt+Shift+F22");
            let r = hk.registration();
            assert_eq!(
                r.toggle,
                HotkeyStatus::Failed(ERROR_HOTKEY_ALREADY_REGISTERED)
            );
            assert_eq!(
                r.clickthrough,
                HotkeyStatus::Registered,
                "an independent hotkey is not given up"
            );
            assert!(
                click_through_startup_allowed(Some(r)),
                "the click-through way out still exists"
            );
            let notices = r.failure_notices();
            assert_eq!(notices.len(), 2);
            assert_eq!(
                notices[0],
                "Ctrl+Alt+Shift+F22 全域快捷鍵註冊失敗 (Win32 Error: 1409)，可能已被其他程式佔用"
            );
        }

        #[test]
        fn a_taken_click_through_hotkey_disables_startup_click_through() {
            // Toggle = Ctrl+Alt+F21 is free; click-through = Ctrl+Alt+Shift+F21 is taken.
            let _squat = Squatter::new(CONTROL | ALT | SHIFT, VK_F21);
            let hk = start("Ctrl+Alt+F21");
            let r = hk.registration();
            assert_eq!(r.toggle, HotkeyStatus::Registered);
            assert_eq!(
                r.clickthrough,
                HotkeyStatus::Failed(ERROR_HOTKEY_ALREADY_REGISTERED)
            );
            assert!(!r.clickthrough_registered());
            assert!(
                !click_through_startup_allowed(Some(r)),
                "the window must not start click-through without a way back out"
            );
            assert_eq!(
                r.failure_notices()[0],
                "Ctrl+Alt+Shift+F21 穿透模式快捷鍵註冊失敗 (Win32 Error: 1409)，可能已被其他程式佔用"
            );
        }

        #[test]
        fn dropping_the_manager_frees_its_hotkeys() {
            // Ctrl+Alt+Shift+F20 / Ctrl+Alt+F20 are released once the manager is gone.
            drop(start("Ctrl+Alt+F20"));
            let again = start("Ctrl+Alt+F20");
            assert_eq!(again.registration().toggle, HotkeyStatus::Registered);
            assert_eq!(again.registration().clickthrough, HotkeyStatus::Registered);
        }
    }
}
