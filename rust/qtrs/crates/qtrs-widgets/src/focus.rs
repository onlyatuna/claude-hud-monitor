use crate::widget::WidgetRef;
use qtrs_core::event::{Event, EventKind, FocusReason};
use qtrs_core::object::ObjectId;

/// Focus policy defining how a widget accepts keyboard focus (`Qt::FocusPolicy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusPolicy {
    /// Widget does not accept keyboard focus at all.
    #[default]
    NoFocus = 0,
    /// Widget accepts focus only via Tab key navigation.
    TabFocus = 1,
    /// Widget accepts focus only by clicking with mouse.
    ClickFocus = 2,
    /// Widget accepts focus by both Tab navigation and mouse click (`TabFocus | ClickFocus`).
    StrongFocus = 3,
    /// Widget accepts focus by Tab, mouse click, and mouse wheel.
    WheelFocus = 7,
}

impl FocusPolicy {
    /// Returns true if the policy permits Tab navigation focus.
    pub fn accepts_tab(&self) -> bool {
        matches!(self, Self::TabFocus | Self::StrongFocus | Self::WheelFocus)
    }

    /// Returns true if the policy permits mouse click focus.
    pub fn accepts_click(&self) -> bool {
        matches!(
            self,
            Self::ClickFocus | Self::StrongFocus | Self::WheelFocus
        )
    }
}

/// Recursively finds a widget by its ObjectId in the widget tree.
pub fn find_widget_by_id(root: &WidgetRef, id: ObjectId) -> Option<WidgetRef> {
    if root.borrow().id() == id {
        return Some(root.clone());
    }
    for child in root.borrow().children() {
        if let Some(found) = find_widget_by_id(&child, id) {
            return Some(found);
        }
    }
    None
}

/// Recursively collects all visible, enabled widgets accepting Tab focus in pre-order traversal.
pub fn collect_tab_focusable(root: &WidgetRef, out: &mut Vec<WidgetRef>) {
    let borrow = root.borrow();
    if !borrow.is_visible() || !borrow.is_enabled() {
        return;
    }
    if borrow.focus_policy().accepts_tab() {
        out.push(root.clone());
    }
    let children = borrow.children();
    drop(borrow);
    for child in children {
        collect_tab_focusable(&child, out);
    }
}

/// Window-level focus manager tracking the currently active focused widget
/// and managing Tab / Shift+Tab navigation order and click focus.
#[derive(Debug, Default)]
pub struct FocusManager {
    focused: Option<ObjectId>,
}

impl FocusManager {
    /// Creates a new focus manager with no initially focused widget.
    pub fn new() -> Self {
        Self { focused: None }
    }

    /// Returns the ObjectId of the currently focused widget, if any.
    pub fn focused_widget_id(&self) -> Option<ObjectId> {
        self.focused
    }

    /// Sets keyboard focus to the specified widget in the widget tree.
    ///
    /// Sends `FocusOut` to the previous focused widget and `FocusIn` to the new target.
    pub fn set_focus(
        &mut self,
        root: &WidgetRef,
        target_id: Option<ObjectId>,
        reason: FocusReason,
    ) -> bool {
        if self.focused == target_id {
            return false;
        }

        // 1. Notify previous focused widget
        if let Some(old_id) = self.focused.take() {
            if let Some(old_widget) = find_widget_by_id(root, old_id) {
                let mut old = old_widget.borrow_mut();
                old.set_has_focus(false);
                let mut ev = Event::new_spontaneous(EventKind::FocusOut { reason });
                old.event(&mut ev);
                old.focus_out_event(reason);
                old.update();
            }
        }

        // 2. Set and notify new focused widget
        self.focused = target_id;
        if let Some(new_id) = target_id {
            if let Some(new_widget) = find_widget_by_id(root, new_id) {
                let mut new = new_widget.borrow_mut();
                new.set_has_focus(true);
                let mut ev = Event::new_spontaneous(EventKind::FocusIn { reason });
                new.event(&mut ev);
                new.focus_in_event(reason);
                new.update();
                return true;
            } else {
                // Target widget was not found in tree
                self.focused = None;
                return false;
            }
        }

        true
    }

    /// Clears keyboard focus from any currently focused widget.
    pub fn clear_focus(&mut self, root: &WidgetRef, reason: FocusReason) -> bool {
        self.set_focus(root, None, reason)
    }

    /// Navigates focus to the next focusable widget in the Tab order sequence.
    pub fn focus_next(&mut self, root: &WidgetRef) -> bool {
        let mut focusables = Vec::new();
        collect_tab_focusable(root, &mut focusables);
        if focusables.is_empty() {
            return false;
        }

        let next_id = match self.focused {
            Some(curr_id) => {
                if let Some(idx) = focusables.iter().position(|w| w.borrow().id() == curr_id) {
                    let next_idx = (idx + 1) % focusables.len();
                    focusables[next_idx].borrow().id()
                } else {
                    focusables[0].borrow().id()
                }
            }
            None => focusables[0].borrow().id(),
        };

        self.set_focus(root, Some(next_id), FocusReason::Tab)
    }

    /// Navigates focus to the previous focusable widget in the Tab order sequence (Shift+Tab).
    pub fn focus_previous(&mut self, root: &WidgetRef) -> bool {
        let mut focusables = Vec::new();
        collect_tab_focusable(root, &mut focusables);
        if focusables.is_empty() {
            return false;
        }

        let prev_id = match self.focused {
            Some(curr_id) => {
                if let Some(idx) = focusables.iter().position(|w| w.borrow().id() == curr_id) {
                    let prev_idx = (idx + focusables.len() - 1) % focusables.len();
                    focusables[prev_idx].borrow().id()
                } else {
                    focusables[focusables.len() - 1].borrow().id()
                }
            }
            None => focusables[focusables.len() - 1].borrow().id(),
        };

        self.set_focus(root, Some(prev_id), FocusReason::Backtab)
    }

    /// Handles mouse click on a hit widget, setting focus if click focus is accepted.
    pub fn handle_mouse_click(&mut self, root: &WidgetRef, hit_widget: &WidgetRef) -> bool {
        let hit_id = hit_widget.borrow().id();
        let policy = hit_widget.borrow().focus_policy();
        if policy.accepts_click() {
            self.set_focus(root, Some(hit_id), FocusReason::Mouse)
        } else {
            // Check if widget belongs to a parent that accepts click focus
            let mut parent_weak = hit_widget.borrow().parent_widget();
            while let Some(parent_w) = parent_weak {
                if let Some(parent_rc) = parent_w.upgrade() {
                    let p_policy = parent_rc.borrow().focus_policy();
                    if p_policy.accepts_click() {
                        let p_id = parent_rc.borrow().id();
                        return self.set_focus(root, Some(p_id), FocusReason::Mouse);
                    }
                    parent_weak = parent_rc.borrow().parent_widget();
                } else {
                    break;
                }
            }
            false
        }
    }
}
