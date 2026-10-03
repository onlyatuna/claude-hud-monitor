use qtrs_core::signal::Signal;
use qtrs_gui::geometry::primitives::{Point, Rect};
use qtrs_gui::paint::Pixmap;
use std::sync::Arc;

use crate::clipboard::PlatformClipboard;
use crate::cursor::PlatformCursor;
use crate::hotkey::PlatformHotkeyManager;
use crate::platform_tray::PlatformTrayIcon;
use crate::platform_window::PlatformWindow;
use crate::screen::PlatformScreen;
use crate::theme::PlatformTheme;
use crate::window::WindowFlags;

pub trait PlatformIntegration: Send + Sync {
    fn create_window(
        &self,
        title: &str,
        rect: Rect,
        flags: WindowFlags,
    ) -> Result<Box<dyn PlatformWindow>, &'static str>;

    fn create_tray_icon(
        &self,
        tooltip: &str,
        pixmap: &Pixmap,
    ) -> Result<Box<dyn PlatformTrayIcon>, &'static str>;

    fn primary_screen(&self) -> Box<dyn PlatformScreen>;
    fn screens(&self) -> Vec<Box<dyn PlatformScreen>>;
    fn screen_at(&self, pos: Point) -> Option<Box<dyn PlatformScreen>>;
    fn screen_changed(&self) -> &Signal<()>;
    fn theme(&self) -> Arc<dyn PlatformTheme>;
    fn clipboard(&self) -> Box<dyn PlatformClipboard>;
    fn create_hotkey_manager(&self) -> Result<Box<dyn PlatformHotkeyManager>, &'static str>;
    fn cursor(&self) -> Box<dyn PlatformCursor>;
}

#[cfg(windows)]
pub mod win32 {
    use super::*;
    use crate::clipboard::Win32Clipboard;
    use crate::cursor::Win32Cursor;
    use crate::hotkey::Win32HotkeyManager;
    use crate::screen::Win32Screen;
    use crate::theme::Win32Theme;
    use crate::tray_icon::TrayIcon;
    use crate::window::NativeWindow;

    pub struct Win32PlatformIntegration {
        screen_changed_signal: Signal<()>,
        theme: Arc<Win32Theme>,
    }

    impl Default for Win32PlatformIntegration {
        fn default() -> Self {
            Self {
                screen_changed_signal: Signal::new(),
                theme: Arc::new(Win32Theme::new()),
            }
        }
    }

    impl PlatformIntegration for Win32PlatformIntegration {
        fn create_window(
            &self,
            title: &str,
            rect: Rect,
            flags: WindowFlags,
        ) -> Result<Box<dyn PlatformWindow>, &'static str> {
            let win = NativeWindow::new(title, rect, flags)?;
            Ok(Box::new(win))
        }

        fn create_tray_icon(
            &self,
            tooltip: &str,
            pixmap: &Pixmap,
        ) -> Result<Box<dyn PlatformTrayIcon>, &'static str> {
            let hicon = TrayIcon::create_hicon_from_pixmap(pixmap)?;
            let tray = TrayIcon::new(tooltip, hicon)?;
            Ok(Box::new(*tray))
        }

        fn primary_screen(&self) -> Box<dyn PlatformScreen> {
            Box::new(Win32Screen::primary())
        }

        fn screens(&self) -> Vec<Box<dyn PlatformScreen>> {
            Win32Screen::all_screens()
                .into_iter()
                .map(|s| Box::new(s) as Box<dyn PlatformScreen>)
                .collect()
        }

        fn screen_at(&self, pos: Point) -> Option<Box<dyn PlatformScreen>> {
            Win32Screen::screen_at(pos).map(|s| Box::new(s) as Box<dyn PlatformScreen>)
        }

        fn screen_changed(&self) -> &Signal<()> {
            &self.screen_changed_signal
        }

        fn theme(&self) -> Arc<dyn PlatformTheme> {
            Arc::clone(&self.theme) as Arc<dyn PlatformTheme>
        }

        fn clipboard(&self) -> Box<dyn PlatformClipboard> {
            Box::new(Win32Clipboard)
        }

        fn create_hotkey_manager(&self) -> Result<Box<dyn PlatformHotkeyManager>, &'static str> {
            Ok(Box::new(Win32HotkeyManager::new(std::ptr::null_mut())))
        }

        fn cursor(&self) -> Box<dyn PlatformCursor> {
            Box::new(Win32Cursor::new())
        }
    }
}

pub mod generic {
    use super::*;
    use crate::clipboard::GenericClipboard;
    use crate::cursor::GenericCursor;
    use crate::hotkey::GenericHotkeyManager;
    use crate::platform_tray::GenericTrayIcon;
    use crate::platform_window::GenericWindow;
    use crate::screen::GenericScreen;
    use crate::theme::GenericTheme;

    pub struct GenericPlatformIntegration {
        screen_changed_signal: Signal<()>,
        theme: Arc<GenericTheme>,
    }

    impl Default for GenericPlatformIntegration {
        fn default() -> Self {
            Self {
                screen_changed_signal: Signal::new(),
                theme: Arc::new(GenericTheme::default()),
            }
        }
    }

    impl PlatformIntegration for GenericPlatformIntegration {
        fn create_window(
            &self,
            title: &str,
            rect: Rect,
            flags: WindowFlags,
        ) -> Result<Box<dyn PlatformWindow>, &'static str> {
            Ok(Box::new(GenericWindow::new(title, rect, flags)))
        }

        fn create_tray_icon(
            &self,
            tooltip: &str,
            pixmap: &Pixmap,
        ) -> Result<Box<dyn PlatformTrayIcon>, &'static str> {
            Ok(Box::new(GenericTrayIcon::new(tooltip, pixmap)))
        }

        fn primary_screen(&self) -> Box<dyn PlatformScreen> {
            Box::new(GenericScreen::default_primary())
        }

        fn screens(&self) -> Vec<Box<dyn PlatformScreen>> {
            vec![Box::new(GenericScreen::default_primary())]
        }

        fn screen_at(&self, pos: Point) -> Option<Box<dyn PlatformScreen>> {
            let primary = GenericScreen::default_primary();
            if primary.geometry().contains(pos) {
                Some(Box::new(primary))
            } else {
                None
            }
        }

        fn screen_changed(&self) -> &Signal<()> {
            &self.screen_changed_signal
        }

        fn theme(&self) -> Arc<dyn PlatformTheme> {
            Arc::clone(&self.theme) as Arc<dyn PlatformTheme>
        }

        fn clipboard(&self) -> Box<dyn PlatformClipboard> {
            Box::new(GenericClipboard::new())
        }

        fn create_hotkey_manager(&self) -> Result<Box<dyn PlatformHotkeyManager>, &'static str> {
            Ok(Box::new(GenericHotkeyManager::new()))
        }

        fn cursor(&self) -> Box<dyn PlatformCursor> {
            Box::new(GenericCursor::new())
        }
    }
}

pub mod unix {
    use super::*;
    use crate::clipboard::GenericClipboard;
    use crate::cursor::UnixCursor;
    use crate::hotkey::UnixHotkeyManager;
    use crate::platform_window::GenericWindow;
    use crate::theme::UnixTheme;
    use crate::tray::DbusStatusNotifierItem;
    use crate::window_wayland::WaylandNativeWindow;
    use crate::window_x11::X11NativeWindow;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DisplayServerKind {
        Wayland,
        X11,
        Generic,
    }

    pub struct UnixPlatformIntegration {
        screen_changed_signal: Signal<()>,
        theme: Arc<UnixTheme>,
    }

    impl UnixPlatformIntegration {
        pub fn detect_display_server() -> DisplayServerKind {
            if std::env::var("WAYLAND_DISPLAY").is_ok() || std::env::var("WAYLAND_SOCKET").is_ok() {
                DisplayServerKind::Wayland
            } else if std::env::var("DISPLAY").is_ok() {
                DisplayServerKind::X11
            } else {
                DisplayServerKind::Generic
            }
        }
    }

    impl Default for UnixPlatformIntegration {
        fn default() -> Self {
            Self {
                screen_changed_signal: Signal::new(),
                theme: Arc::new(UnixTheme::default()),
            }
        }
    }

    impl PlatformIntegration for UnixPlatformIntegration {
        fn create_window(
            &self,
            title: &str,
            rect: Rect,
            flags: WindowFlags,
        ) -> Result<Box<dyn PlatformWindow>, &'static str> {
            match Self::detect_display_server() {
                DisplayServerKind::Wayland => {
                    let win = WaylandNativeWindow::new(title, rect, flags)?;
                    Ok(Box::new(win))
                }
                DisplayServerKind::X11 => {
                    let win = X11NativeWindow::new(title, rect, flags)?;
                    Ok(Box::new(win))
                }
                DisplayServerKind::Generic => Ok(Box::new(GenericWindow::new(title, rect, flags))),
            }
        }

        fn create_tray_icon(
            &self,
            tooltip: &str,
            pixmap: &Pixmap,
        ) -> Result<Box<dyn PlatformTrayIcon>, &'static str> {
            let mut item = DbusStatusNotifierItem::new("qtrs.app", tooltip);
            let _ = item.set_icon(pixmap);
            let _ = item.set_tooltip(tooltip);
            Ok(Box::new(item))
        }

        fn primary_screen(&self) -> Box<dyn PlatformScreen> {
            Box::new(crate::screen::X11Screen::primary())
        }

        fn screens(&self) -> Vec<Box<dyn PlatformScreen>> {
            crate::screen::X11Screen::screens()
                .into_iter()
                .map(|s| Box::new(s) as Box<dyn PlatformScreen>)
                .collect()
        }

        fn screen_at(&self, pos: Point) -> Option<Box<dyn PlatformScreen>> {
            crate::screen::X11Screen::screen_at(pos).map(|s| Box::new(s) as Box<dyn PlatformScreen>)
        }

        fn screen_changed(&self) -> &Signal<()> {
            &self.screen_changed_signal
        }

        fn theme(&self) -> Arc<dyn PlatformTheme> {
            Arc::clone(&self.theme) as Arc<dyn PlatformTheme>
        }

        fn clipboard(&self) -> Box<dyn PlatformClipboard> {
            Box::new(GenericClipboard::new())
        }

        fn create_hotkey_manager(&self) -> Result<Box<dyn PlatformHotkeyManager>, &'static str> {
            Ok(Box::new(UnixHotkeyManager::new()))
        }

        fn cursor(&self) -> Box<dyn PlatformCursor> {
            Box::new(UnixCursor::new())
        }
    }
}

pub mod cocoa {
    use super::*;
    use crate::clipboard::GenericClipboard;
    use crate::cursor::CocoaCursor;
    use crate::hotkey::CocoaHotkeyManager;
    use crate::platform_tray::GenericTrayIcon;
    use crate::platform_window::GenericWindow;
    use crate::theme::CocoaTheme;
    use crate::tray::CocoaStatusItem;
    use crate::window_cocoa::CocoaNativeWindow;

    impl CocoaPlatformIntegration {
        pub fn is_headless() -> bool {
            std::env::var("CI").is_ok()
                || std::env::var("GITHUB_ACTIONS").is_ok()
                || std::env::var("QT_QPA_PLATFORM").as_deref() == Ok("offscreen")
                || std::env::var("QTRS_HEADLESS").is_ok()
        }
    }
    pub struct CocoaPlatformIntegration {
        screen_changed_signal: Signal<()>,
        theme: Arc<CocoaTheme>,
    }

    impl Default for CocoaPlatformIntegration {
        fn default() -> Self {
            Self {
                screen_changed_signal: Signal::new(),
                theme: Arc::new(CocoaTheme::default()),
            }
        }
    }

    impl PlatformIntegration for CocoaPlatformIntegration {
        fn create_window(
            &self,
            title: &str,
            rect: Rect,
            flags: WindowFlags,
        ) -> Result<Box<dyn PlatformWindow>, &'static str> {
            if Self::is_headless() {
                return Ok(Box::new(GenericWindow::new(title, rect, flags)));
            }
            let win = CocoaNativeWindow::new(title, rect, flags)?;
            Ok(Box::new(win))
        }

        fn create_tray_icon(
            &self,
            tooltip: &str,
            pixmap: &Pixmap,
        ) -> Result<Box<dyn PlatformTrayIcon>, &'static str> {
            if Self::is_headless() {
                return Ok(Box::new(GenericTrayIcon::new(tooltip, pixmap)));
            }
            let mut item = CocoaStatusItem::new(1);
            let _ = item.set_icon(pixmap);
            let _ = item.set_tooltip(tooltip);
            Ok(Box::new(item))
        }

        fn primary_screen(&self) -> Box<dyn PlatformScreen> {
            Box::new(crate::screen::CocoaScreen::primary())
        }

        fn screens(&self) -> Vec<Box<dyn PlatformScreen>> {
            crate::screen::CocoaScreen::screens()
                .into_iter()
                .map(|s| Box::new(s) as Box<dyn PlatformScreen>)
                .collect()
        }

        fn screen_at(&self, pos: Point) -> Option<Box<dyn PlatformScreen>> {
            crate::screen::CocoaScreen::screen_at(pos)
                .map(|s| Box::new(s) as Box<dyn PlatformScreen>)
        }

        fn screen_changed(&self) -> &Signal<()> {
            &self.screen_changed_signal
        }

        fn theme(&self) -> Arc<dyn PlatformTheme> {
            Arc::clone(&self.theme) as Arc<dyn PlatformTheme>
        }

        fn clipboard(&self) -> Box<dyn PlatformClipboard> {
            Box::new(GenericClipboard::new())
        }

        fn create_hotkey_manager(&self) -> Result<Box<dyn PlatformHotkeyManager>, &'static str> {
            Ok(Box::new(CocoaHotkeyManager::new()))
        }

        fn cursor(&self) -> Box<dyn PlatformCursor> {
            Box::new(CocoaCursor::new())
        }
    }
}

pub use cocoa::CocoaPlatformIntegration;
pub use generic::GenericPlatformIntegration;
pub use unix::DisplayServerKind;
pub use unix::UnixPlatformIntegration;

#[cfg(windows)]
pub use win32::Win32PlatformIntegration;

static PLATFORM_INTEGRATION: std::sync::RwLock<Option<Arc<dyn PlatformIntegration>>> =
    std::sync::RwLock::new(None);

pub fn platform() -> Arc<dyn PlatformIntegration> {
    {
        let reader = PLATFORM_INTEGRATION.read().unwrap();
        if let Some(integration) = &*reader {
            return Arc::clone(integration);
        }
    }

    let mut writer = PLATFORM_INTEGRATION.write().unwrap();
    if let Some(integration) = &*writer {
        return Arc::clone(integration);
    }

    #[cfg(windows)]
    let default_integration: Arc<dyn PlatformIntegration> =
        Arc::new(win32::Win32PlatformIntegration::default());

    #[cfg(target_os = "linux")]
    let default_integration: Arc<dyn PlatformIntegration> =
        Arc::new(unix::UnixPlatformIntegration::default());

    #[cfg(target_os = "macos")]
    let default_integration: Arc<dyn PlatformIntegration> =
        Arc::new(cocoa::CocoaPlatformIntegration::default());

    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    let default_integration: Arc<dyn PlatformIntegration> =
        Arc::new(generic::GenericPlatformIntegration::default());
    *writer = Some(Arc::clone(&default_integration));
    default_integration
}

pub fn set_platform_integration(integration: Arc<dyn PlatformIntegration>) {
    let mut writer = PLATFORM_INTEGRATION.write().unwrap();
    *writer = Some(integration);
}
