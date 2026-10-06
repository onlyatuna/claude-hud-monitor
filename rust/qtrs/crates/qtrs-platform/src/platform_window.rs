use bitflags::bitflags;

bitflags! {
    /// Edges of a window used for border resizing, exactly matching Qt::Edges.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct WindowEdges: u32 {
        const LEFT = 0x01;
        const TOP = 0x02;
        const RIGHT = 0x04;
        const BOTTOM = 0x08;
        const TOP_LEFT = Self::TOP.bits() | Self::LEFT.bits();
        const TOP_RIGHT = Self::TOP.bits() | Self::RIGHT.bits();
        const BOTTOM_LEFT = Self::BOTTOM.bits() | Self::LEFT.bits();
        const BOTTOM_RIGHT = Self::BOTTOM.bits() | Self::RIGHT.bits();
    }
}

pub type WindowEdge = WindowEdges;

pub trait PlatformWindow: 'static {
    fn show(&self);
    fn hide(&self);
    fn geometry(&self) -> Rect;
    fn set_geometry(&mut self, rect: Rect);
    fn set_stays_on_top(&mut self, enabled: bool);
    fn set_click_through(&mut self, enabled: bool);
    fn start_system_drag(&self) {
        let _ = self.start_system_move();
    }
    fn start_system_move(&self) -> bool {
        self.start_system_drag();
        true
    }
    fn start_system_resize(&self, _edges: WindowEdges) -> bool {
        false
    }
    fn set_cursor(&mut self, _shape: crate::cursor::CursorShape) {}
    fn set_opacity(&mut self, opacity: f32);
    fn opacity(&self) -> f32;
    fn set_minimum_size(&mut self, min_w: i32, min_h: i32);
    fn minimum_size(&self) -> (i32, i32);
    /// Ratio between this window's physical (native) pixels and its device-independent pixels,
    /// taken from the screen the window is on (`QPlatformWindow::devicePixelRatio`, which
    /// `QWindowPrivate::updateDevicePixelRatio` multiplies by the window's screen scale factor,
    /// `qwindow.cpp:1436-1443`). Required: it differs between windows on different screens.
    fn device_pixel_ratio(&self) -> f32;
    fn present(&mut self, pixmap: &mut Pixmap, opacity: f32) -> Result<(), &'static str>;
    fn present_dirty(
        &mut self,
        pixmap: &mut Pixmap,
        opacity: f32,
        _dirty: Rect,
    ) -> Result<(), &'static str> {
        self.present(pixmap, opacity)
    }
    fn present_region(
        &mut self,
        pixmap: &Pixmap,
        dirty: &qtrs_gui::geometry::Region,
    ) -> Result<(), &'static str> {
        let mut clone = (*pixmap).clone();
        let opacity = self.opacity();
        self.present_dirty(&mut clone, opacity, dirty.bounding_rect())
    }
    fn set_event_handler(&mut self, handler: Box<dyn WindowSystemEventHandler>);
    fn set_target_pos(&mut self, _pos: Option<qtrs_gui::geometry::Point>) {}
    fn native_handle(&self) -> isize;
    fn poll_events(&mut self) -> usize {
        0
    }
    fn set_backdrop(&mut self, _backdrop: crate::backdrop::BackdropType, _dark_mode: bool) -> bool {
        false
    }
    fn set_ime_focus(&mut self, _pos: qtrs_gui::geometry::primitives::Point) {}
    fn enable_drop_target(&mut self, _enabled: bool) -> bool {
        false
    }
    fn state_flags(&self) -> crate::window::PlatformWindowStateFlags {
        crate::window::PlatformWindowStateFlags::NONE
    }
    fn owner_thread(&self) -> std::thread::ThreadId {
        std::thread::current().id()
    }
    /// The window's current size in native (physical) pixels, if the platform can report it.
    ///
    /// The paint surface is sized to this instead of `round(logical * dpr)`, so the content and the
    /// native window always have the same pixel size (the logical round trip is off by 1 px at
    /// fractional scale factors, e.g. 427 -> 342 -> 428 at 125%).
    fn native_size(&self) -> Option<(u32, u32)> {
        None
    }
    fn is_within_set_geometry(&self) -> bool {
        self.state_flags().contains(crate::window::PlatformWindowStateFlags::WITHIN_SET_GEOMETRY)
    }
}
use crate::window::WindowFlags;
use crate::window_system_interface::WindowSystemEventHandler;
use qtrs_gui::geometry::primitives::Rect;
use qtrs_gui::paint::Pixmap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

pub struct GenericWindow {
    geometry: Rect,
    visible: AtomicBool,
    stays_on_top: bool,
    click_through: bool,
    handler: Option<Box<dyn WindowSystemEventHandler>>,
    pending_events: Mutex<Vec<crate::window_system_interface::WindowSystemEvent>>,
    last_pixmap: Mutex<Option<Pixmap>>,
    opacity: f32,
    min_size: (i32, i32),
    owner_thread: std::thread::ThreadId,
    state_flags: std::cell::Cell<crate::window::PlatformWindowStateFlags>,
}
impl GenericWindow {
    pub fn new(_title: &str, rect: Rect, flags: WindowFlags) -> Self {
        Self {
            geometry: rect,
            visible: AtomicBool::new(false),
            stays_on_top: flags.contains(WindowFlags::STAYS_ON_TOP),
            click_through: flags.contains(WindowFlags::CLICK_THROUGH),
            handler: None,
            pending_events: Mutex::new(Vec::new()),
            last_pixmap: Mutex::new(None),
            opacity: 1.0,
            min_size: (0, 0),
            owner_thread: std::thread::current().id(),
            state_flags: std::cell::Cell::new(crate::window::PlatformWindowStateFlags::NONE),
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible.load(Ordering::Acquire)
    }

    pub fn queue_event(&self, event: crate::window_system_interface::WindowSystemEvent) {
        self.pending_events.lock().unwrap().push(event);
    }
}

impl PlatformWindow for GenericWindow {
    fn state_flags(&self) -> crate::window::PlatformWindowStateFlags {
        self.state_flags.get()
    }
    fn owner_thread(&self) -> std::thread::ThreadId {
        self.owner_thread
    }
    fn show(&self) {
        self.visible.store(true, Ordering::Release);
    }

    fn hide(&self) {
        self.visible.store(false, Ordering::Release);
    }

    fn geometry(&self) -> Rect {
        self.geometry
    }

    fn set_geometry(&mut self, rect: Rect) {
        self.geometry = rect;
    }

    fn set_stays_on_top(&mut self, enabled: bool) {
        self.stays_on_top = enabled;
    }

    fn set_click_through(&mut self, enabled: bool) {
        self.click_through = enabled;
    }

    fn start_system_drag(&self) {
        let _ = self.start_system_move();
    }

    fn start_system_move(&self) -> bool {
        true
    }

    fn start_system_resize(&self, _edges: WindowEdges) -> bool {
        true
    }

    fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity.clamp(0.0, 1.0);
    }

    fn opacity(&self) -> f32 {
        self.opacity
    }

    fn set_minimum_size(&mut self, min_w: i32, min_h: i32) {
        self.min_size = (min_w.max(0), min_h.max(0));
    }

    fn minimum_size(&self) -> (i32, i32) {
        self.min_size
    }

    /// `GenericWindow` has no screen of its own: it reports the primary screen's ratio.
    fn device_pixel_ratio(&self) -> f32 {
        crate::platform().primary_screen().device_pixel_ratio()
    }

    fn present(&mut self, pixmap: &mut Pixmap, opacity: f32) -> Result<(), &'static str> {
        let mut guard = self.last_pixmap.lock().unwrap();
        *guard = Some((*pixmap).clone());
        self.opacity = opacity;
        Ok(())
    }

    fn set_event_handler(&mut self, handler: Box<dyn WindowSystemEventHandler>) {
        self.handler = Some(handler);
    }

    fn native_handle(&self) -> isize {
        0
    }

    fn poll_events(&mut self) -> usize {
        let events = std::mem::take(&mut *self.pending_events.lock().unwrap());
        let count = events.len();
        if let Some(handler) = self.handler.as_mut() {
            for event in events {
                handler.handle_window_event(event);
            }
        }
        count
    }
}
