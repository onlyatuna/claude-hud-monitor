use crate::layout::Layout;
use crate::widget::{Widget, WidgetBase, WidgetRef, WidgetWeak};
use qtrs_core::event::Event;
use qtrs_core::object::{ObjectData, ObjectId, QObject};
use qtrs_core::signal::Signal;
use qtrs_gui::geometry::primitives::{Margins, Rect, Size};
use qtrs_gui::paint::Painter;

/// Layout that displays one child widget at a time from a stack (`QStackedLayout`).
pub struct StackedLayout {
    geometry: Rect,
    margins: Margins,
    widgets: Vec<WidgetRef>,
    current_index: usize,
    dirty: bool,
}

impl StackedLayout {
    /// Creates a new empty stacked layout.
    pub fn new() -> Self {
        Self {
            geometry: Rect::new(0, 0, 0, 0),
            margins: Margins::new(0, 0, 0, 0),
            widgets: Vec::new(),
            current_index: 0,
            dirty: true,
        }
    }

    /// Appends a widget to the stack, returning its page index.
    pub fn add_widget(&mut self, widget: WidgetRef) -> usize {
        let index = self.widgets.len();
        self.widgets.push(widget);
        self.update_layout();
        index
    }

    /// Inserts a widget at the specified index in the stack.
    pub fn insert_widget(&mut self, index: usize, widget: WidgetRef) {
        let clamped = index.min(self.widgets.len());
        self.widgets.insert(clamped, widget);
        self.update_layout();
    }

    /// Removes the widget at index from the stack.
    pub fn remove_widget(&mut self, index: usize) -> Option<WidgetRef> {
        if index < self.widgets.len() {
            let removed = self.widgets.remove(index);
            if self.current_index >= self.widgets.len() && !self.widgets.is_empty() {
                self.current_index = self.widgets.len() - 1;
            }
            self.update_layout();
            Some(removed)
        } else {
            None
        }
    }

    /// Returns the number of widgets in the stack.
    pub fn count(&self) -> usize {
        self.widgets.len()
    }

    /// Returns the index of the currently visible widget.
    pub fn current_index(&self) -> usize {
        self.current_index
    }

    /// Switches the active visible widget to the specified index.
    pub fn set_current_index(&mut self, index: usize) {
        if index < self.widgets.len() {
            self.current_index = index;
            self.update_layout();
        }
    }

    /// Returns a reference to the currently visible widget, if any.
    pub fn current_widget(&self) -> Option<WidgetRef> {
        self.widgets.get(self.current_index).cloned()
    }
}

impl Default for StackedLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout for StackedLayout {
    fn geometry(&self) -> Rect {
        self.geometry
    }

    fn set_geometry(&mut self, rect: Rect) {
        self.geometry = rect;
        self.dirty = true;
        self.activate();
    }

    fn add_widget(&mut self, widget: WidgetRef) {
        self.add_widget(widget);
    }

    fn add_widget_with_stretch(&mut self, widget: WidgetRef, _stretch: u32) {
        self.add_widget(widget);
    }

    fn set_margins(&mut self, margins: Margins) {
        self.margins = margins;
        self.update_layout();
    }

    fn margins(&self) -> Margins {
        self.margins
    }

    fn set_spacing(&mut self, _spacing: i32) {}

    fn spacing(&self) -> i32 {
        0
    }

    fn size_hint(&self) -> Size {
        if let Some(curr) = self.current_widget() {
            let h = curr.borrow().size_hint();
            Size::new(
                h.width + self.margins.left + self.margins.right,
                h.height + self.margins.top + self.margins.bottom,
            )
        } else {
            Size::new(
                self.margins.left + self.margins.right,
                self.margins.top + self.margins.bottom,
            )
        }
    }

    fn minimum_size(&self) -> Size {
        let page = self
            .current_widget()
            .map(|curr| crate::layout_engine::smart_min_size(&**curr.borrow()))
            .unwrap_or_else(|| Size::new(0, 0));
        Size::new(
            page.width + self.margins.left + self.margins.right,
            page.height + self.margins.top + self.margins.bottom,
        )
    }

    fn expanding_directions(&self) -> (bool, bool) {
        self.current_widget()
            .map(|curr| crate::layout_engine::item_expanding(&**curr.borrow()))
            .unwrap_or((false, false))
    }

    fn invalidate(&mut self) {
        self.dirty = true;
    }

    fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn activate(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;

        let avail_x = self.geometry.x + self.margins.left;
        let avail_y = self.geometry.y + self.margins.top;
        let avail_w = (self.geometry.width - self.margins.left - self.margins.right).max(0);
        let avail_h = (self.geometry.height - self.margins.top - self.margins.bottom).max(0);
        let child_rect = Rect::new(avail_x, avail_y, avail_w, avail_h);

        for (idx, widget) in self.widgets.iter().enumerate() {
            let old_size = {
                let w = widget.borrow();
                let g = w.geometry();
                Size::new(g.width, g.height)
            };
            let w = widget.borrow();
            w.set_geometry(child_rect);
            let should_be_visible = idx == self.current_index;
            if w.is_visible() != should_be_visible {
                w.set_visible(should_be_visible);
            }
            drop(w);
            let new_size = Size::new(child_rect.width, child_rect.height);
            if let Some(child_layout) = widget.borrow().layout_ref_mut() {
                if old_size != new_size || child_layout.is_dirty() {
                    crate::layout_scheduler::LayoutScheduler::invalidate(widget);
                }
            }
        }
    }
    fn widgets(&self) -> Vec<WidgetRef> {
        self.widgets.clone()
    }
}

/// A container widget that stacks child widgets, displaying one at a time (`QStackedWidget`).
pub struct StackedWidget {
    base: WidgetBase,
    layout: std::cell::RefCell<StackedLayout>,
    pub current_changed: Signal<usize>,
}

impl StackedWidget {
    /// Creates a new stacked widget container.
    pub fn new() -> Self {
        let base = WidgetBase::new();
        base.set_size_policy(crate::size_policy::QSizePolicy::new(
            crate::size_policy::Policy::Expanding,
            crate::size_policy::Policy::Expanding,
        ));
        Self {
            base,
            layout: std::cell::RefCell::new(StackedLayout::new()),
            current_changed: Signal::new(),
        }
    }

    /// Appends a widget to the stack and returns its index.
    pub fn add_widget(&mut self, widget: WidgetRef) -> usize {
        self.base.add_child(widget.clone());
        let idx = self.layout.get_mut().add_widget(widget);
        self.update();
        idx
    }

    /// Returns the index of the currently active page.
    pub fn current_index(&self) -> usize {
        self.layout.borrow().current_index()
    }

    /// Sets the active visible page index, emitting `current_changed`.
    pub fn set_current_index(&mut self, index: usize) {
        let changed = {
            let mut layout = self.layout.borrow_mut();
            if index != layout.current_index() && index < layout.count() {
                layout.set_current_index(index);
                true
            } else {
                false
            }
        };
        if changed {
            self.current_changed.emit(&index);
            self.update();
        }
    }

    /// Returns the currently active widget page.
    pub fn current_widget(&self) -> Option<WidgetRef> {
        self.layout.borrow().current_widget()
    }

    /// Returns the number of widgets in the stack.
    pub fn count(&self) -> usize {
        self.layout.borrow().count()
    }
}

impl Default for StackedWidget {
    fn default() -> Self {
        Self::new()
    }
}

impl QObject for StackedWidget {
    fn object_data(&self) -> &ObjectData {
        &self.base.object_data
    }

    fn object_data_mut(&mut self) -> &mut ObjectData {
        &mut self.base.object_data
    }

    fn event(&mut self, _event: &mut Event) -> bool {
        false
    }

    fn as_qobject_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_qobject_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

impl Widget for StackedWidget {
    fn widget_base(&self) -> &crate::widget::WidgetBase {
        &self.base
    }

    fn id(&self) -> ObjectId {
        self.base.object_data.id
    }

    fn geometry(&self) -> Rect {
        self.base.geometry()
    }

    fn set_geometry(&self, rect: Rect) {
        if self.base.geometry() != rect {
            self.base.set_geometry(rect);
            self.layout.borrow_mut().set_geometry(Rect::new(0, 0, rect.width, rect.height));
            self.update();
        }
    }

    fn size_hint(&self) -> Size {
        self.layout.borrow().size_hint()
    }

    fn minimum_size_hint(&self) -> Size {
        self.layout.borrow().minimum_size()
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

    fn set_enabled(&self, enabled: bool) {
        self.base.set_enabled(enabled);
    }

    fn update(&self) {
        self.base.update();
    }

    fn dirty_rect(&self) -> Option<Rect> {
        self.base.dirty_rect()
    }

    fn clear_dirty(&self) {
        self.base.clear_dirty();
    }

    fn layout(&self) -> Option<&dyn Layout> {
        None
    }

    fn layout_mut(&mut self) -> Option<&mut Box<dyn Layout>> {
        None
    }

    fn set_layout(&mut self, _layout: Box<dyn Layout>) {}

    fn parent_widget(&self) -> Option<WidgetWeak> {
        self.base.parent_widget()
    }

    fn set_parent_widget(&self, parent: Option<WidgetWeak>) {
        self.base.set_parent_widget(parent);
    }

    fn window_id(&self) -> Option<ObjectId> {
        self.base.window_id()
    }

    fn set_window_id(&self, window_id: Option<ObjectId>) {
        self.base.set_window_id(window_id);
        for child in self.children() {
            child.borrow().set_window_id(window_id);
        }
    }

    fn children(&self) -> Vec<WidgetRef> {
        self.base.children()
    }

    fn add_child(&mut self, child: WidgetRef) {
        self.base.add_child(child);
    }

    fn remove_child(&mut self, child_id: ObjectId) {
        self.base.remove_child(child_id);
    }

    fn paint_event(&mut self, _painter: &mut Painter) {}

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
