use crate::widget::{EmptyWidget, WidgetRef};
use qtrs_core::event::{Event, EventKind};
use qtrs_core::object::{register_qobject, unregister_qobject, ObjectData, ObjectId, QObject};
use qtrs_gui::geometry::primitives::{Point, Rect, RectF, Size};
use qtrs_gui::paint::{PaintDevice, Painter, Pixmap};
use qtrs_platform::{
    platform, PlatformWindow, WindowFlags, WindowSystemEvent, WindowSystemEventHandler,
};
use std::sync::{Arc, Mutex};

use crate::hit_test::EventTreeDispatcher;

pub struct Window {
    object_data: ObjectData,
    platform_window: Arc<Mutex<Box<dyn PlatformWindow>>>,
    root_widget: WidgetRef,
    shared_root: Arc<Mutex<WidgetRef>>,
    backing_store: Arc<Mutex<Pixmap>>,
    geometry: Arc<Mutex<Rect>>,
    context_menu_cb: Arc<Mutex<Option<Box<dyn Fn(Point) + Send + Sync>>>>,
    mouse_press_cb: Arc<Mutex<Option<Box<dyn Fn(Point, qtrs_platform::MouseButton) -> bool + Send + Sync>>>>,
    mouse_move_cb: Arc<Mutex<Option<Box<dyn Fn(Point) + Send + Sync>>>>,
    resize_cb: Arc<Mutex<Option<Box<dyn Fn(Size) + Send + Sync>>>>,
}

impl Window {
    pub fn new(title: &str, geometry: Rect, flags: WindowFlags) -> Result<Self, &'static str> {
        let p = platform();
        let dpr = p.primary_screen().device_pixel_ratio();
        let native_rect = if dpr > 1.0 {
            qtrs_platform::high_dpi::to_native_rect(geometry, dpr)
        } else {
            geometry
        };
        let platform_win = p.create_window(title, native_rect, flags)?;
        let physical_w = ((geometry.width.max(1) as f32) * dpr).round() as u32;
        let physical_h = ((geometry.height.max(1) as f32) * dpr).round() as u32;
        let backing_store = Pixmap::with_dpr(physical_w, physical_h, dpr)
            .ok_or("Failed to create top-level window offscreen Pixmap backing store")?;
        let window_id = ObjectId::next();

        let root_widget: WidgetRef = std::rc::Rc::new(std::cell::RefCell::new(Box::new(
            EmptyWidget::with_geometry(Rect::new(0, 0, geometry.width, geometry.height)),
        )));
        root_widget.borrow_mut().set_window_id(Some(window_id));

        let root_clone = std::rc::Rc::clone(&root_widget);
        #[allow(clippy::arc_with_non_send_sync)]
        let shared_root = Arc::new(Mutex::new(root_clone));
        let root_for_handler = Arc::clone(&shared_root);
        let context_menu_cb = Arc::new(Mutex::new(None));
        let cb_clone = Arc::clone(&context_menu_cb);
        let mouse_press_cb = Arc::new(Mutex::new(None));
        let press_cb_clone = Arc::clone(&mouse_press_cb);
        let mouse_move_cb = Arc::new(Mutex::new(None));
        let move_cb_clone = Arc::clone(&mouse_move_cb);
        let resize_cb = Arc::new(Mutex::new(None));
        let resize_cb_clone = Arc::clone(&resize_cb);

        let bs_arc = Arc::new(Mutex::new(backing_store));
        let bs_clone = Arc::clone(&bs_arc);
        let geom_arc = Arc::new(Mutex::new(geometry));
        let geom_clone = Arc::clone(&geom_arc);

        let pw_arc = Arc::new(Mutex::new(platform_win));
        let pw_clone = Arc::clone(&pw_arc);

        let handler = WindowEventHandler {
            platform_window: pw_clone,
            backing_store: bs_clone,
            geometry: geom_clone,
            root: root_for_handler,
            dispatcher: EventTreeDispatcher::new(),
            context_menu_cb: cb_clone,
            mouse_press_cb: press_cb_clone,
            mouse_move_cb: move_cb_clone,
            resize_cb: resize_cb_clone,
        };
        pw_arc.lock().unwrap().set_event_handler(Box::new(handler));

        let win = Self {
            object_data: ObjectData::new(window_id),
            platform_window: pw_arc,
            root_widget,
            shared_root,
            backing_store: bs_arc,
            geometry: geom_arc,
            context_menu_cb,
            mouse_press_cb,
            mouse_move_cb,
            resize_cb,
        };
        crate::application::Application::register_window(window_id);
        Ok(win)
    }

    /// Registers this window for QObject ID-based dispatch.
    ///
    /// # Safety
    /// Keep the window alive and unmoved on its registration thread until it is unregistered.
    /// Do not access it through aliases while registry callbacks run.
    pub unsafe fn register(&mut self) {
        // SAFETY: delegated to this method's caller contract.
        unsafe { register_qobject(self) };
    }

    pub fn id(&self) -> ObjectId {
        self.object_data.id
    }

    pub fn root_widget(&self) -> WidgetRef {
        std::rc::Rc::clone(&self.root_widget)
    }
    pub fn set_root_widget(&mut self, widget: WidgetRef) {
        widget.borrow_mut().set_window_id(Some(self.object_data.id));
        let geom = *self.geometry.lock().unwrap();
        widget
            .borrow_mut()
            .set_geometry(Rect::new(0, 0, geom.width, geom.height));
        *self.shared_root.lock().unwrap() = widget.clone();
        self.root_widget = widget;
    }

    pub fn geometry(&self) -> Rect {
        *self.geometry.lock().unwrap()
    }

    pub fn physical_geometry(&self) -> Rect {
        self.platform_window.lock().unwrap().geometry()
    }

    pub fn set_geometry(&mut self, rect: Rect) {
        *self.geometry.lock().unwrap() = rect;
        let dpr = platform().primary_screen().device_pixel_ratio();
        let native_rect = if dpr > 1.0 {
            qtrs_platform::high_dpi::to_native_rect(rect, dpr)
        } else {
            rect
        };
        self.platform_window.lock().unwrap().set_geometry(native_rect);
        let physical_w = ((rect.width.max(1) as f32) * dpr).round() as u32;
        let physical_h = ((rect.height.max(1) as f32) * dpr).round() as u32;
        {
            let mut bs = self.backing_store.lock().unwrap();
            if bs.physical_width() != physical_w || bs.physical_height() != physical_h {
                if let Some(new_pixmap) = Pixmap::with_dpr(physical_w, physical_h, dpr) {
                    *bs = new_pixmap;
                }
            }
        }

        {
            let mut root = self.root_widget.borrow_mut();
            root.set_geometry(Rect::new(0, 0, rect.width, rect.height));
            if let Some(layout) = root.layout_mut() {
                layout.update_layout();
            }
            root.update();
        }
        self.render_and_present();
    }
    pub fn show(&mut self) {
        self.platform_window.lock().unwrap().show();
        self.render_and_present();
    }

    pub fn hide(&mut self) {
        self.platform_window.lock().unwrap().hide();
    }

    pub fn set_stays_on_top(&mut self, enabled: bool) {
        self.platform_window.lock().unwrap().set_stays_on_top(enabled);
    }

    pub fn set_click_through(&mut self, enabled: bool) {
        self.platform_window.lock().unwrap().set_click_through(enabled);
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.platform_window.lock().unwrap().set_opacity(opacity);
        self.render_and_present();
    }
    pub fn set_style_sheet(&mut self, qss: &str) {
        crate::application::Application::set_style_sheet(qss);
        self.render_and_present();
    }

    pub fn opacity(&self) -> f32 {
        self.platform_window.lock().unwrap().opacity()
    }

    pub fn set_minimum_size(&mut self, min_w: i32, min_h: i32) {
        self.platform_window.lock().unwrap().set_minimum_size(min_w, min_h);
    }

    pub fn minimum_size(&self) -> (i32, i32) {
        self.platform_window.lock().unwrap().minimum_size()
    }

    pub fn start_system_drag(&self) {
        self.platform_window.lock().unwrap().start_system_drag();
    }
    pub fn start_system_move(&self) -> bool {
        self.platform_window.lock().unwrap().start_system_move()
    }

    pub fn start_system_resize(&self, edges: qtrs_platform::platform_window::WindowEdges) -> bool {
        self.platform_window.lock().unwrap().start_system_resize(edges)
    }

    pub fn set_cursor(&mut self, shape: qtrs_platform::cursor::CursorShape) {
        self.platform_window.lock().unwrap().set_cursor(shape);
    }

    pub fn set_mouse_press_handler<F: Fn(Point, qtrs_platform::MouseButton) -> bool + Send + Sync + 'static>(&mut self, handler: F) {
        let mut cb = self.mouse_press_cb.lock().unwrap();
        *cb = Some(Box::new(handler));
    }

    pub fn set_mouse_move_handler<F: Fn(Point) + Send + Sync + 'static>(&mut self, handler: F) {
        let mut cb = self.mouse_move_cb.lock().unwrap();
        *cb = Some(Box::new(handler));
    }
    pub fn set_resize_handler<F: Fn(Size) + Send + Sync + 'static>(&mut self, handler: F) {
        let mut cb = self.resize_cb.lock().unwrap();
        *cb = Some(Box::new(handler));
    }
    pub fn set_backdrop(
        &mut self,
        backdrop: qtrs_platform::backdrop::BackdropType,
        dark_mode: bool,
    ) -> bool {
        self.platform_window.lock().unwrap().set_backdrop(backdrop, dark_mode)
    }

    pub fn set_ime_focus(&mut self, pos: Point) {
        self.platform_window.lock().unwrap().set_ime_focus(pos);
    }

    pub fn enable_drop_target(&mut self, enabled: bool) -> bool {
        self.platform_window.lock().unwrap().enable_drop_target(enabled)
    }

    pub fn native_handle(&self) -> isize {
        self.platform_window.lock().unwrap().native_handle()
    }

    pub fn set_context_menu_handler<F: Fn(Point) + Send + Sync + 'static>(&mut self, handler: F) {
        let mut cb = self.context_menu_cb.lock().unwrap();
        *cb = Some(Box::new(handler));
    }

    pub fn render_and_present(&mut self) {
        let geom = *self.geometry.lock().unwrap();
        let root = self.root_widget.clone();
        let mut bs = self.backing_store.lock().unwrap();
        let mut pw = self.platform_window.lock().unwrap();
        do_render_and_present(&mut **pw, &mut *bs, &root, geom);
    }

    pub fn present_custom<F: FnOnce(&mut Painter)>(&mut self, f: F) {
        let mut bs = self.backing_store.lock().unwrap();
        bs.fill(qtrs_gui::tiny_skia::Color::TRANSPARENT);
        {
            let mut painter = Painter::begin(&mut *bs);
            f(&mut painter);
        }
        let mut pw = self.platform_window.lock().unwrap();
        let opacity = pw.opacity();
        let phys_dirty = Rect::new(
            0,
            0,
            bs.physical_width() as i32,
            bs.physical_height() as i32,
        );
        let _ = pw.present_dirty(&mut *bs, opacity, phys_dirty);
    }

    pub fn backing_store(&self) -> std::sync::MutexGuard<'_, Pixmap> {
        self.backing_store.lock().unwrap()
    }

    pub fn save_png(&self, path: &std::path::Path) -> Result<(), &'static str> {
        self.backing_store.lock().unwrap().save_png(path).map_err(|_| "failed to save PNG")
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        crate::application::Application::unregister_window(self.object_data.id);
        // SAFETY: an unsafe registration caller must ensure no callbacks remain active at drop.
        unsafe { unregister_qobject(self.object_data.id) };
    }
}

impl QObject for Window {
    fn object_data(&self) -> &ObjectData {
        &self.object_data
    }

    fn object_data_mut(&mut self) -> &mut ObjectData {
        &mut self.object_data
    }

    fn as_qobject_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_qobject_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn event(&mut self, event: &mut Event) -> bool {
        match &event.kind {
            EventKind::UpdateRequest => {
                self.render_and_present();
                true
            }
            EventKind::Resize { width, height, .. } => {
                {
                    let mut geom = self.geometry.lock().unwrap();
                    geom.width = *width;
                    geom.height = *height;
                }
                {
                    let mut bs = self.backing_store.lock().unwrap();
                    let dpr = bs.device_pixel_ratio();
                    let physical_w = ((*width as f32).max(1.0) * dpr).round() as u32;
                    let physical_h = ((*height as f32).max(1.0) * dpr).round() as u32;
                    if bs.physical_width() != physical_w
                        || bs.physical_height() != physical_h
                    {
                        if let Some(new_pixmap) = Pixmap::with_dpr(physical_w, physical_h, dpr) {
                            *bs = new_pixmap;
                        }
                    }
                }
                {
                    let mut root = self.root_widget.borrow_mut();
                    root.set_geometry(Rect::new(0, 0, *width, *height));
                    if let Some(layout) = root.layout_mut() {
                        layout.update_layout();
                    }
                    root.update();
                }
                if let Some(cb) = self.resize_cb.lock().unwrap().as_ref() {
                    cb(Size::new(*width, *height));
                }
                self.render_and_present();
                true
            }
            EventKind::DpiChanged { dpi_x, .. } => {
                let old_dpr = self.backing_store.lock().unwrap().device_pixel_ratio();
                let new_dpr = (*dpi_x as f32) / 96.0;
                let cur_geom = *self.geometry.lock().unwrap();
                let physical_w = ((cur_geom.width.max(1) as f32) * new_dpr).round() as u32;
                let physical_h = ((cur_geom.height.max(1) as f32) * new_dpr).round() as u32;
                if let Some(new_pixmap) = Pixmap::with_dpr(physical_w, physical_h, new_dpr) {
                    *self.backing_store.lock().unwrap() = new_pixmap;
                    // Propagate DPI changed recursively through the widget tree
                    propagate_dpi_change_recursive(&self.root_widget, old_dpr, new_dpr);

                    // Invalidate and re-layout root widget tree
                    let mut root = self.root_widget.borrow_mut();
                    let root_w = cur_geom.width;
                    let root_h = cur_geom.height;
                    root.set_geometry(Rect::new(0, 0, root_w, root_h));
                    if let Some(layout) = root.layout_mut() {
                        layout.update_layout();
                    }
                    root.update();
                    drop(root);
                    self.render_and_present();
                }
                true
            }
            _ => false,
        }
    }
}
/// Recursively propagates DPI change to all widgets in the tree,
/// triggering `dpi_changed_event` and refreshing nested layouts.
pub fn propagate_dpi_change_recursive(widget_ref: &WidgetRef, old_dpr: f32, new_dpr: f32) {
    let mut widget = widget_ref.borrow_mut();
    widget.dpi_changed_event(old_dpr, new_dpr);
    let geom = widget.geometry();
    if let Some(layout) = widget.layout_mut() {
        layout.set_geometry(Rect::new(0, 0, geom.width, geom.height));
    }
    let children = widget.children();
    drop(widget);

    for child in children {
        propagate_dpi_change_recursive(&child, old_dpr, new_dpr);
    }
}

fn render_widget_recursive(widget_ref: &WidgetRef, painter: &mut Painter, dirty_in_parent: Rect) {
    let mut widget = widget_ref.borrow_mut();
    if !widget.is_visible() {
        return;
    }

    let geom = widget.geometry();
    if !geom.intersects(&dirty_in_parent) {
        return;
    }

    let child_dirty = dirty_in_parent
        .intersected(&geom)
        .translated(-geom.x, -geom.y);

    painter.save();
    painter.translate(geom.x as f32, geom.y as f32);

    widget.paint_event(painter);

    let children = widget.children();
    drop(widget);

    for child in children {
        render_widget_recursive(&child, painter, child_dirty);
    }

    painter.restore();
}

/// Recursively collects and unifies dirty rectangles across the widget tree.
pub fn collect_dirty_region(widget_ref: &WidgetRef, offset: Point) -> Option<Rect> {
    let mut w = widget_ref.borrow_mut();
    let geom = w.geometry();
    let current_offset = Point::new(offset.x + geom.x, offset.y + geom.y);

    let mut dirty_union = w.dirty_rect().map(|d| {
        Rect::new(
            current_offset.x + d.x,
            current_offset.y + d.y,
            d.width,
            d.height,
        )
    });
    w.clear_dirty();

    let children = w.children();
    drop(w);

    for child in children {
        if let Some(child_dirty) = collect_dirty_region(&child, current_offset) {
            dirty_union = match dirty_union {
                Some(u) => Some(u.united(&child_dirty)),
                None => Some(child_dirty),
            };
        }
    }

    dirty_union
}
fn do_render_and_present(
    platform_window: &mut dyn PlatformWindow,
    backing_store: &mut Pixmap,
    root_widget: &WidgetRef,
    geometry: Rect,
) {
    let root_geom = Rect::new(0, 0, geometry.width, geometry.height);
    let dirty = collect_dirty_region(root_widget, Point::new(0, 0))
        .unwrap_or(root_geom)
        .intersected(&root_geom);

    if dirty.is_empty() {
        return;
    }

    let dpr = platform().primary_screen().device_pixel_ratio();
    let phys_dirty = if dpr > 1.0 {
        qtrs_platform::high_dpi::to_native_rect(dirty, dpr)
    } else {
        dirty
    };

    backing_store.clear_rect(phys_dirty);

    {
        let mut painter = Painter::begin(backing_store);
        painter.set_clip_rect(RectF::new(
            dirty.x as f32,
            dirty.y as f32,
            dirty.width as f32,
            dirty.height as f32,
        ));
        render_widget_recursive(root_widget, &mut painter, dirty);
    }

    let opacity = platform_window.opacity();
    let _ = platform_window.present_dirty(backing_store, opacity, phys_dirty);
}

struct WindowEventHandler {
    platform_window: Arc<Mutex<Box<dyn PlatformWindow>>>,
    backing_store: Arc<Mutex<Pixmap>>,
    geometry: Arc<Mutex<Rect>>,
    root: Arc<Mutex<WidgetRef>>,
    dispatcher: EventTreeDispatcher,
    context_menu_cb: Arc<Mutex<Option<Box<dyn Fn(Point) + Send + Sync>>>>,
    mouse_press_cb: Arc<Mutex<Option<Box<dyn Fn(Point, qtrs_platform::MouseButton) -> bool + Send + Sync>>>>,
    mouse_move_cb: Arc<Mutex<Option<Box<dyn Fn(Point) + Send + Sync>>>>,
    resize_cb: Arc<Mutex<Option<Box<dyn Fn(Size) + Send + Sync>>>>,
}
unsafe impl Send for WindowEventHandler {}
unsafe impl Sync for WindowEventHandler {}

impl WindowSystemEventHandler for WindowEventHandler {
    fn handle_window_event(&mut self, event: WindowSystemEvent) {
        let root = self.root.lock().unwrap().clone();
        match event {
            WindowSystemEvent::MouseMove { pos, .. } => {
                let mut ev = Event::new_spontaneous(EventKind::MouseMove { x: pos.x, y: pos.y });
                self.dispatcher.dispatch_event(&root, &mut ev);
                if let Some(cb) = self.mouse_move_cb.lock().unwrap().as_ref() {
                    cb(pos);
                }
            }
            WindowSystemEvent::MouseLeave => {
                self.dispatcher.handle_mouse_leave();
            }
            WindowSystemEvent::MousePress { pos, button, .. } => {
                let btn = match button {
                    qtrs_platform::MouseButton::Left => 1,
                    qtrs_platform::MouseButton::Right => 2,
                    qtrs_platform::MouseButton::Middle => 3,
                    _ => 0,
                };
                let mut ev = Event::new_spontaneous(EventKind::MouseButtonPress {
                    x: pos.x,
                    y: pos.y,
                    button: btn,
                });
                let consumed = self.dispatcher.dispatch_event(&root, &mut ev);
                if !consumed {
                    if let Some(cb) = self.mouse_press_cb.lock().unwrap().as_ref() {
                        cb(pos, button);
                    }
                }
            }
            WindowSystemEvent::MouseRelease { pos, global_pos, button, .. } => {
                let btn = match button {
                    qtrs_platform::MouseButton::Left => 1,
                    qtrs_platform::MouseButton::Right => 2,
                    qtrs_platform::MouseButton::Middle => 3,
                    _ => 0,
                };
                let mut ev = Event::new_spontaneous(EventKind::MouseButtonRelease {
                    x: pos.x,
                    y: pos.y,
                    button: btn,
                });
                self.dispatcher.dispatch_event(&root, &mut ev);
                if button == qtrs_platform::MouseButton::Right {
                    if let Some(cb) = self.context_menu_cb.lock().unwrap().as_ref() {
                        cb(global_pos);
                    }
                }
            }
            WindowSystemEvent::Wheel {
                pos,
                delta,
                modifiers,
                ..
            } => {
                let mut ev = Event::new_spontaneous(EventKind::Wheel {
                    x: pos.x,
                    y: pos.y,
                    pixel_delta_x: delta.x,
                    pixel_delta_y: delta.y,
                    angle_delta_x: 0,
                    angle_delta_y: delta.y,
                    modifiers: modifiers.bits(),
                });
                self.dispatcher.dispatch_event(&root, &mut ev);
            }
            WindowSystemEvent::GeometryChange { geometry } => {
                let size = Size::new(geometry.width, geometry.height);
                let (old_pos, old_size) = {
                    let mut cur = self.geometry.lock().unwrap();
                    let old_pos = Point::new(cur.x, cur.y);
                    let old_size = Size::new(cur.width, cur.height);
                    *cur = geometry;
                    (old_pos, old_size)
                };

                let size_changed = old_size.width != geometry.width || old_size.height != geometry.height;
                let pos_changed = old_pos.x != geometry.x || old_pos.y != geometry.y;

                if size_changed {
                    let mut bs = self.backing_store.lock().unwrap();
                    let dpr = bs.device_pixel_ratio();
                    let physical_w = ((geometry.width as f32).max(1.0) * dpr).round() as u32;
                    let physical_h = ((geometry.height as f32).max(1.0) * dpr).round() as u32;
                    if bs.physical_width() != physical_w || bs.physical_height() != physical_h {
                        if let Some(new_pixmap) = Pixmap::with_dpr(physical_w, physical_h, dpr) {
                            *bs = new_pixmap;
                        }
                    }
                }

                {
                    let mut root_borrow = root.borrow_mut();
                    root_borrow.set_geometry(Rect::new(0, 0, geometry.width, geometry.height));
                    if size_changed {
                        if let Some(layout) = root_borrow.layout_mut() {
                            layout.update_layout();
                        }
                    }
                    root_borrow.update();
                }

                if size_changed {
                    let mut ev = Event::new_spontaneous(EventKind::Resize {
                        width: geometry.width,
                        height: geometry.height,
                        old_width: old_size.width,
                        old_height: old_size.height,
                    });
                    self.dispatcher.dispatch_event(&root, &mut ev);

                    if let Some(cb) = self.resize_cb.lock().unwrap().as_ref() {
                        cb(size);
                    }
                }

                if pos_changed {
                    let mut ev = Event::new_spontaneous(EventKind::Move {
                        x: geometry.x,
                        y: geometry.y,
                        old_x: old_pos.x,
                        old_y: old_pos.y,
                    });
                    self.dispatcher.dispatch_event(&root, &mut ev);
                }

                if let Ok(mut pw) = self.platform_window.try_lock() {
                    let mut bs = self.backing_store.lock().unwrap();
                    do_render_and_present(&mut **pw, &mut *bs, &root, geometry);
                }
            }
            WindowSystemEvent::Resize { size } => {
                let (same_size, cur_geom) = {
                    let mut geom = self.geometry.lock().unwrap();
                    let same = geom.width == size.width && geom.height == size.height;
                    geom.width = size.width;
                    geom.height = size.height;
                    (same, *geom)
                };

                let mut needs_present = false;
                {
                    let mut bs = self.backing_store.lock().unwrap();
                    let dpr = bs.device_pixel_ratio();
                    let physical_w = ((size.width as f32).max(1.0) * dpr).round() as u32;
                    let physical_h = ((size.height as f32).max(1.0) * dpr).round() as u32;
                    if bs.physical_width() != physical_w || bs.physical_height() != physical_h {
                        if let Some(new_pixmap) = Pixmap::with_dpr(physical_w, physical_h, dpr) {
                            *bs = new_pixmap;
                            needs_present = true;
                        }
                    }
                }

                if !same_size || needs_present {
                    {
                        let mut root_borrow = root.borrow_mut();
                        root_borrow.set_geometry(Rect::new(0, 0, size.width, size.height));
                        if let Some(layout) = root_borrow.layout_mut() {
                            layout.update_layout();
                        }
                        root_borrow.update();
                    }

                    let mut ev = Event::new_spontaneous(EventKind::Resize {
                        width: size.width,
                        height: size.height,
                        old_width: 0,
                        old_height: 0,
                    });
                    self.dispatcher.dispatch_event(&root, &mut ev);

                    if let Some(cb) = self.resize_cb.lock().unwrap().as_ref() {
                        cb(size);
                    }

                    if let Ok(mut pw) = self.platform_window.try_lock() {
                        let mut bs = self.backing_store.lock().unwrap();
                        do_render_and_present(&mut **pw, &mut *bs, &root, cur_geom);
                    }
                }
            }
            WindowSystemEvent::KeyPress {
                key,
                modifiers,
                is_repeat,
            } => {
                let mut ev = Event::new_spontaneous(EventKind::KeyPress {
                    key,
                    modifiers: modifiers.bits(),
                    is_repeat,
                });
                self.dispatcher.dispatch_event(&root, &mut ev);
            }
            WindowSystemEvent::KeyRelease { key, modifiers } => {
                let mut ev = Event::new_spontaneous(EventKind::KeyRelease {
                    key,
                    modifiers: modifiers.bits(),
                });
                self.dispatcher.dispatch_event(&root, &mut ev);
            }
            WindowSystemEvent::FocusIn => {
                let mut ev = Event::new_spontaneous(EventKind::FocusIn {
                    reason: qtrs_core::event::FocusReason::ActiveWindow,
                });
                self.dispatcher.dispatch_event(&root, &mut ev);
            }
            WindowSystemEvent::FocusOut => {
                let mut ev = Event::new_spontaneous(EventKind::FocusOut {
                    reason: qtrs_core::event::FocusReason::ActiveWindow,
                });
                self.dispatcher.dispatch_event(&root, &mut ev);
            }
            WindowSystemEvent::DpiChanged { dpi_x, dpi_y } => {
                let old_dpr = self.backing_store.lock().unwrap().device_pixel_ratio();
                let new_dpr = (dpi_x as f32) / 96.0;
                let cur_geom = *self.geometry.lock().unwrap();
                let physical_w = ((cur_geom.width.max(1) as f32) * new_dpr).round() as u32;
                let physical_h = ((cur_geom.height.max(1) as f32) * new_dpr).round() as u32;
                if let Some(new_pixmap) = Pixmap::with_dpr(physical_w, physical_h, new_dpr) {
                    *self.backing_store.lock().unwrap() = new_pixmap;
                    propagate_dpi_change_recursive(&root, old_dpr, new_dpr);

                    let mut root_borrow = root.borrow_mut();
                    root_borrow.set_geometry(Rect::new(0, 0, cur_geom.width, cur_geom.height));
                    if let Some(layout) = root_borrow.layout_mut() {
                        layout.update_layout();
                    }
                    root_borrow.update();
                }

                let mut ev = Event::new_spontaneous(EventKind::DpiChanged { dpi_x, dpi_y });
                self.dispatcher.dispatch_event(&root, &mut ev);

                if let Ok(mut pw) = self.platform_window.try_lock() {
                    let mut bs = self.backing_store.lock().unwrap();
                    do_render_and_present(&mut **pw, &mut *bs, &root, cur_geom);
                }
            }
            WindowSystemEvent::InputMethod {
                commit_string,
                preedit_string,
                cursor_position,
            } => {
                let mut ev = Event::new_spontaneous(EventKind::InputMethod {
                    commit_string,
                    preedit_string,
                    cursor_position,
                });
                self.dispatcher.dispatch_event(&root, &mut ev);
            }
            WindowSystemEvent::DragEnter {
                pos,
                formats,
                drop_action,
            } => {
                let mut ev = Event::new_spontaneous(EventKind::DragEnter {
                    pos_x: pos.x,
                    pos_y: pos.y,
                    formats,
                    drop_action,
                });
                self.dispatcher.dispatch_event(&root, &mut ev);
            }
            WindowSystemEvent::DragMove { pos, drop_action } => {
                let mut ev = Event::new_spontaneous(EventKind::DragMove {
                    pos_x: pos.x,
                    pos_y: pos.y,
                    drop_action,
                });
                self.dispatcher.dispatch_event(&root, &mut ev);
            }
            WindowSystemEvent::DragLeave => {
                let mut ev = Event::new_spontaneous(EventKind::DragLeave);
                self.dispatcher.dispatch_event(&root, &mut ev);
            }
            WindowSystemEvent::Drop {
                pos,
                formats,
                data,
                drop_action,
            } => {
                let mut ev = Event::new_spontaneous(EventKind::Drop {
                    pos_x: pos.x,
                    pos_y: pos.y,
                    formats,
                    data,
                    drop_action,
                });
                self.dispatcher.dispatch_event(&root, &mut ev);
            }
            _ => {}
        }
    }
}
