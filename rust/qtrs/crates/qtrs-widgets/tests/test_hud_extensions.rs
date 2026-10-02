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
