use qtrs_gui::geometry::primitives::{Point, Rect};

pub trait PlatformScreen: Send + Sync {
    fn name(&self) -> String;
    fn geometry(&self) -> Rect;
    fn available_geometry(&self) -> Rect;
    fn is_primary(&self) -> bool;
    fn device_pixel_ratio(&self) -> f32;
}

#[cfg(windows)]
pub mod win32_screen {
    use super::*;
    use std::ptr;
    use windows_sys::core::BOOL;
    use windows_sys::Win32::Foundation::{LPARAM, POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, HDC, HMONITOR, MONITORINFO,
        MONITORINFOEXW, MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

    #[derive(Debug, Clone)]
    pub struct Win32Screen {
        name: String,
        geometry: Rect,
        available_geometry: Rect,
        is_primary: bool,
        dpr: f32,
    }

    impl Win32Screen {
        pub fn from_hmonitor(h_monitor: HMONITOR) -> Option<Self> {
            if h_monitor.is_null() {
                return None;
            }

            unsafe {
                let mut info_ex: MONITORINFOEXW = std::mem::zeroed();
                info_ex.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

                if GetMonitorInfoW(h_monitor, &mut info_ex as *mut _ as *mut MONITORINFO) == 0 {
                    return None;
                }

                let rc = info_ex.monitorInfo.rcMonitor;
                let rc_work = info_ex.monitorInfo.rcWork;

                let geometry = Rect::new(
                    rc.left,
                    rc.top,
                    (rc.right - rc.left).max(0),
                    (rc.bottom - rc.top).max(0),
                );

                let available_geometry = Rect::new(
                    rc_work.left,
                    rc_work.top,
                    (rc_work.right - rc_work.left).max(0),
                    (rc_work.bottom - rc_work.top).max(0),
                );

                let is_primary = (info_ex.monitorInfo.dwFlags & 1) != 0;

                let mut name_len = 0;
                while name_len < info_ex.szDevice.len() && info_ex.szDevice[name_len] != 0 {
                    name_len += 1;
                }
                let name = String::from_utf16_lossy(&info_ex.szDevice[..name_len]);

                let mut dpi_x = 96u32;
                let mut dpi_y = 96u32;
                let hr = GetDpiForMonitor(h_monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
                let dpr = if hr == 0 {
                    dpi_x as f32 / 96.0
                } else {
                    windows_sys::Win32::UI::HiDpi::GetDpiForSystem() as f32 / 96.0
                };

                Some(Self {
                    name,
                    geometry,
                    available_geometry,
                    is_primary,
                    dpr: dpr.max(1.0),
                })
            }
        }

        pub fn primary() -> Self {
            let pt = POINT { x: 0, y: 0 };
            let h_monitor = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };
            if let Some(screen) = Self::from_hmonitor(h_monitor) {
                return screen;
            }

            unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::{
                    GetSystemMetrics, SystemParametersInfoW, SM_CXSCREEN, SM_CYSCREEN,
                    SPI_GETWORKAREA,
                };
                let width = GetSystemMetrics(SM_CXSCREEN);
                let height = GetSystemMetrics(SM_CYSCREEN);
                let mut work_area: RECT = std::mem::zeroed();
                SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work_area as *mut _ as *mut _, 0);
                let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForSystem();
                let dpr = (dpi as f32 / 96.0).max(1.0);

                Self {
                    name: "Primary Monitor".to_string(),
                    geometry: Rect::new(0, 0, width, height),
                    available_geometry: Rect::new(
                        work_area.left,
                        work_area.top,
                        (work_area.right - work_area.left).max(0),
                        (work_area.bottom - work_area.top).max(0),
                    ),
                    is_primary: true,
                    dpr,
                }
            }
        }

        pub fn all_screens() -> Vec<Self> {
            let mut list = Vec::<Self>::new();
            unsafe {
                unsafe extern "system" fn monitor_enum_proc(
                    h_monitor: HMONITOR,
                    _hdc: HDC,
                    _rect: *mut RECT,
                    lparam: LPARAM,
                ) -> BOOL {
                    let list_ref = &mut *(lparam as *mut Vec<Win32Screen>);
                    if let Some(screen) = Win32Screen::from_hmonitor(h_monitor) {
                        if screen.is_primary {
                            list_ref.insert(0, screen);
                        } else {
                            list_ref.push(screen);
                        }
                    }
                    1
                }

                EnumDisplayMonitors(
                    ptr::null_mut(),
                    ptr::null(),
                    Some(monitor_enum_proc),
                    &mut list as *mut _ as LPARAM,
                );
            }

            if list.is_empty() {
                list.push(Self::primary());
            }

            list
        }

        pub fn screen_at(pos: Point) -> Option<Self> {
            let pt = POINT { x: pos.x, y: pos.y };
            let h_monitor = unsafe { MonitorFromPoint(pt, 0) };
            if !h_monitor.is_null() {
                Self::from_hmonitor(h_monitor)
            } else {
                None
            }
        }
    }

    impl PlatformScreen for Win32Screen {
        fn name(&self) -> String {
            self.name.clone()
        }

        fn geometry(&self) -> Rect {
            if self.dpr > 1.0 {
                crate::high_dpi::from_native_rect(self.geometry, self.dpr)
            } else {
                self.geometry
            }
        }

        fn available_geometry(&self) -> Rect {
            if self.dpr > 1.0 {
                crate::high_dpi::from_native_rect(self.available_geometry, self.dpr)
            } else {
                self.available_geometry
            }
        }

        fn is_primary(&self) -> bool {
            self.is_primary
        }

        fn device_pixel_ratio(&self) -> f32 {
            self.dpr
        }
    }
}

#[cfg(windows)]
pub use win32_screen::Win32Screen;

#[derive(Debug, Clone)]
pub struct GenericScreen {
    name: String,
    geometry: Rect,
    available_geometry: Rect,
    is_primary: bool,
    dpr: f32,
}

impl GenericScreen {
    pub fn new(
        name: impl Into<String>,
        geometry: Rect,
        available_geometry: Rect,
        is_primary: bool,
        dpr: f32,
    ) -> Self {
        Self {
            name: name.into(),
            geometry,
            available_geometry,
            is_primary,
            dpr,
        }
    }

    pub fn default_primary() -> Self {
        Self {
            name: "DefaultScreen".to_string(),
            geometry: Rect::new(0, 0, 1920, 1080),
            available_geometry: Rect::new(0, 0, 1920, 1040),
            is_primary: true,
            dpr: 1.0,
        }
    }
}

impl PlatformScreen for GenericScreen {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn geometry(&self) -> Rect {
        self.geometry
    }

    fn available_geometry(&self) -> Rect {
        self.available_geometry
    }

    fn is_primary(&self) -> bool {
        self.is_primary
    }

    fn device_pixel_ratio(&self) -> f32 {
        self.dpr
    }
}

pub mod cocoa_screen {
    use super::*;
    use crate::objc_runtime::{Class, Id, ObjcMsg, Sel};
    use crate::window_cocoa::qt_mac_flip_rect;

    #[derive(Debug, Clone)]
    pub struct CocoaScreen {
        name: String,
        geometry: Rect,
        available_geometry: Rect,
        is_primary: bool,
        dpr: f32,
    }

    impl CocoaScreen {
        pub fn from_nsscreen(screen: Id, is_primary: bool, primary_height: i32) -> Self {
            let sel_frame = Sel::register("frame");
            let sel_visible_frame = Sel::register("visibleFrame");
            let sel_scale = Sel::register("backingScaleFactor");

            let frame = ObjcMsg::send_rect_return(screen, sel_frame);
            let visible_frame = ObjcMsg::send_rect_return(screen, sel_visible_frame);
            let scale = ObjcMsg::send_f64_return(screen, sel_scale);

            let raw_geom = Rect::new(
                frame.origin.x as i32,
                frame.origin.y as i32,
                frame.size.width as i32,
                frame.size.height as i32,
            );
            let raw_avail = Rect::new(
                visible_frame.origin.x as i32,
                visible_frame.origin.y as i32,
                visible_frame.size.width as i32,
                visible_frame.size.height as i32,
            );

            let geometry = qt_mac_flip_rect(raw_geom, primary_height);
            let available_geometry = qt_mac_flip_rect(raw_avail, primary_height);

            Self {
                name: if is_primary {
                    "CocoaPrimaryScreen".to_string()
                } else {
                    "CocoaSecondaryScreen".to_string()
                },
                geometry,
                available_geometry,
                is_primary,
                dpr: scale as f32,
            }
        }

        pub fn primary() -> Self {
            let nsscreen_class = Class::get("NSScreen").unwrap_or(Class::NIL);
            let main_screen = ObjcMsg::send_class_0(nsscreen_class, Sel::register("mainScreen"));
            if !main_screen.is_nil() {
                let frame = ObjcMsg::send_rect_return(main_screen, Sel::register("frame"));
                let primary_height = frame.size.height as i32;
                Self::from_nsscreen(main_screen, true, primary_height)
            } else {
                Self {
                    name: "CocoaPrimaryScreen".to_string(),
                    geometry: Rect::new(0, 0, 1920, 1080),
                    available_geometry: Rect::new(0, 25, 1920, 1055),
                    is_primary: true,
                    dpr: 2.0,
                }
            }
        }

        pub fn screens() -> Vec<Self> {
            let nsscreen_class = Class::get("NSScreen").unwrap_or(Class::NIL);
            let screens_array = ObjcMsg::send_class_0(nsscreen_class, Sel::register("screens"));
            if screens_array.is_nil() {
                return vec![Self::primary()];
            }

            let count = ObjcMsg::send_usize_return(screens_array, Sel::register("count"));
            if count == 0 {
                return vec![Self::primary()];
            }

            let first =
                ObjcMsg::send_id_with_usize(screens_array, Sel::register("objectAtIndex:"), 0);
            let first_frame = ObjcMsg::send_rect_return(first, Sel::register("frame"));
            let primary_height = first_frame.size.height as i32;

            let mut result = Vec::with_capacity(count);
            for i in 0..count {
                let screen_id =
                    ObjcMsg::send_id_with_usize(screens_array, Sel::register("objectAtIndex:"), i);
                if !screen_id.is_nil() {
                    result.push(Self::from_nsscreen(screen_id, i == 0, primary_height));
                }
            }

            if result.is_empty() {
                result.push(Self::primary());
            }

            result
        }

        pub fn screen_at(pos: Point) -> Option<Self> {
            Self::screens()
                .into_iter()
                .find(|s| s.geometry.contains(pos))
        }
    }

    impl PlatformScreen for CocoaScreen {
        fn name(&self) -> String {
            self.name.clone()
        }

        fn geometry(&self) -> Rect {
            self.geometry
        }

        fn available_geometry(&self) -> Rect {
            self.available_geometry
        }

        fn is_primary(&self) -> bool {
            self.is_primary
        }

        fn device_pixel_ratio(&self) -> f32 {
            self.dpr
        }
    }
}

pub use cocoa_screen::CocoaScreen;

pub mod x11_screen {
    use super::*;

    #[derive(Debug, Clone)]
    pub struct X11Screen {
        name: String,
        geometry: Rect,
        available_geometry: Rect,
        is_primary: bool,
        dpr: f32,
    }

    impl X11Screen {
        pub fn primary() -> Self {
            Self {
                name: "X11PrimaryScreen".to_string(),
                geometry: Rect::new(0, 0, 1920, 1080),
                available_geometry: Rect::new(0, 0, 1920, 1040),
                is_primary: true,
                dpr: 1.0,
            }
        }

        pub fn screens() -> Vec<Self> {
            vec![Self::primary()]
        }

        pub fn screen_at(pos: Point) -> Option<Self> {
            let primary = Self::primary();
            if primary.geometry.contains(pos) {
                Some(primary)
            } else {
                None
            }
        }
    }

    impl PlatformScreen for X11Screen {
        fn name(&self) -> String {
            self.name.clone()
        }

        fn geometry(&self) -> Rect {
            self.geometry
        }

        fn available_geometry(&self) -> Rect {
            self.available_geometry
        }

        fn is_primary(&self) -> bool {
            self.is_primary
        }

        fn device_pixel_ratio(&self) -> f32 {
            self.dpr
        }
    }
}

pub use x11_screen::X11Screen;

/// Clamps a candidate window rectangle so that it is guaranteed to be visible on an active screen.
///
/// Behavior:
/// 1. Finds the screen with the largest intersection with `rect`.
/// 2. If the intersection is non-empty, shifts and clamps the window inside that screen's `available_geometry`.
/// 3. If `rect` is completely off-screen (e.g. disconnected external monitor), safely positions
///    the window on the primary screen (or first available screen) within its `available_geometry`.
pub fn ensure_within_screens(
    rect: Rect,
    screens: &[&dyn PlatformScreen],
    min_visible_px: i32,
) -> Rect {
    if screens.is_empty() {
        return rect;
    }

    let min_visible = min_visible_px.max(1);

    // Find primary screen (fallback to first)
    let primary_screen = screens
        .iter()
        .find(|s| s.is_primary())
        .copied()
        .unwrap_or(screens[0]);

    // Find screen with largest intersection
    let mut best_screen: Option<&dyn PlatformScreen> = None;
    let mut best_intersection_area = 0i64;

    for &screen in screens {
        let avail = screen.available_geometry();
        // Calculate intersection of rect and avail
        let inter_x1 = rect.x.max(avail.x);
        let inter_y1 = rect.y.max(avail.y);
        let inter_x2 = (rect.x + rect.width).min(avail.x + avail.width);
        let inter_y2 = (rect.y + rect.height).min(avail.y + avail.height);

        let inter_w = (inter_x2 - inter_x1).max(0);
        let inter_h = (inter_y2 - inter_y1).max(0);

        if inter_w >= min_visible && inter_h >= min_visible {
            let area = (inter_w as i64) * (inter_h as i64);
            if area > best_intersection_area {
                best_intersection_area = area;
                best_screen = Some(screen);
            }
        }
    }

    let target_screen = best_screen.unwrap_or(primary_screen);
    let avail = target_screen.available_geometry();

    let clamped_w = rect.width.min(avail.width).max(10);
    let clamped_h = rect.height.min(avail.height).max(10);

    if best_screen.is_some() {
        // Clamp existing coordinates inside target available area
        let max_x = avail.x + (avail.width - clamped_w).max(0);
        let max_y = avail.y + (avail.height - clamped_h).max(0);
        let clamped_x = rect.x.clamp(avail.x, max_x);
        let clamped_y = rect.y.clamp(avail.y, max_y);
        Rect::new(clamped_x, clamped_y, clamped_w, clamped_h)
    } else {
        // Window was completely off-screen, reposition safely into primary available area
        let default_x = avail.x + ((avail.width - clamped_w) / 2).max(0);
        let default_y = avail.y + ((avail.height - clamped_h) / 4).max(0);
        Rect::new(default_x, default_y, clamped_w, clamped_h)
    }
}

/// Convenience helper to clamp a window rectangle to the active platform screens.
pub fn clamp_window_rect_to_screens(rect: Rect) -> Rect {
    let p = crate::integration::platform();
    let screens = p.screens();
    let screen_refs: Vec<&dyn PlatformScreen> = screens.iter().map(|s| s.as_ref()).collect();
    ensure_within_screens(rect, &screen_refs, 32)
}
