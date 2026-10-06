//! RC-06: event translation and propagation.
//!
//! Qt delivers a mouse event to the widget under the cursor and, while the event comes back
//! ignored (or the widget does not handle it), to each parent in turn, up to the top-level
//! window (`QApplication::notify`, qapplication.cpp:2689-2762). A disabled widget does not handle
//! mouse events at all (`QWidget::event`, qwidget.cpp:8978-8998), so they pass to its parent. A
//! window close request becomes a `QCloseEvent` the window may ignore; if it is accepted the
//! window is hidden (`QWidgetPrivate::handleClose`, qwidget.cpp:8604-8640). `Show`/`Hide` are
//! delivered children-first on show and self-first on hide (`show_helper`/`hide_helper`,
//! qwidget.cpp:8074-8130, 8238-8260).

use qtrs_core::event::{Event, EventKind};
use qtrs_core::object::{ObjectData, ObjectId, QObject};
use qtrs_gui::geometry::primitives::{Point, Rect};
use qtrs_widgets::*;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy)]
enum Resp {
    /// `event()` returns true and leaves the event accepted (a widget that handled it).
    Accept,
    /// `event()` returns true after `event.ignore()` (`QWidget::mousePressEvent` default).
    Ignore,
    /// `event()` returns false (the widget does not handle this kind at all).
    NotHandled,
}

type Log = Rc<RefCell<Vec<String>>>;

struct Probe {
    base: WidgetBase,
    name: &'static str,
    resp: Resp,
    log: Log,
}

impl Probe {
    fn new(name: &'static str, geometry: Rect, resp: Resp, log: &Log) -> Self {
        Self {
            base: WidgetBase::with_geometry(geometry),
            name,
            resp,
            log: Rc::clone(log),
        }
    }
}

impl QObject for Probe {
    fn object_data(&self) -> &ObjectData {
        &self.base.object_data
    }
    fn object_data_mut(&mut self) -> &mut ObjectData {
        &mut self.base.object_data
    }
    fn event(&mut self, event: &mut Event) -> bool {
        let (desc, uses_resp) = match &event.kind {
            EventKind::MouseButtonPress { x, y, .. } => (format!("Press({x},{y})"), true),
            EventKind::MouseButtonRelease { x, y, .. } => (format!("Release({x},{y})"), true),
            EventKind::MouseButtonDblClick { x, y, .. } => (format!("DblClick({x},{y})"), true),
            EventKind::Wheel { x, y, .. } => (format!("Wheel({x},{y})"), true),
            EventKind::Close => ("Close".to_string(), true),
            EventKind::Show => ("Show".to_string(), false),
            EventKind::Hide => ("Hide".to_string(), false),
            EventKind::Move { x, y, .. } => (format!("Move({x},{y})"), false),
            _ => return false,
        };
        self.log.borrow_mut().push(format!("{}:{desc}", self.name));
        if !uses_resp {
            return true;
        }
        match self.resp {
            Resp::Accept => {
                event.accept();
                true
            }
            Resp::Ignore => {
                event.ignore();
                true
            }
            Resp::NotHandled => false,
        }
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
}

fn widget(p: impl Widget + 'static) -> WidgetRef {
    Rc::new(RefCell::new(Box::new(p)))
}

/// root(0,0 400x300) > mid(20,10 200x150) > leaf(30,20 80x40). A point at (60,40) in root
/// coordinates is (40,30) in mid and (10,10) in leaf.
fn tree(leaf: Resp, mid: Resp, root: Resp, log: &Log) -> (WidgetRef, WidgetRef, WidgetRef) {
    let root_w = widget(Probe::new("root", Rect::new(0, 0, 400, 300), root, log));
    let mid_w = widget(Probe::new("mid", Rect::new(20, 10, 200, 150), mid, log));
    let leaf_w = widget(Probe::new("leaf", Rect::new(30, 20, 80, 40), leaf, log));
    mid_w.borrow_mut().add_child(Rc::clone(&leaf_w));
    root_w.borrow_mut().add_child(Rc::clone(&mid_w));
    (root_w, mid_w, leaf_w)
}

fn press(x: i32, y: i32) -> Event {
    Event::new_spontaneous(EventKind::MouseButtonPress { x, y, button: 1 })
}

#[test]
fn unaccepted_press_reaches_parent_widget() {
    let log: Log = Default::default();
    let (root, _mid, _leaf) = tree(Resp::NotHandled, Resp::Ignore, Resp::Accept, &log);
    let mut d = EventTreeDispatcher::new();

    let consumed = d.dispatch_event(&root, &mut press(60, 40));

    assert_eq!(
        *log.borrow(),
        vec!["leaf:Press(10,10)", "mid:Press(40,30)", "root:Press(60,40)"],
        "each ancestor must see the press in its own coordinates, until one accepts"
    );
    assert!(consumed);
}

#[test]
fn accepted_press_stops_at_child() {
    let log: Log = Default::default();
    let (root, _mid, _leaf) = tree(Resp::Accept, Resp::Accept, Resp::Accept, &log);
    let mut d = EventTreeDispatcher::new();

    let consumed = d.dispatch_event(&root, &mut press(60, 40));

    assert_eq!(*log.borrow(), vec!["leaf:Press(10,10)"]);
    assert!(consumed);
}

#[test]
fn press_nobody_accepts_is_reported_unconsumed_after_visiting_every_ancestor() {
    let log: Log = Default::default();
    let (root, _mid, _leaf) = tree(Resp::Ignore, Resp::NotHandled, Resp::Ignore, &log);
    let mut d = EventTreeDispatcher::new();

    let consumed = d.dispatch_event(&root, &mut press(60, 40));

    assert_eq!(
        *log.borrow(),
        vec!["leaf:Press(10,10)", "mid:Press(40,30)", "root:Press(60,40)"]
    );
    assert!(
        !consumed,
        "nothing accepted: the window must be able to fall back to its own handler"
    );
}

#[test]
fn release_and_wheel_propagate_like_press() {
    let log: Log = Default::default();
    let (root, _mid, _leaf) = tree(Resp::NotHandled, Resp::NotHandled, Resp::Accept, &log);
    let mut d = EventTreeDispatcher::new();

    d.dispatch_event(
        &root,
        &mut Event::new_spontaneous(EventKind::MouseButtonRelease { x: 60, y: 40, button: 1 }),
    );
    d.dispatch_event(
        &root,
        &mut Event::new_spontaneous(EventKind::Wheel {
            x: 60,
            y: 40,
            pixel_delta_x: 0,
            pixel_delta_y: 0,
            angle_delta_x: 0,
            angle_delta_y: 120,
            modifiers: 0,
        }),
    );

    assert_eq!(
        *log.borrow(),
        vec![
            "leaf:Release(10,10)",
            "mid:Release(40,30)",
            "root:Release(60,40)",
            "leaf:Wheel(10,10)",
            "mid:Wheel(40,30)",
            "root:Wheel(60,40)",
        ]
    );
}

#[test]
fn disabled_widget_passes_mouse_press_to_its_parent() {
    let log: Log = Default::default();
    let (root, _mid, leaf) = tree(Resp::Accept, Resp::NotHandled, Resp::Accept, &log);
    leaf.borrow().set_enabled(false);
    let mut d = EventTreeDispatcher::new();

    d.dispatch_event(&root, &mut press(60, 40));

    assert_eq!(
        *log.borrow(),
        vec!["mid:Press(40,30)", "root:Press(60,40)"],
        "a disabled widget must not see mouse events (QWidget::event returns false), so the \
         press goes on to the parent"
    );
}

#[test]
fn press_on_child_of_disabled_parent_is_not_delivered_to_the_child() {
    let log: Log = Default::default();
    let (root, mid, _leaf) = tree(Resp::Accept, Resp::Accept, Resp::Accept, &log);
    mid.borrow().set_enabled(false);
    let mut d = EventTreeDispatcher::new();

    d.dispatch_event(&root, &mut press(60, 40));

    assert_eq!(
        *log.borrow(),
        vec!["root:Press(60,40)"],
        "disabling a parent disables its children (QWidget::isEnabled is effective)"
    );
}

#[test]
fn double_click_a_widget_does_not_handle_is_delivered_to_it_as_a_press() {
    // `QWidget::mouseDoubleClickEvent` calls `mousePressEvent` (qwidget.cpp:9636), so a button
    // that only knows presses still sees the second click of a double click.
    let log: Log = Default::default();
    let root = widget(Probe::new("root", Rect::new(0, 0, 400, 300), Resp::NotHandled, &log));
    let only_press = widget(PressOnlyProbe::new("leaf", Rect::new(30, 20, 80, 40), &log));
    root.borrow_mut().add_child(Rc::clone(&only_press));
    let mut d = EventTreeDispatcher::new();

    d.dispatch_event(
        &root,
        &mut Event::new_spontaneous(EventKind::MouseButtonDblClick { x: 40, y: 30, button: 1 }),
    );

    assert_eq!(*log.borrow(), vec!["leaf:Press(10,10)"]);
}

/// Handles `MouseButtonPress` only (returns false for everything else, like `Button`).
struct PressOnlyProbe(Probe);

impl PressOnlyProbe {
    fn new(name: &'static str, geometry: Rect, log: &Log) -> Self {
        Self(Probe::new(name, geometry, Resp::Accept, log))
    }
}

impl QObject for PressOnlyProbe {
    fn object_data(&self) -> &ObjectData {
        self.0.object_data()
    }
    fn object_data_mut(&mut self) -> &mut ObjectData {
        self.0.object_data_mut()
    }
    fn event(&mut self, event: &mut Event) -> bool {
        if matches!(event.kind, EventKind::MouseButtonPress { .. }) {
            self.0.event(event)
        } else {
            false
        }
    }
}

macro_rules! forward_widget {
    ($($f:ident($($a:ident: $t:ty),*) $(-> $r:ty)?;)*) => {
        $(fn $f(&self, $($a: $t),*) $(-> $r)? { self.0.$f($($a),*) })*
    };
}

impl Widget for PressOnlyProbe {
    fn widget_base(&self) -> &WidgetBase {
        self.0.widget_base()
    }
    forward_widget! {
        id() -> ObjectId;
        geometry() -> Rect;
        set_geometry(rect: Rect);
        is_visible() -> bool;
        set_visible(visible: bool);
        is_enabled() -> bool;
        set_enabled(enabled: bool);
        update();
        dirty_rect() -> Option<Rect>;
        clear_dirty();
        parent_widget() -> Option<WidgetWeak>;
        set_parent_widget(parent: Option<WidgetWeak>);
        window_id() -> Option<ObjectId>;
        set_window_id(window_id: Option<ObjectId>);
        children() -> Vec<WidgetRef>;
    }
    fn layout(&self) -> Option<&dyn Layout> {
        None
    }
    fn layout_mut(&mut self) -> Option<&mut Box<dyn Layout>> {
        None
    }
    fn set_layout(&mut self, _layout: Box<dyn Layout>) {}
    fn add_child(&mut self, child: WidgetRef) {
        self.0.add_child(child);
    }
    fn remove_child(&mut self, child_id: ObjectId) {
        self.0.remove_child(child_id);
    }
    fn paint_event(&mut self, _painter: &mut qtrs_gui::paint::Painter) {}
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

/// The built-in widgets follow the same rules: `QAbstractButton` ignores a right press, so it
/// reaches the window (a context menu or a window drag); a label ignores everything; the default
/// `QWidget::wheelEvent` ignores the wheel so a container over a scroll area lets it through.
#[test]
fn right_press_on_a_button_reaches_the_parent() {
    let log: Log = Default::default();
    let root = widget(Probe::new("root", Rect::new(0, 0, 400, 300), Resp::Accept, &log));
    let button: WidgetRef = Rc::new(RefCell::new(Box::new(Button::new("ok"))));
    button.borrow_mut().set_geometry(Rect::new(50, 50, 120, 36));
    root.borrow_mut().add_child(Rc::clone(&button));
    let mut d = EventTreeDispatcher::new();

    d.dispatch_event(
        &root,
        &mut Event::new_spontaneous(EventKind::MouseButtonPress { x: 70, y: 70, button: 2 }),
    );
    assert_eq!(*log.borrow(), vec!["root:Press(70,70)"]);

    log.borrow_mut().clear();
    d.dispatch_event(
        &root,
        &mut Event::new_spontaneous(EventKind::MouseButtonPress { x: 70, y: 70, button: 1 }),
    );
    assert!(log.borrow().is_empty(), "the left press is the button's: {:?}", log.borrow());
}

#[test]
fn wheel_over_a_label_inside_a_scroll_area_scrolls_it() {
    let content: WidgetRef = Rc::new(RefCell::new(Box::new(EmptyWidget::with_geometry(
        Rect::new(0, 0, 200, 600),
    ))));
    let label: WidgetRef = Rc::new(RefCell::new(Box::new(Label::new("row"))));
    label.borrow_mut().set_geometry(Rect::new(10, 10, 80, 20));
    content.borrow_mut().add_child(Rc::clone(&label));

    let area: WidgetRef = Rc::new(RefCell::new(Box::new(ScrollArea::new())));
    area.borrow_mut().set_geometry(Rect::new(0, 0, 200, 100));
    {
        let mut a = area.borrow_mut();
        let a = a.as_any_mut().downcast_mut::<ScrollArea>().unwrap();
        a.set_widget(Rc::clone(&content));
    }
    let root: WidgetRef = Rc::new(RefCell::new(Box::new(EmptyWidget::with_geometry(
        Rect::new(0, 0, 200, 100),
    ))));
    root.borrow_mut().add_child(Rc::clone(&area));
    let mut d = EventTreeDispatcher::new();
    let scroll_y = |area: &WidgetRef| {
        area.borrow().as_any().downcast_ref::<ScrollArea>().unwrap().scroll_position().y
    };
    assert_eq!(scroll_y(&area), 0);

    let wheel = |x, y| {
        Event::new_spontaneous(EventKind::Wheel {
            x,
            y,
            pixel_delta_x: 0,
            pixel_delta_y: 0,
            angle_delta_x: 0,
            angle_delta_y: -120,
            modifiers: 0,
        })
    };
    // Over the label (a child of the content), then over bare content.
    d.dispatch_event(&root, &mut wheel(20, 20));
    let after_label = scroll_y(&area);
    d.dispatch_event(&root, &mut wheel(150, 80));
    let after_content = scroll_y(&area);

    assert!(after_label > 0, "wheel over a label must reach the scroll area");
    assert!(after_content > after_label, "wheel over bare content must reach it too");
}

#[cfg(windows)]
mod window_level {
    use super::*;
    use qtrs_gui::geometry::primitives::Rect;
    use qtrs_platform::WindowFlags;
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsWindowVisible, SendMessageW, WM_CLOSE, WM_LBUTTONDBLCLK, WM_LBUTTONUP,
    };

    fn window_with_root(root: WidgetRef) -> Window {
        let mut win = Window::new("rc06", Rect::new(100, 100, 400, 300), WindowFlags::NORMAL)
            .expect("window");
        win.set_root_widget(root);
        win
    }

    /// The native window moves itself when first shown (frame adjustment): that `Move` is real,
    /// but not what these tests are about.
    fn show_hide_only(log: &Log) -> Vec<String> {
        log.borrow()
            .iter()
            .filter(|e| e.ends_with(":Show") || e.ends_with(":Hide"))
            .cloned()
            .collect()
    }

    fn hwnd(win: &Window) -> HWND {
        win.native_handle() as HWND
    }

    fn visible(win: &Window) -> bool {
        unsafe { IsWindowVisible(hwnd(win)) != 0 }
    }

    #[test]
    fn close_request_without_handler_hides_window() {
        let log: Log = Default::default();
        let (root, _m, _l) = tree(Resp::NotHandled, Resp::NotHandled, Resp::NotHandled, &log);
        let mut win = window_with_root(root);
        win.show();
        assert!(visible(&win));
        log.borrow_mut().clear();

        unsafe { SendMessageW(hwnd(&win), WM_CLOSE, 0, 0) };

        assert!(!visible(&win), "an accepted close event hides the window");
        assert_eq!(log.borrow().first().map(String::as_str), Some("root:Close"));
    }

    #[test]
    fn close_event_ignored_keeps_window_visible() {
        let log: Log = Default::default();
        let (root, _m, _l) = tree(Resp::NotHandled, Resp::NotHandled, Resp::Ignore, &log);
        let mut win = window_with_root(root);
        win.show();

        unsafe { SendMessageW(hwnd(&win), WM_CLOSE, 0, 0) };

        assert!(visible(&win), "an ignored close event must leave the window as it is");
        assert!(
            log.borrow().iter().any(|e| e == "root:Close"),
            "the window widget must be asked: {:?}",
            log.borrow()
        );
        assert!(
            !log.borrow().iter().any(|e| e.ends_with(":Hide")),
            "nothing is hidden when the close is refused: {:?}",
            log.borrow()
        );
    }

    #[test]
    fn window_close_handler_can_refuse_the_close() {
        let log: Log = Default::default();
        let (root, _m, _l) = tree(Resp::NotHandled, Resp::NotHandled, Resp::NotHandled, &log);
        let mut win = window_with_root(root);
        win.set_window_event_handler(|ev: &mut Event| {
            if matches!(ev.kind, EventKind::Close) {
                ev.ignore();
            }
        });
        win.show();

        unsafe { SendMessageW(hwnd(&win), WM_CLOSE, 0, 0) };

        assert!(visible(&win));
    }

    #[test]
    fn show_hide_events_delivered_in_order() {
        let log: Log = Default::default();
        let (root, _m, _l) = tree(Resp::NotHandled, Resp::NotHandled, Resp::NotHandled, &log);
        let mut win = window_with_root(root);

        win.show();
        assert_eq!(
            show_hide_only(&log),
            vec!["leaf:Show", "mid:Show", "root:Show"],
            "show_helper shows children before sending the window's own Show"
        );

        log.borrow_mut().clear();
        win.show();
        assert!(show_hide_only(&log).is_empty(), "showing a visible window sends nothing");

        win.hide();
        assert_eq!(
            show_hide_only(&log),
            vec!["root:Hide", "mid:Hide", "leaf:Hide"],
            "hide_helper sends the window's own Hide before its children's"
        );

        log.borrow_mut().clear();
        win.hide();
        assert!(show_hide_only(&log).is_empty(), "hiding a hidden window sends nothing");
    }

    #[test]
    fn explicitly_hidden_child_gets_no_show_event() {
        let log: Log = Default::default();
        let (root, _m, leaf) = tree(Resp::NotHandled, Resp::NotHandled, Resp::NotHandled, &log);
        leaf.borrow().set_visible(false);
        let mut win = window_with_root(root);

        win.show();

        assert_eq!(show_hide_only(&log), vec!["mid:Show", "root:Show"]);
    }

    #[test]
    fn accepted_close_hides_children_after_the_window() {
        let log: Log = Default::default();
        let (root, _m, _l) = tree(Resp::NotHandled, Resp::NotHandled, Resp::NotHandled, &log);
        let mut win = window_with_root(root);
        win.show();
        log.borrow_mut().clear();

        assert!(win.close());

        assert_eq!(
            show_hide_only(&log),
            vec!["root:Hide", "mid:Hide", "leaf:Hide"]
        );
        assert_eq!(log.borrow().first().map(String::as_str), Some("root:Close"));
        assert!(!win.is_visible());
    }

    #[test]
    fn double_click_and_release_reach_the_window_handler_when_no_widget_takes_them() {
        let log: Log = Default::default();
        let (root, _m, _l) = tree(Resp::Ignore, Resp::Ignore, Resp::Ignore, &log);
        let mut win = window_with_root(root);
        let seen: Rc<RefCell<Vec<String>>> = Default::default();
        let sink = Rc::clone(&seen);
        win.set_window_event_handler(move |ev: &mut Event| match &ev.kind {
            EventKind::MouseButtonDblClick { x, y, .. } => {
                sink.borrow_mut().push(format!("DblClick({x},{y})"))
            }
            EventKind::MouseButtonRelease { x, y, .. } => {
                sink.borrow_mut().push(format!("Release({x},{y})"))
            }
            _ => {}
        });
        win.show();
        let pos = (60 & 0xffff) | ((40 & 0xffff) << 16);

        unsafe {
            SendMessageW(hwnd(&win), WM_LBUTTONDBLCLK, 0, pos);
            SendMessageW(hwnd(&win), WM_LBUTTONUP, 0, pos);
        }

        assert_eq!(*seen.borrow(), vec!["DblClick(60,40)", "Release(60,40)"]);
        assert!(
            log.borrow().iter().any(|e| e == "leaf:DblClick(10,10)"),
            "the widget under the cursor is asked first: {:?}",
            log.borrow()
        );
    }

    #[test]
    fn unhandled_double_click_falls_back_to_the_press_handler() {
        // Without a window DblClick handler, `QWidget::mouseDoubleClickEvent` -> `mousePressEvent`.
        let log: Log = Default::default();
        let (root, _m, _l) = tree(Resp::Ignore, Resp::Ignore, Resp::Ignore, &log);
        let mut win = window_with_root(root);
        let pressed: Rc<RefCell<Vec<Point>>> = Default::default();
        let sink = Rc::clone(&pressed);
        win.set_mouse_press_handler(move |pos, _b| {
            sink.borrow_mut().push(pos);
            true
        });
        win.show();
        let pos = (60 & 0xffff) | ((40 & 0xffff) << 16);

        unsafe { SendMessageW(hwnd(&win), WM_LBUTTONDBLCLK, 0, pos) };

        assert_eq!(*pressed.borrow(), vec![Point::new(60, 40)]);
    }

    #[test]
    fn window_move_reaches_the_window_widget_and_handler() {
        use qtrs_platform::window_system_interface::Delivery;
        let log: Log = Default::default();
        let (root, _m, _l) = tree(Resp::NotHandled, Resp::NotHandled, Resp::NotHandled, &log);
        let mut win = window_with_root(root);
        let seen: Rc<RefCell<Vec<String>>> = Default::default();
        let sink = Rc::clone(&seen);
        win.set_window_event_handler(move |ev: &mut Event| {
            if let EventKind::Move { x, y, old_x, old_y } = &ev.kind {
                sink.borrow_mut().push(format!("Move({x},{y}) from ({old_x},{old_y})"));
            }
        });

        qtrs_platform::handle_geometry_change(
            Delivery::Default,
            hwnd(&win),
            Rect::new(150, 160, 400, 300),
        );

        assert_eq!(*seen.borrow(), vec!["Move(150,160) from (100,100)"]);
        assert_eq!(*log.borrow(), vec!["root:Move(150,160)"], "only the window widget is moved");
    }
}
