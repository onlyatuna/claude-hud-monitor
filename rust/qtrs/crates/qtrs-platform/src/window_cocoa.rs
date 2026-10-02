use qtrs_core::event_loop::CocoaNativeEvent;
use qtrs_core::object::ThreadContext;
use qtrs_gui::geometry::primitives::{Point, Rect, Size};
use qtrs_gui::paint::Pixmap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::objc_runtime::{
    CGFloat, CGRect, CGSize, Class, Id, ObjcMsg, Sel, NS_BACKING_STORE_BUFFERED,
    NS_FLOATING_WINDOW_LEVEL, NS_WINDOW_STYLE_MASK_BORDERLESS, NS_WINDOW_STYLE_MASK_CLOSABLE,
    NS_WINDOW_STYLE_MASK_MINIATURIZABLE, NS_WINDOW_STYLE_MASK_RESIZABLE,
    NS_WINDOW_STYLE_MASK_TITLED,
};
use crate::platform_window::PlatformWindow;
use crate::surface::{CocoaLayerSurface, PlatformSurface};
use crate::window::WindowFlags;
use crate::window_system_interface::{
    KeyboardModifiers, MouseButton, WheelDelta, WindowSystemEvent, WindowSystemEventHandler,
};

pub struct CocoaNativeWindow {
    ns_window: Id,
    ns_view: Id,
    title: String,
    geometry: Rect,
    flags: WindowFlags,
    stays_on_top: bool,
    click_through: bool,
    visible: AtomicBool,
    surface: CocoaLayerSurface,
    pending_events: Mutex<Vec<CocoaNativeEvent>>,
    event_handler: Option<Box<dyn WindowSystemEventHandler>>,
    opacity: f32,
    min_size: (i32, i32),
}

unsafe impl Send for CocoaNativeWindow {}
unsafe impl Sync for CocoaNativeWindow {}

impl CocoaNativeWindow {
    pub fn new(title: &str, rect: Rect, flags: WindowFlags) -> Result<Self, &'static str> {
        ThreadContext::assert_main_thread("CocoaNativeWindow::new");

        let window_class = Class::get("NSWindow").ok_or("Cannot find NSWindow class")?;
        let view_class = Class::get("QNSView")
            .or_else(|| Class::get("NSView"))
            .ok_or("Cannot find class (QNSView / NSView)")?;

        let mut style_mask = if flags.contains(WindowFlags::FRAMELESS) {
            NS_WINDOW_STYLE_MASK_BORDERLESS
        } else {
            NS_WINDOW_STYLE_MASK_TITLED
                | NS_WINDOW_STYLE_MASK_CLOSABLE
                | NS_WINDOW_STYLE_MASK_MINIATURIZABLE
                | NS_WINDOW_STYLE_MASK_RESIZABLE
        };

        if flags.contains(WindowFlags::CUSTOM_FRAMELESS) {
            style_mask |= NS_WINDOW_STYLE_MASK_RESIZABLE;
        }

        let ns_window_alloc = ObjcMsg::send_class_0(window_class, Sel::register("alloc"));
        if ns_window_alloc.is_nil() {
            return Err("NSWindow allocation failed");
        }

        let cg_rect = CGRect::new(
            rect.x as f64,
            rect.y as f64,
            rect.width as f64,
            rect.height as f64,
        );

        let ns_window = ObjcMsg::send_window_init(
            ns_window_alloc,
            Sel::register("initWithContentRect:styleMask:backing:defer:"),
            cg_rect,
            style_mask,
            NS_BACKING_STORE_BUFFERED,
            false,
        );

        ObjcMsg::send_str(ns_window, Sel::register("setTitle:"), title);

        let ns_view_alloc = ObjcMsg::send_class_0(view_class, Sel::register("alloc"));
        let view_rect = CGRect::new(0.0, 0.0, rect.width as f64, rect.height as f64);
        let ns_view = ObjcMsg::send_window_init(
            ns_view_alloc,
            Sel::register("initWithFrame:"),
            view_rect,
            0,
            0,
            false,
        );

        ObjcMsg::send_id(ns_window, Sel::register("setContentView:"), ns_view);

        let stays_on_top = flags.contains(WindowFlags::STAYS_ON_TOP);
        if stays_on_top {
            ObjcMsg::send_int(
                ns_window,
                Sel::register("setLevel:"),
                NS_FLOATING_WINDOW_LEVEL,
            );
        }

        let click_through = flags.contains(WindowFlags::CLICK_THROUGH);
        if click_through {
            ObjcMsg::send_bool(ns_window, Sel::register("setIgnoresMouseEvents:"), true);
        }

        let surface = CocoaLayerSurface::new(
            ns_view.as_ptr() as usize,
            rect.width as u32,
            rect.height as u32,
        )?;

        Ok(Self {
            ns_window,
            ns_view,
            title: title.to_string(),
            geometry: rect,
            flags,
            stays_on_top,
            click_through,
            visible: AtomicBool::new(false),
            surface,
            pending_events: Mutex::new(Vec::new()),
            event_handler: None,
            opacity: 1.0,
            min_size: (0, 0),
        })
    }

    #[inline]
    pub fn ns_window(&self) -> Id {
        self.ns_window
    }

    #[inline]
    pub fn ns_view(&self) -> Id {
        self.ns_view
    }

    #[inline]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[inline]
    pub fn flags(&self) -> WindowFlags {
        self.flags
    }

    pub fn dispatch_cocoa_event(&mut self, event: CocoaNativeEvent) -> bool {
        let Some(handler) = self.event_handler.as_mut() else {
            return false;
        };

        let origin_x = self.geometry.x;
        let origin_y = self.geometry.y;

        match event {
            CocoaNativeEvent::MouseDown {
                x,
                y,
                button,
                modifiers,
            } => {
                let local_pos = Point::new(x as i32, y as i32);
                let global_pos = Point::new(origin_x + local_pos.x, origin_y + local_pos.y);
                let btn = match button {
                    0 => MouseButton::Left,
                    1 => MouseButton::Right,
                    2 => MouseButton::Middle,
                    _ => MouseButton::Left,
                };
                handler.handle_window_event(WindowSystemEvent::MousePress {
                    pos: local_pos,
                    global_pos,
                    button: btn,
                    modifiers: KeyboardModifiers::from_bits(modifiers),
                });
            }
            CocoaNativeEvent::MouseUp {
                x,
                y,
                button,
                modifiers,
            } => {
                let local_pos = Point::new(x as i32, y as i32);
                let global_pos = Point::new(origin_x + local_pos.x, origin_y + local_pos.y);
                let btn = match button {
                    0 => MouseButton::Left,
                    1 => MouseButton::Right,
                    2 => MouseButton::Middle,
                    _ => MouseButton::Left,
                };
                handler.handle_window_event(WindowSystemEvent::MouseRelease {
                    pos: local_pos,
                    global_pos,
                    button: btn,
                    modifiers: KeyboardModifiers::from_bits(modifiers),
                });
            }
            CocoaNativeEvent::MouseMoved { x, y, modifiers: _ } => {
                let local_pos = Point::new(x as i32, y as i32);
                let global_pos = Point::new(origin_x + local_pos.x, origin_y + local_pos.y);
                handler.handle_window_event(WindowSystemEvent::MouseMove {
                    pos: local_pos,
                    global_pos,
                });
            }
            CocoaNativeEvent::ScrollWheel {
                x,
                y,
                delta_x,
                delta_y,
            } => {
                let local_pos = Point::new(x as i32, y as i32);
                let global_pos = Point::new(origin_x + local_pos.x, origin_y + local_pos.y);
                handler.handle_window_event(WindowSystemEvent::Wheel {
                    pos: local_pos,
                    global_pos,
                    delta: WheelDelta::new(delta_x as i32, delta_y as i32),
                    modifiers: KeyboardModifiers::default(),
                });
            }
            CocoaNativeEvent::KeyDown {
                key_code,
                modifiers,
                is_repeat,
            } => {
                handler.handle_window_event(WindowSystemEvent::KeyPress {
                    key: key_code as u32,
                    modifiers: KeyboardModifiers::from_bits(modifiers),
                    is_repeat,
                });
            }
            CocoaNativeEvent::KeyUp {
                key_code,
                modifiers,
            } => {
                handler.handle_window_event(WindowSystemEvent::KeyRelease {
                    key: key_code as u32,
                    modifiers: KeyboardModifiers::from_bits(modifiers),
                });
            }
            CocoaNativeEvent::WindowResized { width, height } => {
                self.geometry.width = width as i32;
                self.geometry.height = height as i32;
                let _ = self.surface.resize(width as u32, height as u32);
                handler.handle_window_event(WindowSystemEvent::Resize {
                    size: Size::new(width as i32, height as i32),
                });
            }
            CocoaNativeEvent::WindowCloseRequested => {
                handler.handle_window_event(WindowSystemEvent::CloseRequest);
            }
        }
        true
    }

    pub fn queue_cocoa_event(&self, event: CocoaNativeEvent) {
        self.pending_events.lock().unwrap().push(event);
    }

    #[inline]
    pub fn is_visible(&self) -> bool {
        self.visible.load(Ordering::Acquire)
    }

    #[inline]
    pub fn is_flipped(&self) -> bool {
        ObjcMsg::send_bool_return(self.ns_view, Sel::register("isFlipped"))
    }
}

impl PlatformWindow for CocoaNativeWindow {
    fn show(&self) {
        ObjcMsg::send_id(
            self.ns_window,
            Sel::register("makeKeyAndOrderFront:"),
            Id::NIL,
        );
        self.visible.store(true, Ordering::Release);
    }

    fn hide(&self) {
        ObjcMsg::send_id(self.ns_window, Sel::register("orderOut:"), Id::NIL);
        self.visible.store(false, Ordering::Release);
    }

    fn geometry(&self) -> Rect {
        self.geometry
    }

    fn set_geometry(&mut self, rect: Rect) {
        self.geometry = rect;
        let cg_rect = CGRect::new(
            rect.x as f64,
            rect.y as f64,
            rect.width as f64,
            rect.height as f64,
        );
        ObjcMsg::send_window_init(
            self.ns_window,
            Sel::register("setFrame:display:"),
            cg_rect,
            0,
            0,
            true,
        );
        let _ = self.surface.resize(rect.width as u32, rect.height as u32);
    }

    fn set_stays_on_top(&mut self, enabled: bool) {
        self.stays_on_top = enabled;
        let level = if enabled { NS_FLOATING_WINDOW_LEVEL } else { 0 };
        ObjcMsg::send_int(self.ns_window, Sel::register("setLevel:"), level);
    }

    fn set_click_through(&mut self, enabled: bool) {
        self.click_through = enabled;
        ObjcMsg::send_bool(
            self.ns_window,
            Sel::register("setIgnoresMouseEvents:"),
            enabled,
        );
    }

    fn start_system_drag(&self) {
        let _ = self.start_system_move();
    }

    fn start_system_move(&self) -> bool {
        let nsapp = ObjcMsg::send_class_0(
            Class::get("NSApplication").unwrap_or(Class::NIL),
            Sel::register("sharedApplication"),
        );
        if !nsapp.is_nil() {
            let current_event = ObjcMsg::send_0(nsapp, Sel::register("currentEvent"));
            if !current_event.is_nil() {
                ObjcMsg::send_id(
                    self.ns_window,
                    Sel::register("performWindowDragWithEvent:"),
                    current_event,
                );
            }
        }
        true
    }

    fn start_system_resize(&self, _edges: crate::platform_window::WindowEdges) -> bool {
        true
    }

    fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity.clamp(0.0, 1.0);
        ObjcMsg::send_length(
            self.ns_window,
            Sel::register("setAlphaValue:"),
            self.opacity as CGFloat,
        );
    }

    fn opacity(&self) -> f32 {
        self.opacity
    }

    fn set_minimum_size(&mut self, min_w: i32, min_h: i32) {
        self.min_size = (min_w.max(0), min_h.max(0));
        ObjcMsg::send_size(
            self.ns_window,
            Sel::register("setMinSize:"),
            CGSize::new(self.min_size.0 as CGFloat, self.min_size.1 as CGFloat),
        );
    }

    fn minimum_size(&self) -> (i32, i32) {
        self.min_size
    }

    fn present(&mut self, pixmap: &mut Pixmap, opacity: f32) -> Result<(), &'static str> {
        self.surface.present(pixmap, opacity)
    }

    fn set_event_handler(&mut self, handler: Box<dyn WindowSystemEventHandler>) {
        self.event_handler = Some(handler);
    }

    fn native_handle(&self) -> isize {
        self.ns_window.0 as isize
    }

    fn set_backdrop(&mut self, backdrop: crate::backdrop::BackdropType, dark_mode: bool) -> bool {
        crate::backdrop::set_cocoa_window_backdrop(
            self.ns_window,
            self.ns_view,
            backdrop,
            dark_mode,
        )
    }

    fn poll_events(&mut self) -> usize {
        let events = std::mem::take(&mut *self.pending_events.lock().unwrap());
        let count = events.len();
        for event in events {
            self.dispatch_cocoa_event(event);
        }
        count
    }
}

#[inline]
pub fn qt_mac_flip_point(pos: Point, reference_height: i32) -> Point {
    Point::new(pos.x, reference_height - pos.y)
}

#[inline]
pub fn qt_mac_flip_rect(rect: Rect, reference_height: i32) -> Rect {
    Rect::new(
        rect.x,
        reference_height - (rect.y + rect.height),
        rect.width,
        rect.height,
    )
}

#[inline]
pub fn qt_mac_primary_screen_height() -> i32 {
    use crate::screen::PlatformScreen;
    crate::screen::GenericScreen::default_primary()
        .geometry()
        .height
}

#[inline]
pub fn qt_mac_flip_global_point(pos: Point) -> Point {
    qt_mac_flip_point(pos, qt_mac_primary_screen_height())
}

#[inline]
pub fn qt_mac_flip_global_rect(rect: Rect) -> Rect {
    qt_mac_flip_rect(rect, qt_mac_primary_screen_height())
}
