//! Parity tests for `ToolTipChange` event (G8.8.e).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use qtrs_core::event::{Event, EventKind};
use qtrs_core::object::{register_qobject, unregister_qobject, ObjectId};
use qtrs_gui::geometry::primitives::Rect;
use qtrs_widgets::widget::Widget;

struct ToolTipProbe {
    base: qtrs_widgets::widget::WidgetBase,
    change_seen: Arc<AtomicBool>,
}

impl qtrs_core::object::QObject for ToolTipProbe {
    fn object_data(&self) -> &qtrs_core::object::ObjectData {
        &self.base.object_data
    }
    fn object_data_mut(&mut self) -> &mut qtrs_core::object::ObjectData {
        &mut self.base.object_data
    }
    fn event(&mut self, ev: &mut Event) -> bool {
        if matches!(ev.kind, EventKind::ToolTipChange) {
            self.change_seen.store(true, Ordering::SeqCst);
            return true;
        }
        false
    }
}

impl Widget for ToolTipProbe {
    fn widget_base(&self) -> &qtrs_widgets::widget::WidgetBase {
        &self.base
    }
    fn id(&self) -> ObjectId {
        self.base.object_data.id
    }
    fn geometry(&self) -> Rect {
        self.base.geometry()
    }
    fn set_geometry(&self, rect: Rect) {
        self.base.set_geometry(rect);
    }
    fn is_visible(&self) -> bool {
        self.base.is_visible()
    }
    fn set_visible(&self, visible: bool) {
        self.base.set_visible(visible);
    }
    fn is_enabled(&self) -> bool {
        self.base.is_enabled()
    }
    fn update(&self) {
        self.base.update();
    }
    fn clear_dirty(&self) {
        self.base.clear_dirty();
    }
    fn dirty_rect(&self) -> Option<Rect> {
        self.base.dirty_rect()
    }
    fn layout(&self) -> Option<&dyn qtrs_widgets::layout::Layout> {
        None
    }
    fn layout_mut(&mut self) -> Option<&mut Box<dyn qtrs_widgets::layout::Layout>> {
        None
    }
    fn set_layout(&mut self, _layout: Box<dyn qtrs_widgets::layout::Layout>) {}
    fn parent_widget(&self) -> Option<qtrs_widgets::widget::WidgetWeak> {
        self.base.parent_widget()
    }
    fn set_parent_widget(&self, parent: Option<qtrs_widgets::widget::WidgetWeak>) {
        self.base.set_parent_widget(parent);
    }
    fn window_id(&self) -> Option<ObjectId> {
        self.base.window_id()
    }
    fn set_window_id(&self, window_id: Option<ObjectId>) {
        self.base.set_window_id(window_id);
    }
    fn children(&self) -> Vec<qtrs_widgets::widget::WidgetRef> {
        self.base.children()
    }
    fn add_child(&mut self, child: qtrs_widgets::widget::WidgetRef) {
        self.base.children.borrow_mut().push(child);
    }
    fn remove_child(&mut self, child_id: ObjectId) {
        self.base
            .children
            .borrow_mut()
            .retain(|c| c.borrow().id() != child_id);
    }
    fn paint_event(&mut self, _painter: &mut qtrs_gui::paint::Painter) {}
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[test]
fn set_tool_tip_delivers_tool_tip_change_event() {
    let seen = Arc::new(AtomicBool::new(false));
    let mut probe = ToolTipProbe {
        base: qtrs_widgets::widget::WidgetBase::new(),
        change_seen: Arc::clone(&seen),
    };
    let id = probe.id();
    unsafe {
        register_qobject(&mut probe);
    }

    assert!(!seen.load(Ordering::SeqCst));
    probe.set_tool_tip("Hello Tooltip");
    assert!(
        seen.load(Ordering::SeqCst),
        "set_tool_tip must send ToolTipChange event (G8.8.e)"
    );

    unsafe {
        unregister_qobject(id);
    }
}
