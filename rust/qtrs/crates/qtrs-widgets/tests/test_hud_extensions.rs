use qtrs_gui::geometry::primitives::Rect;
use qtrs_platform::{Menu, WindowFlags};
use qtrs_widgets::{BoxLayout, EmptyWidget, Layout, StackedWidget, Widget, WidgetRef, Window};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[test]
fn test_stacked_widget_page_switching() {
    let mut stack = StackedWidget::new();
    let page0: WidgetRef = Rc::new(RefCell::new(Box::new(EmptyWidget::new())));
    let page1: WidgetRef = Rc::new(RefCell::new(Box::new(EmptyWidget::new())));

    let idx0 = stack.add_widget(page0.clone());
    let idx1 = stack.add_widget(page1.clone());
    assert_eq!(idx0, 0);
    assert_eq!(idx1, 1);
    assert_eq!(stack.count(), 2);
    assert_eq!(stack.current_index(), 0);

    let changed = Arc::new(AtomicUsize::new(999));
    let changed_clone = Arc::clone(&changed);
    stack.current_changed.connect(move |idx: &usize| {
        changed_clone.store(*idx, Ordering::SeqCst);
    });

    stack.set_current_index(1);
    assert_eq!(stack.current_index(), 1);
    assert_eq!(changed.load(Ordering::SeqCst), 1);
}

#[test]
fn test_box_layout_add_stretch() {
    let mut layout = BoxLayout::horizontal();
    let item1: WidgetRef = Rc::new(RefCell::new(Box::new(EmptyWidget::new())));
    let item2: WidgetRef = Rc::new(RefCell::new(Box::new(EmptyWidget::new())));

    layout.add_widget(item1.clone());
    layout.add_stretch(1);
    layout.add_widget(item2.clone());

    layout.set_geometry(Rect::new(0, 0, 300, 50));
    assert_eq!(item1.borrow().geometry().x, 0);
    assert!(item2.borrow().geometry().x > 100);
}

#[test]
fn test_tray_menu_submenus() {
    let mut root_menu = Menu::new();
    root_menu.add_action(1, "Open");

    let mut style_submenu = Menu::new();
    style_submenu.add_checkable(10, "Cards", true);
    style_submenu.add_checkable(11, "Table", false);

    root_menu.add_submenu("UI Style", style_submenu);
    root_menu.add_separator();
    root_menu.add_action(99, "Exit");

    assert_eq!(root_menu.len(), 4);
    assert!(root_menu.items[1].submenu.is_some());
    assert_eq!(root_menu.items[1].submenu.as_ref().unwrap().len(), 2);
}

#[test]
fn test_window_opacity_and_minimum_size() {
    let mut win = Window::new(
        "Opacity Test Window",
        Rect::new(100, 100, 200, 200),
        WindowFlags::FRAMELESS | WindowFlags::LAYERED,
    )
    .expect("Window creation failed");

    win.set_opacity(0.85);
    assert!((win.opacity() - 0.85).abs() < 1e-4);

    win.set_minimum_size(150, 120);
    assert_eq!(win.minimum_size(), (150, 120));
}

#[test]
fn test_menu_high_dpi_hit_testing() {
    use qtrs_widgets::action::Action;
    use qtrs_widgets::menu::Menu;
    use qtrs_platform::high_dpi::from_native_point;
    use qtrs_gui::geometry::primitives::Point;

    let mut menu = Menu::new("TestMenu");
    let act_refresh = Action::new_ref("Refresh All");
    let act_cards = Action::new_ref("Cards");
    let act_table = Action::new_ref("Table");
    let act_exit = Action::new_ref("Exit");

    menu.add_action(act_refresh.clone());
    menu.add_action(act_cards.clone());
    menu.add_action(act_table.clone());
    menu.add_separator();
    menu.add_action(act_exit.clone());

    menu.popup(Point::new(0, 0));

    let exit_geo = menu.action_geometry(&act_exit).expect("Exit action geometry exists");
    let target_logical_pt = Point::new(exit_geo.center().x, exit_geo.center().y);

    // Test across typical Windows DPI scaling factors: 1.0 (100%), 1.25 (125%), 1.5 (150%), 2.0 (200%)
    for dpr in [1.0f32, 1.25, 1.5, 2.0] {
        let physical_x = (target_logical_pt.x as f32 * dpr).round() as i32;
        let physical_y = (target_logical_pt.y as f32 * dpr).round() as i32;
        let physical_pt = Point::new(physical_x, physical_y);

        // Convert physical point using high_dpi helper
        let resolved_logical = from_native_point(physical_pt, dpr);

        let hit_action = menu.action_at(resolved_logical);
        assert!(hit_action.is_some(), "Must hit an action at DPR {dpr}");
        assert_eq!(
            hit_action.unwrap().borrow().text(),
            "Exit",
            "Must hit Exit action at DPR {dpr}"
        );
    }
}
#[test]
fn test_window_present_custom_physical_backing_store_parity() {
    let mut win = Window::new(
        "Present Custom Test",
        Rect::new(50, 50, 200, 150),
        WindowFlags::FRAMELESS | WindowFlags::LAYERED,
    )
    .expect("Window creation failed");

    let mut painted = false;
    win.present_custom(|_painter| {
        painted = true;
    });
    assert!(painted, "present_custom closure must be invoked");
    assert!(win.backing_store().physical_width() >= 200);
    assert!(win.backing_store().physical_height() >= 150);
}
#[test]
fn test_submenu_hover_and_signature_propagation() {
    use qtrs_widgets::action::Action;
    use qtrs_widgets::menu::Menu;
    use qtrs_gui::geometry::primitives::Point;

    let mut root = Menu::new("RootMenu");
    let sub_ref = root.add_menu("Layout");
    let act_triple = Action::new_ref("Horizontal Triple");
    let act_stack = Action::new_ref("Vertical Stack");
    sub_ref.borrow_mut().add_action(act_triple.clone());
    sub_ref.borrow_mut().add_action(act_stack.clone());

    let act_layout = root.actions()[0].clone();
    root.popup(Point::new(0, 0));

    // Move to root item 0 (Layout) -> opens submenu
    let layout_geo = root.action_geometry(&act_layout).expect("Layout action geometry");
    root.handle_mouse_move_at(layout_geo.center());

    assert!(root.active_action().is_some(), "Root item should be active");
    assert!(root.open_submenu().is_some(), "Submenu should be open");

    let sub_rc = root.open_submenu().unwrap();
    assert!(sub_rc.borrow().active_action().is_none(), "Initially submenu has no active item");

    let sig_before = root.hover_signature();

    // Calculate coordinates inside the submenu item
    let sub_origin = sub_rc.borrow().geometry();
    let triple_local = sub_rc.borrow().action_geometry(&act_triple).expect("Triple action geo");
    let root_coords_for_triple = Point::new(
        sub_origin.x + triple_local.center().x,
        sub_origin.y + triple_local.center().y,
    );

    // Move mouse into submenu item 0
    root.handle_mouse_move_at(root_coords_for_triple);

    assert_eq!(
        sub_rc.borrow().active_action().map(|a| a.borrow().text().to_string()),
        Some("Horizontal Triple".to_string()),
        "Submenu item 0 should be active after hover"
    );

    let sig_after = root.hover_signature();
    assert_ne!(
        sig_before, sig_after,
        "Hover signature must change to trigger window repaint"
    );

    // Move mouse back to root item -> clears submenu active item
    root.handle_mouse_move_at(layout_geo.center());
    assert!(
        sub_rc.borrow().active_action().is_none(),
        "Submenu active item must be cleared when mouse returns to parent menu"
    );
    assert_ne!(
        sig_after,
        root.hover_signature(),
        "Hover signature must change when returning to parent menu"
    );
}
#[test]
fn test_window_system_resize_event_and_backing_store_update() {
    use qtrs_core::event::{Event, EventKind};
    use qtrs_core::object::QObject;
    use qtrs_gui::geometry::primitives::{Rect, Size};
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::Arc;

    let mut win = Window::new("Resize Test", Rect::new(0, 0, 400, 300), WindowFlags::empty())
        .expect("create window");

    assert_eq!(win.geometry().width, 400);
    assert_eq!(win.geometry().height, 300);
    let initial_store_w = win.backing_store().physical_width();
    let initial_store_h = win.backing_store().physical_height();

    let resized_w = Arc::new(AtomicI32::new(0));
    let resized_h = Arc::new(AtomicI32::new(0));
    let rw = Arc::clone(&resized_w);
    let rh = Arc::clone(&resized_h);

    win.set_resize_handler(move |size: Size| {
        rw.store(size.width, Ordering::SeqCst);
        rh.store(size.height, Ordering::SeqCst);
    });

    // Simulate system resize event (e.g. from WM_SIZE)
    let mut resize_event = Event::new_spontaneous(EventKind::Resize {
        width: 600,
        height: 500,
        old_width: 400,
        old_height: 300,
    });

    assert!(win.event(&mut resize_event), "Window must handle EventKind::Resize");

    // Verify window geometry is updated
    assert_eq!(win.geometry().width, 600);
    assert_eq!(win.geometry().height, 500);

    // Verify resize callback was triggered
    assert_eq!(resized_w.load(Ordering::SeqCst), 600);
    assert_eq!(resized_h.load(Ordering::SeqCst), 500);

    // Verify backing store was resized to the new dimensions
    assert!(win.backing_store().physical_width() > initial_store_w);
    assert!(win.backing_store().physical_height() > initial_store_h);
}
#[test]
fn test_window_system_event_handler_resize() {
    use qtrs_gui::geometry::primitives::Size;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::Arc;

    let mut win = Window::new("Handler Resize Test", Rect::new(0, 0, 300, 200), WindowFlags::empty())
        .expect("create window");

    let initial_store_w = win.backing_store().physical_width();
    let initial_store_h = win.backing_store().physical_height();

    let resized_w = Arc::new(AtomicI32::new(0));
    let resized_h = Arc::new(AtomicI32::new(0));
    let rw = Arc::clone(&resized_w);
    let rh = Arc::clone(&resized_h);

    win.set_resize_handler(move |size: Size| {
        rw.store(size.width, Ordering::SeqCst);
        rh.store(size.height, Ordering::SeqCst);
    });

    // Programmatic set_geometry triggers internal resize without deadlock
    win.set_geometry(Rect::new(0, 0, 550, 420));
    assert_eq!(win.geometry().width, 550);
    assert_eq!(win.geometry().height, 420);
    assert!(win.backing_store().physical_width() > initial_store_w);
    assert!(win.backing_store().physical_height() > initial_store_h);
}

#[test]
fn test_geometry_change_event_delivery() {
    use qtrs_gui::geometry::primitives::Size;
    use qtrs_platform::window_system_interface::Delivery;
    use qtrs_platform::handle_geometry_change;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::Arc;

    let mut win = Window::new("Geometry Change Test", Rect::new(0, 0, 320, 240), WindowFlags::empty())
        .expect("create window");

    let initial_store_w = win.backing_store().physical_width();
    let initial_store_h = win.backing_store().physical_height();

    let resized_w = Arc::new(AtomicI32::new(0));
    let resized_h = Arc::new(AtomicI32::new(0));
    let rw = Arc::clone(&resized_w);
    let rh = Arc::clone(&resized_h);

    win.set_resize_handler(move |size: Size| {
        rw.store(size.width, Ordering::SeqCst);
        rh.store(size.height, Ordering::SeqCst);
    });

    #[cfg(windows)]
    {
        let hwnd = win.native_handle() as windows_sys::Win32::Foundation::HWND;
        handle_geometry_change(Delivery::Default, hwnd, Rect::new(10, 20, 480, 360));

        assert_eq!(win.geometry().x, 10);
        assert_eq!(win.geometry().y, 20);
        assert_eq!(win.geometry().width, 480);
        assert_eq!(win.geometry().height, 360);
        assert_eq!(resized_w.load(Ordering::SeqCst), 480);
        assert_eq!(resized_h.load(Ordering::SeqCst), 360);
        assert!(win.backing_store().physical_width() > initial_store_w);
        assert!(win.backing_store().physical_height() > initial_store_h);
    }
}
