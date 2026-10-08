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

/// Points each of `children` at `parent`, so ancestor-dependent lookups (the style sheet
/// cascade) can walk upwards.
///
/// A widget cannot name its own `Rc`, and `add_child` / `Layout::add_widget` only receive the
/// child, so the link is made by whoever holds the owner's `WidgetRef`: layout activation
/// (`adopt_tree`) and painting.
pub fn adopt_children(parent: &WidgetRef, children: &[WidgetRef]) {
    for child in children {
        let Ok(c) = child.try_borrow() else { continue };
        let linked = c
            .parent_widget()
            .and_then(|w| w.upgrade())
            .is_some_and(|owner| Rc::ptr_eq(&owner, parent));
        if !linked {
            c.set_parent_widget(Some(Rc::downgrade(parent)));
        }
    }
}

/// `adopt_children` for every widget below `root`.
pub fn adopt_tree(root: &WidgetRef) {
    let children = match root.try_borrow() {
        Ok(w) => w.children(),
        Err(_) => return,
    };
    adopt_children(root, &children);
    for child in &children {
        adopt_tree(child);
    }
}

/// Sends `StyleChange` (`WidgetBase::style_changed`) to every widget below `children`, depth
/// first: `QStyleSheetStyle::updateObjects` (qstylesheetstyle.cpp:2780).
///
/// A widget that is mutably borrowed is the one currently being driven; it is skipped, as
/// `resolve_style` also stops at it. Its caller is the one changing it.
pub fn style_changed_below(children: Vec<WidgetRef>) {
    for child in children {
        let Ok(widget) = child.try_borrow() else { continue };
        widget.widget_base().style_changed();
        style_changed_below(widget.children());
    }
}

pub trait Widget: QObject + 'static {
    /// The `WidgetBase` holding this widget's shared state.
    ///
    /// Size policy, style sheet and style properties live there, and changing them has to enter
    /// the repaint/relayout protocol (`WidgetBase::update_geometry`, `style_changed`). Having one
    /// accessor instead of per-type forwarding makes it impossible for a widget type to accept
    /// such a change and silently drop it.
    fn widget_base(&self) -> &WidgetBase;

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
        self.widget_base().size_policy()
    }

    /// `QWidget::setSizePolicy`: stores the policy and, if it changed, runs `updateGeometry`.
    fn set_size_policy(&self, policy: QSizePolicy) {
        self.widget_base().set_size_policy(policy);
    }

    /// `QWidget::updateGeometry`: this widget's size hint or policy changed, so the parent's
    /// layout has to run again.
    fn update_geometry(&self) {
        self.widget_base().update_geometry();
    }
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

    /// `QWidget::isActiveWindow` (qwidget.cpp:6967): the window this widget belongs to is
    /// `QApplication::activeWindow()`. A widget tree that belongs to no `Window` is not.
    fn is_active_window(&self) -> bool {
        let mut id = self.window_id();
        let mut parent = self.parent_widget();
        while id.is_none() {
            let Some(p) = parent.and_then(|p| p.upgrade()) else {
                return false;
            };
            let Ok(p) = p.try_borrow() else {
                return false;
            };
            id = p.window_id();
            parent = p.parent_widget();
        }
        id.is_some() && id == crate::application::Application::active_window()
    }

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

    /// `QWidget::setStyleSheet`: the sheet applies to this widget and its subtree, so a
    /// `StyleChange` (repaint, `updateGeometry`, own layout invalid) goes to this widget and to
    /// every descendant, as `QStyleSheetStyle::repolish(w)` does (`updateObjects`,
    /// qstylesheetstyle.cpp:2780).
    fn set_style_sheet(&self, qss: &str) {
        self.widget_base().set_style_sheet(qss);
        style_changed_below(self.children());
    }

    /// This widget's own style sheet (not its ancestors').
    fn style_sheet(&self) -> Option<QStyleSheetStyle> {
        self.widget_base().style_sheet.borrow().clone()
    }

    /// Sets a style property (`QObject::setProperty` on a dynamic property).
    ///
    /// As in Qt, this alone does not restyle the widget: a `[state="x"]` selector is only
    /// re-evaluated when the widget is polished again, so call [`repolish`](Self::repolish)
    /// afterwards (Python does `style().unpolish(w); style().polish(w)`).
    fn set_property(&self, name: &str, value: &str) {
        self.widget_base().set_property(name, value);
    }

    /// `style()->unpolish(w); style()->polish(w)`: re-evaluates the style sheet, which Qt
    /// reports as a `StyleChange` (`update(); updateGeometry(); layout->invalidate()`).
    fn repolish(&self) {
        self.widget_base().style_changed();
    }

    fn property(&self, name: &str) -> Option<String> {
        self.widget_base().property(name)
    }

    /// `QWidget::toolTip`: the text shown when the cursor rests on the widget. Empty means none.
    fn tool_tip(&self) -> String {
        self.widget_base().tool_tip.borrow().clone()
    }

    /// `QWidget::setToolTip`.
    fn set_tool_tip(&self, text: &str) {
        *self.widget_base().tool_tip.borrow_mut() = text.to_string();
    }

    /// `QWidget::toolTipDuration`: how long the tip stays, in ms; -1 (the default) derives it from
    /// the text length.
    fn tool_tip_duration(&self) -> i32 {
        self.widget_base().tool_tip_duration.get()
    }

    fn set_tool_tip_duration(&self, msec: i32) {
        self.widget_base().tool_tip_duration.set(msec);
    }

    /// `Qt::WA_AlwaysShowToolTips` on a window widget: its widgets show tool tips even when the
    /// window is not the active one (`QApplication::event`, qapplication.cpp:1710-1735).
    fn always_show_tool_tips(&self) -> bool {
        self.widget_base().always_show_tool_tips.get()
    }

    fn set_always_show_tool_tips(&self, on: bool) {
        self.widget_base().always_show_tool_tips.set(on);
    }
}

pub struct WidgetBase {
    pub object_data: ObjectData,
    pub geometry: Cell<Rect>,
    pub visible: Cell<bool>,
    /// Effective state (`!WA_Disabled`): false when this widget or an ancestor is disabled.
    pub enabled: Cell<bool>,
    /// Disabled explicitly with `set_enabled(false)` (`WA_ForceDisabled`).
    pub force_disabled: Cell<bool>,
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
    pub tool_tip: RefCell<String>,
    pub tool_tip_duration: Cell<i32>,
    pub always_show_tool_tips: Cell<bool>,
}

impl WidgetBase {
    pub fn new() -> Self {
        let id = ObjectId::next();
        Self {
            object_data: ObjectData::new(id),
            geometry: Cell::new(Rect::new(0, 0, 100, 30)),
            visible: Cell::new(true),
            enabled: Cell::new(true),
            force_disabled: Cell::new(false),
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
            tool_tip: RefCell::new(String::new()),
            tool_tip_duration: Cell::new(-1),
            always_show_tool_tips: Cell::new(false),
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
            // `QWidget::setVisible` invalidates the parent layout (qwidget.cpp:8465-8468).
            self.update_geometry();
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.get()
    }

    /// QSS pseudo-states of the widget's state, as `pseudoClass(QStyle::State)` derives them
    /// (qstylesheetstyle.cpp:1756-1773): `:hover` only while enabled, `:disabled` otherwise,
    /// `:pressed` for a sunken widget and `:focus` while it has keyboard focus.
    pub fn style_pseudo_states<'a>(
        &self,
        hover: bool,
        pressed: bool,
        buf: &'a mut [&'static str; 3],
    ) -> &'a [&'static str] {
        let enabled = self.is_enabled();
        let mut n = 0;
        for (on, name) in [
            (enabled && hover, "hover"),
            (!enabled, "disabled"),
            (pressed, "pressed"),
            (self.has_focus(), "focus"),
        ] {
            if on {
                buf[n] = name;
                n += 1;
            }
        }
        &buf[..n]
    }

    /// `QWidget::setEnabled`: records the explicit state and passes the effective one down the
    /// tree (qwidget.cpp:3405-3476). A widget is not enabled under a disabled parent, and a child
    /// disabled explicitly stays disabled when its parent is enabled again.
    pub fn set_enabled(&self, enabled: bool) {
        self.force_disabled.set(!enabled);
        self.set_enabled_helper(enabled);
    }

    /// `QWidgetPrivate::setEnabled_helper`.
    fn set_enabled_helper(&self, enable: bool) {
        let parent_disabled = self
            .parent_widget()
            .and_then(|p| p.upgrade())
            .is_some_and(|p| p.try_borrow().is_ok_and(|p| !p.is_enabled()));
        if (enable && parent_disabled) || enable == self.enabled.get() {
            return;
        }
        self.enabled.set(enable);
        // The children are this widget's own and those of its layout, as `EmptyWidget::children`.
        let mut children = self.children.borrow().clone();
        if let Some(layout) = self.layout.borrow().as_ref() {
            children.extend(layout.widgets());
        }
        for child in children {
            let Ok(child) = child.try_borrow() else { continue };
            let base = child.widget_base();
            // Enabling skips explicitly disabled children; disabling skips disabled ones.
            let skip = if enable { base.force_disabled.get() } else { !base.enabled.get() };
            if !skip {
                base.set_enabled_helper(enable);
            }
        }
        // `QWidget::changeEvent(EnabledChange)` repaints (qwidget.cpp:9491-9492).
        self.update();
    }

    pub fn update(&self) {
        let g = self.geometry.get();
        self.dirty.set(Some(Rect::new(0, 0, g.width, g.height)));
        let target_receiver = self.window_id.get().unwrap_or(self.object_data.id);
        post_event_to_thread(
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

    /// `QWidget::setSizePolicy`: nothing happens when the policy is unchanged.
    pub fn set_size_policy(&self, policy: QSizePolicy) {
        if self.size_policy.get() == policy {
            return;
        }
        self.size_policy.set(policy);
        self.update_geometry();
    }

    /// `QWidget::updateGeometry` (`QWidgetPrivate::updateGeometry_helper`, qwidget.cpp:10571):
    /// asks the parent's layout to run again, since this widget's size hint or policy changed.
    /// A widget without a parent has no layout above it, so there is no request to make.
    ///
    /// qtrs has no `LayoutRequest` event to wake the event loop: queued requests are delivered by
    /// the next render (`WidgetCommandQueue::flush_layouts`). So the request is paired with an
    /// `update()`, which is what schedules that render; without it a size-hint change on an
    /// otherwise idle window would sit in the queue until something else repainted.
    pub fn update_geometry(&self) {
        self.request_layout();
        self.update();
    }

    /// What `QWidget::event` does for `StyleChange` / `FontChange` (qwidget.cpp:9502-9510):
    /// `update(); updateGeometry(); layout->invalidate();`.
    pub fn style_changed(&self) {
        self.update_geometry();
        if let Ok(mut layout) = self.layout.try_borrow_mut() {
            if let Some(layout) = layout.as_mut() {
                layout.invalidate();
            }
        }
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

    /// Sets this widget's own style sheet. As in Qt, a string without a rule block
    /// (`"color: red;"`) is a declaration list for the widget itself, i.e. `* { … }`.
    pub fn set_style_sheet(&self, qss: &str) {
        if qss.trim().is_empty() {
            *self.style_sheet.borrow_mut() = None;
        } else if qss.contains('{') {
            *self.style_sheet.borrow_mut() = Some(QStyleSheetStyle::parse(qss));
        } else {
            *self.style_sheet.borrow_mut() = Some(QStyleSheetStyle::parse(&format!("* {{ {qss} }}")));
        }
        self.style_changed();
    }

    pub fn style_sheet(&self) -> Option<std::cell::Ref<'_, QStyleSheetStyle>> {
        let borrow = self.style_sheet.borrow();
        if borrow.is_some() {
            Some(std::cell::Ref::map(borrow, |s| s.as_ref().unwrap()))
        } else {
            None
        }
    }

    /// Resolves this widget's style the way `QStyleSheetStyle` does: the application sheet,
    /// then the sheets of its ancestors from the outermost inwards, then its own sheet.
    ///
    /// An ancestor that is mutably borrowed (it is the widget currently being driven) ends the
    /// walk; painting and layout of children happen after the parent's borrow is released.
    pub fn resolve_style(
        &self,
        ctx: &crate::style::stylesheet::WidgetStyleContext,
    ) -> crate::style::stylesheet::ResolvedStyle {
        let mut ancestors: Vec<QStyleSheetStyle> = Vec::new();
        let mut next = self.parent.borrow().clone();
        while let Some(weak) = next {
            let Some(rc) = weak.upgrade() else { break };
            let Ok(widget) = rc.try_borrow() else { break };
            if let Some(sheet) = widget.style_sheet() {
                ancestors.push(sheet);
            }
            next = widget.parent_widget();
        }

        let app = crate::application::Application::style_sheet();
        let own = self.style_sheet.borrow();
        let sheets: Vec<&QStyleSheetStyle> = app
            .as_deref()
            .into_iter()
            .chain(ancestors.iter().rev())
            .chain(own.as_ref())
            .collect();
        QStyleSheetStyle::resolve_chain(&sheets, ctx)
    }

    /// Stores a style property. Like `QObject::setProperty`, this does not restyle the widget;
    /// `style_changed` (the repolish) does.
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
                // The default `wheel_event` does nothing, like `QWidget::wheelEvent`, which
                // ignores the event so that it reaches the parent (a scroll area).
                false
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
    fn widget_base(&self) -> &crate::widget::WidgetBase {
        &self.base
    }

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
        Size::new(0, 0)
    }

    /// `QWidget::minimumSizeHint` of a widget with a layout: the layout's minimum size.
    fn minimum_size_hint(&self) -> Size {
        if let Some(layout) = self.base.layout.borrow().as_ref() {
            layout.minimum_size()
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
            self.base.update_geometry();
        }
    }

    fn is_enabled(&self) -> bool {
        self.base.enabled.get()
    }

    fn set_enabled(&self, enabled: bool) {
        self.base.set_enabled(enabled);
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
        if let Some(win_id) = self.base.window_id.get() {
            for w in layout.widgets() {
                w.borrow().set_window_id(Some(win_id));
            }
        }
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
