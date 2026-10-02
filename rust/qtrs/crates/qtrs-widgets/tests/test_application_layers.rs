use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;

use qtrs_core::application::{ApplicationAttribute, CoreApplication};
use qtrs_core::event::{Event, EventFilter, EventKind, FilterResult};
use qtrs_core::object::{register_boxed_qobject, ObjectData, ObjectId, QObject};
use qtrs_gui::application::{ApplicationState, GuiApplication, LayoutDirection};
use qtrs_gui::geometry::primitives::Rect;
use qtrs_gui::paint::palette::{ColorGroup, ColorRole, Palette};
use qtrs_gui::text::font::Font;
use qtrs_platform::WindowFlags;
use qtrs_widgets::application::Application;
use qtrs_widgets::widget::EmptyWidget;
use qtrs_widgets::window::Window;
static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

// -----------------------------------------------------------------------------
// Test 1: CoreApplication Lifecycle, Arguments, Metadata & Attributes
// -----------------------------------------------------------------------------

#[test]
fn test_core_application_metadata_and_attributes() {
    let _guard = TEST_MUTEX.lock().unwrap();
    CoreApplication::reset_for_test();
    assert!(!CoreApplication::instance_exists());

    let app = CoreApplication::new(vec![
        "myapp_binary".to_string(),
        "--config=production".to_string(),
        "--port=8080".to_string(),
    ]);

    assert!(CoreApplication::instance_exists());
    assert!(CoreApplication::instance_id().is_some());

    // 1. Arguments reflection
    let args = CoreApplication::arguments();
    assert_eq!(args.len(), 3);
    assert_eq!(args[1], "--config=production");
    assert_eq!(args[2], "--port=8080");

    // 2. Application metadata
    CoreApplication::set_application_name("SuperApp");
    assert_eq!(CoreApplication::application_name(), "SuperApp");

    CoreApplication::set_application_version("2.5.0");
    assert_eq!(CoreApplication::application_version(), "2.5.0");

    CoreApplication::set_organization_name("MyOrg");
    assert_eq!(CoreApplication::organization_name(), "MyOrg");

    CoreApplication::set_organization_domain("myorg.io");
    assert_eq!(CoreApplication::organization_domain(), "myorg.io");

    // 3. Application Attributes
    assert!(!CoreApplication::test_attribute(
        ApplicationAttribute::AaUseHighDpiPixmaps
    ));
    CoreApplication::set_attribute(ApplicationAttribute::AaUseHighDpiPixmaps, true);
    assert!(CoreApplication::test_attribute(
        ApplicationAttribute::AaUseHighDpiPixmaps
    ));
    CoreApplication::set_attribute(ApplicationAttribute::AaUseHighDpiPixmaps, false);
    assert!(!CoreApplication::test_attribute(
        ApplicationAttribute::AaUseHighDpiPixmaps
    ));

    // 4. Clean exit
    drop(app);
    assert!(!CoreApplication::instance_exists());
}

// -----------------------------------------------------------------------------
// Test 2: CoreApplication Exec, ProcessEvents, and AboutToQuit Signal
// -----------------------------------------------------------------------------

#[test]
fn test_core_application_exec_quit_and_about_to_quit() {
    let _guard = TEST_MUTEX.lock().unwrap();
    CoreApplication::reset_for_test();

    let mut app = CoreApplication::new(vec!["test_runner".to_string()]);

    let quit_signal_received = Arc::new(AtomicBool::new(false));
    let quit_signal_clone = Arc::clone(&quit_signal_received);

    app.about_to_quit().connect(move |_| {
        quit_signal_clone.store(true, Ordering::SeqCst);
    });

    // Post an exit event to the event loop
    CoreApplication::exit(42);

    let code = app.exec();
    assert_eq!(code, 42);
    assert!(quit_signal_received.load(Ordering::SeqCst));

    // Process events test - ensure non-blocking call succeeds without panic
    let _ = CoreApplication::process_events(false);
    drop(app);
}

// -----------------------------------------------------------------------------
// Test 3: Application Event Filtering & Synchronous send_event
// -----------------------------------------------------------------------------

struct SpyEventFilter {
    data: ObjectData,
    pub filtered_count: Arc<AtomicI32>,
}

impl SpyEventFilter {
    fn new(counter: Arc<AtomicI32>) -> Self {
        Self {
            data: ObjectData::with_auto_id(),
            filtered_count: counter,
        }
    }
}

impl QObject for SpyEventFilter {
    fn object_data(&self) -> &ObjectData {
        &self.data
    }
    fn object_data_mut(&mut self) -> &mut ObjectData {
        &mut self.data
    }
    fn as_qobject_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
    fn as_qobject_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
    fn meta_object(&self) -> &'static qtrs_core::meta::MetaObject {
        &qtrs_core::meta::QOBJECT_META_OBJECT
    }
    fn event(&mut self, _event: &mut Event) -> bool {
        true
    }
    fn event_filter(&mut self, _watched: ObjectId, _event: &mut Event) -> bool {
        self.filtered_count.fetch_add(1, Ordering::SeqCst);
        false
    }
}

impl EventFilter for SpyEventFilter {
    fn event_filter(&mut self, _watched: ObjectId, _event: &mut Event) -> FilterResult {
        self.filtered_count.fetch_add(1, Ordering::SeqCst);
        FilterResult::Pass
    }
}

#[test]
fn test_application_level_event_filters() {
    let _guard = TEST_MUTEX.lock().unwrap();
    CoreApplication::reset_for_test();

    let app = CoreApplication::new(vec![]);
    let count = Arc::new(AtomicI32::new(0));

    let filter_obj = Box::new(SpyEventFilter::new(Arc::clone(&count)));
    // SAFETY: test-only; objects outlive their registration scope and no aliases exist.
    let (filter_id, _pinned) = unsafe { register_boxed_qobject(filter_obj) };

    CoreApplication::install_event_filter(filter_id);

    // Create and register a test target object
    let target = Box::new(SpyEventFilter::new(Arc::new(AtomicI32::new(0))));
    // SAFETY: test-only; objects outlive their registration scope and no aliases exist.
    let (target_id, _pinned_target) = unsafe { register_boxed_qobject(target) };

    let mut ev = Event::new(EventKind::UpdateRequest);
    let handled = CoreApplication::send_event(target_id, &mut ev);
    assert!(handled);
    assert_eq!(count.load(Ordering::SeqCst), 1);

    // Remove event filter
    CoreApplication::remove_event_filter(filter_id);
    let mut ev2 = Event::new(EventKind::UpdateRequest);
    let handled2 = CoreApplication::send_event(target_id, &mut ev2);
    assert!(handled2);

    assert_eq!(count.load(Ordering::SeqCst), 1); // Should not increase

    drop(app);
}

// -----------------------------------------------------------------------------
// Test 4: GuiApplication Palette, Font, StyleHints & ApplicationState
// -----------------------------------------------------------------------------

#[test]
fn test_gui_application_palette_font_and_state() {
    let _guard = TEST_MUTEX.lock().unwrap();
    GuiApplication::reset_for_test();

    let gui_app = GuiApplication::new(vec!["gui_test".to_string()]);

    // 1. Global palette
    let dark_pal = Palette::dark();
    GuiApplication::set_palette(dark_pal);
    let current_pal = GuiApplication::palette();
    let window_bg = current_pal.color(ColorGroup::Active, ColorRole::Window);
    assert_eq!(
        window_bg,
        qtrs_gui::tiny_skia::Color::from_rgba8(30, 30, 30, 255)
    );
    // 2. Global font
    let font = Font::new("Segoe UI", 12.0);
    GuiApplication::set_font(font);
    assert_eq!(GuiApplication::font().family, "Segoe UI");
    assert_eq!(GuiApplication::font().size, 12.0);
    // 3. Style Hints
    let mut hints = GuiApplication::style_hints();
    hints.mouse_double_click_interval_ms = 600;
    GuiApplication::set_style_hints(hints);
    assert_eq!(
        GuiApplication::style_hints().mouse_double_click_interval_ms,
        600
    );

    // 4. Layout direction
    assert!(GuiApplication::is_left_to_right());
    GuiApplication::set_layout_direction(LayoutDirection::RightToLeft);
    assert!(GuiApplication::is_right_to_left());

    // 5. Application state change signal
    let state_notified = Arc::new(AtomicBool::new(false));
    let state_clone = Arc::clone(&state_notified);
    gui_app.application_state_changed().connect(move |state| {
        if *state == ApplicationState::ApplicationInactive {
            state_clone.store(true, Ordering::SeqCst);
        }
    });

    gui_app.set_application_state(ApplicationState::ApplicationInactive);
    assert!(state_notified.load(Ordering::SeqCst));
    assert_eq!(
        GuiApplication::application_state(),
        ApplicationState::ApplicationInactive
    );

    drop(gui_app);
}

// -----------------------------------------------------------------------------
// Test 5: Application Top-Level Window Lifecycle & Focus Tracking
// -----------------------------------------------------------------------------

#[test]
fn test_widget_application_window_registry_and_focus() {
    let _guard = TEST_MUTEX.lock().unwrap();
    Application::reset_for_test();

    let app = Application::new(vec!["widget_test".to_string()]);

    assert_eq!(Application::top_level_windows().len(), 0);

    // Create top-level windows
    let win1 = Window::new(
        "Window 1",
        Rect::new(100, 100, 400, 300),
        WindowFlags::NORMAL,
    )
    .expect("Failed to create win1");
    let win2 = Window::new(
        "Window 2",
        Rect::new(150, 150, 400, 300),
        WindowFlags::NORMAL,
    )
    .expect("Failed to create win2");

    let windows = Application::top_level_windows();
    assert_eq!(windows.len(), 2);
    assert!(windows.contains(&win1.id()));
    assert!(windows.contains(&win2.id()));

    // Active window tracking
    Application::set_active_window(Some(win1.id()));
    assert_eq!(Application::active_window(), Some(win1.id()));

    // Focus widget tracking
    let focus_changed_received = Arc::new(AtomicBool::new(false));
    let focus_clone = Arc::clone(&focus_changed_received);
    app.focus_changed().connect(move |(old_id, new_id)| {
        if old_id.is_none() && new_id.is_some() {
            focus_clone.store(true, Ordering::SeqCst);
        }
    });

    let widget_ref: qtrs_widgets::widget::WidgetRef = Rc::new(RefCell::new(Box::new(
        EmptyWidget::with_geometry(Rect::new(0, 0, 50, 50)),
    )));
    app.set_focus_widget(Some(Rc::clone(&widget_ref)));
    assert!(focus_changed_received.load(Ordering::SeqCst));
    assert!(Application::focus_widget().is_some());

    // Interactive settings
    Application::set_double_click_interval(500);
    assert_eq!(Application::double_click_interval(), 500);

    Application::set_wheel_scroll_lines(5);
    assert_eq!(Application::wheel_scroll_lines(), 5);

    // Closing/dropping win1 reduces registry
    drop(win1);
    let remaining = Application::top_level_windows();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0], win2.id());

    // Dropping win2 empties registry
    drop(win2);
    assert_eq!(Application::top_level_windows().len(), 0);

    drop(app);
}
