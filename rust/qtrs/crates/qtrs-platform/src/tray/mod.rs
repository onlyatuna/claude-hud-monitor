pub use crate::platform_tray::PlatformTrayIcon;

#[cfg(windows)]
pub mod win32;
#[cfg(windows)]
pub use win32::Win32TrayIcon;

pub mod dbus;
pub use dbus::DbusStatusNotifierItem;
pub mod dbus_connection;
pub use dbus_connection::{DbusConnection, DbusMessage};

pub mod macos;
pub use macos::CocoaStatusItem;
