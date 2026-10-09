//! RC-38: `QEvent::EnabledChange`.
//!
//! `QWidgetPrivate::setEnabled_helper` (qwidget.cpp:3429-3476) sends `EnabledChange` to each widget
//! whose effective state changes, after its children (and after the focus move when it was the
//! focus widget); nothing when the state does not change.
//! Reference: PySide6 6.11.2, `root` holding `c` (with buttons `x`, `a`) and button `b`:
//! - `b.setEnabled(False)` -> `b:EC:off`; again -> nothing; `b.setEnabled(True)` -> `b:EC:on`;
//! - `x` focused, `c.setEnabled(False)` -> `x:Out`, `x:EC:off`, `a:EC:off`, `c:EC:off`;
//!   `c.setEnabled(True)` -> `x:EC:on`, `a:EC:on`, `c:EC:on`;
//! - `x` disabled first: `c` off -> `a:EC:off`, `c:EC:off`; `c` on -> `a:EC:on`, `c:EC:on`;
//! - `a` focused, `c.setEnabled(False)` -> `x:EC:off`, `a:Out`, `a:EC:off`, `c:EC:off`;
//! - `a` focused, `a.setEnabled(False)` -> `a:Out`, `b:In`, `a:EC:off`, focus widget `b`.
//! `on`/`off` is `isEnabled()` inside the handler. All of it is read right after `setEnabled`.

use qtrs_core::event::{Event, EventKind, FocusReason};
use qtrs_core::object::{ObjectData, ObjectId, QObject};
use qtrs_gui::geometry::primitives::Rect;
use qtrs_widgets::*;
use std::cell::RefCell;
use std::rc::Rc;

type Log = Rc<RefCell<Vec<String>>>;

struct Probe {
    base: WidgetBase,
    name: &'static str,
    log: Log,
}

impl QObject for Probe {
    fn event(&mut self, event: &mut Event) -> bool {
        if matches!(event.kind, EventKind::EnabledChange) {
            let state = if self.base.is_enabled() { "on" } else { "off" };
            self.log
                .borrow_mut()
                .push(format!("{}:EC:{state}", self.name));
        }
        false
    }
    fn object_data(&self) -> &ObjectData {
        &self.base.object_data
    }
    fn object_data_mut(&mut self) -> &mut ObjectData {
        &mut self.base.object_data
    }
}

impl Widget for Probe {
    fn widget_base(&self) -> &WidgetBase {
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
    }
    fn children(&self) -> Vec<WidgetRef> {
        self.base.children()
    }
    fn add_child(&mut self, child: WidgetRef) {
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
    fn focus_policy(&self) -> FocusPolicy {
        self.base.focus_policy()
    }
    fn has_focus(&self) -> bool {
        self.base.has_focus()
    }
    fn set_has_focus(&self, focus: bool) {
        self.base.set_has_focus(focus);
    }
    fn focus_in_event(&mut self, _reason: FocusReason) {
        self.log.borrow_mut().push(format!("{}:In", self.name));
    }
    fn focus_out_event(&mut self, _reason: FocusReason) {
        self.log.borrow_mut().push(format!("{}:Out", self.name));
    }
}

fn probe(name: &'static str, policy: FocusPolicy, log: &Log) -> WidgetRef {
    let base = WidgetBase::with_geometry(Rect::new(0, 0, 10, 10));
    base.set_focus_policy(policy);
    Rc::new(RefCell::new(Box::new(Probe {
        base,
        name,
        log: Rc::clone(log),
    })))
}

struct Tree {
    root: WidgetRef,
    c: WidgetRef,
    x: WidgetRef,
    a: WidgetRef,
    b: WidgetRef,
    log: Log,
    d: EventTreeDispatcher,
}

/// `root` holds `c` (with `x`, `a`) and `b`; `focus` gets the focus.
fn tree(focus: Option<&str>) -> Tree {
    let log = Log::default();
    let root = probe("root", FocusPolicy::NoFocus, &log);
    let c = probe("c", FocusPolicy::NoFocus, &log);
    let x = probe("x", FocusPolicy::StrongFocus, &log);
    let a = probe("a", FocusPolicy::StrongFocus, &log);
    let b = probe("b", FocusPolicy::StrongFocus, &log);
    c.borrow_mut().add_child(x.clone());
    c.borrow_mut().add_child(a.clone());
    root.borrow_mut().add_child(c.clone());
    root.borrow_mut().add_child(b.clone());
    adopt_tree(&root);
    let mut t = Tree {
        root,
        c,
        x,
        a,
        b,
        log,
        d: EventTreeDispatcher::new(),
    };
    if let Some(name) = focus {
        let id = t.named(name).borrow().id();
        assert!(t
            .d
            .focus_manager_mut()
            .set_focus(&t.root, Some(id), FocusReason::Other));
    }
    t.log.borrow_mut().clear();
    t
}

impl Tree {
    fn named(&self, name: &str) -> &WidgetRef {
        match name {
            "c" => &self.c,
            "x" => &self.x,
            "a" => &self.a,
            _ => &self.b,
        }
    }
    /// The events so far, cleared.
    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.log.borrow_mut())
    }
}

#[test]
fn a_change_is_notified_once_and_no_change_not_at_all() {
    let t = tree(None);

    set_widget_enabled(&t.b, false);
    assert_eq!(t.take(), ["b:EC:off"]);
    set_widget_enabled(&t.b, false);
    assert!(t.take().is_empty());
    set_widget_enabled(&t.b, true);
    assert_eq!(t.take(), ["b:EC:on"]);
}

#[test]
fn the_children_are_notified_before_the_parent() {
    let t = tree(Some("x"));

    set_widget_enabled(&t.c, false);
    assert_eq!(t.take(), ["x:Out", "x:EC:off", "a:EC:off", "c:EC:off"]);
    set_widget_enabled(&t.c, true);
    assert_eq!(t.take(), ["x:EC:on", "a:EC:on", "c:EC:on"]);
}

#[test]
fn an_explicitly_disabled_child_is_not_notified() {
    let t = tree(None);
    set_widget_enabled(&t.x, false);
    t.take();

    set_widget_enabled(&t.c, false);
    assert_eq!(t.take(), ["a:EC:off", "c:EC:off"]);
    set_widget_enabled(&t.c, true);
    assert_eq!(t.take(), ["a:EC:on", "c:EC:on"]);
    assert!(!t.x.borrow().is_enabled());
}

#[test]
fn the_focus_widget_loses_the_focus_in_its_place_in_the_order() {
    let t = tree(Some("a"));

    set_widget_enabled(&t.c, false);
    assert_eq!(t.take(), ["x:EC:off", "a:Out", "a:EC:off", "c:EC:off"]);
}

#[test]
fn the_focus_moves_on_before_the_widget_is_notified() {
    let t = tree(Some("a"));

    set_widget_enabled(&t.a, false);
    assert_eq!(t.take(), ["a:Out", "b:In", "a:EC:off"]);
    assert_eq!(
        t.d.focus_manager().focused_widget_id(),
        Some(t.b.borrow().id())
    );
}
