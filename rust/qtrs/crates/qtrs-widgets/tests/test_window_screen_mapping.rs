//! A top-level window's logical rect goes to native coordinates through the screen it lands on:
//! `QWindowPrivate::create` and `QWindow::setGeometry` pick that screen with `screenForGeometry`
//! (the screen under the rect's centre, else the last one it intersects, else the current one;
//! `qwindow.cpp:566-568, 1867-1870, 1894-1910`) and map with `toNativePixels` /
//! `toNativeGlobalPosition`, which scale relative to the screen's native top-left
//! (`qwindowsintegration.cpp:299-300`, `qhighdpiscaling_p.h:150-158, 219-252`,
//! `qhighdpiscaling.cpp:713-721`).
//!
//! The fake platform reports Qt's logical screen geometry (native top-left, size divided by the
//! ratio) and gives each window the ratio of the screen under its native centre, as Windows
//! does, so these tests need neither a second monitor nor `Win32Screen`.

use qtrs_gui::geometry::primitives::{Point, Rect};
use qtrs_gui::paint::Pixmap;
use qtrs_platform::platform_window::PlatformWindow;
use qtrs_platform::screen::PlatformScreen;
use qtrs_platform::window_system_interface::WindowSystemEventHandler;
use qtrs_platform::{
    set_platform_integration, GenericPlatformIntegration, PlatformIntegration, WindowFlags,
};
use qtrs_widgets::Window;
use std::sync::{Arc, Mutex};

/// A screen by its native rect and ratio; the first one is the primary screen.
#[derive(Clone, Copy)]
struct ScreenSpec {
    native: Rect,
    dpr: f32,
}

impl ScreenSpec {
    /// `QScreen::geometry` on Windows: the native top-left and the scaled size.
    fn logical(&self) -> Rect {
        let scale = |v: i32| (f64::from(v) / f64::from(self.dpr)).round() as i32;
        Rect::new(
            self.native.x,
            self.native.y,
            scale(self.native.width),
            scale(self.native.height),
        )
    }
}

const fn spec(x: i32, y: i32, width: i32, height: i32, dpr: f32) -> ScreenSpec {
    ScreenSpec {
        native: Rect::new(x, y, width, height),
        dpr,
    }
}

struct FakeScreen {
    spec: ScreenSpec,
    primary: bool,
}

impl PlatformScreen for FakeScreen {
    fn name(&self) -> String {
        if self.primary { "primary" } else { "secondary" }.to_string()
    }
    fn geometry(&self) -> Rect {
        self.spec.logical()
    }
    fn available_geometry(&self) -> Rect {
        self.spec.logical()
    }
    fn is_primary(&self) -> bool {
        self.primary
    }
    fn device_pixel_ratio(&self) -> f32 {
        self.spec.dpr
    }
}

#[derive(Default)]
struct Log {
    /// The rect of `create_window` followed by every rect of `PlatformWindow::set_geometry`.
    native_rects: Vec<Rect>,
    /// Every position of `PlatformWindow::set_target_pos`.
    targets: Vec<Point>,
}

struct FakeWindow {
    geometry: Rect,
    screens: Arc<Vec<ScreenSpec>>,
    log: Arc<Mutex<Log>>,
    min: (i32, i32),
}

impl PlatformWindow for FakeWindow {
    fn show(&self) {}
    fn hide(&self) {}
    fn is_active(&self) -> bool {
        false
    }
    fn geometry(&self) -> Rect {
        self.geometry
    }
    fn set_geometry(&mut self, rect: Rect) {
        self.geometry = rect;
        self.log.lock().unwrap().native_rects.push(rect);
    }
    fn set_stays_on_top(&mut self, _enabled: bool) {}
    fn set_click_through(&mut self, _enabled: bool) {}
    fn set_opacity(&mut self, _opacity: f32) {}
    fn opacity(&self) -> f32 {
        1.0
    }
    fn set_minimum_size(&mut self, w: i32, h: i32) {
        self.min = (w, h);
    }
    fn minimum_size(&self) -> (i32, i32) {
        self.min
    }
    /// The ratio of the screen under the window's native centre, else the primary screen's.
    fn device_pixel_ratio(&self) -> f32 {
        let centre = self.geometry.center();
        self.screens
            .iter()
            .find(|s| s.native.contains(centre))
            .unwrap_or(&self.screens[0])
            .dpr
    }
    fn present(&mut self, _pixmap: &mut Pixmap, _opacity: f32) -> Result<(), &'static str> {
        Ok(())
    }
    fn set_event_handler(&mut self, _handler: Box<dyn WindowSystemEventHandler>) {}
    fn set_target_pos(&mut self, pos: Option<Point>) {
        if let Some(pos) = pos {
            self.log.lock().unwrap().targets.push(pos);
        }
    }
    fn native_handle(&self) -> isize {
        0
    }
}

struct FakePlatform {
    inner: GenericPlatformIntegration,
    screens: Arc<Vec<ScreenSpec>>,
    log: Arc<Mutex<Log>>,
}

impl FakePlatform {
    fn screen(&self, index: usize) -> Box<dyn PlatformScreen> {
        Box::new(FakeScreen {
            spec: self.screens[index],
            primary: index == 0,
        })
    }
}

impl PlatformIntegration for FakePlatform {
    fn create_window(
        &self,
        _title: &str,
        rect: Rect,
        _flags: WindowFlags,
    ) -> Result<Box<dyn PlatformWindow>, &'static str> {
        self.log.lock().unwrap().native_rects.push(rect);
        Ok(Box::new(FakeWindow {
            geometry: rect,
            screens: Arc::clone(&self.screens),
            log: Arc::clone(&self.log),
            min: (0, 0),
        }))
    }
    fn create_tray_icon(
        &self,
        tooltip: &str,
        pixmap: &Pixmap,
    ) -> Result<Box<dyn qtrs_platform::platform_tray::PlatformTrayIcon>, &'static str> {
        self.inner.create_tray_icon(tooltip, pixmap)
    }
    fn primary_screen(&self) -> Box<dyn PlatformScreen> {
        self.screen(0)
    }
    fn screens(&self) -> Vec<Box<dyn PlatformScreen>> {
        (0..self.screens.len()).map(|i| self.screen(i)).collect()
    }
    fn screen_at(&self, pos: Point) -> Option<Box<dyn PlatformScreen>> {
        self.screens()
            .into_iter()
            .find(|s| s.geometry().contains(pos))
    }
    fn screen_changed(&self) -> &qtrs_core::signal::Signal<()> {
        self.inner.screen_changed()
    }
    fn theme(&self) -> Arc<dyn qtrs_platform::theme::PlatformTheme> {
        self.inner.theme()
    }
    fn clipboard(&self) -> Box<dyn qtrs_platform::clipboard::PlatformClipboard> {
        self.inner.clipboard()
    }
    fn create_hotkey_manager(
        &self,
    ) -> Result<Box<dyn qtrs_platform::hotkey::PlatformHotkeyManager>, &'static str> {
        self.inner.create_hotkey_manager()
    }
    fn cursor(&self) -> Box<dyn qtrs_platform::cursor::PlatformCursor> {
        self.inner.cursor()
    }
}

/// The platform integration is process-global: tests take turns.
static PLATFORM_LOCK: Mutex<()> = Mutex::new(());

struct Desktop {
    log: Arc<Mutex<Log>>,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl Desktop {
    fn new(screens: &[ScreenSpec]) -> Self {
        let guard = PLATFORM_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let log = Arc::new(Mutex::new(Log::default()));
        set_platform_integration(Arc::new(FakePlatform {
            inner: GenericPlatformIntegration::default(),
            screens: Arc::new(screens.to_vec()),
            log: Arc::clone(&log),
        }));
        Desktop { log, _guard: guard }
    }

    fn window(&self, rect: Rect) -> Window {
        Window::new("screen mapping", rect, WindowFlags::FRAMELESS).expect("window")
    }

    fn created(&self) -> Rect {
        self.log.lock().unwrap().native_rects[0]
    }

    fn last(&self) -> Rect {
        *self.log.lock().unwrap().native_rects.last().unwrap()
    }

    fn last_target(&self) -> Point {
        *self
            .log
            .lock()
            .unwrap()
            .targets
            .last()
            .expect("a target position")
    }
}

// 1920x1080 primary at 100%, and the same screen at 150% (2880x1620 native) on its right.
const PRIMARY_100: ScreenSpec = spec(0, 0, 1920, 1080, 1.0);
const RIGHT_150: ScreenSpec = spec(1920, 0, 2880, 1620, 1.5);

// --- which ratio: the target screen's, not the primary screen's ---------------------------------

#[test]
fn a_window_on_an_unscaled_screen_beside_a_scaled_primary_is_not_scaled() {
    // Primary 150% (logical 0,0 1920x1080); the 100% screen right of it starts at native 2880.
    let d = Desktop::new(&[spec(0, 0, 2880, 1620, 1.5), spec(2880, 0, 1920, 1080, 1.0)]);
    let _w = d.window(Rect::new(3000, 100, 400, 300));
    assert_eq!(
        d.created(),
        Rect::new(3000, 100, 400, 300),
        "position and size at 100%"
    );
    assert_eq!(d.last(), Rect::new(3000, 100, 400, 300));
}

// --- which origin: the target screen's ------------------------------------------------------------

#[test]
fn a_window_on_a_second_screen_of_the_same_ratio_maps_from_that_screens_origin() {
    // Both screens 150%, so only the origin differs from the primary screen's mapping.
    let d = Desktop::new(&[spec(0, 0, 2880, 1620, 1.5), spec(2880, 0, 2880, 1620, 1.5)]);
    let _w = d.window(Rect::new(3000, 100, 400, 300));
    // 2880 + (3000 - 2880) * 1.5 = 3060.
    assert_eq!(d.created(), Rect::new(3060, 150, 600, 450));
    assert_eq!(d.last(), Rect::new(3060, 150, 600, 450));
}

#[test]
fn origins_left_of_and_below_the_primary_screen() {
    let left = Desktop::new(&[PRIMARY_100, spec(-2880, 0, 2880, 1620, 1.5)]);
    let _w = left.window(Rect::new(-2000, 100, 400, 300));
    // -2880 + (-2000 + 2880) * 1.5 = -1560.
    assert_eq!(left.last(), Rect::new(-1560, 150, 600, 450));
    drop(_w);
    drop(left);

    let below = Desktop::new(&[PRIMARY_100, spec(0, 1080, 2880, 1620, 1.5)]);
    let _w = below.window(Rect::new(100, 1200, 400, 300));
    // 1080 + (1200 - 1080) * 1.5 = 1260.
    assert_eq!(below.last(), Rect::new(150, 1260, 600, 450));
}

// --- which screen: screenForGeometry ---------------------------------------------------------------

#[test]
fn the_screen_under_the_centre_maps_a_rect_that_starts_on_another_screen() {
    let d = Desktop::new(&[PRIMARY_100, RIGHT_150]);
    // Starts on the primary screen; its centre (2000, 250) is on the 150% screen.
    let _w = d.window(Rect::new(1800, 100, 400, 300));
    // 1920 + (1800 - 1920) * 1.5 = 1740.
    assert_eq!(d.created(), Rect::new(1740, 150, 600, 450));
}

#[test]
fn a_rect_whose_centre_is_on_no_screen_maps_through_a_screen_it_intersects() {
    let d = Desktop::new(&[PRIMARY_100, RIGHT_150]);
    // The 150% screen is logical x 1920..3840; the centre (3900, 1050) is past it.
    let _w = d.window(Rect::new(3700, 900, 400, 300));
    // 1920 + (3700 - 1920) * 1.5 = 4590, 900 * 1.5 = 1350.
    assert_eq!(d.created(), Rect::new(4590, 1350, 600, 450));
}

// --- moving an existing window ---------------------------------------------------------------------

#[test]
fn set_geometry_maps_through_the_screen_the_window_moves_to() {
    let d = Desktop::new(&[PRIMARY_100, RIGHT_150]);
    let mut w = d.window(Rect::new(100, 100, 400, 300));
    assert_eq!(d.last(), Rect::new(100, 100, 400, 300));

    w.set_geometry(Rect::new(2000, 100, 400, 300));
    assert_eq!(
        d.last(),
        Rect::new(2040, 150, 600, 450),
        "onto the 150% screen"
    );
}

#[test]
fn set_geometry_back_to_the_primary_screen_does_not_keep_the_old_screens_ratio() {
    let d = Desktop::new(&[PRIMARY_100, RIGHT_150]);
    let mut w = d.window(Rect::new(2000, 100, 400, 300));

    w.set_geometry(Rect::new(100, 100, 400, 300));
    assert_eq!(d.last(), Rect::new(100, 100, 400, 300));
}

#[test]
fn set_geometry_silent_maps_like_set_geometry() {
    let d = Desktop::new(&[PRIMARY_100, RIGHT_150]);
    let mut w = d.window(Rect::new(100, 100, 400, 300));

    w.set_geometry_silent(Rect::new(2000, 100, 400, 300));
    assert_eq!(d.last(), Rect::new(2040, 150, 600, 450));
}

#[test]
fn present_custom_at_places_the_window_through_the_screen_under_the_rect() {
    let d = Desktop::new(&[PRIMARY_100, RIGHT_150]);
    let mut w = d.window(Rect::new(100, 100, 400, 300));

    w.present_custom_at(Rect::new(2000, 100, 200, 100), |_| {});
    assert_eq!(
        d.last_target(),
        Point::new(2040, 150),
        "native position for the layered window"
    );
}
