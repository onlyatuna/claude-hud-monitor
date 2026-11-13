//! Translucent outline shown while the sidebar is dragged near a screen edge: where the bar will
//! dock (port of `python/ui/dock_preview.py`).

use std::cell::Cell;
use std::rc::Rc;

use ferrokit_gui::geometry::primitives::{Rect, RectF};
use ferrokit_gui::paint::brush::Brush;
use ferrokit_gui::paint::painter::RenderHint;
use ferrokit_gui::tiny_skia::Color as SkiaColor;
use ferrokit_platform::WindowFlags;
use ferrokit_widgets::{EmptyWidget, Window};

use super::gauge;
use super::round_pen;
use super::sidebar::tokens;

/// Shared with the Python HUD (`python/ui/dock_preview.py`): keep the values identical.
/// 0-255, the focus colour of the sidebar theme behind the outline.
pub const DOCK_PREVIEW_FILL_ALPHA: u8 = 46;
pub const DOCK_PREVIEW_BORDER_ALPHA: u8 = 235;
pub const DOCK_PREVIEW_BORDER_PX: f32 = 2.0;
// corner radius = the sidebar's own `radius` token (6) * the sidebar scale, as the bar itself is rounded

fn with_alpha(color: SkiaColor, alpha: u8) -> SkiaColor {
    let mut color = color;
    color.set_alpha(alpha as f32 / 255.0);
    color
}

/// Python `preview_colors`: (fill, border) in the dark / light sidebar theme.
pub fn preview_colors(dark: bool) -> (SkiaColor, SkiaColor) {
    let base = gauge::skia(tokens(dark).focus);
    (
        with_alpha(base, DOCK_PREVIEW_FILL_ALPHA),
        with_alpha(base, DOCK_PREVIEW_BORDER_ALPHA),
    )
}

/// Python `preview_radius`.
pub fn preview_radius(dark: bool, scale: f64) -> f32 {
    (tokens(dark).radius as f64 * scale) as f32
}

/// The flags of the overlay: frameless tool window, per-pixel alpha, click-through and never
/// activated (`TOOLTIP`: `WS_EX_NOACTIVATE` + `SW_SHOWNOACTIVATE`); `TOOLTIP` also starts it
/// topmost, `show_at` follows `always_on_top` afterwards.
pub fn preview_flags() -> WindowFlags {
    WindowFlags::FRAMELESS
        | WindowFlags::TOOL
        | WindowFlags::LAYERED
        | WindowFlags::TOOLTIP
        | WindowFlags::CLICK_THROUGH
}

/// Python `DockPreview`: a frameless, click-through, non-activating top-level (it has no owner
/// window), that outlines the docked rectangle of the dragged HUD.
pub struct DockPreview {
    window: Window,
    /// `(dark, scale)` the paint handler reads.
    style: Rc<Cell<(bool, f64)>>,
    /// The stay-on-top state the window was last given (`TOOLTIP` starts it topmost).
    on_top: Option<bool>,
}

impl DockPreview {
    pub fn new() -> Result<Self, &'static str> {
        // No owner window: Windows keeps an owned window above its owner, and the outline must
        // sit behind the HUD.
        let window = Window::new("", Rect::new(0, 0, 1, 1), preview_flags())?;
        let style = Rc::new(Cell::new((true, 1.0)));
        {
            let style = Rc::clone(&style);
            if let Some(root) = window
                .root_widget()
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<EmptyWidget>()
            {
                root.set_paint_handler(move |p| {
                    let (dark, scale) = style.get();
                    let (fill, border) = preview_colors(dark);
                    let radius = preview_radius(dark, scale);
                    let g = p.device();
                    let (w, h) = (g.width(), g.height());
                    let inset = DOCK_PREVIEW_BORDER_PX / 2.0;
                    p.set_render_hint(RenderHint::Antialiasing, true);
                    p.set_brush(Brush::Color(fill));
                    p.set_pen(Some(round_pen(border, DOCK_PREVIEW_BORDER_PX)));
                    p.draw_rounded_rect(
                        RectF::new(inset, inset, w - 2.0 * inset, h - 2.0 * inset),
                        radius,
                        radius,
                    );
                });
            }
        }
        Ok(Self {
            window,
            style,
            on_top: None,
        })
    }

    /// Outlines `rect` (global coordinates); the HUD itself is not moved and does not lose the focus.
    pub fn show_at(&mut self, rect: Rect, dark: bool, scale: f64, on_top: bool) {
        if self.on_top != Some(on_top) {
            self.window.set_stays_on_top(on_top);
            self.on_top = Some(on_top);
        }
        let restyled = self.style.replace((dark, scale)) != (dark, scale);
        if self.window.geometry() != rect {
            self.window.set_geometry(rect);
        } else if restyled {
            self.window.render_and_present();
        }
        if !self.window.is_visible() {
            self.window.show();
        }
    }

    /// Keeps the outline directly below the dragged HUD in the window stacking order (Python
    /// `stack_below`). The single place that orders the two windows; called after every `show_at`.
    /// `show_at` has already put the outline in the HUD's stacking band (`always_on_top`), which
    /// `Window::stack_under` needs; it answers `false` when the bands differ or the system refuses
    /// (the outline then keeps the place the system gave it).
    pub fn stack_below(&self, hud: &Window) -> bool {
        let stacked = self.window.stack_under(hud);
        if !stacked {
            log::debug!("[HUD] dock preview could not be stacked below the HUD");
        }
        stacked
    }

    pub fn hide(&mut self) {
        if self.window.is_visible() {
            self.window.hide();
        }
    }

    #[cfg(test)]
    pub fn is_visible(&self) -> bool {
        self.window.is_visible()
    }

    #[cfg(test)]
    pub fn geometry(&self) -> Rect {
        self.window.geometry()
    }

    #[cfg(test)]
    pub fn window(&self) -> &Window {
        &self.window
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_the_sidebar_focus_colour_at_the_shared_alphas() {
        for (dark, hex) in [(true, (0x38, 0xbd, 0xf8)), (false, (0x03, 0x69, 0xa1))] {
            let (fill, border) = preview_colors(dark);
            for (colour, alpha) in [
                (fill, DOCK_PREVIEW_FILL_ALPHA),
                (border, DOCK_PREVIEW_BORDER_ALPHA),
            ] {
                let c = colour.to_color_u8();
                assert_eq!((c.red(), c.green(), c.blue()), hex);
                assert_eq!(c.alpha(), alpha);
            }
        }
        assert_eq!(
            (
                DOCK_PREVIEW_FILL_ALPHA,
                DOCK_PREVIEW_BORDER_ALPHA,
                DOCK_PREVIEW_BORDER_PX
            ),
            (46, 235, 2.0)
        );
    }

    #[test]
    fn radius_is_the_sidebar_token_times_the_scale() {
        for dark in [true, false] {
            assert_eq!(preview_radius(dark, 1.0), 6.0);
            assert_eq!(preview_radius(dark, 1.5), 9.0);
        }
    }

    #[test]
    fn the_overlay_never_takes_input_or_activation() {
        let flags = preview_flags();
        for flag in [
            WindowFlags::FRAMELESS,
            WindowFlags::TOOL,
            WindowFlags::LAYERED,
            WindowFlags::TOOLTIP,
            WindowFlags::CLICK_THROUGH,
        ] {
            assert!(flags.contains(flag), "{flag:?}");
        }
    }
}
