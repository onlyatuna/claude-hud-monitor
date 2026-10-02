//! 2D painting subsystem.
//!
//! Provides canvas abstractions (`PaintDevice`), raster image backends (`Image`),
//! painter paths, brushes/gradients, composition modes, palettes, and vector painter.
pub mod brush;
pub mod composition;
pub mod paint_device;
pub mod painter;
pub mod palette;
pub mod path;
pub mod pixmap;

pub use brush::*;
pub use composition::*;
pub use paint_device::PaintDevice;
pub use painter::{Painter, PainterState, Pen};
pub use palette::*;
pub use path::*;
pub use pixmap::Pixmap;
