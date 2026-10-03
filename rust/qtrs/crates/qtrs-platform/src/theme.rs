use qtrs_core::signal::Signal;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ColorScheme {
    Unknown = 0,
    Light = 1,
    Dark = 2,
}

impl From<u8> for ColorScheme {
    fn from(val: u8) -> Self {
        match val {
            1 => ColorScheme::Light,
            2 => ColorScheme::Dark,
            _ => ColorScheme::Unknown,
        }
    }
}

pub trait PlatformTheme: Send + Sync {
    fn color_scheme(&self) -> ColorScheme;
    fn theme_changed(&self) -> &Signal<ColorScheme>;
    fn refresh(&self);
    /// Family of the system menu font (`QPlatformTheme::MenuFont`), which Qt uses as the base
    /// font of every `QMenu`. `None` when the platform has no such query.
    fn menu_font_family(&self) -> Option<String> {
        None
    }
}

#[cfg(windows)]
pub mod win32_theme {
    use super::*;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, REG_DWORD,
    };

    pub struct Win32Theme {
        cached_scheme: AtomicU8,
        theme_changed_signal: Signal<ColorScheme>,
    }

    impl Default for Win32Theme {
        fn default() -> Self {
            let initial = Self::query_color_scheme();
            Self {
                cached_scheme: AtomicU8::new(initial as u8),
                theme_changed_signal: Signal::new(),
            }
        }
    }

    impl Win32Theme {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn query_color_scheme() -> ColorScheme {
            let subkey: Vec<u16> =
                "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
            let value_name: Vec<u16> = "AppsUseLightTheme"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            unsafe {
                let mut hkey: HKEY = std::ptr::null_mut();
                if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) != 0 {
                    return ColorScheme::Unknown;
                }

                let mut val_type: u32 = 0;
                let mut data: u32 = 0;
                let mut data_size: u32 = std::mem::size_of::<u32>() as u32;

                let status = RegQueryValueExW(
                    hkey,
                    value_name.as_ptr(),
                    std::ptr::null_mut(),
                    &mut val_type,
                    &mut data as *mut u32 as *mut u8,
                    &mut data_size,
                );

                RegCloseKey(hkey);

                if status == 0 && val_type == REG_DWORD {
                    if data == 0 {
                        ColorScheme::Dark
                    } else {
                        ColorScheme::Light
                    }
                } else {
                    ColorScheme::Unknown
                }
            }
        }
    }

    impl PlatformTheme for Win32Theme {
        fn color_scheme(&self) -> ColorScheme {
            ColorScheme::from(self.cached_scheme.load(Ordering::Acquire))
        }

        fn theme_changed(&self) -> &Signal<ColorScheme> {
            &self.theme_changed_signal
        }

        fn menu_font_family(&self) -> Option<String> {
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                SystemParametersInfoW, NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS,
            };
            // SAFETY: `NONCLIENTMETRICSW` is plain data; `cbSize` is set as the API requires.
            unsafe {
                let mut ncm: NONCLIENTMETRICSW = std::mem::zeroed();
                ncm.cbSize = std::mem::size_of::<NONCLIENTMETRICSW>() as u32;
                if SystemParametersInfoW(SPI_GETNONCLIENTMETRICS, ncm.cbSize, &mut ncm as *mut _ as *mut _, 0) == 0 {
                    return None;
                }
                let face = &ncm.lfMenuFont.lfFaceName;
                let len = face.iter().position(|&c| c == 0).unwrap_or(face.len());
                let name = String::from_utf16_lossy(&face[..len]);
                (!name.is_empty()).then_some(name)
            }
        }

        fn refresh(&self) {
            let current = Self::query_color_scheme();
            let previous =
                ColorScheme::from(self.cached_scheme.swap(current as u8, Ordering::AcqRel));
            if current != previous {
                self.theme_changed_signal.emit(&current);
            }
        }
    }
}

#[cfg(windows)]
pub use win32_theme::Win32Theme;

pub struct GenericTheme {
    cached_scheme: AtomicU8,
    theme_changed_signal: Signal<ColorScheme>,
}

impl Default for GenericTheme {
    fn default() -> Self {
        Self::new(ColorScheme::Light)
    }
}

impl GenericTheme {
    pub fn new(initial: ColorScheme) -> Self {
        Self {
            cached_scheme: AtomicU8::new(initial as u8),
            theme_changed_signal: Signal::new(),
        }
    }

    pub fn set_color_scheme(&self, scheme: ColorScheme) {
        let previous = ColorScheme::from(self.cached_scheme.swap(scheme as u8, Ordering::AcqRel));
        if scheme != previous {
            self.theme_changed_signal.emit(&scheme);
        }
    }
}

impl PlatformTheme for GenericTheme {
    fn color_scheme(&self) -> ColorScheme {
        ColorScheme::from(self.cached_scheme.load(Ordering::Acquire))
    }

    fn theme_changed(&self) -> &Signal<ColorScheme> {
        &self.theme_changed_signal
    }

    fn refresh(&self) {}
}

pub mod cocoa_theme {
    use super::*;
    use crate::objc_runtime::{Class, ObjcMsg, Sel};

    pub struct CocoaTheme {
        cached_scheme: AtomicU8,
        theme_changed_signal: Signal<ColorScheme>,
    }

    impl Default for CocoaTheme {
        fn default() -> Self {
            let initial = Self::query_color_scheme();
            Self {
                cached_scheme: AtomicU8::new(initial as u8),
                theme_changed_signal: Signal::new(),
            }
        }
    }

    impl CocoaTheme {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn query_color_scheme() -> ColorScheme {
            let nsapp_class = Class::get("NSApplication").unwrap_or(Class::NIL);
            let nsapp = ObjcMsg::send_class_0(nsapp_class, Sel::register("sharedApplication"));
            if nsapp.is_nil() {
                return ColorScheme::Dark;
            }

            let appearance = ObjcMsg::send_0(nsapp, Sel::register("effectiveAppearance"));
            if appearance.is_nil() {
                return ColorScheme::Dark;
            }

            // In macOS AppKit, effectiveAppearance name can be checked
            let name_id = ObjcMsg::send_0(appearance, Sel::register("name"));
            if !name_id.is_nil() {
                ColorScheme::Dark
            } else {
                ColorScheme::Light
            }
        }

        pub fn set_color_scheme(&self, scheme: ColorScheme) {
            let previous =
                ColorScheme::from(self.cached_scheme.swap(scheme as u8, Ordering::AcqRel));
            if scheme != previous {
                self.theme_changed_signal.emit(&scheme);
            }
        }
    }

    impl PlatformTheme for CocoaTheme {
        fn color_scheme(&self) -> ColorScheme {
            ColorScheme::from(self.cached_scheme.load(Ordering::Acquire))
        }

        fn theme_changed(&self) -> &Signal<ColorScheme> {
            &self.theme_changed_signal
        }

        fn refresh(&self) {
            let current = Self::query_color_scheme();
            let previous =
                ColorScheme::from(self.cached_scheme.swap(current as u8, Ordering::AcqRel));
            if current != previous {
                self.theme_changed_signal.emit(&current);
            }
        }
    }
}

pub use cocoa_theme::CocoaTheme;

pub mod unix_theme {
    use super::*;

    pub struct UnixTheme {
        cached_scheme: AtomicU8,
        theme_changed_signal: Signal<ColorScheme>,
    }

    impl Default for UnixTheme {
        fn default() -> Self {
            let initial = Self::query_color_scheme();
            Self {
                cached_scheme: AtomicU8::new(initial as u8),
                theme_changed_signal: Signal::new(),
            }
        }
    }

    impl UnixTheme {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn query_color_scheme() -> ColorScheme {
            if let Ok(theme) = std::env::var("GTK_THEME") {
                if theme.to_lowercase().contains("dark") {
                    return ColorScheme::Dark;
                } else if theme.to_lowercase().contains("light") {
                    return ColorScheme::Light;
                }
            }
            ColorScheme::Dark
        }

        pub fn set_color_scheme(&self, scheme: ColorScheme) {
            let previous =
                ColorScheme::from(self.cached_scheme.swap(scheme as u8, Ordering::AcqRel));
            if scheme != previous {
                self.theme_changed_signal.emit(&scheme);
            }
        }
    }

    impl PlatformTheme for UnixTheme {
        fn color_scheme(&self) -> ColorScheme {
            ColorScheme::from(self.cached_scheme.load(Ordering::Acquire))
        }

        fn theme_changed(&self) -> &Signal<ColorScheme> {
            &self.theme_changed_signal
        }

        fn refresh(&self) {
            let current = Self::query_color_scheme();
            let previous =
                ColorScheme::from(self.cached_scheme.swap(current as u8, Ordering::AcqRel));
            if current != previous {
                self.theme_changed_signal.emit(&current);
            }
        }
    }
}

pub use unix_theme::UnixTheme;
