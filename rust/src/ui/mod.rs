// src/ui/mod.rs — Qt HUD UI module

pub mod brand;
#[cfg(all(test, windows))]
mod button_hover_tests;
#[cfg(all(test, windows))]
mod geometry_persist_tests;
pub mod dock_geometry;
pub mod gauge;
pub mod hud_window;
pub mod placement;
pub mod provider_card;
#[cfg(all(test, windows))]
mod provider_card_relayout_tests;
#[cfg(test)]
mod qss_box_audit;
pub mod sidebar;
pub mod round_mask;
pub mod styles;
pub mod table_layout;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(all(test, windows))]
mod tool_tip_tests;
pub mod tray_icon;
pub mod usage_table;
#[cfg(all(test, windows))]
mod usage_table_relayout_tests;

#[cfg(target_os = "windows")]
pub fn enable_win32_dark_mode(hwnd: isize) {
    use std::ffi::{CString, OsStr};
    use std::os::windows::ffi::OsStrExt;
    use std::sync::LazyLock;

    type FnSetPreferredAppMode = unsafe extern "system" fn(i32) -> i32;
    type FnAllowDarkModeForWindow = unsafe extern "system" fn(isize, bool) -> bool;
    type FnFlushMenuThemes = unsafe extern "system" fn();

    struct DarkModeFns {
        set_preferred_app_mode: Option<FnSetPreferredAppMode>,
        allow_dark_for_window: Option<FnAllowDarkModeForWindow>,
        flush_menu_themes: Option<FnFlushMenuThemes>,
    }

    unsafe impl Send for DarkModeFns {}
    unsafe impl Sync for DarkModeFns {}

    static FNS: LazyLock<DarkModeFns> = LazyLock::new(|| {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetModuleHandleA(lpLibFileName: *const u8) -> isize;
            fn LoadLibraryA(lpLibFileName: *const u8) -> isize;
            fn GetProcAddress(hModule: isize, lpProcName: *const u8) -> usize;
        }

        unsafe {
            let uxtheme_name = CString::new("uxtheme.dll").unwrap();
            let mut uxtheme = GetModuleHandleA(uxtheme_name.as_ptr() as *const u8);
            if uxtheme == 0 {
                uxtheme = LoadLibraryA(uxtheme_name.as_ptr() as *const u8);
            }
            if uxtheme != 0 {
                let set_preferred_app_mode: Option<FnSetPreferredAppMode> =
                    std::mem::transmute(GetProcAddress(uxtheme, 135 as *const u8));
                let allow_dark_for_window: Option<FnAllowDarkModeForWindow> =
                    std::mem::transmute(GetProcAddress(uxtheme, 133 as *const u8));
                let flush_menu_themes: Option<FnFlushMenuThemes> =
                    std::mem::transmute(GetProcAddress(uxtheme, 136 as *const u8));
                DarkModeFns {
                    set_preferred_app_mode,
                    allow_dark_for_window,
                    flush_menu_themes,
                }
            } else {
                DarkModeFns {
                    set_preferred_app_mode: None,
                    allow_dark_for_window: None,
                    flush_menu_themes: None,
                }
            }
        }
    });

    #[link(name = "uxtheme")]
    extern "system" {
        fn SetWindowTheme(hWnd: isize, pszSubAppName: *const u16, pszSubIdList: *const u16) -> i32;
    }

    unsafe {
        if let Some(set_mode) = FNS.set_preferred_app_mode {
            set_mode(2);
        }
        if hwnd != 0 {
            if let Some(allow_win) = FNS.allow_dark_for_window {
                allow_win(hwnd, true);
            }
            let dark_theme: Vec<u16> = OsStr::new("DarkMode_Explorer\0").encode_wide().collect();
            let _ = SetWindowTheme(hwnd, dark_theme.as_ptr(), std::ptr::null());
        }
        if let Some(flush) = FNS.flush_menu_themes {
            flush();
        }
    }
}

/// Sets a label's text colour the way the Python HUD does (`label.setStyleSheet("color: …;")`):
/// a widget-local style sheet, which outranks the application's `QLabel { color }` rule.
/// `Label::set_color` is the palette colour, which that rule overrides.
pub(crate) fn set_label_color(
    w: &ferrokit_widgets::WidgetRef,
    color: ferrokit_gui::tiny_skia::Color,
) {
    if let Some(lbl) = w
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<ferrokit_widgets::Label>()
    {
        use ferrokit_widgets::Widget;
        lbl.set_color(color);
        let c = color.to_color_u8();
        lbl.set_style_sheet(&format!(
            "color: rgba({}, {}, {}, {});",
            c.red(),
            c.green(),
            c.blue(),
            c.alpha()
        ));
    }
}

/// A pen with round caps and joins. The HUD's drawing was matched against these; ferrokit's
/// `Pen::new` now defaults to Qt's square cap and bevel join.
pub(crate) fn round_pen(
    color: ferrokit_gui::tiny_skia::Color,
    width: f32,
) -> ferrokit_gui::paint::Pen {
    use ferrokit_gui::tiny_skia::{LineCap, LineJoin};
    ferrokit_gui::paint::Pen::new(color, width)
        .with_cap(LineCap::Round)
        .with_join(LineJoin::Round)
}

/// Whether the UI is dark for an `appearance` setting. `"auto"` follows the operating system's
/// colour scheme (Python `_system_is_dark`); an unknown scheme counts as dark, as there.
pub(crate) fn resolve_is_dark(appearance: &str) -> bool {
    match appearance {
        "light" => false,
        "dark" => true,
        _ => !matches!(
            ferrokit_platform::platform().theme().color_scheme(),
            ferrokit_platform::ColorScheme::Light
        ),
    }
}

/// A Qt signal of the HUD's own widgets (`Signal(str)`, `Signal(list)`, ...): plain callbacks on the
/// UI thread. `ferrokit_core::signal::Signal::connect` needs `Send + Sync` slots, which the HUD's
/// `Rc`-held widgets can never be, so signals between HUD classes are callbacks like these.
pub struct Notifier<T> {
    slots: std::cell::RefCell<Vec<Slot<T>>>,
}

type Slot<T> = std::rc::Rc<dyn Fn(&T)>;

impl<T> Default for Notifier<T> {
    fn default() -> Self {
        Self {
            slots: std::cell::RefCell::new(Vec::new()),
        }
    }
}

impl<T> Notifier<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// `signal.connect(slot)`.
    pub fn connect(&self, slot: impl Fn(&T) + 'static) {
        self.slots.borrow_mut().push(std::rc::Rc::new(slot));
    }

    /// `signal.emit(value)`. Slots run in connection order; a slot may connect or emit again.
    pub fn emit(&self, value: &T) {
        let slots: Vec<_> = self.slots.borrow().clone();
        for slot in slots {
            slot(value);
        }
    }
}

/// A value that only the thread that made it may touch, made `Send + Sync` so that a
/// `ferrokit_core::timer::Timer::timeout` slot (which has to be) can hold the HUD's `Rc` state.
/// Every HUD timer fires on the UI thread; `get` returns `None` anywhere else, and dropping the
/// value anywhere else leaks it instead of running its destructor on the wrong thread (an `Rc`
/// count changed from two threads is a data race).
pub struct UiThread<T> {
    value: std::mem::ManuallyDrop<T>,
    thread: std::thread::ThreadId,
}

// SAFETY: `value` is only reachable through `get`, which refuses every thread but the one that
// made it, and `Drop` runs the destructor on that thread only.
unsafe impl<T> Send for UiThread<T> {}
unsafe impl<T> Sync for UiThread<T> {}

impl<T> UiThread<T> {
    pub fn new(value: T) -> Self {
        Self {
            value: std::mem::ManuallyDrop::new(value),
            thread: std::thread::current().id(),
        }
    }

    pub fn get(&self) -> Option<&T> {
        (std::thread::current().id() == self.thread).then_some(&*self.value)
    }
}

impl<T> Drop for UiThread<T> {
    fn drop(&mut self) {
        if std::thread::current().id() == self.thread {
            // SAFETY: `value` is dropped exactly once, here, and never used afterwards.
            unsafe { std::mem::ManuallyDrop::drop(&mut self.value) };
        }
    }
}

#[cfg(test)]
mod ui_thread_tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn the_owning_thread_reads_and_drops_the_value() {
        let rc = Rc::new(5);
        let cell = UiThread::new(Rc::clone(&rc));
        assert_eq!(cell.get().map(|v| **v), Some(5));
        assert_eq!(Rc::strong_count(&rc), 2);
        drop(cell);
        assert_eq!(Rc::strong_count(&rc), 1);
    }

    #[test]
    fn another_thread_neither_reads_nor_drops_the_value() {
        let rc = Rc::new(5);
        let cell = UiThread::new(Rc::clone(&rc));
        let moved = std::thread::spawn(move || {
            assert!(cell.get().is_none());
            // dropped here, on the wrong thread: leaked, so the count is not touched
        });
        moved.join().unwrap();
        assert_eq!(Rc::strong_count(&rc), 2, "the Rc count must not change off-thread");
    }
}
