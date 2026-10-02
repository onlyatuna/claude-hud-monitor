//! High-DPI scaling helpers, matching Qt's `QHighDpi` (qhighdpiscaling.cpp).

use qtrs_gui::geometry::primitives::{Point, Rect, Size};

/// Converts native physical pixels to device-independent logical pixels (DIPs).
/// Matches `QHighDpi::fromNativePixels`.
#[inline]
pub fn from_native_pixels(val: f32, dpr: f32) -> f32 {
    if dpr > 0.0 {
        val / dpr
    } else {
        val
    }
}

/// Converts device-independent logical pixels (DIPs) to native physical pixels.
/// Matches `QHighDpi::toNativePixels`.
#[inline]
pub fn to_native_pixels(val: f32, dpr: f32) -> f32 {
    if dpr > 0.0 {
        val * dpr
    } else {
        val
    }
}

/// Converts a physical point to a logical point.
/// Matches `QHighDpi::fromNativeLocalPosition` / `fromNativeGlobalPosition`.
#[inline]
pub fn from_native_point(pt: Point, dpr: f32) -> Point {
    if dpr <= 1.0 {
        pt
    } else {
        Point::new(
            (pt.x as f32 / dpr).round() as i32,
            (pt.y as f32 / dpr).round() as i32,
        )
    }
}

/// Converts a logical point to a physical point.
/// Matches `QHighDpi::toNativeLocalPosition` / `toNativeGlobalPosition`.
#[inline]
pub fn to_native_point(pt: Point, dpr: f32) -> Point {
    if dpr <= 1.0 {
        pt
    } else {
        Point::new(
            (pt.x as f32 * dpr).round() as i32,
            (pt.y as f32 * dpr).round() as i32,
        )
    }
}

/// Converts a physical size to a logical size.
#[inline]
pub fn from_native_size(size: Size, dpr: f32) -> Size {
    if dpr <= 1.0 {
        size
    } else {
        Size::new(
            (size.width as f32 / dpr).round() as i32,
            (size.height as f32 / dpr).round() as i32,
        )
    }
}

/// Converts a logical size to a physical size.
#[inline]
pub fn to_native_size(size: Size, dpr: f32) -> Size {
    if dpr <= 1.0 {
        size
    } else {
        Size::new(
            (size.width as f32 * dpr).round() as i32,
            (size.height as f32 * dpr).round() as i32,
        )
    }
}

/// Converts a physical rect to a logical rect.
/// Matches `QHighDpi::fromNativeWindowGeometry`.
#[inline]
pub fn from_native_rect(rect: Rect, dpr: f32) -> Rect {
    if dpr <= 1.0 {
        rect
    } else {
        Rect::new(
            (rect.x as f32 / dpr).round() as i32,
            (rect.y as f32 / dpr).round() as i32,
            (rect.width as f32 / dpr).round() as i32,
            (rect.height as f32 / dpr).round() as i32,
        )
    }
}

/// Converts a logical rect to a physical rect.
/// Matches `QHighDpi::toNativeWindowGeometry`.
#[inline]
pub fn to_native_rect(rect: Rect, dpr: f32) -> Rect {
    if dpr <= 1.0 {
        rect
    } else {
        Rect::new(
            (rect.x as f32 * dpr).round() as i32,
            (rect.y as f32 * dpr).round() as i32,
            (rect.width as f32 * dpr).round() as i32,
            (rect.height as f32 * dpr).round() as i32,
        )
    }
}
