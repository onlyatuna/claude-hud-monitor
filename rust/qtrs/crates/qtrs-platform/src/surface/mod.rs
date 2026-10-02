use qtrs_gui::geometry::Rect;
use qtrs_gui::paint::Pixmap;

pub trait PlatformSurface: Send + Sync {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn resize(&mut self, width: u32, height: u32) -> Result<(), &'static str>;
    fn present(&mut self, pixmap: &mut Pixmap, opacity: f32) -> Result<(), &'static str>;
    fn present_dirty(
        &mut self,
        pixmap: &mut Pixmap,
        opacity: f32,
        dirty: Rect,
    ) -> Result<(), &'static str>;
}

pub mod macos;
pub mod wayland;
#[cfg(windows)]
pub mod win32;
pub mod x11;

#[cfg(windows)]
pub use win32::Win32LayeredSurface;
#[cfg(windows)]
pub type LayeredSurface = Win32LayeredSurface;

pub use macos::CocoaLayerSurface;
pub use wayland::WaylandShmSurface;
pub use x11::X11ShmSurface;
