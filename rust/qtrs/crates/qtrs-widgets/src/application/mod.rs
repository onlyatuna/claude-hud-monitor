use std::cell::RefCell;
use std::sync::{Arc, LazyLock, RwLock};

use crate::style::stylesheet::QStyleSheetStyle;

use qtrs_core::application::CoreApplication;
use qtrs_core::meta::MetaObject;
use qtrs_core::object::{ObjectData, ObjectId, QObject};
use qtrs_core::signal::Signal;
use qtrs_gui::application::GuiApplication;
use qtrs_gui::paint::palette::Palette;
use qtrs_gui::text::font::Font;

use crate::widget::WidgetRef;

thread_local! {
    static TOP_LEVEL_WINDOWS: RefCell<Vec<ObjectId>> = const { RefCell::new(Vec::new()) };
    static ACTIVE_WINDOW_ID: RefCell<Option<ObjectId>> = const { RefCell::new(None) };
    static FOCUS_WIDGET: RefCell<Option<WidgetRef>> = const { RefCell::new(None) };
}

static GLOBAL_DOUBLE_CLICK_INTERVAL_MS: RwLock<u32> = RwLock::new(400);
static GLOBAL_CURSOR_FLASH_TIME_MS: RwLock<u32> = RwLock::new(1000);
static GLOBAL_WHEEL_SCROLL_LINES: RwLock<i32> = RwLock::new(3);
static GLOBAL_START_DRAG_DISTANCE: RwLock<i32> = RwLock::new(10);
static GLOBAL_START_DRAG_TIME_MS: RwLock<u32> = RwLock::new(500);
static GLOBAL_STYLESHEET: LazyLock<RwLock<Option<Arc<QStyleSheetStyle>>>> =
    LazyLock::new(|| RwLock::new(None));

/// Full widget application singleton equivalent to Qt's `QApplication`.
///
/// Inherits and encapsulates `GuiApplication` and `CoreApplication`, adding:
/// - Top-level window registry & management (`topLevelWindows`)
/// - Focus widget & active window tracking (`focusWidget`, `activeWindow`)
/// - Interactive timing parameters (double-click interval, wheel scroll lines, drag thresholds)
/// - Desktop integration & shutdown coordination
pub struct Application {
    gui_app: GuiApplication,
    focus_changed: Signal<(Option<ObjectId>, Option<ObjectId>)>,
}

impl Application {
    /// Initializes a new `Application` instance.
    pub fn new(args: Vec<String>) -> Self {
        let gui_app = GuiApplication::new(args);

        Self {
            gui_app,
            focus_changed: Signal::new(),
        }
    }
    /// Sets the application-wide style sheet.
    pub fn set_style_sheet(qss: &str) {
        let style = if qss.trim().is_empty() {
            None
        } else {
            Some(Arc::new(QStyleSheetStyle::parse(qss)))
        };
        if let Ok(mut lock) = GLOBAL_STYLESHEET.write() {
            *lock = style;
        }
    }

    /// Retrieves the application-wide style sheet if set.
    pub fn style_sheet() -> Option<Arc<QStyleSheetStyle>> {
        GLOBAL_STYLESHEET.read().ok().and_then(|lock| lock.clone())
    }

    /// Access the underlying `GuiApplication`.
    pub fn gui_application(&self) -> &GuiApplication {
        &self.gui_app
    }

    /// Access the underlying mutable `GuiApplication`.
    pub fn gui_application_mut(&mut self) -> &mut GuiApplication {
        &mut self.gui_app
    }

    /// Access the underlying `CoreApplication`.
    pub fn core_application(&self) -> &CoreApplication {
        self.gui_app.core_application()
    }

    /// Access the underlying mutable `CoreApplication`.
    pub fn core_application_mut(&mut self) -> &mut CoreApplication {
        self.gui_app.core_application_mut()
    }

    /// Enters the main event loop and returns the exit code.
    pub fn exec(&mut self) -> i32 {
        self.gui_app.exec()
    }

    /// Exits the application with an exit code.
    pub fn exit(return_code: i32) {
        GuiApplication::exit(return_code);
    }

    /// Tells the application to quit cleanly.
    pub fn quit() {
        GuiApplication::quit();
    }
    /// Registers a top-level window with the application.
    pub fn register_window(window_id: ObjectId) {
        let _ = TOP_LEVEL_WINDOWS.try_with(|wins| {
            let mut list = wins.borrow_mut();
            if !list.contains(&window_id) {
                list.push(window_id);
            }
        });
    }

    /// Unregisters a top-level window when it is closed or destroyed.
    pub fn unregister_window(window_id: ObjectId) {
        let remaining_count = TOP_LEVEL_WINDOWS
            .try_with(|wins| {
                let mut list = wins.borrow_mut();
                list.retain(|&id| id != window_id);
                list.len()
            })
            .unwrap_or(0);

        let _ = ACTIVE_WINDOW_ID.try_with(|act| {
            let mut curr = act.borrow_mut();
            if *curr == Some(window_id) {
                *curr = None;
            }
        });

        // Trigger quitOnLastWindowClosed logic if enabled
        if remaining_count == 0 && GuiApplication::quit_on_last_window_closed() {
            Self::quit();
        }
    }

    /// Returns a list of all active top-level window object IDs.
    pub fn top_level_windows() -> Vec<ObjectId> {
        TOP_LEVEL_WINDOWS
            .try_with(|wins| wins.borrow().clone())
            .unwrap_or_default()
    }

    /// Returns the currently active window object ID.
    pub fn active_window() -> Option<ObjectId> {
        ACTIVE_WINDOW_ID
            .try_with(|act| *act.borrow())
            .unwrap_or(None)
    }

    /// Sets the active window object ID.
    pub fn set_active_window(window_id: Option<ObjectId>) {
        let _ = ACTIVE_WINDOW_ID.try_with(|act| {
            *act.borrow_mut() = window_id;
        });
    }

    // --- Focus Management ---

    /// Returns the currently focused widget reference if any.
    pub fn focus_widget() -> Option<WidgetRef> {
        FOCUS_WIDGET
            .try_with(|fw| fw.borrow().clone())
            .unwrap_or(None)
    }

    /// Sets the focused widget and emits focus change notifications.
    pub fn set_focus_widget(&self, widget: Option<WidgetRef>) {
        let (old_id, new_id) = FOCUS_WIDGET
            .try_with(|fw| {
                let mut current = fw.borrow_mut();
                let old_id = current.as_ref().map(|w| w.borrow().id());
                let new_id = widget.as_ref().map(|w| w.borrow().id());
                *current = widget;
                (old_id, new_id)
            })
            .unwrap_or((None, None));

        if old_id != new_id {
            self.focus_changed.emit(&(old_id, new_id));
        }
    }

    /// Signal emitted when the focused widget changes.
    pub fn focus_changed(&self) -> &Signal<(Option<ObjectId>, Option<ObjectId>)> {
        &self.focus_changed
    }

    // --- Global Palette & Font Passthrough ---

    pub fn palette() -> Palette {
        GuiApplication::palette()
    }

    pub fn set_palette(palette: Palette) {
        GuiApplication::set_palette(palette);
    }

    pub fn font() -> Font {
        GuiApplication::font()
    }

    pub fn set_font(font: Font) {
        GuiApplication::set_font(font);
    }

    // --- Interactive Settings ---

    pub fn double_click_interval() -> u32 {
        *GLOBAL_DOUBLE_CLICK_INTERVAL_MS.read().unwrap()
    }

    pub fn set_double_click_interval(ms: u32) {
        *GLOBAL_DOUBLE_CLICK_INTERVAL_MS.write().unwrap() = ms;
    }

    pub fn cursor_flash_time() -> u32 {
        *GLOBAL_CURSOR_FLASH_TIME_MS.read().unwrap()
    }

    pub fn set_cursor_flash_time(ms: u32) {
        *GLOBAL_CURSOR_FLASH_TIME_MS.write().unwrap() = ms;
    }

    pub fn wheel_scroll_lines() -> i32 {
        *GLOBAL_WHEEL_SCROLL_LINES.read().unwrap()
    }

    pub fn set_wheel_scroll_lines(lines: i32) {
        *GLOBAL_WHEEL_SCROLL_LINES.write().unwrap() = lines;
    }

    pub fn start_drag_distance() -> i32 {
        *GLOBAL_START_DRAG_DISTANCE.read().unwrap()
    }

    pub fn set_start_drag_distance(l: i32) {
        *GLOBAL_START_DRAG_DISTANCE.write().unwrap() = l;
    }

    pub fn start_drag_time() -> u32 {
        *GLOBAL_START_DRAG_TIME_MS.read().unwrap()
    }

    pub fn set_start_drag_time(ms: u32) {
        *GLOBAL_START_DRAG_TIME_MS.write().unwrap() = ms;
    }

    /// Resets all application singletons and thread-local state for tests.
    #[doc(hidden)]
    pub fn reset_for_test() {
        GuiApplication::reset_for_test();
        TOP_LEVEL_WINDOWS.with(|wins| wins.borrow_mut().clear());
        ACTIVE_WINDOW_ID.with(|act| *act.borrow_mut() = None);
        FOCUS_WIDGET.with(|fw| *fw.borrow_mut() = None);
        *GLOBAL_DOUBLE_CLICK_INTERVAL_MS.write().unwrap() = 400;
        *GLOBAL_CURSOR_FLASH_TIME_MS.write().unwrap() = 1000;
        *GLOBAL_WHEEL_SCROLL_LINES.write().unwrap() = 3;
        *GLOBAL_START_DRAG_DISTANCE.write().unwrap() = 10;
        *GLOBAL_START_DRAG_TIME_MS.write().unwrap() = 500;
    }
}

static APP_META_OBJECT: MetaObject = MetaObject::new(
    "QApplication",
    Some(&qtrs_core::meta::QOBJECT_META_OBJECT),
    &[],
    &[],
    &[],
    &[],
);

impl QObject for Application {
    fn object_data(&self) -> &ObjectData {
        self.gui_app.object_data()
    }

    fn object_data_mut(&mut self) -> &mut ObjectData {
        self.gui_app.object_data_mut()
    }

    fn as_qobject_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_qobject_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn meta_object(&self) -> &'static MetaObject {
        &APP_META_OBJECT
    }
}
