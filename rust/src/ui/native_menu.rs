// src/ui/native_menu.rs — Native Win32 popup context menu
//
// Solves the problem of in-window egui popup menus being clipped by the tiny 140px HUD window.
// Uses TrackPopupMenuEx to create a true OS-level popup menu that floats freely outside the window.

use crate::config::Config;
use eframe::egui;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    RefreshAll,
    SetLayoutHorizontal,
    SetLayoutVertical,
    ToggleClickThrough,
    ToggleAlwaysOnTop,
    ToggleLock,
    SetOpacity(u32),  // 100, 90, 80, 70, 50, 30
    SetInterval(u64), // 30, 60, 120, 300
    SetClaudeProfile(String),
    ToggleAutostart,
    OpenLogs,
    ResetGeometry,
    ToggleHide,
    Exit,
}

#[cfg(target_os = "windows")]
use std::collections::HashMap;
#[cfg(target_os = "windows")]
use std::sync::Mutex;

#[cfg(target_os = "windows")]
type MenuIconCache = Mutex<Option<HashMap<(usize, u32, u32), image::RgbaImage>>>;

#[cfg(target_os = "windows")]
static MENU_ICON_CACHE: MenuIconCache = Mutex::new(None);

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

        const ICON_CLAUDE: &[u8] = include_bytes!("../../assets/menu/menu_claude.png");
        const ICON_TARGET: &[u8] = include_bytes!("../../assets/menu/menu_target.png");
        const ICON_REFRESH: &[u8] = include_bytes!("../../assets/menu/menu_refresh.png");
        const ICON_LAYOUT: &[u8] = include_bytes!("../../assets/menu/menu_layout.png");
        const ICON_LAPTOP: &[u8] = include_bytes!("../../assets/menu/menu_laptop.png");
        const ICON_PHONE: &[u8] = include_bytes!("../../assets/menu/menu_phone.png");
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
        let mut bitmaps: Vec<isize> = Vec::with_capacity(32);

        let root = CreatePopupMenu();

        // 1. Refresh All
        let text = to_wide("立即重新整理所有 AI (Refresh All)");
        AppendMenuW(root, MF_STRING, 1001, text.as_ptr());
        attach_icon(root, 1001, false, ICON_REFRESH, false, cx, cy, &mut bitmaps);

        AppendMenuW(root, MF_SEPARATOR, 0, std::ptr::null());

        // Claude Accounts Submenu
        let claude_sub = CreatePopupMenu();
        let profiles = crate::providers::claude::discover_profiles();
        let is_auto = config.claude_profile == "auto";

        let (active_prof, _) =
            crate::providers::claude::resolve_active_profile(&config.claude_profile);
        let auto_title = if is_auto && active_prof.id != "default" {
            format!(
                "智慧自動追蹤 (目前: {})",
                active_prof.short_name
            )
        } else {
            "智慧自動追蹤 (最近活躍)".to_string()
        };
        let t_auto = to_wide(&auto_title);
        AppendMenuW(claude_sub, MF_STRING, 1300, t_auto.as_ptr());
        attach_icon(claude_sub, 1300, false, ICON_TARGET, is_auto, cx, cy, &mut bitmaps);

        AppendMenuW(claude_sub, MF_SEPARATOR, 0, std::ptr::null());

        for (i, p) in profiles.iter().take(40).enumerate() {
            let is_selected = !is_auto
                && (config.claude_profile == p.id
                    || (config.claude_profile == ".claude" && p.id == "default"));
            let prefix = if is_selected { "✓ " } else { "    " };
            let label = format!("{}{}", prefix, p.display_name);
            let t_p = to_wide(&label);
            AppendMenuW(claude_sub, MF_STRING, 1301 + i, t_p.as_ptr());
        }

        let t_claude = to_wide("Claude 帳號 (Claude Account)");
        AppendMenuW(root, MF_POPUP, claude_sub as usize, t_claude.as_ptr());
        attach_icon(root, 2, true, ICON_CLAUDE, false, cx, cy, &mut bitmaps);

        AppendMenuW(root, MF_SEPARATOR, 0, std::ptr::null());

        // 2. Layout Submenu
        let layout_sub = CreatePopupMenu();
        let is_horiz = config.layout_mode == "horizontal";
        let t_h = to_wide("橫向三欄並排 (Horizontal Triple)");
        let t_v = to_wide("直立三層堆疊 (Vertical Stack)");
        AppendMenuW(layout_sub, MF_STRING, 1002, t_h.as_ptr());
        attach_icon(
            layout_sub,
            1002,
            false,
            ICON_LAPTOP,
            is_horiz,
            cx,
            cy,
            &mut bitmaps,
        );
        AppendMenuW(layout_sub, MF_STRING, 1003, t_v.as_ptr());
        attach_icon(
            layout_sub,
            1003,
            false,
            ICON_PHONE,
            !is_horiz,
            cx,
            cy,
            &mut bitmaps,
        );

        let t_layout = to_wide("顯示佈局 (Layout)");
        AppendMenuW(root, MF_POPUP, layout_sub as usize, t_layout.as_ptr());
        attach_icon(root, 4, true, ICON_LAYOUT, false, cx, cy, &mut bitmaps);

        // 3. Click-through
        let t_ct = to_wide("滑鼠點擊穿透 (Alt+Shift+C)");
        AppendMenuW(root, MF_STRING, 1004, t_ct.as_ptr());
        attach_icon(
            root,
            1004,
            false,
            ICON_GHOST,
            config.click_through,
            cx,
            cy,
            &mut bitmaps,
        );

        // 4. Always on Top
        let t_aot = to_wide("視窗永遠置頂 (Always on Top)");
        AppendMenuW(root, MF_STRING, 1005, t_aot.as_ptr());
        attach_icon(
            root,
            1005,
            false,
            ICON_PIN,
            config.always_on_top,
            cx,
            cy,
            &mut bitmaps,
        );

        // 5. Lock position
        let t_lock = to_wide("鎖定視窗位置 (Lock Drag)");
        AppendMenuW(root, MF_STRING, 1006, t_lock.as_ptr());
        attach_icon(
            root,
            1006,
            false,
            ICON_LOCK,
            config.locked,
            cx,
            cy,
            &mut bitmaps,
        );

        // 6. Opacity Submenu
        let op_sub = CreatePopupMenu();
        let cur_op = (config.opacity * 100.0).round() as u32;
        let op_values = [100u32, 90, 80, 70, 50, 30];
        for (i, &val) in op_values.iter().enumerate() {
            let is_cur = (cur_op as i32 - val as i32).abs() < 5;
            let t = to_wide(&format!("{} {}%", if is_cur { "✓ " } else { "    " }, val));
            AppendMenuW(op_sub, MF_STRING, 1100 + i, t.as_ptr());
        }

        let t_op = to_wide("視窗透明度 (Opacity)");
        AppendMenuW(root, MF_POPUP, op_sub as usize, t_op.as_ptr());
        attach_icon(root, 8, true, ICON_OPACITY, false, cx, cy, &mut bitmaps);

        // 7. Interval Submenu
        let int_sub = CreatePopupMenu();
        let int_values = [30u64, 60, 120, 300];
        for (i, &sec) in int_values.iter().enumerate() {
            let is_cur = config.refresh_interval_sec == sec;
            let t = to_wide(&format!(
                "{} {} 秒",
                if is_cur { "✓ " } else { "    " },
                sec
            ));
            AppendMenuW(int_sub, MF_STRING, 1200 + i, t.as_ptr());
        }

        let t_int = to_wide("更新頻率 (Interval)");
        AppendMenuW(root, MF_POPUP, int_sub as usize, t_int.as_ptr());
        attach_icon(root, 9, true, ICON_TIMER, false, cx, cy, &mut bitmaps);

        // 8. Autostart
        let t_as = to_wide("開機自動啟動 (Start on Boot)");
        AppendMenuW(root, MF_STRING, 1007, t_as.as_ptr());
        attach_icon(
            root,
            1007,
            false,
            ICON_ROCKET,
            is_autostart,
            cx,
            cy,
            &mut bitmaps,
        );

        AppendMenuW(root, MF_SEPARATOR, 0, std::ptr::null());

        // 9. Open Logs
        let t_log = to_wide("開啟記錄檔目錄 (Open Logs)");
        AppendMenuW(root, MF_STRING, 1009, t_log.as_ptr());
        attach_icon(root, 1009, false, ICON_FOLDER, false, cx, cy, &mut bitmaps);

        // 10. Reset Geometry
        let t_reset = to_wide("重設預設尺寸與位置");
        AppendMenuW(root, MF_STRING, 1008, t_reset.as_ptr());
        attach_icon(root, 1008, false, ICON_RESET, false, cx, cy, &mut bitmaps);

        // 11. Hide HUD
        let t_hide = to_wide("隱藏 HUD (Alt+C 重新喚出)");
        AppendMenuW(root, MF_STRING, 1011, t_hide.as_ptr());
        attach_icon(root, 1011, false, ICON_EYE, false, cx, cy, &mut bitmaps);

        AppendMenuW(root, MF_SEPARATOR, 0, std::ptr::null());

        // 12. Exit
        let t_exit = to_wide("結束程式 (Exit)");
        AppendMenuW(root, MF_STRING, 1010, t_exit.as_ptr());
        attach_icon(root, 1010, false, ICON_EXIT, false, cx, cy, &mut bitmaps);

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
            1300 => Some(MenuAction::SetClaudeProfile("auto".into())),
            c if (1301..1350).contains(&c) => {
                let idx = (c - 1301) as usize;
                if idx < profiles.len() {
                    Some(MenuAction::SetClaudeProfile(profiles[idx].id.clone()))
                } else {
                    None
                }
            }
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
fn draw_checkmark(dest: &mut [u8], stride: usize, cx: u32, cy: u32) {
    let scale = cx as f32 / 16.0;
    let ax = 2.5 * scale;
    let ay = 8.5 * scale;
    let bx = 6.0 * scale;
    let by = 12.0 * scale;
    let cx2 = 13.5 * scale;
    let cy2 = 4.0 * scale;
    let radius = 1.15 * scale;

    let dist_seg = |px: f32, py: f32, x1: f32, y1: f32, x2: f32, y2: f32| -> f32 {
        let dx = x2 - x1;
        let dy = y2 - y1;
        let len2 = dx * dx + dy * dy;
        let t = (((px - x1) * dx + (py - y1) * dy) / len2).clamp(0.0, 1.0);
        let qx = x1 + t * dx;
        let qy = y1 + t * dy;
        ((px - qx) * (px - qx) + (py - qy) * (py - qy)).sqrt()
    };

    for y in 0..cy {
        for x in 0..cx {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let d1 = dist_seg(px, py, ax, ay, bx, by);
            let d2 = dist_seg(px, py, bx, by, cx2, cy2);
            let d = d1.min(d2);
            let cov = (1.0 - (d - (radius - 0.5))).clamp(0.0, 1.0);
            if cov > 0.0 {
                let alpha = (cov * 255.0).round() as u8;
                let offset = y as usize * stride + x as usize * 4;
                // PARGB: white (255, 255, 255) with alpha
                dest[offset] = alpha; // Blue
                dest[offset + 1] = alpha; // Green
                dest[offset + 2] = alpha; // Red
                dest[offset + 3] = alpha; // Alpha
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn create_menu_pargb_bitmap(png_bytes: &[u8], checked: bool, cx: u32, cy: u32) -> Option<isize> {
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

    let gap = (4.0 * (cx as f32 / 16.0)).round().max(2.0) as u32;
    let total_w = cx + gap + cx;
    let total_h = cy;

    let key = (png_bytes.as_ptr() as usize, cx, cy);
    let rgba = {
        let mut cache = MENU_ICON_CACHE.lock().unwrap();
        let map = cache.get_or_insert_with(HashMap::new);
        if let Some(cached) = map.get(&key) {
            cached.clone()
        } else {
            let img = image::load_from_memory(png_bytes).ok()?;
            let resized = img.resize_exact(cx, cy, image::imageops::FilterType::Lanczos3);
            let r = resized.to_rgba8();
            map.insert(key, r.clone());
            r
        }
    };

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: total_w as i32,
            biHeight: -(total_h as i32), // Top-down DIB
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0, // BI_RGB
            biSizeImage: total_w * total_h * 4,
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
        let stride = (total_w * 4) as usize;
        let dest = std::slice::from_raw_parts_mut(bits_ptr, (total_w * total_h * 4) as usize);
        dest.fill(0);

        // 1. Antialiased white checkmark on the left (0..cx)
        if checked {
            draw_checkmark(dest, stride, cx, cy);
        }

        // 2. Icon on the right (cx + gap .. total_w)
        let icon_x_offset = (cx + gap) as usize;
        let src = rgba.as_raw();
        for y in 0..cy as usize {
            for x in 0..cx as usize {
                let src_idx = (y * cx as usize + x) * 4;
                let r = src[src_idx] as u32;
                let g = src[src_idx + 1] as u32;
                let b = src[src_idx + 2] as u32;
                let a = src[src_idx + 3] as u32;

                if a > 0 {
                    let pr = ((r * a + 127) / 255) as u8;
                    let pg = ((g * a + 127) / 255) as u8;
                    let pb = ((b * a + 127) / 255) as u8;

                    let dst_idx = y * stride + (icon_x_offset + x) * 4;
                    dest[dst_idx] = pb; // Blue
                    dest[dst_idx + 1] = pg; // Green
                    dest[dst_idx + 2] = pr; // Red
                    dest[dst_idx + 3] = a as u8; // Alpha
                }
            }
        }
    }

    Some(hbmp)
}

#[cfg(target_os = "windows")]
#[allow(clippy::too_many_arguments)]
fn attach_icon(
    hmenu: isize,
    id_or_pos: u32,
    by_position: bool,
    png_bytes: &[u8],
    checked: bool,
    cx: u32,
    cy: u32,
    bitmaps: &mut Vec<isize>,
) {
    if let Some(hbmp) = create_menu_pargb_bitmap(png_bytes, checked, cx, cy) {
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
#[allow(dead_code)]
pub fn show_native_context_menu(
    _hwnd: isize,
    _config: &Config,
    _is_autostart: bool,
) -> Option<MenuAction> {
    None
}

/// Cross-platform egui context menu rendering fallback (used on macOS/Linux or embedded menus)
#[allow(dead_code)]
pub fn render_context_menu_items(
    ui: &mut egui::Ui,
    cfg: &Config,
    is_autostart: bool,
) -> Option<MenuAction> {
    let mut selected: Option<MenuAction> = None;

    if ui.button("🔄 立即重新整理").clicked() {
        selected = Some(MenuAction::RefreshAll);
        ui.close_menu();
    }
    ui.separator();

    let profiles = crate::providers::claude::discover_profiles();
    let is_auto = cfg.claude_profile == "auto";
    let (active_prof, _) = crate::providers::claude::resolve_active_profile(&cfg.claude_profile);

    ui.menu_button("👤 Claude 帳號", |ui| {
        let auto_label = if is_auto {
            if active_prof.id != "default" {
                format!("✔ 🎯 智慧自動追蹤 (目前: {})", active_prof.short_name)
            } else {
                "✔ 🎯 智慧自動追蹤 (最近活躍)".to_string()
            }
        } else {
            "   🎯 智慧自動追蹤 (最近活躍)".to_string()
        };

        if ui.button(auto_label).clicked() {
            selected = Some(MenuAction::SetClaudeProfile("auto".to_string()));
            ui.close_menu();
        }

        ui.separator();

        for p in &profiles {
            let is_sel = !is_auto
                && (cfg.claude_profile == p.id
                    || (cfg.claude_profile == ".claude" && p.id == "default"));
            let label = if is_sel {
                format!("✔ {}", p.display_name)
            } else {
                format!("   {}", p.display_name)
            };
            if ui.button(label).clicked() {
                selected = Some(MenuAction::SetClaudeProfile(p.id.clone()));
                ui.close_menu();
            }
        }
    });

    ui.separator();

    ui.menu_button("📐 佈局模式", |ui| {
        let is_horiz = cfg.layout_mode == "horizontal";
        let is_vert = cfg.layout_mode == "vertical";
        let horiz_label = if is_horiz {
            "✔ 水平排列 (Horizontal)"
        } else {
            "   水平排列 (Horizontal)"
        };
        let vert_label = if is_vert {
            "✔ 垂直排列 (Vertical)"
        } else {
            "   垂直排列 (Vertical)"
        };

        if ui.button(horiz_label).clicked() {
            selected = Some(MenuAction::SetLayoutHorizontal);
            ui.close_menu();
        }
        if ui.button(vert_label).clicked() {
            selected = Some(MenuAction::SetLayoutVertical);
            ui.close_menu();
        }
    });

    let aot_label = if cfg.always_on_top {
        "✔ 置頂顯示"
    } else {
        "   置頂顯示"
    };
    if ui.button(aot_label).clicked() {
        selected = Some(MenuAction::ToggleAlwaysOnTop);
        ui.close_menu();
    }

    let ct_label = if cfg.click_through {
        "✔ 點擊穿透 (Ghost)"
    } else {
        "   點擊穿透 (Ghost)"
    };
    if ui.button(ct_label).clicked() {
        selected = Some(MenuAction::ToggleClickThrough);
        ui.close_menu();
    }

    let lock_label = if cfg.locked {
        "✔ 鎖定視窗位置"
    } else {
        "   鎖定視窗位置"
    };
    if ui.button(lock_label).clicked() {
        selected = Some(MenuAction::ToggleLock);
        ui.close_menu();
    }

    ui.separator();

    ui.menu_button("🌫 不透明度", |ui| {
        let op = (cfg.opacity * 100.0).round() as u32;
        let levels = [100, 90, 80, 70, 50, 30];
        for lvl in levels {
            let label = if (op as i32 - lvl as i32).abs() <= 5 {
                format!("✔ {}%", lvl)
            } else {
                format!("   {}%", lvl)
            };
            if ui.button(label).clicked() {
                selected = Some(MenuAction::SetOpacity(lvl));
                ui.close_menu();
            }
        }
    });

    ui.menu_button("⏱ 更新頻率", |ui| {
        let cur_int = cfg.refresh_interval_sec;
        let intervals = [
            (30, "30 秒"),
            (60, "60 秒 (預設)"),
            (120, "2 分鐘"),
            (300, "5 分鐘"),
        ];
        for (sec, text) in intervals {
            let label = if cur_int == sec {
                format!("✔ {}", text)
            } else {
                format!("   {}", text)
            };
            if ui.button(label).clicked() {
                selected = Some(MenuAction::SetInterval(sec));
                ui.close_menu();
            }
        }
    });

    let as_label = if is_autostart {
        "✔ 開機自動啟動"
    } else {
        "   開機自動啟動"
    };
    if ui.button(as_label).clicked() {
        selected = Some(MenuAction::ToggleAutostart);
        ui.close_menu();
    }

    ui.separator();

    if ui.button("📏 重設視窗大小").clicked() {
        selected = Some(MenuAction::ResetGeometry);
        ui.close_menu();
    }

    if ui.button("📂 開啟記錄檔目錄").clicked() {
        selected = Some(MenuAction::OpenLogs);
        ui.close_menu();
    }

    ui.separator();

    if ui.button("❌ 結束程式").clicked() {
        selected = Some(MenuAction::Exit);
        ui.close_menu();
    }

    selected
}
