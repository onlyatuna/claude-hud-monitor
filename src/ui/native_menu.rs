// src/ui/native_menu.rs — Native Win32 popup context menu
//
// Solves the problem of in-window egui popup menus being clipped by the tiny 140px HUD window.
// Uses TrackPopupMenuEx to create a true OS-level popup menu that floats freely outside the window.

use crate::config::Config;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    RefreshAll,
    SetLayoutHorizontal,
    SetLayoutVertical,
    ToggleClickThrough,
    ToggleAlwaysOnTop,
    ToggleLock,
    SetOpacity(u32),  // 100, 90, 80, 70, 50, 30
    SetInterval(u64), // 30, 60, 120, 300
    ToggleAutostart,
    OpenLogs,
    ResetGeometry,
    ToggleHide,
    Exit,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct POINT {
    x: i32,
    y: i32,
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[allow(non_snake_case)]
struct MENUITEMINFOW {
    cbSize: u32,
    fMask: u32,
    fType: u32,
    fState: u32,
    wID: u32,
    hSubMenu: isize,
    hbmpChecked: isize,
    hbmpUnchecked: isize,
    dwItemData: usize,
    dwTypeData: *mut u16,
    cch: u32,
    hbmpItem: isize,
}

#[cfg(target_os = "windows")]
#[link(name = "user32")]
extern "system" {
    fn CreatePopupMenu() -> isize;
    fn AppendMenuW(hMenu: isize, uFlags: u32, uIDNewItem: usize, lpNewItem: *const u16) -> i32;
    fn TrackPopupMenuEx(
        hMenu: isize,
        uFlags: u32,
        x: i32,
        y: i32,
        hWnd: isize,
        lptpm: *const std::ffi::c_void,
    ) -> i32;
    fn DestroyMenu(hMenu: isize) -> i32;
    fn GetCursorPos(lpPoint: *mut POINT) -> i32;
    fn GetForegroundWindow() -> isize;
    fn SetForegroundWindow(hWnd: isize) -> i32;
    fn PostMessageW(hWnd: isize, Msg: u32, wParam: usize, lParam: isize) -> i32;
    fn SetMenuItemInfoW(
        hMenu: isize,
        item: u32,
        fByPosition: i32,
        lpmii: *const MENUITEMINFOW,
    ) -> i32;
    fn GetSystemMetrics(nIndex: i32) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "gdi32")]
extern "system" {
    fn DeleteObject(ho: isize) -> i32;
}

#[cfg(target_os = "windows")]
pub fn show_native_context_menu(
    hwnd: isize,
    config: &Config,
    is_autostart: bool,
) -> Option<MenuAction> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    fn to_wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    const TPM_RETURNCMD: u32 = 0x0100;
    const TPM_NONOTIFY: u32 = 0x0080;
    const TPM_RIGHTBUTTON: u32 = 0x0002;

    const MF_STRING: u32 = 0x00000000;
    const MF_POPUP: u32 = 0x00000010;
    const MF_SEPARATOR: u32 = 0x00000800;

    unsafe {
        let target_hwnd = if hwnd != 0 {
            hwnd
        } else {
            GetForegroundWindow()
        };
        if target_hwnd != 0 {
            SetForegroundWindow(target_hwnd);
            enable_win32_dark_mode(target_hwnd);
        }

        const ICON_REFRESH: &[u8] = include_bytes!("../../assets/menu/menu_refresh.png");
        const ICON_LAYOUT: &[u8] = include_bytes!("../../assets/menu/menu_layout.png");
        const ICON_GHOST: &[u8] = include_bytes!("../../assets/ghost.png");
        const ICON_PIN: &[u8] = include_bytes!("../../assets/menu/menu_pin.png");
        const ICON_LOCK: &[u8] = include_bytes!("../../assets/menu/menu_lock.png");
        const ICON_OPACITY: &[u8] = include_bytes!("../../assets/menu/menu_opacity.png");
        const ICON_TIMER: &[u8] = include_bytes!("../../assets/menu/menu_timer.png");
        const ICON_ROCKET: &[u8] = include_bytes!("../../assets/menu/menu_rocket.png");
        const ICON_FOLDER: &[u8] = include_bytes!("../../assets/menu/menu_folder.png");
        const ICON_RESET: &[u8] = include_bytes!("../../assets/menu/menu_reset.png");
        const ICON_EYE: &[u8] = include_bytes!("../../assets/menu/menu_eye.png");
        const ICON_EXIT: &[u8] = include_bytes!("../../assets/menu/menu_exit.png");

        const SM_CXSMICON: i32 = 49;
        const SM_CYSMICON: i32 = 50;
        let cx = GetSystemMetrics(SM_CXSMICON).max(16) as u32;
        let cy = GetSystemMetrics(SM_CYSMICON).max(16) as u32;
        let mut bitmaps: Vec<isize> = Vec::with_capacity(16);

        let root = CreatePopupMenu();

        // 1. Refresh All
        let text = to_wide("    立即重新整理所有 AI (Refresh All)");
        AppendMenuW(root, MF_STRING, 1001, text.as_ptr());
        attach_icon(root, 1001, false, ICON_REFRESH, cx, cy, &mut bitmaps);

        AppendMenuW(root, MF_SEPARATOR, 0, std::ptr::null());

        // 2. Layout Submenu
        let layout_sub = CreatePopupMenu();
        let is_horiz = config.layout_mode == "horizontal";
        let t_h = to_wide(&format!(
            "{}橫向三欄並排 (Horizontal Triple)",
            if is_horiz { "✓  " } else { "    " }
        ));
        let t_v = to_wide(&format!(
            "{}直立三層堆疊 (Vertical Stack)",
            if !is_horiz { "✓  " } else { "    " }
        ));
        AppendMenuW(layout_sub, MF_STRING, 1002, t_h.as_ptr());
        AppendMenuW(layout_sub, MF_STRING, 1003, t_v.as_ptr());

        let t_layout = to_wide("    顯示佈局 (Layout)");
        AppendMenuW(root, MF_POPUP, layout_sub as usize, t_layout.as_ptr());
        attach_icon(root, 2, true, ICON_LAYOUT, cx, cy, &mut bitmaps);

        // 3. Click-through
        let t_ct = to_wide(&format!(
            "{}滑鼠點擊穿透 (Alt+Shift+C)",
            if config.click_through {
                "✓  "
            } else {
                "    "
            }
        ));
        AppendMenuW(root, MF_STRING, 1004, t_ct.as_ptr());
        attach_icon(root, 1004, false, ICON_GHOST, cx, cy, &mut bitmaps);

        // 4. Always on Top
        let t_aot = to_wide(&format!(
            "{}視窗永遠置頂 (Always on Top)",
            if config.always_on_top {
                "✓  "
            } else {
                "    "
            }
        ));
        AppendMenuW(root, MF_STRING, 1005, t_aot.as_ptr());
        attach_icon(root, 1005, false, ICON_PIN, cx, cy, &mut bitmaps);

        // 5. Lock position
        let t_lock = to_wide(&format!(
            "{}鎖定視窗位置 (Lock Drag)",
            if config.locked { "✓  " } else { "    " }
        ));
        AppendMenuW(root, MF_STRING, 1006, t_lock.as_ptr());
        attach_icon(root, 1006, false, ICON_LOCK, cx, cy, &mut bitmaps);

        // 6. Opacity Submenu
        let op_sub = CreatePopupMenu();
        let cur_op = (config.opacity * 100.0).round() as u32;
        let op_values = [100u32, 90, 80, 70, 50, 30];
        for (i, &val) in op_values.iter().enumerate() {
            let is_cur = (cur_op as i32 - val as i32).abs() < 5;
            let t = to_wide(&format!("{}{}%", if is_cur { "✓  " } else { "    " }, val));
            AppendMenuW(op_sub, MF_STRING, 1100 + i, t.as_ptr());
        }

        let t_op = to_wide("    視窗透明度 (Opacity)");
        AppendMenuW(root, MF_POPUP, op_sub as usize, t_op.as_ptr());
        attach_icon(root, 6, true, ICON_OPACITY, cx, cy, &mut bitmaps);

        // 7. Interval Submenu
        let int_sub = CreatePopupMenu();
        let int_values = [30u64, 60, 120, 300];
        for (i, &sec) in int_values.iter().enumerate() {
            let is_cur = config.refresh_interval_sec == sec;
            let t = to_wide(&format!(
                "{}{} 秒",
                if is_cur { "✓  " } else { "    " },
                sec
            ));
            AppendMenuW(int_sub, MF_STRING, 1200 + i, t.as_ptr());
        }

        let t_int = to_wide("    更新頻率 (Interval)");
        AppendMenuW(root, MF_POPUP, int_sub as usize, t_int.as_ptr());
        attach_icon(root, 7, true, ICON_TIMER, cx, cy, &mut bitmaps);

        // 8. Autostart
        let t_as = to_wide(&format!(
            "{}開機自動啟動 (Start on Boot)",
            if is_autostart { "✓  " } else { "    " }
        ));
        AppendMenuW(root, MF_STRING, 1007, t_as.as_ptr());
        attach_icon(root, 1007, false, ICON_ROCKET, cx, cy, &mut bitmaps);

        AppendMenuW(root, MF_SEPARATOR, 0, std::ptr::null());

        // 9. Open Logs
        let t_log = to_wide("    開啟記錄檔目錄 (Open Logs)");
        AppendMenuW(root, MF_STRING, 1009, t_log.as_ptr());
        attach_icon(root, 1009, false, ICON_FOLDER, cx, cy, &mut bitmaps);

        // 10. Reset Geometry
        let t_reset = to_wide("    重設預設尺寸與位置");
        AppendMenuW(root, MF_STRING, 1008, t_reset.as_ptr());
        attach_icon(root, 1008, false, ICON_RESET, cx, cy, &mut bitmaps);

        // 11. Hide HUD
        let t_hide = to_wide("    隱藏 HUD (Alt+C 重新喚出)");
        AppendMenuW(root, MF_STRING, 1011, t_hide.as_ptr());
        attach_icon(root, 1011, false, ICON_EYE, cx, cy, &mut bitmaps);

        AppendMenuW(root, MF_SEPARATOR, 0, std::ptr::null());

        // 12. Exit
        let t_exit = to_wide("    結束程式 (Exit)");
        AppendMenuW(root, MF_STRING, 1010, t_exit.as_ptr());
        attach_icon(root, 1010, false, ICON_EXIT, cx, cy, &mut bitmaps);

        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);

        let cmd = TrackPopupMenuEx(
            root,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
            pt.x,
            pt.y,
            target_hwnd,
            std::ptr::null(),
        );

        DestroyMenu(root);
        for hbmp in bitmaps {
            DeleteObject(hbmp);
        }
        if target_hwnd != 0 {
            PostMessageW(target_hwnd, 0, 0, 0);
        }

        match cmd {
            1001 => Some(MenuAction::RefreshAll),
            1002 => Some(MenuAction::SetLayoutHorizontal),
            1003 => Some(MenuAction::SetLayoutVertical),
            1004 => Some(MenuAction::ToggleClickThrough),
            1005 => Some(MenuAction::ToggleAlwaysOnTop),
            1006 => Some(MenuAction::ToggleLock),
            1007 => Some(MenuAction::ToggleAutostart),
            1008 => Some(MenuAction::ResetGeometry),
            1009 => Some(MenuAction::OpenLogs),
            1010 => Some(MenuAction::Exit),
            1011 => Some(MenuAction::ToggleHide),
            c if (1100..1110).contains(&c) => {
                let idx = (c - 1100) as usize;
                if idx < op_values.len() {
                    Some(MenuAction::SetOpacity(op_values[idx]))
                } else {
                    None
                }
            }
            c if (1200..1210).contains(&c) => {
                let idx = (c - 1200) as usize;
                if idx < int_values.len() {
                    Some(MenuAction::SetInterval(int_values[idx]))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

#[cfg(target_os = "windows")]
pub fn enable_win32_dark_mode(hwnd: isize) {
    use std::ffi::{CString, OsStr};
    use std::os::windows::ffi::OsStrExt;
    use std::sync::OnceLock;

    type FnSetPreferredAppMode = unsafe extern "system" fn(i32) -> i32;
    type FnAllowDarkModeForWindow = unsafe extern "system" fn(isize, bool) -> bool;
    type FnFlushMenuThemes = unsafe extern "system" fn();

    struct DarkModeFns {
        set_preferred_app_mode: Option<FnSetPreferredAppMode>,
        allow_dark_for_window: Option<FnAllowDarkModeForWindow>,
        flush_menu_themes: Option<FnFlushMenuThemes>,
    }

    static FNS: OnceLock<DarkModeFns> = OnceLock::new();

    let fns = FNS.get_or_init(|| {
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
        if let Some(set_mode) = fns.set_preferred_app_mode {
            set_mode(2);
        }
        if hwnd != 0 {
            if let Some(allow_win) = fns.allow_dark_for_window {
                allow_win(hwnd, true);
            }
            let dark_theme: Vec<u16> = OsStr::new("DarkMode_Explorer\0").encode_wide().collect();
            let _ = SetWindowTheme(hwnd, dark_theme.as_ptr(), std::ptr::null());
        }
        if let Some(flush) = fns.flush_menu_themes {
            flush();
        }
    }
}

#[cfg(target_os = "windows")]
fn create_menu_pargb_bitmap(png_bytes: &[u8], cx: u32, cy: u32) -> Option<isize> {
    #[repr(C)]
    #[allow(non_snake_case)]
    struct BITMAPINFOHEADER {
        biSize: u32,
        biWidth: i32,
        biHeight: i32,
        biPlanes: u16,
        biBitCount: u16,
        biCompression: u32,
        biSizeImage: u32,
        biXPelsPerMeter: i32,
        biYPelsPerMeter: i32,
        biClrUsed: u32,
        biClrImportant: u32,
    }
    #[repr(C)]
    #[allow(non_snake_case)]
    struct BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER,
        bmiColors: [u32; 1],
    }
    #[link(name = "gdi32")]
    extern "system" {
        fn CreateDIBSection(
            hdc: isize,
            pbmi: *const BITMAPINFO,
            usage: u32,
            ppvBits: *mut *mut u8,
            hSection: isize,
            offset: u32,
        ) -> isize;
    }

    let img = image::load_from_memory(png_bytes).ok()?;
    let resized = img.resize_exact(cx, cy, image::imageops::FilterType::Lanczos3);
    let rgba = resized.to_rgba8();

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: cx as i32,
            biHeight: -(cy as i32), // Top-down DIB
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0, // BI_RGB
            biSizeImage: cx * cy * 4,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [0],
    };

    let mut bits_ptr: *mut u8 = std::ptr::null_mut();
    let hbmp = unsafe {
        CreateDIBSection(
            0,
            &bmi,
            0, // DIB_RGB_COLORS
            &mut bits_ptr,
            0,
            0,
        )
    };

    if hbmp == 0 || bits_ptr.is_null() {
        return None;
    }

    unsafe {
        let dest = std::slice::from_raw_parts_mut(bits_ptr, (cx * cy * 4) as usize);
        let src = rgba.as_raw();
        for i in 0..(cx * cy) as usize {
            let r = src[i * 4] as u32;
            let g = src[i * 4 + 1] as u32;
            let b = src[i * 4 + 2] as u32;
            let a = src[i * 4 + 3] as u32;

            let pr = ((r * a + 127) / 255) as u8;
            let pg = ((g * a + 127) / 255) as u8;
            let pb = ((b * a + 127) / 255) as u8;

            dest[i * 4] = pb; // Blue
            dest[i * 4 + 1] = pg; // Green
            dest[i * 4 + 2] = pr; // Red
            dest[i * 4 + 3] = a as u8; // Alpha
        }
    }

    Some(hbmp)
}

#[cfg(target_os = "windows")]
fn attach_icon(
    hmenu: isize,
    id_or_pos: u32,
    by_position: bool,
    png_bytes: &[u8],
    cx: u32,
    cy: u32,
    bitmaps: &mut Vec<isize>,
) {
    if let Some(hbmp) = create_menu_pargb_bitmap(png_bytes, cx, cy) {
        unsafe {
            let mut mii = std::mem::zeroed::<MENUITEMINFOW>();
            mii.cbSize = std::mem::size_of::<MENUITEMINFOW>() as u32;
            mii.fMask = 0x00000080; // MIIM_BITMAP
            mii.hbmpItem = hbmp;
            SetMenuItemInfoW(hmenu, id_or_pos, if by_position { 1 } else { 0 }, &mii);
        }
        bitmaps.push(hbmp);
    }
}

#[cfg(not(target_os = "windows"))]
pub fn show_native_context_menu(
    _hwnd: isize,
    _config: &Config,
    _is_autostart: bool,
) -> Option<MenuAction> {
    None
}
