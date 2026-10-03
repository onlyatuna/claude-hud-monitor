use crate::accessibility::{AccessibleAction, AccessibleRole};
use crate::style::stylesheet::QStyleSheetStyle;
use crate::focus::FocusPolicy;
use crate::layout::Layout;
pub use crate::size_policy::{Policy, QSizePolicy};
use qtrs_core::event::{Event, EventKind, FocusReason};
use qtrs_core::event_loop::post_event_to_thread;
use qtrs_core::object::{ObjectData, ObjectId, QObject, ThreadId};
use qtrs_gui::geometry::primitives::{Point, Rect, RectF, Size};
use qtrs_gui::paint::Painter;
use std::cell::{Cell, RefCell, RefMut};
use std::rc::{Rc, Weak};

pub type WidgetRef = Rc<RefCell<Box<dyn Widget>>>;
pub type WidgetWeak = Weak<RefCell<Box<dyn Widget>>>;

pub trait Widget: QObject + 'static {
    fn id(&self) -> ObjectId;

    fn geometry(&self) -> Rect;

    fn set_geometry(&self, rect: Rect);
    fn size_hint(&self) -> Size {
        Size::new(100, 30)
    }

    fn minimum_size(&self) -> Size {
        Size::new(0, 0)
    }

    fn minimum_size_hint(&self) -> Size {
        self.minimum_size()
    }

    fn maximum_size(&self) -> Size {
        Size::new(16777215, 16777215)
    }

    fn size_policy(&self) -> QSizePolicy {
        QSizePolicy::default()
    }

    fn set_size_policy(&self, _policy: QSizePolicy) {}
    fn is_visible(&self) -> bool;

    fn set_visible(&self, visible: bool);
    fn is_enabled(&self) -> bool;

    fn set_enabled(&self, enabled: bool);

    fn update(&self);
    fn dirty_rect(&self) -> Option<Rect>;

    fn clear_dirty(&self);

    fn layout(&self) -> Option<&dyn Layout> {
        None
    }

    fn layout_mut(&mut self) -> Option<&mut Box<dyn Layout>> {
        None
    }

    fn layout_ref_mut(&self) -> Option<RefMut<'_, Box<dyn Layout>>> {
        None
    }

    fn set_layout(&mut self, _layout: Box<dyn Layout>) {}

    fn update_layout(&self) {
        let g = self.geometry();
        if let Some(mut layout) = self.layout_ref_mut() {
            layout.set_geometry(Rect::new(0, 0, g.width, g.height));
            layout.activate();
        }
        for child in self.children() {
            if child.borrow().layout_ref_mut().is_some() {
                child.borrow().update_layout();
            }
        }
        crate::layout_scheduler::LayoutScheduler::activate_pending();
    }
    fn parent_widget(&self) -> Option<WidgetWeak>;

    fn set_parent_widget(&self, parent: Option<WidgetWeak>);
    fn window_id(&self) -> Option<ObjectId>;

    fn set_window_id(&self, window_id: Option<ObjectId>);
    fn children(&self) -> Vec<WidgetRef>;

    fn add_child(&mut self, child: WidgetRef);

    fn remove_child(&mut self, child_id: ObjectId);

    /// Requests a deferred layout update for this widget or its parent container.
    fn request_layout(&self) {
        if let Some(parent) = self.parent_widget() {
            crate::command::WidgetCommandQueue::post_layout_weak(&parent);
        }
    }

    /// Schedules this widget to be safely deleted and removed from its parent
    /// after the current event dispatch stack unwinds (Qt `QObject::deleteLater` equivalent).
    fn delete_later(&self) {
        crate::command::WidgetCommandQueue::post_delete(self.parent_widget(), self.id());
    }

    /// Virtual paint event handler (`QWidget::paintEvent` equivalent).
    ///
    /// Called when the widget needs to repaint its content.
    /// The painter is pre-translated to the widget's local coordinates `(0, 0)`.
    fn paint_event(&mut self, _painter: &mut Painter) {}

    fn mouse_press_event(&mut self, _pos: Point, _button: u32, _modifiers: u32) {}

    fn mouse_release_event(&mut self, _pos: Point, _button: u32, _modifiers: u32) {}

    fn mouse_move_event(&mut self, _pos: Point) {}

    fn enter_event(&mut self, _pos: Point) {}

    fn leave_event(&mut self) {}

    fn wheel_event(&mut self, _pos: Point, _delta_y: i32, _modifiers: u32) {}

    fn resize_event(&self, _new_size: Size, _old_size: Size) {}

    fn focus_policy(&self) -> FocusPolicy {
        FocusPolicy::NoFocus
    }

    fn set_focus_policy(&self, _policy: FocusPolicy) {}
    fn has_focus(&self) -> bool {
        false
    }

    fn set_has_focus(&self, _focus: bool) {}
    fn accessible_role(&self) -> AccessibleRole {
        AccessibleRole::Custom
    }

    fn accessible_name(&self) -> String {
        QObject::object_name(self).unwrap_or_default().to_owned()
    }

    fn accessible_value(&self) -> Option<String> {
        None
    }

    fn accessible_actions(&self) -> Vec<AccessibleAction> {
        Vec::new()
    }
    fn perform_accessible_action(&mut self, _action: AccessibleAction) -> bool {
        false
    }

    fn focus_in_event(&mut self, _reason: FocusReason) {}

    fn focus_out_event(&mut self, _reason: FocusReason) {}

    fn key_press_event(&mut self, _key: u32, _modifiers: u32, _is_repeat: bool) {}

    fn key_release_event(&mut self, _key: u32, _modifiers: u32) {}

    /// Called when the screen DPI changes during Per-Monitor V2 dynamic dragging or display reconfiguration.
    /// Widgets should invalidate cached metrics, size hints, and layouts.
    fn dpi_changed_event(&mut self, _old_dpr: f32, _new_dpr: f32) {
        self.update();
    }
    fn as_any(&self) -> &dyn std::any::Any;

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    fn set_style_sheet(&self, _qss: &str) {}
    fn style_sheet(&self) -> Option<&QStyleSheetStyle> {
        None
    }

    fn set_property(&self, _name: &str, _value: &str) {}
    fn property(&self, _name: &str) -> Option<&str> {
        None
    }
}

pub struct WidgetBase {
    pub object_data: ObjectData,
    pub geometry: Cell<Rect>,
    pub visible: Cell<bool>,
    pub enabled: Cell<bool>,
    pub dirty: Cell<Option<Rect>>,
    pub parent: RefCell<Option<WidgetWeak>>,
    pub window_id: Cell<Option<ObjectId>>,
    pub children: RefCell<Vec<WidgetRef>>,
    pub layout: RefCell<Option<Box<dyn Layout>>>,
    pub focus_policy: Cell<FocusPolicy>,
    pub has_focus: Cell<bool>,
    pub size_policy: Cell<QSizePolicy>,
    pub style_sheet: RefCell<Option<QStyleSheetStyle>>,
    pub properties: RefCell<Vec<(String, String)>>,
}

impl WidgetBase {
    pub fn new() -> Self {
        let id = ObjectId::next();
        Self {
            object_data: ObjectData::new(id),
            geometry: Cell::new(Rect::new(0, 0, 100, 30)),
            visible: Cell::new(true),
            enabled: Cell::new(true),
            dirty: Cell::new(Some(Rect::new(0, 0, 100, 30))),
            parent: RefCell::new(None),
            window_id: Cell::new(None),
            children: RefCell::new(Vec::new()),
            layout: RefCell::new(None),
            focus_policy: Cell::new(FocusPolicy::NoFocus),
            has_focus: Cell::new(false),
            size_policy: Cell::new(QSizePolicy::default()),
            style_sheet: RefCell::new(None),
            properties: RefCell::new(Vec::new()),
        }
    }

    pub fn with_geometry(geometry: Rect) -> Self {
        let base = Self::new();
        base.geometry.set(geometry);
        base.dirty.set(Some(Rect::new(0, 0, geometry.width, geometry.height)));
        base
    }

    pub fn geometry(&self) -> Rect {
        self.geometry.get()
    }

    pub fn set_geometry(&self, rect: Rect) {
        if self.geometry.get() != rect {
            self.geometry.set(rect);
            self.update();
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible.get()
    }

    pub fn set_visible(&self, visible: bool) {
        if self.visible.get() != visible {
            self.visible.set(visible);
            self.update();
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.get()
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.set(enabled);
    }

    pub fn update(&self) {
        let g = self.geometry.get();
        self.dirty.set(Some(Rect::new(0, 0, g.width, g.height)));
        let target_receiver = self.window_id.get().unwrap_or(self.object_data.id);
        let _ = post_event_to_thread(
            ThreadId::current(),
            target_receiver,
            Event::new(EventKind::UpdateRequest),
        );
    }

    pub fn dirty_rect(&self) -> Option<Rect> {
        self.dirty.get()
    }

    pub fn clear_dirty(&self) {
        self.dirty.set(None);
    }

    pub fn parent_widget(&self) -> Option<WidgetWeak> {
        self.parent.borrow().clone()
    }

    pub fn set_parent_widget(&self, parent: Option<WidgetWeak>) {
        *self.parent.borrow_mut() = parent;
    }

    pub fn window_id(&self) -> Option<ObjectId> {
        self.window_id.get()
    }

    pub fn set_window_id(&self, window_id: Option<ObjectId>) {
        self.window_id.set(window_id);
    }

    pub fn focus_policy(&self) -> FocusPolicy {
        self.focus_policy.get()
    }

    pub fn set_focus_policy(&self, policy: FocusPolicy) {
        self.focus_policy.set(policy);
    }

    pub fn has_focus(&self) -> bool {
        self.has_focus.get()
    }

    pub fn set_has_focus(&self, focus: bool) {
        self.has_focus.set(focus);
    }

    pub fn size_policy(&self) -> QSizePolicy {
        self.size_policy.get()
    }

    pub fn set_size_policy(&self, policy: QSizePolicy) {
        self.size_policy.set(policy);
    }

    pub fn request_layout(&self) {
        if let Some(parent) = self.parent.borrow().as_ref() {
            crate::command::WidgetCommandQueue::post_layout_weak(parent);
        }
    }

    pub fn delete_later(&self) {
        crate::command::WidgetCommandQueue::post_delete(
            self.parent.borrow().clone(),
            self.object_data.id,
        );
    }

    pub fn set_style_sheet(&self, qss: &str) {
        if qss.trim().is_empty() {
            *self.style_sheet.borrow_mut() = None;
        } else {
            *self.style_sheet.borrow_mut() = Some(QStyleSheetStyle::parse(qss));
        }
        self.dirty.set(Some(self.geometry.get()));
    }

    pub fn style_sheet(&self) -> Option<std::cell::Ref<'_, QStyleSheetStyle>> {
        let borrow = self.style_sheet.borrow();
        if borrow.is_some() {
            Some(std::cell::Ref::map(borrow, |s| s.as_ref().unwrap()))
        } else {
            None
        }
    }

    pub fn set_property(&self, name: &str, value: &str) {
        let mut props = self.properties.borrow_mut();
        if let Some(pos) = props.iter().position(|(k, _)| k == name) {
            props[pos].1 = value.to_string();
        } else {
            props.push((name.to_string(), value.to_string()));
        }
        self.dirty.set(Some(self.geometry.get()));
    }

    pub fn property(&self, name: &str) -> Option<String> {
        self.properties
            .borrow()
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
    }

    pub fn children(&self) -> Vec<WidgetRef> {
        self.children.borrow().clone()
    }

    pub fn add_child(&mut self, child: WidgetRef) {
        let child_id = child.borrow().id();
        self.children.get_mut().retain(|c| c.borrow().id() != child_id);
        if let Some(win_id) = self.window_id.get() {
            child.borrow().set_window_id(Some(win_id));
        }
        self.children.get_mut().push(child);
    }

    pub fn remove_child(&mut self, child_id: ObjectId) {
        self.children.get_mut().retain(|c| c.borrow().id() != child_id);
    }
}

impl Default for WidgetBase {
    fn default() -> Self {
        Self::new()
    }
}

pub type PaintHandler = Box<dyn FnMut(&mut Painter) + 'static>;

pub type ResizeHandler = Box<dyn FnMut(Size, Size) + 'static>;

pub struct EmptyWidget {
    pub base: WidgetBase,
    pub background_color: Option<qtrs_gui::tiny_skia::Color>,
    pub paint_handler: Option<PaintHandler>,
    pub resize_handler: RefCell<Option<ResizeHandler>>,
}

pub type CustomWidget = EmptyWidget;

impl EmptyWidget {
    pub fn new() -> Self {
        Self {
            base: WidgetBase::new(),
            background_color: None,
            paint_handler: None,
            resize_handler: RefCell::new(None),
        }
    }

    pub fn with_geometry(geometry: Rect) -> Self {
        Self {
            base: WidgetBase::with_geometry(geometry),
            background_color: None,
            paint_handler: None,
            resize_handler: RefCell::new(None),
        }
    }

    pub fn set_background_color(&mut self, color: Option<qtrs_gui::tiny_skia::Color>) {
        self.background_color = color;
        self.update();
    }

    pub fn set_paint_handler<F>(&mut self, handler: F)
    where
        F: FnMut(&mut Painter) + 'static,
    {
        self.paint_handler = Some(Box::new(handler));
        self.update();
    }

    pub fn with_paint_handler<F>(mut self, handler: F) -> Self
    where
        F: FnMut(&mut Painter) + 'static,
    {
        self.paint_handler = Some(Box::new(handler));
        self
    }

    pub fn set_resize_handler<F: FnMut(Size, Size) + 'static>(&self, handler: F) {
        *self.resize_handler.borrow_mut() = Some(Box::new(handler));
    }
}

impl Default for EmptyWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl QObject for EmptyWidget {
    fn object_data(&self) -> &ObjectData {
        &self.base.object_data
    }

    fn object_data_mut(&mut self) -> &mut ObjectData {
        &mut self.base.object_data
    }

    fn as_qobject_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_qobject_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn event(&mut self, event: &mut Event) -> bool {
        match &event.kind {
            EventKind::MouseMove { x, y } => {
                self.mouse_move_event(Point::new(*x, *y));
                false
            }
            EventKind::Enter { x, y } => {
                self.enter_event(Point::new(*x, *y));
                false
            }
            EventKind::Leave => {
                self.leave_event();
                false
            }
            EventKind::MouseButtonPress { x, y, button } => {
                self.mouse_press_event(Point::new(*x, *y), *button, 0);
                false
            }
            EventKind::MouseButtonRelease { x, y, button } => {
                self.mouse_release_event(Point::new(*x, *y), *button, 0);
                false
            }
            EventKind::Wheel {
                x,
                y,
                angle_delta_y,
                modifiers,
                ..
            } => {
                self.wheel_event(Point::new(*x, *y), *angle_delta_y, *modifiers);
                true
            }
            EventKind::Resize {
                width,
                height,
                old_width,
                old_height,
            } => {
                self.resize_event(
                    Size::new(*width, *height),
                    Size::new(*old_width, *old_height),
                );
                true
            }
            EventKind::FocusIn { reason } => {
                self.focus_in_event(*reason);
                true
            }
            EventKind::FocusOut { reason } => {
                self.focus_out_event(*reason);
                true
            }
            EventKind::KeyPress {
                key,
                modifiers,
                is_repeat,
            } => {
                self.key_press_event(*key, *modifiers, *is_repeat);
                true
            }
            EventKind::KeyRelease { key, modifiers } => {
                self.key_release_event(*key, *modifiers);
                true
            }
            _ => false,
        }
    }
}

impl Widget for EmptyWidget {
    fn id(&self) -> ObjectId {
        self.base.object_data.id
    }
    fn geometry(&self) -> Rect {
        self.base.geometry.get()
    }

    fn set_geometry(&self, rect: Rect) {
        let old_rect = self.base.geometry.get();
        let size_changed = old_rect.width != rect.width || old_rect.height != rect.height;
        let old_size = Size::new(old_rect.width, old_rect.height);
        let new_size = Size::new(rect.width, rect.height);
        self.base.geometry.set(rect);
        if size_changed {
            self.resize_event(new_size, old_size);
        }

        if size_changed {
            if let Some(layout) = self.base.layout.borrow_mut().as_mut() {
                layout.invalidate();
            }
        }
        self.update();
    }

    fn size_hint(&self) -> Size {
        if let Some(layout) = self.base.layout.borrow().as_ref() {
            layout.size_hint()
        } else {
            let g = self.base.geometry.get();
            Size::new(g.width, g.height)
        }
    }

    fn minimum_size(&self) -> Size {
        if let Some(layout) = self.base.layout.borrow().as_ref() {
            layout.size_hint()
        } else {
            Size::new(0, 0)
        }
    }

    fn maximum_size(&self) -> Size {
        Size::new(16777215, 16777215)
    }

    fn is_visible(&self) -> bool {
        self.base.visible.get()
    }

    fn set_visible(&self, visible: bool) {
        if self.base.visible.get() != visible {
            self.base.visible.set(visible);
            self.update();
        }
    }

    fn is_enabled(&self) -> bool {
        self.base.enabled.get()
    }

    fn set_enabled(&self, enabled: bool) {
        self.base.enabled.set(enabled);
    }

    fn update(&self) {
        self.base.update();
    }

    fn dirty_rect(&self) -> Option<Rect> {
        self.base.dirty.get()
    }

    fn clear_dirty(&self) {
        self.base.dirty.set(None);
    }

    fn layout_mut(&mut self) -> Option<&mut Box<dyn Layout>> {
        self.base.layout.get_mut().as_mut()
    }

    fn layout_ref_mut(&self) -> Option<RefMut<'_, Box<dyn Layout>>> {
        if let Ok(borrow) = self.base.layout.try_borrow_mut() {
            if borrow.is_some() {
                Some(RefMut::map(borrow, |opt| opt.as_mut().unwrap()))
            } else {
                None
            }
        } else {
            None
        }
    }
    fn set_layout(&mut self, mut layout: Box<dyn Layout>) {
        let g = self.base.geometry.get();
        layout.set_geometry(Rect::new(0, 0, g.width, g.height));
        *self.base.layout.get_mut() = Some(layout);
        self.update();
    }

    fn parent_widget(&self) -> Option<WidgetWeak> {
        self.base.parent.borrow().clone()
    }

    fn set_parent_widget(&self, parent: Option<WidgetWeak>) {
        *self.base.parent.borrow_mut() = parent;
    }

    fn window_id(&self) -> Option<ObjectId> {
        self.base.window_id.get()
    }

    fn set_window_id(&self, window_id: Option<ObjectId>) {
        self.base.window_id.set(window_id);
        for child in self.children() {
            child.borrow().set_window_id(window_id);
        }
    }

    fn children(&self) -> Vec<WidgetRef> {
        let mut list = self.base.children.borrow().clone();
        if let Some(layout) = self.base.layout.borrow().as_ref() {
            for w in layout.widgets() {
                if !list.iter().any(|c| c.borrow().id() == w.borrow().id()) {
                    list.push(w);
                }
            }
        }
        list
    }

    fn add_child(&mut self, child: WidgetRef) {
        let child_id = child.borrow().id();
        self.base.children.get_mut().retain(|c| c.borrow().id() != child_id);
        if let Some(win_id) = self.base.window_id.get() {
            child.borrow().set_window_id(Some(win_id));
        }
        self.base.children.get_mut().push(child);
        self.update();
    }

    fn remove_child(&mut self, child_id: ObjectId) {
        self.base.children.get_mut().retain(|c| c.borrow().id() != child_id);
        self.update();
    }

    fn focus_policy(&self) -> FocusPolicy {
        self.base.focus_policy.get()
    }

    fn set_focus_policy(&self, policy: FocusPolicy) {
        self.base.focus_policy.set(policy);
    }

    fn has_focus(&self) -> bool {
        self.base.has_focus.get()
    }

    fn set_has_focus(&self, focus: bool) {
        self.base.has_focus.set(focus);
    }

    fn size_policy(&self) -> QSizePolicy {
        self.base.size_policy.get()
    }

    fn set_size_policy(&self, policy: QSizePolicy) {
        self.base.size_policy.set(policy);
    }

    fn paint_event(&mut self, painter: &mut Painter) {
        if let Some(color) = self.background_color {
            let brush = qtrs_gui::paint::Brush::Color(color);
            painter.set_brush(brush);
            painter.set_pen(None);
            let g = self.base.geometry.get();
            let rect_f = RectF::new(0.0, 0.0, g.width as f32, g.height as f32);
            painter.draw_rect(rect_f);
        }
        if let Some(handler) = &mut self.paint_handler {
            handler(painter);
        }
    }
    fn resize_event(&self, new_size: Size, old_size: Size) {
        if let Ok(mut h) = self.resize_handler.try_borrow_mut() {
            if let Some(cb) = h.as_mut() {
                cb(new_size, old_size);
            }
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
