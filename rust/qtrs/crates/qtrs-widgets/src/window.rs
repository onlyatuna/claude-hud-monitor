use crate::widget::{EmptyWidget, WidgetRef};
use qtrs_core::event::{Event, EventKind};
use qtrs_core::object::{register_qobject, unregister_qobject, ObjectData, ObjectId, QObject};
use qtrs_gui::geometry::primitives::{Point, Rect, RectF, Size};
use qtrs_gui::paint::{BackingStore, Painter};
use qtrs_platform::{
    platform, PlatformWindow, WindowFlags, WindowSystemEvent, WindowSystemEventHandler,
};
use crate::hit_test::EventTreeDispatcher;
type ContextMenuCallback = std::rc::Rc<std::cell::RefCell<Option<Box<dyn Fn(Point)>>>>;
type MousePressCallback = std::rc::Rc<std::cell::RefCell<Option<Box<dyn Fn(Point, qtrs_platform::MouseButton) -> bool>>>>;
type MouseMoveCallback = std::rc::Rc<std::cell::RefCell<Option<Box<dyn Fn(Point)>>>>;
type ResizeCallback = std::rc::Rc<std::cell::RefCell<Option<Box<dyn Fn(Size)>>>>;

/// Resize/render counters for one window (diagnostics and tests).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderStats {
    /// Native `Resize` events that changed the window size (never coalesced).
    pub resize_event_count: u64,
    /// Deferred renders actually queued on the event loop.
    pub deferred_render_schedule_count: u64,
    /// `LayoutScheduler::activate_pending()` passes run by the deferred render phase.
    pub layout_activation_count: u64,
    /// Deferred render phases executed.
    pub render_count: u64,
    /// Deferred render phases that painted and presented a non-empty region.
    pub present_count: u64,
    /// Deferred renders skipped because a platform-window / backing-store / root borrow was held.
    pub borrow_skipped_count: u64,
    /// Next-turn retries queued after a borrow conflict.
    pub borrow_retry_count: u64,
    /// Synchronous renders performed while a native interactive sizing loop was active.
    pub interactive_render_count: u64,
    /// Resize callbacks invoked (any path).
    pub resize_callback_count: u64,
    /// `EventKind::Resize` delivered straight to `Window::event` (posted/programmatic, not the
    /// canonical `WindowSystemEvent::Resize` path).
    pub event_resize_count: u64,
    /// `Window::render_and_present` calls (synchronous, outside the deferred/interactive gate).
    pub direct_render_count: u64,
    /// Direct renders that painted and presented a non-empty region.
    pub direct_present_count: u64,
}

thread_local! {
    static RENDER_STATES: std::cell::RefCell<std::collections::HashMap<ObjectId, std::rc::Rc<RenderState>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Per-window coalescing state for native-resize driven repaints.
///
/// `MetaCall` closures must be `Send`, so a queued render captures only the window's
/// `ObjectId` and resolves this state through a thread-local registry when it runs.
///
/// State machine (all flags per window, single thread):
///
/// * IDLE: `dirty=false pending=false rendering=false`
/// * REQUESTED: `dirty=true pending=true` (exactly one `MetaCall` queued)
/// * RENDERING: `dirty=false pending=false rendering=true`; invalidations only set `dirty`
/// * RENDER AGAIN: `dirty` set while rendering -> re-queued once after the phase
/// * BORROW CONFLICT: `dirty=true pending=false`; one bounded next-turn retry is queued
///   (`retry_used`). If the retry also conflicts the window is *parked* (`dirty` kept, nothing
///   queued, no spin) and is re-armed by the next invalidation (`request_render`) or when a
///   synchronous `Window::render_and_present` releases its borrows (`rearm_if_parked`).
///
/// `UpdateRequest` and native `Resize` both enter through `request_render`.
struct RenderState {
    window_id: ObjectId,
    platform_window: std::rc::Weak<std::cell::RefCell<Box<dyn PlatformWindow>>>,
    backing_store: std::rc::Weak<std::cell::RefCell<BackingStore>>,
    geometry: std::rc::Rc<std::cell::Cell<Rect>>,
    root: std::cell::RefCell<WidgetRef>,
    dirty: std::cell::Cell<bool>,
    render_pending: std::cell::Cell<bool>,
    currently_rendering: std::cell::Cell<bool>,
    /// Set only while `Window::set_geometry` is inside the native `set_geometry` call; it
    /// renders synchronously itself, so native Resize must not queue another render.
    within_set_geometry: std::cell::Cell<bool>,
    /// Native interactive sizing loop is active (`InteractiveResizeStart`..`End`): native Resize
    /// renders synchronously instead of deferring. Never read by event/callback semantics.
    interactive_resize: std::cell::Cell<bool>,
    /// A next-turn retry after a borrow conflict was already used for this request.
    retry_used: std::cell::Cell<bool>,
    stats: std::cell::Cell<RenderStats>,
}

/// Clears a flag on every exit path, including unwinding.
struct FlagGuard<'a>(&'a std::cell::Cell<bool>);

impl Drop for FlagGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

impl RenderState {
    fn register(
        window_id: ObjectId,
        platform_window: std::rc::Weak<std::cell::RefCell<Box<dyn PlatformWindow>>>,
        backing_store: std::rc::Weak<std::cell::RefCell<BackingStore>>,
        geometry: std::rc::Rc<std::cell::Cell<Rect>>,
        root: WidgetRef,
    ) -> std::rc::Rc<Self> {
        let state = std::rc::Rc::new(Self {
            window_id,
            platform_window,
            backing_store,
            geometry,
            root: std::cell::RefCell::new(root),
            dirty: std::cell::Cell::new(false),
            render_pending: std::cell::Cell::new(false),
            currently_rendering: std::cell::Cell::new(false),
            interactive_resize: std::cell::Cell::new(false),
            within_set_geometry: std::cell::Cell::new(false),
            retry_used: std::cell::Cell::new(false),
            stats: std::cell::Cell::new(RenderStats::default()),
        });
        RENDER_STATES.with(|m| m.borrow_mut().insert(window_id, std::rc::Rc::clone(&state)));
        state
    }

    fn unregister(window_id: ObjectId) {
        let _ = RENDER_STATES.try_with(|m| m.borrow_mut().remove(&window_id));
    }

    fn bump(&self, f: impl FnOnce(&mut RenderStats)) {
        let mut s = self.stats.get();
        f(&mut s);
        self.stats.set(s);
    }

    /// Marks the window dirty and queues at most one deferred render.
    fn request_render(&self) {
        qtrs_platform::resize_debug::count(qtrs_platform::resize_debug::Count::RenderRequested);
        self.dirty.set(true);
        self.retry_used.set(false);
        if self.render_pending.get() || self.currently_rendering.get() {
            // Pending: the queued render reads the latest geometry. Rendering: re-queued on completion.
            return;
        }
        if !self.queue_render() {
            // No event loop on this thread to defer to: render in place.
            self.render_phase();
        }
    }

    /// Posts the one deferred render. Events posted from inside a pump wait for the next pump
    /// (`insertion_offset`), so this never re-dispatches within the current pump.
    fn queue_render(&self) -> bool {
        self.render_pending.set(true);
        qtrs_platform::resize_debug::count(qtrs_platform::resize_debug::Count::RenderQueuedDeferred);
        let id = self.window_id;
        let queued = qtrs_core::event_loop::post_event_to_thread(
            qtrs_core::object::ThreadId::current(),
            ObjectId(0),
            Event::new(EventKind::MetaCall(Box::new(move |_| run_deferred_render(id)))),
        );
        if queued {
            self.bump(|s| s.deferred_render_schedule_count += 1);
        } else {
            self.render_pending.set(false);
        }
        queued
    }

    /// Re-queues a render left `dirty` by a borrow conflict that exhausted its retry.
    fn rearm_if_parked(&self) {
        if self.dirty.get() && !self.render_pending.get() && !self.currently_rendering.get() {
            self.request_render();
        }
    }

    /// True if a repaint is owed: the window flag, or any widget with a dirty rect.
    fn has_pending_invalidation(&self) -> bool {
        if self.dirty.get() {
            return true;
        }
        match self.root.try_borrow() {
            Ok(root) => tree_has_dirty(&root),
            Err(_) => true,
        }
    }

    /// A borrow was held elsewhere: keep `dirty`, queue at most one next-turn retry.
    fn on_borrow_conflict(&self) {
        qtrs_platform::resize_debug::count(qtrs_platform::resize_debug::Count::SkipBorrowConflict);
        qtrs_platform::resize_debug::note(|| "render skipped: borrow conflict".to_string());
        self.bump(|s| s.borrow_skipped_count += 1);
        if self.retry_used.get() || self.render_pending.get() {
            return;
        }
        self.retry_used.set(true);
        if self.queue_render() {
            self.bump(|s| s.borrow_retry_count += 1);
        }
    }
    /// Interactive-resize policy: render the latest geometry now, through the same guards
    /// (`currently_rendering`, `dirty`, borrow-conflict retry). A `MetaCall` queued before the
    /// loop started stays queued but finds `dirty == false` and does nothing, so there is never
    /// a sync + deferred double render.
    fn render_now(&self) {
        qtrs_platform::resize_debug::count(qtrs_platform::resize_debug::Count::RenderNow);
        self.dirty.set(true);
        if self.currently_rendering.get() {
            // Re-queued once by the running phase's tail.
            return;
        }
        let before = self.stats.get().render_count;
        self.render_phase();
        if self.stats.get().render_count != before {
            self.bump(|s| s.interactive_render_count += 1);
        }
    }

    /// Leaves interactive mode; anything still owed (borrow conflict) is re-armed.
    fn end_interactive_resize(&self) {
        self.interactive_resize.set(false);
        self.rearm_if_parked();
    }


    /// Layout activation + paint + present, using the latest geometry.
    fn render_phase(&self) {
        if self.currently_rendering.get() {
            qtrs_platform::resize_debug::count(qtrs_platform::resize_debug::Count::SkipReentrant);
            qtrs_platform::resize_debug::note(|| "render_phase skipped: already rendering".to_string());
            return;
        }
        if !self.dirty.get() {
            qtrs_platform::resize_debug::count(qtrs_platform::resize_debug::Count::SkipNotDirty);
            qtrs_platform::resize_debug::note(|| "render_phase skipped: not dirty".to_string());
            return;
        }
        let (Some(pw_rc), Some(bs_rc)) = (self.platform_window.upgrade(), self.backing_store.upgrade())
        else {
            return;
        };
        let Ok(mut pw) = pw_rc.try_borrow_mut() else {
            self.on_borrow_conflict();
            return;
        };
        let Ok(mut bs) = bs_rc.try_borrow_mut() else {
            self.on_borrow_conflict();
            return;
        };
        let Ok(root) = self.root.try_borrow().map(|r| r.clone()) else {
            self.on_borrow_conflict();
            return;
        };

        self.dirty.set(false);
        self.retry_used.set(false);
        {
            self.currently_rendering.set(true);
            let _guard = FlagGuard(&self.currently_rendering);
            let dbg_lay = qtrs_platform::resize_debug::start();
            crate::layout_scheduler::LayoutScheduler::activate_pending();
            qtrs_platform::resize_debug::end(qtrs_platform::resize_debug::Phase::LayoutActivate, dbg_lay);
            let g = self.geometry.get();
            let trace_hwnd = pw.native_handle() as usize;
            let trace_dpr = platform().primary_screen().device_pixel_ratio();
            let trace_phys = (
                (g.width as f32 * trace_dpr).round() as u32,
                (g.height as f32 * trace_dpr).round() as u32,
            );
            qtrs_platform::resize_trace::record_at(
                qtrs_platform::resize_trace::TraceKind::RequestedGeometry,
                trace_hwnd,
                (g.x, g.y),
                (g.width as u32, g.height as u32),
                trace_phys,
            );
            qtrs_platform::resize_trace::record_window_rect(
                qtrs_platform::resize_trace::TraceKind::WindowRectBeforeRender,
                trace_hwnd,
            );
            qtrs_platform::resize_trace::record(
                qtrs_platform::resize_trace::TraceKind::RenderStart,
                trace_hwnd,
                (g.width as u32, g.height as u32),
                trace_phys,
            );
            let presented = do_render_and_present(&mut **pw, &mut bs, &root, g);
            qtrs_platform::resize_debug::mark_rendered();
            qtrs_platform::resize_trace::record(
                qtrs_platform::resize_trace::TraceKind::RenderEnd,
                trace_hwnd,
                (g.width as u32, g.height as u32),
                trace_phys,
            );
            self.bump(|s| {
                s.layout_activation_count += 1;
                s.render_count += 1;
                if presented {
                    s.present_count += 1;
                }
            });
        }
        drop(bs);
        drop(pw);

        // An invalidation arrived while rendering (e.g. a nested native resize).
        if self.dirty.get() && !self.render_pending.get() {
            self.request_render();
        }
    }
}

fn tree_has_dirty(widget: &WidgetRef) -> bool {
    let Ok(w) = widget.try_borrow() else {
        return true;
    };
    if w.dirty_rect().is_some() {
        return true;
    }
    let children = w.children();
    drop(w);
    children.iter().any(tree_has_dirty)
}

/// Body of the queued `MetaCall`; the window may be gone by now.
fn run_deferred_render(window_id: ObjectId) {
    let state = RENDER_STATES
        .try_with(|m| m.borrow().get(&window_id).cloned())
        .ok()
        .flatten();
    let Some(state) = state else {
        return;
    };
    state.render_pending.set(false);
    state.render_phase();
}

pub struct Window {
    object_data: ObjectData,
    platform_window: std::rc::Rc<std::cell::RefCell<Box<dyn PlatformWindow>>>,
    root_widget: WidgetRef,
    backing_store: std::rc::Rc<std::cell::RefCell<BackingStore>>,
    geometry: std::rc::Rc<std::cell::Cell<Rect>>,
    context_menu_cb: ContextMenuCallback,
    mouse_press_cb: MousePressCallback,
    mouse_move_cb: MouseMoveCallback,
    resize_cb: ResizeCallback,
    render_state: std::rc::Rc<RenderState>,
}

impl Window {
    pub fn new(title: &str, geometry: Rect, flags: WindowFlags) -> Result<Self, &'static str> {
        let _t = qtrs_gui::startup_trace::span(|| format!("Window::new({title:?})"));
        let p = {
            let _t = qtrs_gui::startup_trace::span(|| "platform() first use".into());
            platform()
        };
        let dpr = p.primary_screen().device_pixel_ratio();
        let native_rect = if dpr > 1.0 {
            qtrs_platform::high_dpi::to_native_rect(geometry, dpr)
        } else {
            geometry
        };
        let platform_win = {
            let _t = qtrs_gui::startup_trace::span(|| "create_window (native)".into());
            p.create_window(title, native_rect, flags)?
        };
        let backing_store = BackingStore::new(Size::new(geometry.width, geometry.height), dpr)
            .ok_or("Failed to create top-level window offscreen BackingStore")?;
        let window_id = ObjectId::next();

        let root_widget: WidgetRef = std::rc::Rc::new(std::cell::RefCell::new(Box::new(
            EmptyWidget::with_geometry(Rect::new(0, 0, geometry.width, geometry.height)),
        )));
        root_widget.borrow_mut().set_window_id(Some(window_id));

        let context_menu_cb = std::rc::Rc::new(std::cell::RefCell::new(None));
        let cb_clone = std::rc::Rc::clone(&context_menu_cb);
        let mouse_press_cb = std::rc::Rc::new(std::cell::RefCell::new(None));
        let press_cb_clone = std::rc::Rc::clone(&mouse_press_cb);
        let mouse_move_cb = std::rc::Rc::new(std::cell::RefCell::new(None));
        let move_cb_clone = std::rc::Rc::clone(&mouse_move_cb);
        let resize_cb = std::rc::Rc::new(std::cell::RefCell::new(None));
        let resize_cb_clone = std::rc::Rc::clone(&resize_cb);

        let bs_rc = std::rc::Rc::new(std::cell::RefCell::new(backing_store));
        let bs_clone = std::rc::Rc::clone(&bs_rc);
        let geom_cell = std::rc::Rc::new(std::cell::Cell::new(geometry));
        let geom_clone = std::rc::Rc::clone(&geom_cell);

        let pw_rc = std::rc::Rc::new(std::cell::RefCell::new(platform_win));
        let pw_clone = std::rc::Rc::clone(&pw_rc);

        let render_state = RenderState::register(
            window_id,
            std::rc::Rc::downgrade(&pw_rc),
            std::rc::Rc::downgrade(&bs_rc),
            std::rc::Rc::clone(&geom_cell),
            root_widget.clone(),
        );
        let handler = WindowEventHandler {
            platform_window: pw_clone,
            backing_store: bs_clone,
            geometry: geom_clone,
            root: root_widget.clone(),
            dispatcher: EventTreeDispatcher::new(),
            context_menu_cb: cb_clone,
            mouse_press_cb: press_cb_clone,
            mouse_move_cb: move_cb_clone,
            resize_cb: resize_cb_clone,
            render: std::rc::Rc::clone(&render_state),
        };
        pw_rc.borrow_mut().set_event_handler(Box::new(handler));

        let win = Self {
            object_data: ObjectData::new(window_id),
            platform_window: pw_rc,
            root_widget,
            backing_store: bs_rc,
            geometry: geom_cell,
            context_menu_cb,
            mouse_press_cb,
            mouse_move_cb,
            resize_cb,
            render_state,
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
        let geom = self.geometry.get();
        widget
            .borrow_mut()
            .set_geometry(Rect::new(0, 0, geom.width, geom.height));
        *self.render_state.root.borrow_mut() = widget.clone();
        self.root_widget = widget;
    }

    pub fn geometry(&self) -> Rect {
        self.geometry.get()
    }

    pub fn physical_geometry(&self) -> Rect {
        self.platform_window.borrow().geometry()
    }

    pub fn set_geometry(&mut self, rect: Rect) {
        let old_geom = self.geometry.get();
        let old_size = Size::new(old_geom.width, old_geom.height);
        self.geometry.set(rect);
        let size_changed = old_size.width != rect.width || old_size.height != rect.height;

        let dpr = platform().primary_screen().device_pixel_ratio();
        let native_rect = if dpr > 1.0 {
            qtrs_platform::high_dpi::to_native_rect(rect, dpr)
        } else {
            rect
        };
        {
            self.render_state.within_set_geometry.set(true);
            let _guard = FlagGuard(&self.render_state.within_set_geometry);
            self.platform_window.borrow_mut().set_geometry(native_rect);
        }

        // 1. Root geometry updated
        self.root_widget
            .borrow_mut()
            .set_geometry(Rect::new(0, 0, rect.width, rect.height));

        // 2. Dispatch ResizeEvent & resize callback
        if size_changed {
            let mut ev = Event::new_spontaneous(EventKind::Resize {
                width: rect.width,
                height: rect.height,
                old_width: old_size.width,
                old_height: old_size.height,
            });
            self.root_widget.borrow_mut().event(&mut ev);
            if let Some(cb) = self.resize_cb.borrow().as_ref() {
                self.render_state.bump(|s| s.resize_callback_count += 1);
                cb(Size::new(rect.width, rect.height));
            }
        }

        // 3. Layout Invalidation & Activation via LayoutScheduler
        if size_changed {
            crate::layout_scheduler::LayoutScheduler::invalidate(&self.root_widget);
            crate::layout_scheduler::LayoutScheduler::activate_pending();
        }

        // 4. Backing store invalidation & paint (Lazy Resize in do_render_and_present)
        self.render_and_present();
    }

    pub fn set_geometry_silent(&mut self, rect: Rect) {
        let p = platform();
        let dpr = p.primary_screen().device_pixel_ratio();
        let native_rect = if dpr > 1.0 {
            qtrs_platform::high_dpi::to_native_rect(rect, dpr)
        } else {
            rect
        };
        self.geometry.set(rect);
        self.platform_window.borrow_mut().set_geometry(native_rect);
    }

    pub fn show(&mut self) {
        let _t = qtrs_gui::startup_trace::span(|| "Window::show".into());
        {
            let _s = qtrs_gui::startup_trace::span(|| "platform_window.show() (ShowWindow + messages it dispatches)".into());
            self.platform_window.borrow_mut().show();
        }
        self.render_and_present();
    }

    pub fn hide(&mut self) {
        self.platform_window.borrow_mut().hide();
    }

    pub fn set_stays_on_top(&mut self, enabled: bool) {
        self.platform_window.borrow_mut().set_stays_on_top(enabled);
    }

    pub fn set_click_through(&mut self, enabled: bool) {
        self.platform_window.borrow_mut().set_click_through(enabled);
    }

    pub fn set_opacity(&mut self, opacity: f32) {
        self.platform_window.borrow_mut().set_opacity(opacity);
        self.render_and_present();
    }
    pub fn set_style_sheet(&mut self, qss: &str) {
        crate::application::Application::set_style_sheet(qss);
        self.render_and_present();
    }

    pub fn opacity(&self) -> f32 {
        self.platform_window.borrow().opacity()
    }

    pub fn set_minimum_size(&mut self, min_w: i32, min_h: i32) {
        self.platform_window.borrow_mut().set_minimum_size(min_w, min_h);
    }

    pub fn minimum_size(&self) -> (i32, i32) {
        self.platform_window.borrow().minimum_size()
    }

    pub fn start_system_drag(&self) {
        self.platform_window.borrow().start_system_drag();
    }
    pub fn start_system_move(&self) -> bool {
        self.platform_window.borrow().start_system_move()
    }

    pub fn start_system_resize(&self, edges: qtrs_platform::platform_window::WindowEdges) -> bool {
        self.platform_window.borrow().start_system_resize(edges)
    }

    pub fn set_cursor(&mut self, shape: qtrs_platform::cursor::CursorShape) {
        self.platform_window.borrow_mut().set_cursor(shape);
    }

    pub fn set_mouse_press_handler<F: Fn(Point, qtrs_platform::MouseButton) -> bool + 'static>(&mut self, handler: F) {
        let mut cb = self.mouse_press_cb.borrow_mut();
        *cb = Some(Box::new(handler));
    }

    pub fn set_mouse_move_handler<F: Fn(Point) + 'static>(&mut self, handler: F) {
        let mut cb = self.mouse_move_cb.borrow_mut();
        *cb = Some(Box::new(handler));
    }
    pub fn set_resize_handler<F: Fn(Size) + 'static>(&mut self, handler: F) {
        let mut cb = self.resize_cb.borrow_mut();
        *cb = Some(Box::new(handler));
    }
    pub fn set_backdrop(
        &mut self,
        backdrop: qtrs_platform::backdrop::BackdropType,
        dark_mode: bool,
    ) -> bool {
        self.platform_window.borrow_mut().set_backdrop(backdrop, dark_mode)
    }

    pub fn set_ime_focus(&mut self, pos: Point) {
        self.platform_window.borrow_mut().set_ime_focus(pos);
    }

    pub fn enable_drop_target(&mut self, enabled: bool) -> bool {
        self.platform_window.borrow_mut().enable_drop_target(enabled)
    }

    pub fn native_handle(&self) -> isize {
        self.platform_window.borrow().native_handle()
    }

    pub fn set_context_menu_handler<F: Fn(Point) + 'static>(&mut self, handler: F) {
        let mut cb = self.context_menu_cb.borrow_mut();
        *cb = Some(Box::new(handler));
    }

    #[track_caller]
    pub fn render_and_present(&mut self) {
        let geom = self.geometry.get();
        {
            let c = std::panic::Location::caller();
            qtrs_platform::resize_debug::note(|| format!(
                "render_and_present() called from {}:{} with geometry {}x{}@({},{})",
                c.file(), c.line(), geom.width, geom.height, geom.x, geom.y
            ));
        }
        let root = self.root_widget.clone();
        {
            let mut bs = self.backing_store.borrow_mut();
            let mut pw = self.platform_window.borrow_mut();
            let presented = do_render_and_present(&mut **pw, &mut bs, &root, geom);
            self.render_state.bump(|s| {
                s.direct_render_count += 1;
                if presented {
                    s.direct_present_count += 1;
                }
            });
        }
        // Borrows released: recover a deferred render parked by a borrow conflict.
        self.render_state.rearm_if_parked();
    }

    pub fn present_custom<F: FnOnce(&mut Painter)>(&mut self, f: F) {
        let geom = self.geometry.get();
        let dpr = platform().primary_screen().device_pixel_ratio();

        let mut bs = self.backing_store.borrow_mut();
        bs.resize(Size::new(geom.width, geom.height), dpr);
        bs.fill(qtrs_gui::tiny_skia::Color::TRANSPARENT);
        {
            let mut painter = Painter::begin(&mut **bs);
            f(&mut painter);
        }
        let mut pw = self.platform_window.borrow_mut();
        let phys_dirty = Rect::new(
            0,
            0,
            bs.physical_width() as i32,
            bs.physical_height() as i32,
        );
        let dirty_region = qtrs_gui::geometry::Region::from_rect(phys_dirty);
        let _ = pw.present_region(&bs, &dirty_region);
    }

    pub fn backing_store(&self) -> std::cell::Ref<'_, BackingStore> {
        self.backing_store.borrow()
    }
    pub fn backing_store_handle(&self) -> std::rc::Rc<std::cell::RefCell<BackingStore>> {
        std::rc::Rc::clone(&self.backing_store)
    }

    pub fn save_png(&self, path: &std::path::Path) -> Result<(), &'static str> {
        self.backing_store.borrow().save_png(path).map_err(|_| "failed to save PNG")
    }

    /// Snapshot of the resize/render counters (diagnostics and tests).
    pub fn render_stats(&self) -> RenderStats {
        self.render_state.stats.get()
    }

    /// True while a native interactive sizing loop is active.
    pub fn is_interactive_resize(&self) -> bool {
        self.render_state.interactive_resize.get()
    }

    /// True when no repaint is owed or queued (`dirty`, `render_pending`, `currently_rendering` all clear).
    pub fn render_idle(&self) -> bool {
        let s = &self.render_state;
        !s.dirty.get() && !s.render_pending.get() && !s.currently_rendering.get()
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            let hwnd = self.platform_window.borrow().native_handle() as windows_sys::Win32::Foundation::HWND;
            qtrs_platform::unregister_window_event_binding(hwnd);
        }
        // Deferred renders already queued for this window become no-ops.
        RenderState::unregister(self.object_data.id);
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
                // Same gate as native Resize. Skip stale requests whose dirty state a
                // synchronous render already consumed.
                if self.render_state.has_pending_invalidation() {
                    self.render_state.request_render();
                }
                true
            }
            // Direct/programmatic entry only. Native `WM_SIZE` no longer posts this (see
            // `WindowSystemEvent::Resize`, the canonical path); counted in `event_resize_count`.
            EventKind::Resize { width, height, .. } => {
                self.render_state.bump(|s| s.event_resize_count += 1);
                let mut cur = self.geometry.get();
                cur.width = *width;
                cur.height = *height;
                self.geometry.set(cur);

                // 1. Root geometry
                self.root_widget
                    .borrow_mut()
                    .set_geometry(Rect::new(0, 0, *width, *height));

                // 2. Resize callback
                if let Some(cb) = self.resize_cb.borrow().as_ref() {
                    self.render_state.bump(|s| s.resize_callback_count += 1);
                    cb(Size::new(*width, *height));
                }

                // 3. Layout Invalidation & Activation via LayoutScheduler
                crate::layout_scheduler::LayoutScheduler::invalidate(&self.root_widget);
                crate::layout_scheduler::LayoutScheduler::activate_pending();

                // 4. Backing store invalidation & paint
                self.render_and_present();
                true
            }
            EventKind::DpiChanged { dpi_x, .. } => {
                let old_dpr = self.backing_store.borrow().device_pixel_ratio();
                let new_dpr = (*dpi_x as f32) / 96.0;
                let cur_geom = self.geometry.get();
                let size = Size::new(cur_geom.width, cur_geom.height);
                self.backing_store.borrow_mut().resize(size, new_dpr);
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

    let dbg_w = qtrs_platform::resize_debug::start();
    widget.paint_event(painter);
    if let Some(t0) = dbg_w {
        let us = t0.elapsed().as_micros();
        if us >= 1500 {
            qtrs_platform::resize_debug::note(|| format!(
                "slow paint_event {:.1}ms: widget {:?} geom {}x{}@({},{})",
                us as f64 / 1000.0, widget.object_name(), geom.width, geom.height, geom.x, geom.y
            ));
        }
    }

    let children = widget.children();
    drop(widget);

    for child in children {
        render_widget_recursive(&child, painter, child_dirty);
    }

    painter.restore();
}

/// Recursively collects and unifies dirty rectangles across the widget tree.
pub fn collect_dirty_region(widget_ref: &WidgetRef, offset: Point) -> Option<Rect> {
    let w = widget_ref.borrow_mut();
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
    backing_store: &mut BackingStore,
    root_widget: &WidgetRef,
    geometry: Rect,
) -> bool {
    let _t = qtrs_gui::startup_trace::span_min(0.5, || "do_render_and_present".into());
    let dpr = platform().primary_screen().device_pixel_ratio();
    let logical_size = Size::new(geometry.width, geometry.height);

    // --- Lazy Backing Store Resize (Qt QWidgetRepaintManager::paintAndFlush parity) ---
    // If the backing store dimensions or DPR differ from the required top-level size,
    // reallocate lazily here, and mark the entire window dirty so the new buffer is fully painted.
    let dbg_bs = qtrs_platform::resize_debug::start();
    if backing_store.resize_to_native(logical_size, dpr, platform_window.native_size()) {
        qtrs_platform::resize_debug::count(qtrs_platform::resize_debug::Count::BackingStoreResized);
        root_widget.borrow_mut().update();
    }
    qtrs_platform::resize_debug::end(qtrs_platform::resize_debug::Phase::BackingStoreResize, dbg_bs);

    let root_geom = Rect::new(0, 0, geometry.width, geometry.height);
    let dbg_col = qtrs_platform::resize_debug::start();
    let dirty = collect_dirty_region(root_widget, Point::new(0, 0))
        .unwrap_or(root_geom)
        .intersected(&root_geom);
    qtrs_platform::resize_debug::end(qtrs_platform::resize_debug::Phase::CollectDirty, dbg_col);
    if dirty.is_empty() {
        qtrs_platform::resize_debug::count(qtrs_platform::resize_debug::Count::NoDirtyRegion);
        return false;
    }

    let phys_dirty = if dpr > 1.0 {
        qtrs_platform::high_dpi::to_native_rect(dirty, dpr)
    } else {
        dirty
    };

    let dbg_clr = qtrs_platform::resize_debug::start();
    backing_store.clear_rect(phys_dirty);
    qtrs_platform::resize_debug::end(qtrs_platform::resize_debug::Phase::Clear, dbg_clr);

    let dbg_paint = qtrs_platform::resize_debug::start();
    let _paint_span = qtrs_gui::startup_trace::span_min(0.5, || "paint (render_widget_recursive)".into());
    {
        let mut painter = Painter::begin(&mut **backing_store);
        painter.set_clip_rect(RectF::new(
            dirty.x as f32,
            dirty.y as f32,
            dirty.width as f32,
            dirty.height as f32,
        ));
        render_widget_recursive(root_widget, &mut painter, dirty);
    }
    qtrs_platform::resize_debug::end(qtrs_platform::resize_debug::Phase::Paint, dbg_paint);
    drop(_paint_span);

    let dirty_region = qtrs_gui::geometry::Region::from_rect(phys_dirty);
    let _present_span = qtrs_gui::startup_trace::span_min(0.5, || "present_region".into());
    let _ = platform_window.present_region(backing_store, &dirty_region);
    true
}

struct WindowEventHandler {
    platform_window: std::rc::Rc<std::cell::RefCell<Box<dyn PlatformWindow>>>,
    backing_store: std::rc::Rc<std::cell::RefCell<BackingStore>>,
    geometry: std::rc::Rc<std::cell::Cell<Rect>>,
    root: WidgetRef,
    dispatcher: EventTreeDispatcher,
    context_menu_cb: ContextMenuCallback,
    mouse_press_cb: MousePressCallback,
    mouse_move_cb: MouseMoveCallback,
    resize_cb: ResizeCallback,
    render: std::rc::Rc<RenderState>,
}

impl WindowSystemEventHandler for WindowEventHandler {
    fn handle_window_event(&mut self, event: WindowSystemEvent) {
        let _t = qtrs_gui::startup_trace::span_min(2.0, || {
            let d = format!("{event:?}");
            format!("handle_window_event {}", d.chars().take(60).collect::<String>())
        });
        let root = self.root.clone();
        match event {
            WindowSystemEvent::MouseMove { pos, .. } => {
                let mut ev = Event::new_spontaneous(EventKind::MouseMove { x: pos.x, y: pos.y });
                self.dispatcher.dispatch_event(&root, &mut ev);
                if let Some(cb) = self.mouse_move_cb.borrow().as_ref() {
                    cb(pos);
                }
                if self.render.has_pending_invalidation() {
                    self.render.request_render();
                }
            }
            WindowSystemEvent::MouseLeave => {
                self.dispatcher.handle_mouse_leave();
                if self.render.has_pending_invalidation() {
                    self.render.request_render();
                }
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
                    if let Some(cb) = self.mouse_press_cb.borrow().as_ref() {
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
                    if let Some(cb) = self.context_menu_cb.borrow().as_ref() {
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
            WindowSystemEvent::InteractiveResizeStart => {
                self.render.interactive_resize.set(true);
            }
            WindowSystemEvent::InteractiveResizeEnd => {
                self.render.end_interactive_resize();
            }
            WindowSystemEvent::GeometryChange { geometry } => {
                let (old_pos, _) = {
                    let mut cur = self.geometry.get();
                    let old_pos = Point::new(cur.x, cur.y);
                    cur.x = geometry.x;
                    cur.y = geometry.y;
                    self.geometry.set(cur);
                    (old_pos, ())
                };

                let pos_changed = old_pos.x != geometry.x || old_pos.y != geometry.y;

                if pos_changed {
                    let mut ev = Event::new_spontaneous(EventKind::Move {
                        x: geometry.x,
                        y: geometry.y,
                        old_x: old_pos.x,
                        old_y: old_pos.y,
                    });
                    self.dispatcher.dispatch_event(&root, &mut ev);
                }
            }
            WindowSystemEvent::Resize { size } => {
                let old_size = {
                    let mut cur = self.geometry.get();
                    let old_size = Size::new(cur.width, cur.height);
                    cur.width = size.width;
                    cur.height = size.height;
                    self.geometry.set(cur);
                    old_size
                };

                let size_changed = old_size.width != size.width || old_size.height != size.height;
                qtrs_platform::resize_debug::note(|| format!(
                    "Resize event {}x{} (logical) vs widget geometry {}x{}: {}; within_set_geometry={} interactive={}",
                    size.width, size.height, old_size.width, old_size.height,
                    if size_changed { "changed" } else { "UNCHANGED -> no layout, no render" },
                    self.render.within_set_geometry.get(),
                    self.render.interactive_resize.get(),
                ));

                if size_changed {
                    self.render.bump(|s| s.resize_event_count += 1);
                    // 1. Update root widget geometry (widget geometry = new size)
                    let dbg_disp = qtrs_platform::resize_debug::start();
                    root.borrow_mut().set_geometry(Rect::new(0, 0, size.width, size.height));

                    // 2. Dispatch Resize event to widgets and callback (never coalesced)
                    let mut ev = Event::new_spontaneous(EventKind::Resize {
                        width: size.width,
                        height: size.height,
                        old_width: old_size.width,
                        old_height: old_size.height,
                    });
                    self.dispatcher.dispatch_event(&root, &mut ev);
                    qtrs_platform::resize_debug::end(qtrs_platform::resize_debug::Phase::ResizeDispatch, dbg_disp);

                    let dbg_cb = qtrs_platform::resize_debug::start();
                    if let Some(cb) = self.resize_cb.borrow().as_ref() {
                        self.render.bump(|s| s.resize_callback_count += 1);
                        cb(size);
                    }
                    qtrs_platform::resize_debug::end(qtrs_platform::resize_debug::Phase::ResizeCallback, dbg_cb);

                    // 3. Invalidate layout (deduplicated by LayoutScheduler); activation, paint
                    //    and present happen once in the coalesced deferred render.
                    let dbg_inv = qtrs_platform::resize_debug::start();
                    crate::layout_scheduler::LayoutScheduler::invalidate(&root);
                    qtrs_platform::resize_debug::end(qtrs_platform::resize_debug::Phase::LayoutInvalidate, dbg_inv);

                    // 4. Window::set_geometry renders synchronously itself (explicit flag,
                    //    not RefCell borrow state).
                    if !self.render.within_set_geometry.get() {
                        if self.render.interactive_resize.get() {
                            // Native sizing loop: layout activation + paint + present now.
                            qtrs_platform::resize_debug::note(|| "-> render_now".to_string());
                            self.render.render_now();
                        } else {
                            qtrs_platform::resize_debug::note(|| "-> request_render (deferred)".to_string());
                            self.render.request_render();
                        }
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
                let old_dpr = self.backing_store.borrow().device_pixel_ratio();
                let new_dpr = (dpi_x as f32) / 96.0;
                let cur_geom = self.geometry.get();
                let size = Size::new(cur_geom.width, cur_geom.height);
                self.backing_store.borrow_mut().resize(size, new_dpr);
                propagate_dpi_change_recursive(&root, old_dpr, new_dpr);

                let mut root_borrow = root.borrow_mut();
                root_borrow.set_geometry(Rect::new(0, 0, cur_geom.width, cur_geom.height));
                if let Some(layout) = root_borrow.layout_mut() {
                    layout.update_layout();
                }
                root_borrow.update();

                let mut ev = Event::new_spontaneous(EventKind::DpiChanged { dpi_x, dpi_y });
                self.dispatcher.dispatch_event(&root, &mut ev);

                if let Ok(mut pw) = self.platform_window.try_borrow_mut() {
                    let mut bs = self.backing_store.borrow_mut();
                    do_render_and_present(&mut **pw, &mut bs, &root, cur_geom);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_native_geometry_change_dispatches_resize_callback_exactly_once() {
        let mut win = Window::new(
            "Single Resize Test",
            Rect::new(0, 0, 300, 200),
            WindowFlags::empty(),
        )
        .expect("create test window");

        let resize_count = Arc::new(AtomicU32::new(0));
        let count_cb = Arc::clone(&resize_count);
        win.set_resize_handler(move |_size: Size| {
            count_cb.fetch_add(1, Ordering::SeqCst);
        });

        #[cfg(windows)]
        {
            use qtrs_platform::handle_geometry_change;
            use qtrs_platform::window_system_interface::Delivery;

            let hwnd = win.native_handle() as windows_sys::Win32::Foundation::HWND;

            // 1. Initial geometry change (both pos and size change: 0,0,300,200 -> 50,60,500,400)
            handle_geometry_change(Delivery::Default, hwnd, Rect::new(50, 60, 500, 400));
            assert_eq!(win.geometry().x, 50);
            assert_eq!(win.geometry().y, 60);
            assert_eq!(win.geometry().width, 500);
            assert_eq!(win.geometry().height, 400);
            assert_eq!(
                resize_count.load(Ordering::SeqCst),
                1,
                "Resize callback must be executed exactly once per native resize event"
            );

            // 2. Position-only change (50,60,500,400 -> 100,120,500,400)
            handle_geometry_change(Delivery::Default, hwnd, Rect::new(100, 120, 500, 400));
            assert_eq!(win.geometry().x, 100);
            assert_eq!(win.geometry().y, 120);
            assert_eq!(win.geometry().width, 500);
            assert_eq!(win.geometry().height, 400);
            assert_eq!(
                resize_count.load(Ordering::SeqCst),
                1,
                "Position-only geometry change must NOT trigger resize callback"
            );

            // 3. Second resize change (100,120,500,400 -> 100,120,600,450)
            handle_geometry_change(Delivery::Default, hwnd, Rect::new(100, 120, 600, 450));
            assert_eq!(win.geometry().x, 100);
            assert_eq!(win.geometry().y, 120);
            assert_eq!(win.geometry().width, 600);
            assert_eq!(win.geometry().height, 450);
            assert_eq!(
                resize_count.load(Ordering::SeqCst),
                2,
                "Second resize change must increment callback execution by exactly once"
            );
        }
    }
}
