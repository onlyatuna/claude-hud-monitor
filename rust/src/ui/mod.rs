// src/ui/mod.rs — Qt HUD UI module

pub mod hud_window;
pub mod provider_card;
pub mod styles;
pub mod tray_icon;
pub mod usage_table;

#[cfg(target_os = "windows")]
pub fn enable_win32_dark_mode(hwnd: isize) {
    use std::ffi::{CString, OsStr};
    use std::os::windows::ffi::OsStrExt;
    use std::sync::LazyLock;

    type FnSetPreferredAppMode = unsafe extern "system" fn(i32) -> i32;
    type FnAllowDarkModeForWindow = unsafe extern "system" fn(isize, bool) -> bool;
    type FnFlushMenuThemes = unsafe extern "system" fn();

    struct DarkModeFns {
        set_preferred_app_mode: Option<FnSetPreferredAppMode>,
        allow_dark_for_window: Option<FnAllowDarkModeForWindow>,
        flush_menu_themes: Option<FnFlushMenuThemes>,
    }

    unsafe impl Send for DarkModeFns {}
    unsafe impl Sync for DarkModeFns {}

    static FNS: LazyLock<DarkModeFns> = LazyLock::new(|| {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetModuleHandleA(lpLibFileName: *const u8) -> isize;
            fn LoadLibraryA(lpLibFileName: *const u8) -> isize;
            fn GetProcAddress(hModule: isize, lpProcName: *const u8) -> usize;
        }

        unsafe {
            let uxtheme_name = CString::new("uxtheme.dll").unwrap();
            let mut uxtheme = GetModuleHandleA(uxtheme_name.as_ptr() as *const u8);
            if uxtheme == 0 {
                uxtheme = LoadLibraryA(uxtheme_name.as_ptr() as *const u8);
            }
            if uxtheme != 0 {
                let set_preferred_app_mode: Option<FnSetPreferredAppMode> =
                    std::mem::transmute(GetProcAddress(uxtheme, 135 as *const u8));
                let allow_dark_for_window: Option<FnAllowDarkModeForWindow> =
                    std::mem::transmute(GetProcAddress(uxtheme, 133 as *const u8));
                let flush_menu_themes: Option<FnFlushMenuThemes> =
                    std::mem::transmute(GetProcAddress(uxtheme, 136 as *const u8));
                DarkModeFns {
                    set_preferred_app_mode,
                    allow_dark_for_window,
                    flush_menu_themes,
                }
            } else {
                DarkModeFns {
                    set_preferred_app_mode: None,
                    allow_dark_for_window: None,
                    flush_menu_themes: None,
                }
            }
        }
    });

    #[link(name = "uxtheme")]
    extern "system" {
        fn SetWindowTheme(hWnd: isize, pszSubAppName: *const u16, pszSubIdList: *const u16) -> i32;
    }

    unsafe {
        if let Some(set_mode) = FNS.set_preferred_app_mode {
            set_mode(2);
        }
        if hwnd != 0 {
            if let Some(allow_win) = FNS.allow_dark_for_window {
                allow_win(hwnd, true);
            }
            let dark_theme: Vec<u16> = OsStr::new("DarkMode_Explorer\0").encode_wide().collect();
            let _ = SetWindowTheme(hwnd, dark_theme.as_ptr(), std::ptr::null());
        }
        if let Some(flush) = FNS.flush_menu_themes {
            flush();
        }
    }
}
