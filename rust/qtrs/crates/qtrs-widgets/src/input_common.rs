//! Shared `Widget` boilerplate for the leaf input widgets (combo box, spin
//! boxes, sliders, dial, progress bar).
//!
//! Every such widget stores its state in a `base: WidgetBase` field and has no
//! children or layout. The macro expands inside an `impl Widget for T` block and
//! provides every trait method except `size_hint`, `update`, focus handling,
//! input handlers and `paint_event`, which each widget implements itself.

macro_rules! leaf_widget_common {
    () => {
        fn id(&self) -> qtrs_core::object::ObjectId {
            self.base.object_data.id
        }

        fn geometry(&self) -> qtrs_gui::geometry::primitives::Rect {
            self.base.geometry.get()
        }

        fn set_geometry(&self, rect: qtrs_gui::geometry::primitives::Rect) {
            if self.base.geometry.get() != rect {
                self.base.geometry.set(rect);
                self.update();
            }
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

        fn dirty_rect(&self) -> Option<qtrs_gui::geometry::primitives::Rect> {
            self.base.dirty.get()
        }

        fn clear_dirty(&self) {
            self.base.dirty.set(None);
        }

        fn layout(&self) -> Option<&dyn crate::layout::Layout> {
            None
        }

        fn layout_mut(&mut self) -> Option<&mut Box<dyn crate::layout::Layout>> {
            None
        }

        fn set_layout(&mut self, _layout: Box<dyn crate::layout::Layout>) {}

        fn parent_widget(&self) -> Option<crate::widget::WidgetWeak> {
            self.base.parent.borrow().clone()
        }

        fn set_parent_widget(&self, parent: Option<crate::widget::WidgetWeak>) {
            *self.base.parent.borrow_mut() = parent;
        }

        fn window_id(&self) -> Option<qtrs_core::object::ObjectId> {
            self.base.window_id.get()
        }

        fn set_window_id(&self, window_id: Option<qtrs_core::object::ObjectId>) {
            self.base.window_id.set(window_id);
        }
        fn children(&self) -> Vec<crate::widget::WidgetRef> {
            Vec::new()
        }

        fn add_child(&mut self, _child: crate::widget::WidgetRef) {}

        fn remove_child(&mut self, _child_id: qtrs_core::object::ObjectId) {}

        fn focus_policy(&self) -> crate::focus::FocusPolicy {
            self.base.focus_policy.get()
        }

        fn set_focus_policy(&self, policy: crate::focus::FocusPolicy) {
            self.base.focus_policy.set(policy);
        }

        fn has_focus(&self) -> bool {
            self.base.has_focus.get()
        }

        fn set_has_focus(&self, focus: bool) {
            self.base.has_focus.set(focus);
        }

        fn size_policy(&self) -> crate::size_policy::QSizePolicy {
            self.base.size_policy.get()
        }

        fn set_size_policy(&self, policy: crate::size_policy::QSizePolicy) {
            self.base.size_policy.set(policy);
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    };
}

/// Expands to the `QObject` accessor methods for a widget with a `base: WidgetBase` field.
macro_rules! leaf_qobject_common {
    () => {
        fn object_data(&self) -> &qtrs_core::object::ObjectData {
            &self.base.object_data
        }

        fn object_data_mut(&mut self) -> &mut qtrs_core::object::ObjectData {
            &mut self.base.object_data
        }

        fn as_qobject_any(&self) -> Option<&dyn std::any::Any> {
            Some(self)
        }

        fn as_qobject_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
            Some(self)
        }
    };
}

/// Standard event dispatch from `QObject::event` to the `Widget` virtual handlers.
///
/// Returns `false` for event kinds the leaf input widgets do not consume.
pub(crate) fn dispatch_input_event<W: crate::widget::Widget + ?Sized>(
    widget: &mut W,
    event: &qtrs_core::event::Event,
) -> bool {
    use qtrs_core::event::EventKind;
    use qtrs_gui::geometry::primitives::{Point, Size};
    match &event.kind {
        EventKind::MouseButtonPress { x, y, button } => {
            widget.mouse_press_event(Point::new(*x, *y), *button, 0);
            false
        }
        EventKind::MouseButtonRelease { x, y, button } => {
            widget.mouse_release_event(Point::new(*x, *y), *button, 0);
            false
        }
        EventKind::MouseMove { x, y } => {
            widget.mouse_move_event(Point::new(*x, *y));
            false
        }
        EventKind::Enter { x, y } => {
            widget.enter_event(Point::new(*x, *y));
            false
        }
        EventKind::Leave => {
            widget.leave_event();
            false
        }
        EventKind::Wheel {
            x,
            y,
            angle_delta_y,
            modifiers,
            ..
        } => {
            widget.wheel_event(Point::new(*x, *y), *angle_delta_y, *modifiers);
            true
        }
        EventKind::Resize {
            width,
            height,
            old_width,
            old_height,
        } => {
            widget.resize_event(
                Size::new(*width, *height),
                Size::new(*old_width, *old_height),
            );
            true
        }
        EventKind::FocusIn { reason } => {
            widget.focus_in_event(*reason);
            true
        }
        EventKind::FocusOut { reason } => {
            widget.focus_out_event(*reason);
            true
        }
        EventKind::KeyPress {
            key,
            modifiers,
            is_repeat,
        } => {
            widget.key_press_event(*key, *modifiers, *is_repeat);
            true
        }
        EventKind::KeyRelease { key, modifiers } => {
            widget.key_release_event(*key, *modifiers);
            true
        }
        _ => false,
    }
}

/// Posts a repaint request for `dirty` (widget-local coordinates), mirroring `CheckBox::update`.
pub(crate) fn request_update(
    base: &crate::widget::WidgetBase,
    dirty: qtrs_gui::geometry::primitives::Rect,
) {
    base.dirty.set(Some(dirty));
    let target_receiver = base.window_id.get().unwrap_or(base.object_data.id);
    let _ = qtrs_core::event_loop::post_event_to_thread(
        qtrs_core::object::ThreadId::current(),
        target_receiver,
        qtrs_core::event::Event::new(qtrs_core::event::EventKind::UpdateRequest),
    );
}

/// Local (0, 0)-anchored rectangle covering the whole widget.
pub(crate) fn local_rect(base: &crate::widget::WidgetBase) -> qtrs_gui::geometry::primitives::Rect {
    let g = base.geometry.get();
    qtrs_gui::geometry::primitives::Rect::new(0, 0, g.width, g.height)
}
