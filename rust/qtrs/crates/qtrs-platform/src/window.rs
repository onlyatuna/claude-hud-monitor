#[allow(unused_imports)]
use crate::window_system_interface::{
    Delivery, KeyboardModifiers, MouseButton, WheelDelta, WindowSystemEvent,
    WindowSystemEventHandler,
};
#[allow(unused_imports)]
use qtrs_core::event::{Event, EventKind};
use qtrs_core::event_loop::EventLoopHandle;
use qtrs_core::object::ObjectId;
use qtrs_gui::geometry::primitives::{Rect, Size};
#[allow(unused_imports)]
use crate::presenter::SurfacePresenter;
#[allow(unused_imports)]
use crate::surface::PlatformSurface;
use std::collections::HashMap;
#[allow(unused_imports)]
use std::ptr;
#[allow(unused_imports)]
use std::sync::{Mutex, Once, RwLock};
#[cfg(windows)]
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
#[cfg(not(windows))]
pub use crate::HWND;
#[cfg(windows)]
use windows_sys::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
#[cfg(windows)]
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
#[cfg(windows)]
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(windows)]
use windows_sys::Win32::UI::Controls::{MARGINS, WM_MOUSELEAVE};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, TrackMouseEvent, TRACKMOUSEEVENT, TME_LEAVE, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos, GetWindowRect, IsZoomed,
    RegisterClassExW, SetWindowPos, ShowWindow,
    HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION,
    HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, NCCALCSIZE_PARAMS, SWP_FRAMECHANGED,
    CS_DBLCLKS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_SHOW, SW_SHOWNOACTIVATE, WM_CLOSE,
    WM_CONTEXTMENU, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_ERASEBKGND, WM_GETMINMAXINFO,
    WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDBLCLK, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_MOVE,
    WM_NCCALCSIZE, WM_NCHITTEST, WM_PAINT, WM_RBUTTONDBLCLK, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_SETCURSOR, WM_SETFOCUS, WM_SETTINGCHANGE, WM_SHOWWINDOW, WM_SIZE, WM_THEMECHANGED,
    WNDCLASSEXW, IDC_ARROW, LoadCursorW,
    WS_CAPTION, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_MAXIMIZEBOX, WS_MINIMIZEBOX,
    WS_OVERLAPPEDWINDOW, WS_POPUP, WS_THICKFRAME, GetForegroundWindow, IsChild,
};
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct WindowFlags: u32 {
        const NORMAL                 = 0;
        const FRAMELESS              = 1 << 0;
        const STAYS_ON_TOP           = 1 << 1;
        const TOOL                   = 1 << 2;
        const LAYERED                = 1 << 3;
        const CLICK_THROUGH          = 1 << 4;
        const CUSTOM_FRAMELESS       = 1 << 5;
        /// `Qt::ToolTip`: a frameless, always-on-top tool window that is shown without taking
        /// activation (`WS_EX_NOACTIVATE` + `SW_SHOWNOACTIVATE`, `qwindowswindow.cpp:799-815,
        /// 1019, 2031-2036`). It never becomes the active window, so showing it does not move
        /// keyboard focus away from the window it annotates.
        const TOOLTIP                = 1 << 6;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomFramelessConfig {
    pub caption_height: i32,
    pub resize_border: i32,
}

impl Default for CustomFramelessConfig {
    fn default() -> Self {
        Self {
            caption_height: 32,
            resize_border: 8,
        }
    }
}
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct PlatformWindowStateFlags: u32 {
        const NONE                 = 0;
        const WITHIN_SET_GEOMETRY  = 1 << 0;
        const WITHIN_SET_STYLE     = 1 << 1;
        const WITHIN_SET_PARENT    = 1 << 2;
        const RESIZE_MOVE_ACTIVE   = 1 << 3;
    }
}

pub struct SetGeometryGuard<'a> {
    flags: &'a std::cell::Cell<PlatformWindowStateFlags>,
}

impl<'a> SetGeometryGuard<'a> {
    pub fn new(flags: &'a std::cell::Cell<PlatformWindowStateFlags>) -> Self {
        let cur = flags.get();
        flags.set(cur | PlatformWindowStateFlags::WITHIN_SET_GEOMETRY);
        Self { flags }
    }
}

impl Drop for SetGeometryGuard<'_> {
    fn drop(&mut self) {
        // Critical: ONLY clear WITHIN_SET_GEOMETRY bit, never wipe out other flags set by re-entrant operations!
        let cur = self.flags.get();
        self.flags.set(cur & !PlatformWindowStateFlags::WITHIN_SET_GEOMETRY);
    }
}

static WINDOW_EVENT_BINDINGS: RwLock<Option<HashMap<isize, (EventLoopHandle, ObjectId)>>> =
    RwLock::new(None);

static WINDOW_FRAMELESS_CONFIGS: RwLock<Option<HashMap<isize, CustomFramelessConfig>>> =
    RwLock::new(None);

static WINDOW_MIN_SIZES: RwLock<Option<HashMap<isize, (i32, i32)>>> = RwLock::new(None);

thread_local! {
    static WINDOW_EVENT_HANDLERS: std::cell::RefCell<HashMap<isize, Box<dyn WindowSystemEventHandler>>> =
        std::cell::RefCell::new(HashMap::new());
}
pub fn set_window_min_size(hwnd: HWND, min_w: i32, min_h: i32) {
    let mut writer = WINDOW_MIN_SIZES.write().unwrap();
    if writer.is_none() {
        *writer = Some(HashMap::new());
    }
    if let Some(map) = writer.as_mut() {
        map.insert(hwnd as isize, (min_w, min_h));
    }
}

pub fn get_window_min_size(hwnd: HWND) -> Option<(i32, i32)> {
    let reader = WINDOW_MIN_SIZES.read().unwrap();
    reader
        .as_ref()
        .and_then(|map| map.get(&(hwnd as isize)).copied())
}

pub fn remove_window_min_size(hwnd: HWND) {
    let mut writer = WINDOW_MIN_SIZES.write().unwrap();
    if let Some(map) = writer.as_mut() {
        map.remove(&(hwnd as isize));
    }
}

pub fn register_window_event_binding(hwnd: HWND, handle: EventLoopHandle, receiver: ObjectId) {
    let mut map = WINDOW_EVENT_BINDINGS.write().unwrap();
    if map.is_none() {
        *map = Some(HashMap::new());
    }
    map.as_mut()
        .unwrap()
        .insert(hwnd as isize, (handle, receiver));
}

pub fn unregister_window_event_binding(hwnd: HWND) {
    let mut map = WINDOW_EVENT_BINDINGS.write().unwrap();
    if let Some(m) = map.as_mut() {
        m.remove(&(hwnd as isize));
    }
    let mut cfg_map = WINDOW_FRAMELESS_CONFIGS.write().unwrap();
    if let Some(m) = cfg_map.as_mut() {
        m.remove(&(hwnd as isize));
    }
    let mut min_map = WINDOW_MIN_SIZES.write().unwrap();
    if let Some(m) = min_map.as_mut() {
        m.remove(&(hwnd as isize));
    }
    let _ = WINDOW_EVENT_HANDLERS.try_with(|map| {
        map.borrow_mut().remove(&(hwnd as isize));
    });
}

#[inline]
fn get_window_event_binding(hwnd: HWND) -> Option<(EventLoopHandle, ObjectId)> {
    let map = WINDOW_EVENT_BINDINGS.read().unwrap();
    map.as_ref().and_then(|m| m.get(&(hwnd as isize)).cloned())
}

#[inline]
#[allow(dead_code)]
fn get_window_frameless_config(hwnd: HWND) -> Option<CustomFramelessConfig> {
    let map = WINDOW_FRAMELESS_CONFIGS.read().unwrap();
    map.as_ref().and_then(|m| m.get(&(hwnd as isize)).copied())
}

pub fn set_window_frameless_config(hwnd: HWND, config: CustomFramelessConfig) {
    let mut map = WINDOW_FRAMELESS_CONFIGS.write().unwrap();
    if map.is_none() {
        *map = Some(HashMap::new());
    }
    map.as_mut().unwrap().insert(hwnd as isize, config);
}

pub fn set_window_event_handler(hwnd: HWND, handler: Box<dyn WindowSystemEventHandler>) {
    let _ = WINDOW_EVENT_HANDLERS.try_with(|map| {
        map.borrow_mut().insert(hwnd as isize, handler);
    });
}

static WINDOW_SYSTEM_EVENT_QUEUE: Mutex<Option<Vec<(isize, WindowSystemEvent)>>> = Mutex::new(None);

pub fn post_window_system_event(hwnd: HWND, event: WindowSystemEvent) {
    {
        let mut queue = WINDOW_SYSTEM_EVENT_QUEUE.lock().unwrap();
        if queue.is_none() {
            *queue = Some(Vec::new());
        }
        if let Some(q) = queue.as_mut() {
            q.push((hwnd as isize, event));
        }
    }

    if let Some((handle, _)) = get_window_event_binding(hwnd) {
        handle.wake_up();
    }
}

pub fn flush_window_system_events() -> bool {
    let events: Vec<(isize, WindowSystemEvent)> = {
        let mut queue = WINDOW_SYSTEM_EVENT_QUEUE.lock().unwrap();
        queue.as_mut().map(std::mem::take).unwrap_or_default()
    };

    if events.is_empty() {
        return false;
    }

    for (hwnd_isize, event) in events {
        send_window_system_event_immediately(hwnd_isize as HWND, event);
    }
    true
}

thread_local! {
    /// HWNDs currently inside the native sizing loop (`WM_ENTERSIZEMOVE`..`WM_EXITSIZEMOVE`).
    static INTERACTIVE_RESIZE_HWNDS: std::cell::RefCell<std::collections::HashSet<isize>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// Whether `hwnd` is inside the native interactive sizing loop (UI thread only).
pub fn is_interactive_resize(hwnd: HWND) -> bool {
    INTERACTIVE_RESIZE_HWNDS
        .try_with(|s| s.borrow().contains(&(hwnd as isize)))
        .unwrap_or(false)
}

fn set_interactive_resize_flag(hwnd: HWND, active: bool) {
    let _ = INTERACTIVE_RESIZE_HWNDS.try_with(|s| {
        let mut s = s.borrow_mut();
        if active {
            s.insert(hwnd as isize);
        } else {
            s.remove(&(hwnd as isize));
        }
    });
}

/// Mirrors the native sizing-loop state onto the surfaces before they are resized.
fn sync_interactive_resize(
    hwnd: HWND,
    presenter: &mut Option<crate::presenter::WindowsPresenter>,
    layered_surface: &mut Option<crate::surface::WindowsSurface>,
) {
    let active = is_interactive_resize(hwnd);
    if let Some(p) = presenter {
        p.set_interactive_resize(active);
    }
    if let Some(crate::surface::WindowsSurface::Layered(s)) = layered_surface {
        s.set_interactive_resize(active);
    }
}

thread_local! {
    static NESTED_EVENTS: std::cell::RefCell<Vec<(HWND, WindowSystemEvent)>> = const { std::cell::RefCell::new(Vec::new()) };
    static IS_DISPATCHING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn send_window_system_event_immediately(hwnd: HWND, event: WindowSystemEvent) {
    let is_dispatching = IS_DISPATCHING.try_with(|d| d.get()).unwrap_or(false);
    if is_dispatching {
        let _ = NESTED_EVENTS.try_with(|q| q.borrow_mut().push((hwnd, event)));
        return;
    }

    let _ = IS_DISPATCHING.try_with(|d| d.set(true));
    let _ = WINDOW_EVENT_HANDLERS.try_with(|map| {
        if let Some(handler) = map.borrow_mut().get_mut(&(hwnd as isize)) {
            handler.handle_window_event(event);
        }
    });

    loop {
        let next = NESTED_EVENTS.try_with(|q| {
            if q.borrow().is_empty() {
                None
            } else {
                Some(q.borrow_mut().remove(0))
            }
        }).unwrap_or(None);

        match next {
            Some((h, ev)) => {
                let _ = WINDOW_EVENT_HANDLERS.try_with(|map| {
                    if let Some(handler) = map.borrow_mut().get_mut(&(h as isize)) {
                        handler.handle_window_event(ev);
                    }
                });
            }
            None => break,
        }
    }
    let _ = IS_DISPATCHING.try_with(|d| d.set(false));
}
fn is_window_thread(hwnd: HWND) -> bool {
    #[cfg(windows)]
    {
        if hwnd.is_null() {
            return qtrs_core::object::ThreadContext::is_main_thread();
        }
        unsafe {
            let win_tid = windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
                hwnd,
                ptr::null_mut(),
            );
            let cur_tid = windows_sys::Win32::System::Threading::GetCurrentThreadId();
            win_tid == cur_tid
        }
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        qtrs_core::object::ThreadContext::is_main_thread()
    }
}

pub fn dispatch_window_system_event(
    delivery: Delivery,
    hwnd: HWND,
    event: WindowSystemEvent,
) {
    match delivery {
        Delivery::Synchronous => {
            send_window_system_event_immediately(hwnd, event);
        }
        Delivery::Asynchronous => {
            post_window_system_event(hwnd, event);
        }
        Delivery::Default => {
            if is_window_thread(hwnd) {
                send_window_system_event_immediately(hwnd, event);
            } else {
                post_window_system_event(hwnd, event);
            }
        }
    }
}

pub fn handle_geometry_change(
    delivery: Delivery,
    hwnd: HWND,
    geometry: Rect,
) {
    let size = Size::new(geometry.width, geometry.height);
    dispatch_window_system_event(
        delivery,
        hwnd,
        WindowSystemEvent::GeometryChange { geometry },
    );
    dispatch_window_system_event(
        delivery,
        hwnd,
        WindowSystemEvent::Resize { size },
    );
}

#[cfg(windows)]
fn query_keyboard_modifiers() -> KeyboardModifiers {
    unsafe {
        let is_down = |vk: u16| -> bool { (GetKeyState(vk as i32) as u16 & 0x8000) != 0 };
        KeyboardModifiers {
            shift: is_down(VK_SHIFT),
            control: is_down(VK_CONTROL),
            alt: is_down(VK_MENU),
            meta: is_down(VK_LWIN) || is_down(VK_RWIN),
        }
    }
}

/// `QWindowsMouseHandler::keyStateToMouseButtons`: the `MK_*` flags a mouse message carries in
/// `wParam` are the buttons held down while it was generated.
#[cfg(windows)]
fn mouse_buttons_from_mk(wparam: WPARAM) -> qtrs_core::event::MouseButtons {
    use qtrs_core::event::MouseButtons;
    const MK_LBUTTON: usize = 0x0001;
    const MK_RBUTTON: usize = 0x0002;
    const MK_MBUTTON: usize = 0x0010;
    const MK_XBUTTON1: usize = 0x0020;
    const MK_XBUTTON2: usize = 0x0040;
    let mut buttons = MouseButtons::NO_BUTTON;
    for (flag, button) in [
        (MK_LBUTTON, MouseButtons::LEFT),
        (MK_RBUTTON, MouseButtons::RIGHT),
        (MK_MBUTTON, MouseButtons::MIDDLE),
        (MK_XBUTTON1, MouseButtons::BACK),
        (MK_XBUTTON2, MouseButtons::FORWARD),
    ] {
        if wparam & flag != 0 {
            buttons = buttons.union(button);
        }
    }
    buttons
}

#[cfg(windows)]
fn get_cursor_global_pos() -> qtrs_gui::geometry::primitives::Point {
    unsafe {
        let mut pt: windows_sys::Win32::Foundation::POINT = std::mem::zeroed();
        GetCursorPos(&mut pt);
        qtrs_gui::geometry::primitives::Point::new(pt.x, pt.y)
    }
}

/// `QWindowsWindow::setWindowLayered` + `setWindowOpacity` for a window that is not `LAYERED`:
/// translucent (`opacity < 1`) means `WS_EX_LAYERED` plus `SetLayeredWindowAttributes(LWA_ALPHA)`
/// with `qRound(255 * opacity)`; opaque removes the style again and repaints.
#[cfg(windows)]
fn apply_system_window_opacity(hwnd: HWND, opacity: f32) {
    use windows_sys::Win32::Graphics::Gdi::InvalidateRect;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, IsWindowVisible, SetLayeredWindowAttributes, SetWindowLongPtrW,
        GWL_EXSTYLE, LWA_ALPHA,
    };
    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let is_layered = ex_style & WS_EX_LAYERED as isize != 0;
        let needs_layered = opacity < 1.0;
        if needs_layered != is_layered {
            let new_style = if needs_layered {
                ex_style | WS_EX_LAYERED as isize
            } else {
                ex_style & !(WS_EX_LAYERED as isize)
            };
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_style);
        }
        if needs_layered {
            let alpha = (opacity * 255.0).round() as u8;
            SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA);
        } else if IsWindowVisible(hwnd) != 0 {
            InvalidateRect(hwnd, ptr::null(), 1);
        }
    }
}

#[cfg(windows)]
pub fn get_window_dpr(hwnd: HWND) -> f32 {
    unsafe {
        let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForWindow(hwnd);
        if dpi > 0 {
            dpi as f32 / 96.0
        } else {
            1.0
        }
    }
}

#[cfg(windows)]
fn get_x_lparam(lparam: LPARAM) -> i32 {
    (lparam as usize & 0xffff) as i16 as i32
}

#[cfg(windows)]
fn get_y_lparam(lparam: LPARAM) -> i32 {
    ((lparam as usize >> 16) & 0xffff) as i16 as i32
}

#[cfg(windows)]
/// Win32 Window Procedure (wndproc).
unsafe extern "system" fn native_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let _t = qtrs_gui::startup_trace::span_min(3.0, || format!("wndproc msg=0x{msg:04X}"));
    native_window_proc_inner(hwnd, msg, wparam, lparam)
}

unsafe fn native_window_proc_inner(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCCALCSIZE => {
            crate::resize_debug::count(crate::resize_debug::Count::NcCalcSize);
            if wparam != 0 {
                let ncp = &mut *(lparam as *mut NCCALCSIZE_PARAMS);
                let client_rect = &mut ncp.rgrc[0];

                if IsZoomed(hwnd) != 0 {
                    let mut monitor_info: MONITORINFO = std::mem::zeroed();
                    monitor_info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
                    let h_monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
                    if !h_monitor.is_null() && GetMonitorInfoW(h_monitor, &mut monitor_info) != 0 {
                        *client_rect = monitor_info.rcWork;
                    }
                }
                return 0;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_NCHITTEST => {
            if let Some(config) = get_window_frameless_config(hwnd) {
                let x = get_x_lparam(lparam);
                let y = get_y_lparam(lparam);

                let mut window_rect: RECT = std::mem::zeroed();
                GetWindowRect(hwnd, &mut window_rect);

                let is_zoomed = IsZoomed(hwnd) != 0;
                let border = if is_zoomed { 0 } else { config.resize_border };

                let on_left = x >= window_rect.left && x < window_rect.left + border;
                let on_right = x <= window_rect.right && x > window_rect.right - border;
                let on_top = y >= window_rect.top && y < window_rect.top + border;
                let on_bottom = y <= window_rect.bottom && y > window_rect.bottom - border;

                if on_top && on_left {
                    return HTTOPLEFT as LRESULT;
                }
                if on_top && on_right {
                    return HTTOPRIGHT as LRESULT;
                }
                if on_bottom && on_left {
                    return HTBOTTOMLEFT as LRESULT;
                }
                if on_bottom && on_right {
                    return HTBOTTOMRIGHT as LRESULT;
                }
                if on_left {
                    return HTLEFT as LRESULT;
                }
                if on_right {
                    return HTRIGHT as LRESULT;
                }
                if on_top {
                    return HTTOP as LRESULT;
                }
                if on_bottom {
                    return HTBOTTOM as LRESULT;
                }

                if y < window_rect.top + config.caption_height {
                    return HTCAPTION as LRESULT;
                }

                return HTCLIENT as LRESULT;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_GETMINMAXINFO => {
            use windows_sys::Win32::UI::WindowsAndMessaging::MINMAXINFO;
            if let Some((min_w, min_h)) = get_window_min_size(hwnd) {
                let mmi = &mut *(lparam as *mut MINMAXINFO);
                if min_w > 0 {
                    mmi.ptMinTrackSize.x = min_w;
                }
                if min_h > 0 {
                    mmi.ptMinTrackSize.y = min_h;
                }
                return 0;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_ERASEBKGND => 1,
        WM_CLOSE => {
            dispatch_window_system_event(Delivery::Default, hwnd, WindowSystemEvent::CloseRequest);
            if let Some((handle, receiver)) = get_window_event_binding(hwnd) {
                handle.post_event(receiver, Event::new_spontaneous(EventKind::Close));
            }
            0
        }
        windows_sys::Win32::UI::WindowsAndMessaging::WM_ENTERSIZEMOVE => {
            crate::resize_debug::enter_size_move();
            set_interactive_resize_flag(hwnd, true);
            // Per-HWND: delivered to this window's own event handler.
            dispatch_window_system_event(
                Delivery::Default,
                hwnd,
                WindowSystemEvent::InteractiveResizeStart,
            );
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        windows_sys::Win32::UI::WindowsAndMessaging::WM_EXITSIZEMOVE => {
            set_interactive_resize_flag(hwnd, false);
            dispatch_window_system_event(
                Delivery::Default,
                hwnd,
                WindowSystemEvent::InteractiveResizeEnd,
            );
            crate::resize_debug::exit_size_move();
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_DESTROY => {
            set_interactive_resize_flag(hwnd, false);
            unregister_window_event_binding(hwnd);
            0
        }
        WM_SIZE => {
            let dpr = get_window_dpr(hwnd);
            let phys_w = (lparam as usize & 0xffff) as i32;
            let phys_h = ((lparam as usize >> 16) & 0xffff) as i32;
            crate::resize_debug::wm_size_begin(hwnd as isize, phys_w, phys_h);
            let logical_size = crate::high_dpi::from_native_size(
                qtrs_gui::geometry::primitives::Size::new(phys_w, phys_h),
                dpr,
            );
            crate::resize_trace::record(
                crate::resize_trace::TraceKind::WmSize,
                hwnd as usize,
                (logical_size.width as u32, logical_size.height as u32),
                (phys_w as u32, phys_h as u32),
            );
            let mut r: RECT = unsafe { std::mem::zeroed() };
            let (x, y) = if unsafe { GetWindowRect(hwnd, &mut r) } != 0 {
                let logical_pt = crate::high_dpi::from_native_point(
                    qtrs_gui::geometry::primitives::Point::new(r.left, r.top),
                    dpr,
                );
                (logical_pt.x, logical_pt.y)
            } else {
                (0, 0)
            };
            let logical_rect = Rect::new(x, y, logical_size.width, logical_size.height);
            crate::resize_debug::note(|| format!(
                "WM_SIZE phys {}x{} dpr {} -> logical {}x{} (wparam={})",
                phys_w, phys_h, dpr, logical_size.width, logical_size.height, wparam
            ));

            // The single canonical resize delivery: WindowSystemEvent::Resize (geometry, widget
            // Resize, callback, layout, render). No second `EventKind::Resize` is posted to the
            // bound QObject; it used to re-run the callback and a synchronous render.
            handle_geometry_change(Delivery::Default, hwnd, logical_rect);
            crate::resize_debug::wm_size_end(hwnd as isize);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_MOVE => {
            crate::resize_debug::wm_move_begin(hwnd as isize);
            let dpr = get_window_dpr(hwnd);
            let phys_x = get_x_lparam(lparam);
            let phys_y = get_y_lparam(lparam);
            let pos = crate::high_dpi::from_native_point(
                qtrs_gui::geometry::primitives::Point::new(phys_x, phys_y),
                dpr,
            );
            let mut r: RECT = unsafe { std::mem::zeroed() };
            let (w, h) = if unsafe { GetWindowRect(hwnd, &mut r) } != 0 {
                let logical_size = crate::high_dpi::from_native_size(
                    qtrs_gui::geometry::primitives::Size::new(r.right - r.left, r.bottom - r.top),
                    dpr,
                );
                (logical_size.width, logical_size.height)
            } else {
                (0, 0)
            };
            let logical_rect = Rect::new(pos.x, pos.y, w, h);
            handle_geometry_change(Delivery::Default, hwnd, logical_rect);

            if let Some((handle, receiver)) = get_window_event_binding(hwnd) {
                handle.post_event(
                    receiver,
                    Event::new_spontaneous(EventKind::Move {
                        x: pos.x,
                        y: pos.y,
                        old_x: 0,
                        old_y: 0,
                    }),
                );
            }
            crate::resize_debug::wm_move_end(hwnd as isize);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_SHOWWINDOW => {
            let shown = wparam != 0;
            if let Some((handle, receiver)) = get_window_event_binding(hwnd) {
                let kind = if shown {
                    EventKind::Show
                } else {
                    EventKind::Hide
                };
                handle.post_event(receiver, Event::new_spontaneous(kind));
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_PAINT => {
            crate::resize_debug::count(crate::resize_debug::Count::WmPaint);
            if let Some((handle, receiver)) = get_window_event_binding(hwnd) {
                handle.post_event(receiver, Event::new_spontaneous(EventKind::Expose));
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_SETFOCUS => {
            dispatch_window_system_event(Delivery::Default, hwnd, WindowSystemEvent::FocusIn);
            if let Some((handle, receiver)) = get_window_event_binding(hwnd) {
                handle.post_event(
                    receiver,
                    Event::new_spontaneous(EventKind::FocusIn {
                        reason: qtrs_core::event::FocusReason::ActiveWindow,
                    }),
                );
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_KILLFOCUS => {
            dispatch_window_system_event(Delivery::Default, hwnd, WindowSystemEvent::FocusOut);
            if let Some((handle, receiver)) = get_window_event_binding(hwnd) {
                handle.post_event(
                    receiver,
                    Event::new_spontaneous(EventKind::FocusOut {
                        reason: qtrs_core::event::FocusReason::ActiveWindow,
                    }),
                );
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_DPICHANGED => {
            let dpi_x = (wparam as u32) & 0xffff;
            let dpi_y = ((wparam as u32) >> 16) & 0xffff;

            let rect_ptr = lparam as *const RECT;
            if !rect_ptr.is_null() {
                let r = *rect_ptr;
                let width = (r.right - r.left).max(0);
                let height = (r.bottom - r.top).max(0);
                SetWindowPos(
                    hwnd,
                    ptr::null_mut(),
                    r.left,
                    r.top,
                    width,
                    height,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }

            dispatch_window_system_event(
                Delivery::Default,
                hwnd,
                WindowSystemEvent::DpiChanged { dpi_x, dpi_y },
            );
            if let Some((handle, receiver)) = get_window_event_binding(hwnd) {
                handle.post_event(
                    receiver,
                    Event::new_spontaneous(EventKind::DpiChanged { dpi_x, dpi_y }),
                );
            }
            0
        }
        WM_DISPLAYCHANGE => {
            crate::integration::platform().screen_changed().emit(&());
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED => {
            crate::integration::platform().theme().refresh();
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_SETCURSOR => {
            let hit_test = (lparam & 0xFFFF) as u32;
            if hit_test == HTCLIENT {
                let shape = crate::cursor::win32_cursor::Win32Cursor::current_global_shape();
                crate::cursor::win32_cursor::Win32Cursor::set_shape(shape);
                return 1;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            let dpr = get_window_dpr(hwnd);
            let phys_x = get_x_lparam(lparam);
            let phys_y = get_y_lparam(lparam);
            let pos = crate::high_dpi::from_native_point(
                qtrs_gui::geometry::primitives::Point::new(phys_x, phys_y),
                dpr,
            );
            let phys_global = get_cursor_global_pos();
            let global_pos = crate::high_dpi::from_native_point(phys_global, dpr);
            let button = match msg {
                WM_LBUTTONDOWN => MouseButton::Left,
                WM_RBUTTONDOWN => MouseButton::Right,
                WM_MBUTTONDOWN => MouseButton::Middle,
                _ => MouseButton::None,
            };
            let modifiers = query_keyboard_modifiers();

            dispatch_window_system_event(
                Delivery::Default,
                hwnd,
                WindowSystemEvent::MousePress {
                    pos,
                    global_pos,
                    button,
                    modifiers,
                },
            );

            if let Some((handle, receiver)) = get_window_event_binding(hwnd) {
                let btn_code = match button {
                    MouseButton::Left => 1,
                    MouseButton::Right => 2,
                    MouseButton::Middle => 3,
                    _ => 0,
                };
                handle.post_event(
                    receiver,
                    Event::new_spontaneous(EventKind::MouseButtonPress {
                        x: pos.x,
                        y: pos.y,
                        button: btn_code,
                    }),
                );
            }
            0
        }
        WM_LBUTTONDBLCLK | WM_RBUTTONDBLCLK | WM_MBUTTONDBLCLK => {
            let dpr = get_window_dpr(hwnd);
            let phys_x = get_x_lparam(lparam);
            let phys_y = get_y_lparam(lparam);
            let pos = crate::high_dpi::from_native_point(
                qtrs_gui::geometry::primitives::Point::new(phys_x, phys_y),
                dpr,
            );
            let phys_global = get_cursor_global_pos();
            let global_pos = crate::high_dpi::from_native_point(phys_global, dpr);
            let button = match msg {
                WM_LBUTTONDBLCLK => MouseButton::Left,
                WM_RBUTTONDBLCLK => MouseButton::Right,
                WM_MBUTTONDBLCLK => MouseButton::Middle,
                _ => MouseButton::None,
            };
            let modifiers = query_keyboard_modifiers();

            dispatch_window_system_event(
                Delivery::Default,
                hwnd,
                WindowSystemEvent::MouseDoubleClick {
                    pos,
                    global_pos,
                    button,
                    modifiers,
                },
            );

            if let Some((handle, receiver)) = get_window_event_binding(hwnd) {
                let btn_code = match button {
                    MouseButton::Left => 1,
                    MouseButton::Right => 2,
                    MouseButton::Middle => 3,
                    _ => 0,
                };
                handle.post_event(
                    receiver,
                    Event::new_spontaneous(EventKind::MouseButtonDblClick {
                        x: pos.x,
                        y: pos.y,
                        button: btn_code,
                    }),
                );
            }
            0
        }
        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => {
            let dpr = get_window_dpr(hwnd);
            let phys_x = get_x_lparam(lparam);
            let phys_y = get_y_lparam(lparam);
            let pos = crate::high_dpi::from_native_point(
                qtrs_gui::geometry::primitives::Point::new(phys_x, phys_y),
                dpr,
            );
            let phys_global = get_cursor_global_pos();
            let global_pos = crate::high_dpi::from_native_point(phys_global, dpr);
            let button = match msg {
                WM_LBUTTONUP => MouseButton::Left,
                WM_RBUTTONUP => MouseButton::Right,
                WM_MBUTTONUP => MouseButton::Middle,
                _ => MouseButton::None,
            };
            let modifiers = query_keyboard_modifiers();

            dispatch_window_system_event(
                Delivery::Default,
                hwnd,
                WindowSystemEvent::MouseRelease {
                    pos,
                    global_pos,
                    button,
                    modifiers,
                },
            );

            if let Some((handle, receiver)) = get_window_event_binding(hwnd) {
                let btn_code = match button {
                    MouseButton::Left => 1,
                    MouseButton::Right => 2,
                    MouseButton::Middle => 3,
                    _ => 0,
                };
                handle.post_event(
                    receiver,
                    Event::new_spontaneous(EventKind::MouseButtonRelease {
                        x: pos.x,
                        y: pos.y,
                        button: btn_code,
                    }),
                );
            }
            0
        }
        WM_MOUSEMOVE => {
            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            TrackMouseEvent(&mut tme);

            let dpr = get_window_dpr(hwnd);
            let phys_x = get_x_lparam(lparam);
            let phys_y = get_y_lparam(lparam);
            let pos = crate::high_dpi::from_native_point(
                qtrs_gui::geometry::primitives::Point::new(phys_x, phys_y),
                dpr,
            );
            let phys_global = get_cursor_global_pos();
            let global_pos = crate::high_dpi::from_native_point(phys_global, dpr);

            dispatch_window_system_event(
                Delivery::Default,
                hwnd,
                WindowSystemEvent::MouseMove {
                    pos,
                    global_pos,
                    buttons: mouse_buttons_from_mk(wparam),
                },
            );

            if let Some((handle, receiver)) = get_window_event_binding(hwnd) {
                handle.post_event(
                    receiver,
                    Event::new_spontaneous(EventKind::MouseMove { x: pos.x, y: pos.y }),
                );
            }
            0
        }
        WM_MOUSEWHEEL => {
            let dpr = get_window_dpr(hwnd);
            let global_x = get_x_lparam(lparam);
            let global_y = get_y_lparam(lparam);
            let phys_global = qtrs_gui::geometry::primitives::Point::new(global_x, global_y);
            let global_pos = crate::high_dpi::from_native_point(phys_global, dpr);

            let mut pt = windows_sys::Win32::Foundation::POINT {
                x: global_x,
                y: global_y,
            };
            windows_sys::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);
            let pos = crate::high_dpi::from_native_point(
                qtrs_gui::geometry::primitives::Point::new(pt.x, pt.y),
                dpr,
            );
            let wheel_delta = (wparam >> 16) as i16 as i32;
            let modifiers = query_keyboard_modifiers();

            dispatch_window_system_event(
                Delivery::Default,
                hwnd,
                WindowSystemEvent::Wheel {
                    pos,
                    global_pos,
                    delta: WheelDelta {
                        y: wheel_delta,
                        x: 0,
                    },
                    modifiers,
                },
            );

            if let Some((handle, receiver)) = get_window_event_binding(hwnd) {
                handle.post_event(
                    receiver,
                    Event::new_spontaneous(EventKind::Wheel {
                        x: pt.x,
                        y: pt.y,
                        pixel_delta_x: 0,
                        pixel_delta_y: 0,
                        angle_delta_x: 0,
                        angle_delta_y: wheel_delta,
                        modifiers: modifiers.bits(),
                    }),
                );
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_MOUSELEAVE => {
            dispatch_window_system_event(Delivery::Default, hwnd, WindowSystemEvent::MouseLeave);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_CONTEXTMENU => {
            let global_x = get_x_lparam(lparam);
            let global_y = get_y_lparam(lparam);
            let mut pt = windows_sys::Win32::Foundation::POINT {
                x: global_x,
                y: global_y,
            };
            windows_sys::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut pt);

            if let Some((handle, receiver)) = get_window_event_binding(hwnd) {
                handle.post_event(
                    receiver,
                    Event::new_spontaneous(EventKind::ContextMenu {
                        x: pt.x,
                        y: pt.y,
                        global_x,
                        global_y,
                        reason: qtrs_core::event::ContextMenuReason::Mouse,
                    }),
                );
            }
            0
        }
        WM_KEYDOWN => {
            let key = wparam as u32;
            let is_repeat = (lparam & (1 << 30)) != 0;
            let modifiers = query_keyboard_modifiers();

            dispatch_window_system_event(
                Delivery::Default,
                hwnd,
                WindowSystemEvent::KeyPress {
                    key,
                    modifiers,
                    is_repeat,
                },
            );

            if let Some((handle, receiver)) = get_window_event_binding(hwnd) {
                handle.post_event(
                    receiver,
                    Event::new_spontaneous(EventKind::KeyPress {
                        key,
                        modifiers: modifiers.bits(),
                        is_repeat,
                    }),
                );
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_KEYUP => {
            let key = wparam as u32;
            let modifiers = query_keyboard_modifiers();
            dispatch_window_system_event(
                Delivery::Default,
                hwnd,
                WindowSystemEvent::KeyRelease { key, modifiers },
            );

            if let Some((handle, receiver)) = get_window_event_binding(hwnd) {
                handle.post_event(
                    receiver,
                    Event::new_spontaneous(EventKind::KeyRelease {
                        key,
                        modifiers: modifiers.bits(),
                    }),
                );
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        windows_sys::Win32::UI::WindowsAndMessaging::WM_IME_STARTCOMPOSITION => {
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        windows_sys::Win32::UI::WindowsAndMessaging::WM_IME_COMPOSITION => {
            let mut ime_ctx = crate::ime::Win32InputContext::new(hwnd);
            let (commit_opt, preedit_opt, cursor_pos) = ime_ctx.handle_composition(lparam);

            let commit_string = commit_opt.unwrap_or_default();
            let preedit_string = preedit_opt.unwrap_or_default();

            if !commit_string.is_empty() || !preedit_string.is_empty() {
                dispatch_window_system_event(
                    Delivery::Default,
                    hwnd,
                    WindowSystemEvent::InputMethod {
                        commit_string: commit_string.clone(),
                        preedit_string: preedit_string.clone(),
                        cursor_position: cursor_pos,
                    },
                );

                if let Some((handle, receiver)) = get_window_event_binding(hwnd) {
                    handle.post_event(
                        receiver,
                        Event::new_spontaneous(EventKind::InputMethod {
                            commit_string,
                            preedit_string,
                            cursor_position: cursor_pos,
                        }),
                    );
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        windows_sys::Win32::UI::WindowsAndMessaging::WM_IME_ENDCOMPOSITION => {
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        0x0218 => {
            // WM_POWERBROADCAST (aligned with QWindowsContext sleep-resume)
            let power_event = match wparam as u32 {
                0x0012 | 0x0007 => Some(crate::window_system_interface::PowerEvent::Resume),
                0x0004 => Some(crate::window_system_interface::PowerEvent::Suspend),
                _ => None,
            };
            if let Some(event) = power_event {
                dispatch_window_system_event(
                    Delivery::Default,
                    hwnd,
                    WindowSystemEvent::Power { event },
                );
            }
            1
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(windows)]
pub fn set_dpi_awareness() -> bool {
    unsafe {
        use windows_sys::Win32::UI::HiDpi::{
            SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        };
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) != 0
    }
}

#[cfg(not(windows))]
pub fn set_dpi_awareness() -> bool {
    true
}

#[cfg(windows)]
static REGISTER_WINDOW_CLASS_ONCE: Once = Once::new();
#[cfg(windows)]
const NATIVE_WINDOW_CLASS_NAME: &[u16] = &[
    'Q' as u16, 't' as u16, 'r' as u16, 's' as u16, 'N' as u16, 'a' as u16, 't' as u16, 'i' as u16,
    'v' as u16, 'e' as u16, 'W' as u16, 'i' as u16, 'n' as u16, 'd' as u16, 'o' as u16, 'w' as u16,
    'C' as u16, 'l' as u16, 'a' as u16, 's' as u16, 's' as u16, 0,
];

#[cfg(windows)]
fn ensure_native_window_class_registered() {
    REGISTER_WINDOW_CLASS_ONCE.call_once(|| unsafe {
        let h_instance = GetModuleHandleW(ptr::null());
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            // Qt registers every window class with CS_DBLCLKS (qwindowswindowclassdescription.cpp:67).
            style: CS_DBLCLKS,
            lpfnWndProc: Some(native_window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: h_instance,
            hIcon: ptr::null_mut(),
            hCursor: LoadCursorW(ptr::null_mut(), IDC_ARROW),
            hbrBackground: ptr::null_mut(),
            lpszMenuName: ptr::null(),
            lpszClassName: NATIVE_WINDOW_CLASS_NAME.as_ptr(),
            hIconSm: ptr::null_mut(),
        };
        RegisterClassExW(&wc);
    });
}

#[cfg(windows)]
pub struct NativeWindow {
    hwnd: HWND,
    title: String,
    flags: WindowFlags,
    geometry: Rect,
    drop_target: *mut crate::drag_drop::win32_ole::OleDropTarget,
    opacity: f32,
    min_size: (i32, i32),
    layered_surface: Option<crate::surface::WindowsSurface>,
    presenter: Option<crate::presenter::WindowsPresenter>,
    owner_thread: std::thread::ThreadId,
    state_flags: std::cell::Cell<PlatformWindowStateFlags>,
    ime_enabled: std::cell::Cell<bool>,
    target_pos: Option<qtrs_gui::geometry::Point>,
}

#[cfg(windows)]
unsafe impl Send for NativeWindow {}
#[cfg(windows)]
unsafe impl Sync for NativeWindow {}

#[cfg(windows)]
impl NativeWindow {
    pub fn new(title: &str, rect: Rect, flags: WindowFlags) -> Result<Self, &'static str> {
        ensure_native_window_class_registered();

        let mut dw_style = WS_CLIPCHILDREN | WS_CLIPSIBLINGS;
        let mut dw_ex_style = 0u32;

        if flags.contains(WindowFlags::LAYERED) || flags.contains(WindowFlags::TOOLTIP) {
            dw_style |= WS_POPUP;
        } else if flags.contains(WindowFlags::CUSTOM_FRAMELESS) {
            dw_style |= WS_THICKFRAME | WS_CAPTION | WS_MINIMIZEBOX | WS_MAXIMIZEBOX;
        } else if flags.contains(WindowFlags::FRAMELESS) {
            dw_style |= WS_POPUP;
        } else {
            dw_style |= WS_OVERLAPPEDWINDOW;
        }

        if flags.contains(WindowFlags::STAYS_ON_TOP) || flags.contains(WindowFlags::TOOLTIP) {
            dw_ex_style |= WS_EX_TOPMOST;
        }
        if flags.contains(WindowFlags::TOOLTIP) {
            dw_ex_style |= WS_EX_NOACTIVATE;
        }
        if flags.contains(WindowFlags::TOOL) || flags.contains(WindowFlags::TOOLTIP) {
            dw_ex_style |= WS_EX_TOOLWINDOW;
        } else {
            dw_ex_style |= WS_EX_APPWINDOW;
        }
        if flags.contains(WindowFlags::LAYERED) {
            dw_ex_style |= WS_EX_LAYERED;
        }
        if flags.contains(WindowFlags::CLICK_THROUGH) {
            dw_ex_style |= WS_EX_TRANSPARENT;
        }

        let wide_title: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();

        let hwnd = unsafe {
            let h_instance = GetModuleHandleW(ptr::null());
            CreateWindowExW(
                dw_ex_style,
                NATIVE_WINDOW_CLASS_NAME.as_ptr(),
                wide_title.as_ptr(),
                dw_style,
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                ptr::null_mut(),
                ptr::null_mut(),
                h_instance,
                ptr::null(),
            )
        };

        if hwnd.is_null() {
            return Err("CreateWindowExW failed");
        }

        crate::ime::set_window_ime_enabled(hwnd, false);

        if flags.contains(WindowFlags::CUSTOM_FRAMELESS) && !flags.contains(WindowFlags::LAYERED) {
            unsafe {
                let margins = MARGINS {
                    cxLeftWidth: 1,
                    cxRightWidth: 1,
                    cyTopHeight: 0,
                    cyBottomHeight: 1,
                };
                DwmExtendFrameIntoClientArea(hwnd, &margins);
                SetWindowPos(
                    hwnd,
                    ptr::null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }
            set_window_frameless_config(hwnd, CustomFramelessConfig::default());
        }

        let layered_surface = if flags.contains(WindowFlags::LAYERED) {
            crate::surface::WindowsSurface::create(
                hwnd,
                rect.width.max(1) as u32,
                rect.height.max(1) as u32,
            )
            .ok()
        } else {
            None
        };

        Ok(Self {
            hwnd,
            title: title.to_string(),
            flags,
            geometry: rect,
            drop_target: std::ptr::null_mut(),
            opacity: 1.0,
            min_size: (0, 0),
            layered_surface,
            presenter: None,
            owner_thread: std::thread::current().id(),
            state_flags: std::cell::Cell::new(PlatformWindowStateFlags::NONE),
            ime_enabled: std::cell::Cell::new(false),
            target_pos: None,
        })
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn flags(&self) -> WindowFlags {
        self.flags
    }

    pub fn geometry(&self) -> Rect {
        #[cfg(windows)]
        if !self.hwnd.is_null() {
            let mut r: RECT = unsafe { std::mem::zeroed() };
            if unsafe { GetWindowRect(self.hwnd, &mut r) } != 0 {
                return Rect::new(r.left, r.top, r.right - r.left, r.bottom - r.top);
            }
        }
        self.geometry
    }

    pub fn bind_event_loop(&self, handle: EventLoopHandle, receiver: ObjectId) {
        if !self.hwnd.is_null() {
            register_window_event_binding(self.hwnd, handle, receiver);
        }
    }

    pub fn unbind_event_loop(&self) {
        if !self.hwnd.is_null() {
            unregister_window_event_binding(self.hwnd);
        }
    }

    pub fn set_event_handler(&mut self, handler: Box<dyn WindowSystemEventHandler>) {
        if !self.hwnd.is_null() {
            set_window_event_handler(self.hwnd, handler);
        }
    }

    pub fn show(&self) {
        if !self.hwnd.is_null() {
            unsafe {
                let cmd = if self.flags.contains(WindowFlags::TOOLTIP) {
                    SW_SHOWNOACTIVATE
                } else {
                    SW_SHOW
                };
                ShowWindow(self.hwnd, cmd);
            }
        }
    }

    pub fn hide(&self) {
        if !self.hwnd.is_null() {
            unsafe {
                ShowWindow(self.hwnd, SW_HIDE);
            }
        }
    }

    pub fn set_geometry(&mut self, rect: Rect) {
        if !self.hwnd.is_null() {
            let _guard = SetGeometryGuard::new(&self.state_flags);
            unsafe {
                SetWindowPos(
                    self.hwnd,
                    ptr::null_mut(),
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            if (rect.width != self.geometry.width || rect.height != self.geometry.height)
                && rect.width > 0
                && rect.height > 0
            {
                sync_interactive_resize(self.hwnd, &mut self.presenter, &mut self.layered_surface);
                if let Some(surface) = &mut self.layered_surface {
                    if surface.resize(rect.width as u32, rect.height as u32).is_err() {
                        self.layered_surface = None;
                    }
                }
                if let Some(presenter) = &mut self.presenter {
                    if presenter.resize(rect.width as u32, rect.height as u32).is_err() {
                        self.presenter = None;
                    }
                }
            }
            self.geometry = rect;
        }
    }

    pub fn set_stays_on_top(&mut self, enabled: bool) {
        if !self.hwnd.is_null() {
            use windows_sys::Win32::UI::WindowsAndMessaging::{HWND_NOTOPMOST, HWND_TOPMOST};
            let insert_after = if enabled {
                HWND_TOPMOST
            } else {
                HWND_NOTOPMOST
            };
            unsafe {
                SetWindowPos(
                    self.hwnd,
                    insert_after,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
            self.flags.set(WindowFlags::STAYS_ON_TOP, enabled);
        }
    }

    pub fn set_click_through(&mut self, enabled: bool) {
        if !self.hwnd.is_null() {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE,
            };
            unsafe {
                let ex_style = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE) as u32;
                let new_style = if enabled {
                    ex_style | WS_EX_TRANSPARENT
                } else {
                    ex_style & !WS_EX_TRANSPARENT
                };
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, new_style as isize);
            }
            self.flags.set(WindowFlags::CLICK_THROUGH, enabled);
        }
    }

    pub fn start_system_drag(&self) {
        let _ = self.start_system_move();
    }

    pub fn start_system_move(&self) -> bool {
        if !self.hwnd.is_null() {
            unsafe {
                use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
                use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_SYSCOMMAND};
                ReleaseCapture();
                PostMessageW(self.hwnd, WM_SYSCOMMAND, 0xF012 /*SC_DRAGMOVE*/, 0);
            }
            true
        } else {
            false
        }
    }

    pub fn start_system_resize(&self, edges: crate::platform_window::WindowEdges) -> bool {
        if !self.hwnd.is_null() {
            unsafe {
                use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
                use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_SYSCOMMAND};
                ReleaseCapture();
                let orientation = edges_to_win_orientation(edges);
                PostMessageW(self.hwnd, WM_SYSCOMMAND, orientation, 0);
            }
            true
        } else {
            false
        }
    }

    /// `QWindowsWindow::setOpacity`. A `LAYERED` window's presenter blends the opacity into every
    /// present; any other window is made layered by the system while it is translucent
    /// (`QWindowsWindow::setWindowLayered` / `setWindowOpacity`, `qwindowswindow.cpp:494-530`).
    pub fn set_opacity(&mut self, opacity: f32) {
        let opacity = opacity.clamp(0.0, 1.0);
        let changed = self.opacity != opacity;
        self.opacity = opacity;
        if let Some(p) = &mut self.presenter {
            p.set_opacity(opacity);
        }
        if changed && !self.hwnd.is_null() && !self.flags.contains(WindowFlags::LAYERED) {
            apply_system_window_opacity(self.hwnd, opacity);
        }
    }

    pub fn opacity(&self) -> f32 {
        self.opacity
    }

    pub fn set_minimum_size(&mut self, min_w: i32, min_h: i32) {
        self.min_size = (min_w.max(0), min_h.max(0));
        if !self.hwnd.is_null() {
            set_window_min_size(self.hwnd, self.min_size.0, self.min_size.1);
        }
    }

    pub fn minimum_size(&self) -> (i32, i32) {
        self.min_size
    }

    pub fn create_layered_surface(&self) -> Result<crate::layered::LayeredSurface, &'static str> {
        crate::layered::LayeredSurface::new(
            self.hwnd,
            self.geometry.width as u32,
            self.geometry.height as u32,
        )
    }

    pub fn get_or_create_layered_surface(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<&mut crate::surface::WindowsSurface, &'static str> {
        if self.layered_surface.is_none() {
            self.layered_surface = Some(crate::surface::WindowsSurface::create(self.hwnd, width, height)?);
        }
        sync_interactive_resize(self.hwnd, &mut self.presenter, &mut self.layered_surface);
        let surface = self.layered_surface.as_mut().unwrap();
        if surface.width() != width || surface.height() != height {
            if let Err(e) = surface.resize(width, height) {
                self.layered_surface = None;
                return Err(e);
            }
        }
        Ok(self.layered_surface.as_mut().unwrap())
    }

    /// The current presenter, if one has been created (instrumentation/tests).
    pub fn presenter(&self) -> Option<&crate::presenter::WindowsPresenter> {
        self.presenter.as_ref()
    }

    pub fn presenter_mut(&mut self) -> Option<&mut crate::presenter::WindowsPresenter> {
        self.presenter.as_mut()
    }

    pub fn get_or_create_presenter(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<&mut crate::presenter::WindowsPresenter, &'static str> {
        if self.presenter.is_none() {
            let p = if self.flags.contains(WindowFlags::LAYERED) {
                if let Ok(dcomp) = crate::surface::dcomp::DCompSurface::new(self.hwnd, width, height) {
                    crate::presenter::WindowsPresenter::DirectComposition(dcomp)
                } else {
                    let layered = crate::presenter::Win32LayeredPresenter::new(
                        self.hwnd,
                        width,
                        height,
                        self.opacity,
                    )?;
                    crate::presenter::WindowsPresenter::Layered(layered)
                }
            } else {
                let dc = crate::presenter::Win32DcPresenter::new(self.hwnd, width, height)?;
                crate::presenter::WindowsPresenter::Dc(dc)
            };
            self.presenter = Some(p);
        }
        sync_interactive_resize(self.hwnd, &mut self.presenter, &mut self.layered_surface);
        let p = self.presenter.as_mut().unwrap();
        if let Err(e) = p.resize(width, height) {
            self.presenter = None;
            return Err(e);
        }
        Ok(self.presenter.as_mut().unwrap())
    }

    pub fn close(&mut self) {
        self.layered_surface = None;
        self.presenter = None;
        if !self.drop_target.is_null() {
            self.drop_target = std::ptr::null_mut();
        }
        if !self.hwnd.is_null() {
            remove_window_min_size(self.hwnd);
            unsafe {
                DestroyWindow(self.hwnd);
            }
            self.hwnd = ptr::null_mut();
        }
    }

    pub fn set_custom_frameless_config(&mut self, caption_height: i32, resize_border: i32) {
        if !self.hwnd.is_null() {
            set_window_frameless_config(
                self.hwnd,
                CustomFramelessConfig {
                    caption_height,
                    resize_border,
                },
            );
        }
    }

    pub fn custom_frameless_config(&self) -> Option<CustomFramelessConfig> {
        if !self.hwnd.is_null() {
            get_window_frameless_config(self.hwnd)
        } else {
            None
        }
    }
}

#[cfg(windows)]
impl crate::platform_window::PlatformWindow for NativeWindow {
    fn state_flags(&self) -> PlatformWindowStateFlags {
        self.state_flags.get()
    }

    fn owner_thread(&self) -> std::thread::ThreadId {
        self.owner_thread
    }

    fn show(&self) {
        self.show();
    }

    fn hide(&self) {
        self.hide();
    }

    /// `QWindowsWindow::isActive` (`qwindowswindow.cpp:1904-1910`): the window is the foreground
    /// window, or a child of it.
    fn is_active(&self) -> bool {
        if self.hwnd.is_null() {
            return false;
        }
        let foreground = unsafe { GetForegroundWindow() };
        !foreground.is_null()
            && (foreground == self.hwnd || unsafe { IsChild(foreground, self.hwnd) } != 0)
    }

    fn geometry(&self) -> Rect {
        self.geometry()
    }

    fn set_geometry(&mut self, rect: Rect) {
        self.set_geometry(rect);
    }
    fn set_target_pos(&mut self, pos: Option<qtrs_gui::geometry::Point>) {
        self.target_pos = pos;
    }

    fn set_stays_on_top(&mut self, enabled: bool) {
        self.set_stays_on_top(enabled);
    }

    fn set_click_through(&mut self, enabled: bool) {
        self.set_click_through(enabled);
    }

    fn start_system_drag(&self) {
        let _ = self.start_system_move();
    }

    fn start_system_move(&self) -> bool {
        self.start_system_move()
    }

    fn start_system_resize(&self, edges: crate::platform_window::WindowEdges) -> bool {
        self.start_system_resize(edges)
    }
    fn set_cursor(&mut self, shape: crate::cursor::CursorShape) {
        crate::cursor::win32_cursor::Win32Cursor::set_shape(shape);
    }

    fn set_opacity(&mut self, opacity: f32) {
        self.set_opacity(opacity);
    }

    fn opacity(&self) -> f32 {
        self.opacity
    }

    fn set_minimum_size(&mut self, min_w: i32, min_h: i32) {
        self.set_minimum_size(min_w, min_h);
    }

    fn minimum_size(&self) -> (i32, i32) {
        self.minimum_size()
    }

    fn present_region(
        &mut self,
        pixmap: &qtrs_gui::paint::Pixmap,
        dirty: &qtrs_gui::geometry::Region,
    ) -> Result<(), &'static str> {
        let width = pixmap.physical_width();
        let height = pixmap.physical_height();
        let opacity = self.opacity;
        let target_pos = self.target_pos.take();
        let dpr = get_window_dpr(self.hwnd);
        let phys_target = target_pos.map(|p| {
            if dpr > 1.0 {
                crate::high_dpi::to_native_point(p, dpr)
            } else {
                p
            }
        });
        let dbg_t = crate::resize_debug::start();
        let res = match self.get_or_create_presenter(width, height) {
            Ok(p) => {
                p.set_opacity(opacity);
                if let crate::presenter::WindowsPresenter::Layered(lp) = p {
                    if let Some(pos) = phys_target {
                        lp.set_target_pos(Some(windows_sys::Win32::Foundation::POINT { x: pos.x, y: pos.y }));
                    }
                }
                p.present(pixmap, dirty)
            }
            Err(e) => Err(e),
        };

        if res.is_err() {
            crate::resize_debug::count(crate::resize_debug::Count::PresentError);
            self.presenter.take();
            let p = self.get_or_create_presenter(width, height)?;
            p.set_opacity(opacity);
            if let crate::presenter::WindowsPresenter::Layered(lp) = p {
                if let Some(pos) = phys_target {
                    lp.set_target_pos(Some(windows_sys::Win32::Foundation::POINT { x: pos.x, y: pos.y }));
                }
            }
            let full = qtrs_gui::geometry::Region::from_coords(0, 0, width as i32, height as i32);
            let r = p.present(pixmap, &full);
            if r.is_ok() {
                if let Some(pos) = target_pos {
                    self.geometry.x = pos.x;
                    self.geometry.y = pos.y;
                }
                self.geometry.width = width as i32;
                self.geometry.height = height as i32;
            }
            crate::resize_debug::end(crate::resize_debug::Phase::Present, dbg_t);
            return r;
        }

        if let Some(pos) = target_pos {
            self.geometry.x = pos.x;
            self.geometry.y = pos.y;
        }
        self.geometry.width = width as i32;
        self.geometry.height = height as i32;

        crate::resize_debug::end(crate::resize_debug::Phase::Present, dbg_t);
        Ok(())
    }

    fn present(
        &mut self,
        pixmap: &mut qtrs_gui::paint::Pixmap,
        opacity: f32,
    ) -> Result<(), &'static str> {
        self.set_opacity(opacity);
        let width = pixmap.physical_width();
        let height = pixmap.physical_height();
        let full = qtrs_gui::geometry::Region::from_coords(0, 0, width as i32, height as i32);
        self.present_region(pixmap, &full)
    }

    fn present_dirty(
        &mut self,
        pixmap: &mut qtrs_gui::paint::Pixmap,
        opacity: f32,
        dirty_rect: Rect,
    ) -> Result<(), &'static str> {
        self.set_opacity(opacity);
        let region = qtrs_gui::geometry::Region::from_rect(dirty_rect);
        self.present_region(pixmap, &region)
    }
    fn set_event_handler(&mut self, handler: Box<dyn WindowSystemEventHandler>) {
        self.set_event_handler(handler);
    }

    fn native_handle(&self) -> isize {
        self.hwnd as isize
    }

    fn device_pixel_ratio(&self) -> f32 {
        get_window_dpr(self.hwnd)
    }

    fn native_size(&self) -> Option<(u32, u32)> {
        if self.hwnd.is_null() {
            return None;
        }
        let mut r: RECT = unsafe { std::mem::zeroed() };
        // Same rect `UpdateLayeredWindowIndirect` is given as `psize`.
        if unsafe { GetWindowRect(self.hwnd, &mut r) } == 0 {
            return None;
        }
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        (w > 0 && h > 0).then_some((w as u32, h as u32))
    }

    fn set_backdrop(&mut self, backdrop: crate::backdrop::BackdropType, dark_mode: bool) -> bool {
        crate::backdrop::set_window_backdrop(self.hwnd, backdrop, dark_mode)
    }

    fn set_ime_focus(&mut self, pos: qtrs_gui::geometry::primitives::Point) {
        if !self.ime_enabled.replace(true) {
            crate::ime::set_window_ime_enabled(self.hwnd, true);
        }
        let mut ime = crate::ime::Win32InputContext::new(self.hwnd);
        ime.set_micro_focus(pos);
    }

    fn enable_drop_target(&mut self, enabled: bool) -> bool {
        if enabled {
            if !self.drop_target.is_null() {
                return true;
            }
            let hwnd_isize = self.hwnd as isize;
            let callback = move |ev: crate::drag_drop::DropEvent| {
                let hwnd = hwnd_isize as HWND;
                match ev {
                    crate::drag_drop::DropEvent::Enter {
                        pos,
                        formats,
                        effect,
                    } => {
                        dispatch_window_system_event(
                            Delivery::Default,
                            hwnd,
                            WindowSystemEvent::DragEnter {
                                pos,
                                formats,
                                drop_action: effect,
                            },
                        );
                    }
                    crate::drag_drop::DropEvent::Over { pos, effect } => {
                        dispatch_window_system_event(
                            Delivery::Default,
                            hwnd,
                            WindowSystemEvent::DragMove {
                                pos,
                                drop_action: effect,
                            },
                        );
                    }
                    crate::drag_drop::DropEvent::Leave => {
                        dispatch_window_system_event(Delivery::Default, hwnd, WindowSystemEvent::DragLeave);
                    }
                    crate::drag_drop::DropEvent::Drop {
                        pos,
                        formats,
                        data,
                        effect,
                    } => {
                        dispatch_window_system_event(
                            Delivery::Default,
                            hwnd,
                            WindowSystemEvent::Drop {
                                pos,
                                formats,
                                data,
                                drop_action: effect,
                            },
                        );
                    }
                }
            };
            if let Ok(target) =
                crate::drag_drop::win32_ole::register_drop_target(self.hwnd, callback)
            {
                self.drop_target = target;
                true
            } else {
                false
            }
        } else {
            if !self.drop_target.is_null() {
                crate::drag_drop::win32_ole::revoke_drop_target(self.hwnd, self.drop_target);
                self.drop_target = std::ptr::null_mut();
            }
            true
        }
    }
}

#[cfg(windows)]
impl Drop for NativeWindow {
    fn drop(&mut self) {
        self.close();
    }
}
#[cfg(not(windows))]
pub type NativeWindow = crate::platform_window::GenericWindow;
#[allow(dead_code)]
#[inline]
pub(crate) fn edges_to_win_orientation(edges: crate::platform_window::WindowEdges) -> usize {
    use crate::platform_window::WindowEdges;
    if edges == WindowEdges::LEFT {
        0xf001 // SC_SIZELEFT
    } else if edges == WindowEdges::RIGHT {
        0xf002 // SC_SIZERIGHT
    } else if edges == WindowEdges::TOP {
        0xf003 // SC_SIZETOP
    } else if edges == (WindowEdges::TOP | WindowEdges::LEFT) {
        0xf004 // SC_SIZETOPLEFT
    } else if edges == (WindowEdges::TOP | WindowEdges::RIGHT) {
        0xf005 // SC_SIZETOPRIGHT
    } else if edges == WindowEdges::BOTTOM {
        0xf006 // SC_SIZEBOTTOM
    } else if edges == (WindowEdges::BOTTOM | WindowEdges::LEFT) {
        0xf007 // SC_SIZEBOTTOMLEFT
    } else if edges == (WindowEdges::BOTTOM | WindowEdges::RIGHT) {
        0xf008 // SC_SIZEBOTTOMRIGHT
    } else {
        0xf000 // SC_SIZE
    }
}
pub fn post_system_move(_hwnd: isize) -> bool {
    #[cfg(windows)]
    if _hwnd != 0 {
        unsafe {
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
            use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_SYSCOMMAND};
            ReleaseCapture();
            PostMessageW(_hwnd as HWND, WM_SYSCOMMAND, 0xF012 /*SC_DRAGMOVE*/, 0);
        }
        return true;
    }
    false
}

pub fn post_system_resize(_hwnd: isize, _edges: crate::platform_window::WindowEdges) -> bool {
    #[cfg(windows)]
    if _hwnd != 0 {
        unsafe {
            use windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
            use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_SYSCOMMAND};
            ReleaseCapture();
            let orientation = edges_to_win_orientation(_edges);
            PostMessageW(_hwnd as HWND, WM_SYSCOMMAND, orientation, 0);
        }
        return true;
    }
    false
}

pub fn calc_frameless_edge(_hwnd: isize, pos: qtrs_gui::geometry::primitives::Point, locked: bool) -> crate::platform_window::WindowEdges {
    if locked {
        return crate::platform_window::WindowEdges::empty();
    }
    const M: i32 = 8;
    #[cfg(windows)]
    let (w, h) = if _hwnd != 0 {
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;
            let mut rect: windows_sys::Win32::Foundation::RECT = std::mem::zeroed();
            GetClientRect(_hwnd as HWND, &mut rect);
            let dpr = get_window_dpr(_hwnd as HWND);
            (
                ((rect.right - rect.left) as f32 / dpr).round() as i32,
                ((rect.bottom - rect.top) as f32 / dpr).round() as i32,
            )
        }
    } else {
        (600, 400)
    };
    #[cfg(not(windows))]
    let (w, h) = (600, 400);

    let mut edges = crate::platform_window::WindowEdges::empty();
    if pos.x < M {
        edges |= crate::platform_window::WindowEdges::LEFT;
    } else if pos.x > w - M {
        edges |= crate::platform_window::WindowEdges::RIGHT;
    }
    if pos.y < M {
        edges |= crate::platform_window::WindowEdges::TOP;
    } else if pos.y > h - M {
        edges |= crate::platform_window::WindowEdges::BOTTOM;
    }
    edges
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, IsWindow, GWL_EXSTYLE, GWL_STYLE, WS_CAPTION, WS_EX_LAYERED,
        WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
    };

    #[test]
    fn test_window_flags_default_and_bits() {
        let default_flags = WindowFlags::default();
        assert_eq!(default_flags, WindowFlags::NORMAL);

        let combined = WindowFlags::FRAMELESS | WindowFlags::STAYS_ON_TOP | WindowFlags::LAYERED;
        assert!(combined.contains(WindowFlags::FRAMELESS));
        assert!(combined.contains(WindowFlags::STAYS_ON_TOP));
        assert!(combined.contains(WindowFlags::LAYERED));
        assert!(!combined.contains(WindowFlags::TOOL));
    }

    #[test]
    fn test_window_flags_to_win32_styles() {
        let rect = Rect::new(100, 100, 400, 300);
        let win = NativeWindow::new("Style Test Window", rect, WindowFlags::NORMAL)
            .expect("failed to create standard window");

        unsafe {
            assert_ne!(IsWindow(win.hwnd()), 0);
            let style = GetWindowLongPtrW(win.hwnd(), GWL_STYLE) as u32;
            let ex_style = GetWindowLongPtrW(win.hwnd(), GWL_EXSTYLE) as u32;

            assert_ne!(style & WS_OVERLAPPEDWINDOW, 0);
            assert_eq!(style & WS_POPUP, 0);
            assert_eq!(ex_style & WS_EX_LAYERED, 0);
        }

        let frameless_win = NativeWindow::new(
            "Frameless Test Window",
            rect,
            WindowFlags::FRAMELESS
                | WindowFlags::STAYS_ON_TOP
                | WindowFlags::LAYERED
                | WindowFlags::TOOL,
        )
        .expect("failed to create frameless layered window");

        unsafe {
            assert_ne!(IsWindow(frameless_win.hwnd()), 0);
            let style = GetWindowLongPtrW(frameless_win.hwnd(), GWL_STYLE) as u32;
            let ex_style = GetWindowLongPtrW(frameless_win.hwnd(), GWL_EXSTYLE) as u32;

            assert_ne!(style & WS_POPUP, 0);
            assert_ne!(ex_style & WS_EX_TOPMOST, 0);
            assert_ne!(ex_style & WS_EX_LAYERED, 0);
            assert_ne!(ex_style & WS_EX_TOOLWINDOW, 0);
        }

        let custom_win = NativeWindow::new(
            "Custom Frameless Test Window",
            rect,
            WindowFlags::CUSTOM_FRAMELESS | WindowFlags::STAYS_ON_TOP,
        )
        .expect("failed to create custom window");

        unsafe {
            assert_ne!(IsWindow(custom_win.hwnd()), 0);
            let style = GetWindowLongPtrW(custom_win.hwnd(), GWL_STYLE) as u32;
            let ex_style = GetWindowLongPtrW(custom_win.hwnd(), GWL_EXSTYLE) as u32;

            assert_ne!(style & WS_THICKFRAME, 0);
            assert_ne!(style & WS_CAPTION, 0);
            assert_ne!(style & WS_MAXIMIZEBOX, 0);
            assert_eq!(style & WS_POPUP, 0);
            assert_ne!(ex_style & WS_EX_TOPMOST, 0);
        }
    }

    #[test]
    fn test_custom_frameless_nccalcsize_and_nchittest() {
        let rect = Rect::new(200, 200, 600, 400);
        let mut win = NativeWindow::new(
            "NCCalcSize & NCHitTest Window",
            rect,
            WindowFlags::CUSTOM_FRAMELESS,
        )
        .expect("failed to create window");

        win.set_custom_frameless_config(40, 10);
        let cfg = win.custom_frameless_config().expect("failed to get config");
        assert_eq!(cfg.caption_height, 40);
        assert_eq!(cfg.resize_border, 10);

        unsafe {
            let mut ncp: NCCALCSIZE_PARAMS = std::mem::zeroed();
            ncp.rgrc[0] = RECT {
                left: 200,
                top: 200,
                right: 800,
                bottom: 600,
            };
            let ret = native_window_proc(win.hwnd(), WM_NCCALCSIZE, 1, &mut ncp as *mut _ as isize);
            assert_eq!(ret, 0);

            let lparam_topleft = (202 & 0xffff) | ((202 & 0xffff) << 16);
            let hit = native_window_proc(win.hwnd(), WM_NCHITTEST, 0, lparam_topleft);
            assert_eq!(hit, HTTOPLEFT as isize);

            let lparam_bottomright = (798 & 0xffff) | ((598 & 0xffff) << 16);
            let hit = native_window_proc(win.hwnd(), WM_NCHITTEST, 0, lparam_bottomright);
            assert_eq!(hit, HTBOTTOMRIGHT as isize);

            let lparam_caption = (500 & 0xffff) | ((220 & 0xffff) << 16);
            let hit = native_window_proc(win.hwnd(), WM_NCHITTEST, 0, lparam_caption);
            assert_eq!(hit, HTCAPTION as isize);

            let lparam_client = (500 & 0xffff) | ((300 & 0xffff) << 16);
            let hit = native_window_proc(win.hwnd(), WM_NCHITTEST, 0, lparam_client);
            assert_eq!(hit, HTCLIENT as isize);
        }
    }

    #[test]
    fn test_native_window_lifecycle_and_methods() {
        let rect = Rect::new(100, 100, 500, 300);
        let mut win = NativeWindow::new("Lifecycle Window", rect, WindowFlags::FRAMELESS)
            .expect("failed to create window");

        assert_eq!(win.geometry(), rect);
        assert_eq!(win.title(), "Lifecycle Window");

        let new_rect = Rect::new(150, 150, 600, 400);
        win.set_geometry(new_rect);
        assert_eq!(win.geometry(), new_rect);

        win.set_stays_on_top(true);
        assert!(win.flags().contains(WindowFlags::STAYS_ON_TOP));

        win.set_click_through(true);
        assert!(win.flags().contains(WindowFlags::CLICK_THROUGH));

        win.show();
        win.hide();

        win.close();
        assert!(win.hwnd().is_null());
    }

    #[test]
    fn test_native_window_wndproc_events() {
        let rect = Rect::new(50, 50, 300, 200);
        let win = NativeWindow::new("WndProc Event Window", rect, WindowFlags::NORMAL)
            .expect("failed to create window");

        unsafe {
            let erase_ret = native_window_proc(win.hwnd(), WM_ERASEBKGND, 0, 0);
            assert_eq!(erase_ret, 1);

            let mouse_pos = (50 & 0xffff) | ((60 & 0xffff) << 16);
            let _ = native_window_proc(win.hwnd(), WM_MOUSEMOVE, 0, mouse_pos);
            let _ = native_window_proc(win.hwnd(), WM_LBUTTONDOWN, 0, mouse_pos);
        }
    }
}
