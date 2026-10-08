use crate::widget::WidgetRef;
use qtrs_core::event::{Event, EventKind, FocusReason};
use qtrs_core::object::ObjectId;
use std::cell::Cell;
use std::rc::Rc;

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
    collect_tab_chain(root, None, out);
}

/// `collect_tab_focusable`, also keeping `from` (the focus widget, though disabled) in its place.
fn collect_tab_chain(root: &WidgetRef, from: Option<ObjectId>, out: &mut Vec<WidgetRef>) {
    let borrow = root.borrow();
    let is_from = from == Some(borrow.id());
    if !is_from && (!borrow.is_visible() || !borrow.is_enabled()) {
        return;
    }
    if is_from || borrow.focus_policy().accepts_tab() {
        out.push(root.clone());
    }
    let children = borrow.children();
    drop(borrow);
    for child in children {
        collect_tab_chain(&child, from, out);
    }
}

/// `QWidget::focusNextChild` from `from`: the widget after it in the Tab chain, wrapping around.
fn next_in_tab_chain(root: &WidgetRef, from: ObjectId) -> Option<WidgetRef> {
    let mut chain = Vec::new();
    collect_tab_chain(root, Some(from), &mut chain);
    let i = chain.iter().position(|w| w.borrow().id() == from)?;
    (chain.len() > 1).then(|| chain[(i + 1) % chain.len()].clone())
}

fn send_focus_out(root: &WidgetRef, id: ObjectId, reason: FocusReason) {
    if let Some(old_widget) = find_widget_by_id(root, id) {
        let mut old = old_widget.borrow_mut();
        old.set_has_focus(false);
        let mut ev = Event::new_spontaneous(EventKind::FocusOut { reason });
        old.event(&mut ev);
        old.focus_out_event(reason);
        old.update();
    }
}

/// A window's focus widget (`window()->focusWidget()`), shared with the widget that holds the
/// focus so that disabling it drops the focus at once (`WidgetBase::set_enabled`).
#[derive(Debug, Default)]
pub(crate) struct FocusState {
    focused: Cell<Option<ObjectId>>,
    lost: Cell<Option<LostFocus>>,
}

/// A focus widget that was disabled; its `FocusOut` and the focus move are still to come.
#[derive(Debug, Clone, Copy)]
struct LostFocus {
    id: ObjectId,
    parent_enabled: bool,
}

impl FocusState {
    /// `id` is being disabled: if it is the focus widget, the window has no focus widget from now
    /// on, and `FocusManager::process_pending` finishes the job. Returns whether `id` had focus.
    pub(crate) fn lose(&self, id: ObjectId, parent_enabled: bool) -> bool {
        if self.focused.get() != Some(id) {
            return false;
        }
        self.focused.set(None);
        self.lost.set(Some(LostFocus { id, parent_enabled }));
        true
    }
}

/// Window-level focus manager tracking the currently active focused widget
/// and managing Tab / Shift+Tab navigation order and click focus.
#[derive(Debug, Default)]
pub struct FocusManager {
    state: Rc<FocusState>,
}

impl FocusManager {
    /// Creates a new focus manager with no initially focused widget.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the ObjectId of the currently focused widget, if any.
    pub fn focused_widget_id(&self) -> Option<ObjectId> {
        self.state.focused.get()
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
        self.process_pending(root);
        if self.state.focused.get() == target_id {
            return false;
        }
        if let Some(old_id) = self.state.focused.take() {
            send_focus_out(root, old_id, reason);
        }
        let Some(new_id) = target_id else {
            return true;
        };
        match find_widget_by_id(root, new_id) {
            Some(new_widget) => {
                self.give_focus(&new_widget, reason);
                true
            }
            None => false,
        }
    }

    fn give_focus(&self, widget: &WidgetRef, reason: FocusReason) {
        let mut new = widget.borrow_mut();
        self.state.focused.set(Some(new.id()));
        *new.widget_base().focus_state.borrow_mut() = Rc::downgrade(&self.state);
        new.set_has_focus(true);
        let mut ev = Event::new_spontaneous(EventKind::FocusIn { reason });
        new.event(&mut ev);
        new.focus_in_event(reason);
        new.update();
    }

    /// Finishes what `QWidgetPrivate::setEnabled_helper` does when it disables the focus widget
    /// (qwidget.cpp:3442-3446): `focusNextChild()`, or `clearFocus()` when the parent is disabled
    /// or no widget is next. The widget dropped the focus itself, but it is borrowed while it is
    /// disabled, so its `FocusOut` and the next widget's `FocusIn` are sent here.
    pub fn process_pending(&mut self, root: &WidgetRef) {
        let Some(lost) = self.state.lost.take() else {
            return;
        };
        let next = lost
            .parent_enabled
            .then(|| next_in_tab_chain(root, lost.id))
            .flatten();
        let reason = if next.is_some() { FocusReason::Tab } else { FocusReason::Other };
        send_focus_out(root, lost.id, reason);
        if let Some(next) = next {
            self.give_focus(&next, reason);
        }
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

        let next_id = match self.state.focused.get() {
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

        let prev_id = match self.state.focused.get() {
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
