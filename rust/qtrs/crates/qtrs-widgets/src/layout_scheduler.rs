use std::cell::{Cell, RefCell};
use std::rc::Rc;

use qtrs_core::object::ObjectId;
use qtrs_gui::geometry::primitives::{Rect, Size};

use crate::widget::{WidgetRef, WidgetWeak};

thread_local! {
    static DIRTY_LAYOUTS: RefCell<Vec<(ObjectId, WidgetWeak)>> = const { RefCell::new(Vec::new()) };
    static IS_ACTIVATING: Cell<bool> = const { Cell::new(false) };
}

/// Scheduler managing layout lifecycle by separating layout invalidation from activation.
///
/// Prevents O(N^2) recursive cascades and reentrant layouts during resize and tree mutations.
pub struct LayoutScheduler;

impl LayoutScheduler {
    pub fn invalidate(widget: &WidgetRef) {
        let Ok(w) = widget.try_borrow() else {
            return;
        };
        if let Some(mut layout) = w.layout_ref_mut() {
            layout.invalidate();
        }
        let id = w.id();
        drop(w);

        DIRTY_LAYOUTS.with(|q| {
            let mut list = q.borrow_mut();
            if !list.iter().any(|(existing_id, _)| *existing_id == id) {
                list.push((id, Rc::downgrade(widget)));
            }
        });
    }

    /// Marks the given weak widget reference's layout as dirty.
    pub fn invalidate_weak(weak: &WidgetWeak) {
        if let Some(w) = weak.upgrade() {
            Self::invalidate(&w);
        }
    }

    /// Returns whether any layout is pending activation.
    pub fn has_pending() -> bool {
        DIRTY_LAYOUTS.with(|q| !q.borrow().is_empty())
    }

    /// Activates all pending layouts iteratively without call-stack recursion.
    pub fn activate_pending() {
        if IS_ACTIVATING.with(|flag| flag.get()) {
            return;
        }
        IS_ACTIVATING.with(|flag| flag.set(true));

        while DIRTY_LAYOUTS.with(|q| !q.borrow().is_empty()) {
            let items: Vec<(ObjectId, WidgetWeak)> = DIRTY_LAYOUTS.with(|q| {
                std::mem::take(&mut *q.borrow_mut())
            });

            for (_id, weak) in items {
                if let Some(widget_ref) = weak.upgrade() {
                    let Ok(widget) = widget_ref.try_borrow() else {
                        // Already borrowed on the current stack; its layout pass is already active.
                        continue;
                    };
                    let g = widget.geometry();
                    if let Some(mut layout) = widget.layout_ref_mut() {
                        if layout.is_dirty() || layout.geometry().size() != Size::new(g.width, g.height) {
                            layout.set_geometry(Rect::new(0, 0, g.width, g.height));
                            layout.activate();
                        }
                    }
                    widget.update();
                }
            }
        }

        IS_ACTIVATING.with(|flag| flag.set(false));
    }
}
