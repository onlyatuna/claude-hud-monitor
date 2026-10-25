//! Main HUD window component matching Python ui/hud_window.py.
//!
//! Hosts the header bar, StackedWidget (Cards vs Table mode),
//! frameless window management, Acrylic backdrop blur, and click-through ghost mode.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use chrono::Local;
use ferrokit_core::event::{Event, EventKind};
use ferrokit_core::object::{register_owned_qobject, ObjectData, ObjectDataMut, ObjectId, OwnedObject};
use ferrokit_core::timer::Timer;
use ferrokit_core::QObject;
use ferrokit_gui::geometry::primitives::{Margins, Point, Rect, RectF};
use ferrokit_gui::geometry::Region;
use ferrokit_gui::paint::{Brush, RenderHint};
use ferrokit_platform::backdrop::BackdropType;
use ferrokit_platform::cursor::CursorShape;
use ferrokit_platform::WindowFlags;
use ferrokit_widgets::cursor::Cursor;
use ferrokit_widgets::{
    BoxLayout, Button, EmptyWidget, Label, Layout, StackedWidget, Widget, WidgetRef, Window,
};
use parking_lot::Mutex;

use super::dock_geometry::{
    dock_to_edge, orientation_for_edge, pick_work_area, snap_rect, snap_rect_corner, step_scale,
    Offset, Rect as GeoRect, DEFAULT_MARGIN, EDGES,
};
use super::provider_card::ProviderCardWidget;
use super::sidebar::{Orientation, Scheme, SidebarWidget, State, DEFAULT_ORDER};
use super::UiThread;
use super::styles::{get_theme, Theme};
use super::table_layout::{
    compact_min_size, table_state, COMPACT_MARGIN, DEF_COMPACT_H, DEF_COMPACT_W,
};
use super::round_mask::rounded_rects;
use super::usage_table::{TableViews, UsageTable, PROVIDER_ORDER};
use crate::config::{
    Config, ConfigManager, ResizeDebouncer, DEFAULT_HORIZONTAL_HEIGHT, DEFAULT_HORIZONTAL_WIDTH,
    DEFAULT_TABLE_HEIGHT, DEFAULT_TABLE_WIDTH, DEFAULT_VERTICAL_HEIGHT, DEFAULT_VERTICAL_WIDTH,
    MIN_HORIZONTAL_HEIGHT, MIN_HORIZONTAL_WIDTH, MIN_TABLE_HEIGHT, MIN_TABLE_WIDTH,
    MIN_VERTICAL_HEIGHT, MIN_VERTICAL_WIDTH,
};
use crate::providers::base::UsageMetrics;
use crate::providers::Provider;
use crate::refresh_controller::RefreshController;

/// The root layout's margins and spacing: `(12, 8, 12, 10)` and 6 for the full table.
const FULL_MARGINS: (i32, i32, i32, i32) = (12, 8, 12, 10);
const FULL_SPACING: i32 = 6;

/// Smallest table window: it holds the collapsed dial row, and the full table may be dragged down to it
/// (Python `compact_min_size(len(self.table.columns))`).
fn table_min_size() -> (i32, i32) {
    compact_min_size(PROVIDER_ORDER.len() as i32)
}

/// Python `_saved_table_size`: the remembered window size of the collapsed or the full table; an
/// undersized value falls back to the default.
fn saved_table_size(cfg: &Config, compact: bool) -> (i32, i32) {
    if compact {
        let (min_w, min_h) = table_min_size();
        let (w, h) = (cfg.table_compact_width as i32, cfg.table_compact_height as i32);
        (
            if w >= min_w { w } else { DEF_COMPACT_W },
            if h >= min_h { h } else { DEF_COMPACT_H },
        )
    } else {
        let (w, h) = (cfg.table_width, cfg.table_height);
        (
            if w >= MIN_TABLE_WIDTH { w } else { DEFAULT_TABLE_WIDTH } as i32,
            if h >= MIN_TABLE_HEIGHT { h } else { DEFAULT_TABLE_HEIGHT } as i32,
        )
    }
}

/// Python `_persist_geometry`'s table size: into the collapsed or the full keys, never below the minimum.
fn apply_table_size(cfg: &mut Config, width: i32, height: i32) {
    let (w, h) = (width as u32, height as u32);
    if cfg.table_compact {
        let (min_w, min_h) = table_min_size();
        cfg.table_compact_width = w.max(min_w as u32);
        cfg.table_compact_height = h.max(min_h as u32);
    } else {
        cfg.table_width = w.max(MIN_TABLE_WIDTH);
        cfg.table_height = h.max(MIN_TABLE_HEIGHT);
    }
}

/// Python `_persist_geometry`'s size part: the active mode's width and height, never below the
/// mode's minimum.
fn apply_size(cfg: &mut Config, width: i32, height: i32) {
    if cfg.ui_mode == "sidebar" {
        // Python `_persist_geometry`: the sidebar is placed by `_finish_sidebar_move`, not saved here
        return;
    }
    let (w, h) = (width as u32, height as u32);
    if cfg.ui_mode == "table" {
        apply_table_size(cfg, width, height);
    } else if cfg.layout_mode == "horizontal" {
        cfg.horizontal_width = w.max(MIN_HORIZONTAL_WIDTH);
        cfg.horizontal_height = h.max(MIN_HORIZONTAL_HEIGHT);
    } else {
        cfg.vertical_width = w.max(MIN_VERTICAL_WIDTH);
        cfg.vertical_height = h.max(MIN_VERTICAL_HEIGHT);
    }
}

/// What the header and the root layout look like around the table: the collapsed dial row hides the
/// header widgets and shrinks the margins (Python `_set_table_compact`, `_sync_header`,
/// `_sync_table_state`). Cloneable so the window's resize handler can run it without the HUD.
#[derive(Clone)]
struct TableChrome {
    config: Arc<Mutex<Config>>,
    views: TableViews,
    root: WidgetRef,
    /// Status dot, title, time.
    header: [WidgetRef; 3],
    ghost: WidgetRef,
}

impl TableChrome {
    /// Python `_sync_header`: the collapsed table shows no header widgets.
    fn sync_header(&self) {
        let (in_table, click_through) = {
            let cfg = self.config.lock();
            (cfg.ui_mode == "table", cfg.click_through)
        };
        let show = !(in_table && self.views.is_compact());
        for w in &self.header {
            w.borrow().set_visible(show);
        }
        self.ghost.borrow().set_visible(show && click_through);
    }

    fn set_margins(&self, compact: bool) {
        let (m, spacing) = if compact {
            ((COMPACT_MARGIN, COMPACT_MARGIN, COMPACT_MARGIN, COMPACT_MARGIN), 0)
        } else {
            (FULL_MARGINS, FULL_SPACING)
        };
        // The resize handler can run while the root is borrowed by a layout pass: skip then, the
        // layout pass that follows the resize picks the margins up on the next sync.
        let Ok(mut root) = self.root.try_borrow_mut() else {
            return;
        };
        if let Some(layout) = root.layout_mut() {
            layout.set_margins(Margins::new(m.0, m.1, m.2, m.3));
            layout.set_spacing(spacing);
        }
    }

    /// Python `_set_table_compact`: collapse the table to one dial per service (or restore it) and
    /// adapt the window chrome.
    fn set_compact(&self, compact: bool) {
        self.views.set_compact(compact);
        self.set_margins(compact);
        self.config.lock().table_compact = compact;
        self.sync_header();
    }

    /// Python `_sync_table_state`: collapse or expand for the window's new size.
    fn sync_state(&self, width: i32, height: i32) {
        if self.config.lock().ui_mode != "table" {
            return;
        }
        let was = self.views.is_compact();
        let now = table_state(width, height, was);
        if now != was {
            self.set_compact(now);
        }
    }

    /// Leaving the table style: the full margins and the header again.
    fn leave_table(&self) {
        self.set_margins(false);
        self.sync_header();
    }

    fn relayout(&self) {
        self.root.borrow().update_layout();
    }
}

/// Python `_persist_geometry`: position and size now, superseding any pending debounced save.
/// SAVE_LOCK -> Config lock; the Config lock is released before the file write.
fn persist_rect(debouncer: &ResizeDebouncer, geom: Rect) {
    debouncer.update_and_save_now(|cfg| {
        if cfg.ui_mode == "sidebar" {
            return;
        }
        cfg.window_x = Some(geom.x);
        cfg.window_y = Some(geom.y);
        apply_size(cfg, geom.width, geom.height);
    });
}

fn make_widget<W: Widget + 'static>(w: W) -> WidgetRef {
    Rc::new(RefCell::new(Box::new(w)))
}

fn set_label_text(w: &WidgetRef, text: impl Into<String>) {
    if let Some(lbl) = w.borrow_mut().as_any_mut().downcast_mut::<Label>() {
        lbl.set_text(text);
    }
}

/// Header status dot (Python `_on_busy_changed`): `#38bdf8` while fetching, `#f59e0b` while a
/// provider reports an error, `#10b981` otherwise.
fn status_dot_color(busy: bool, any_error: bool) -> ferrokit_gui::tiny_skia::Color {
    let (r, g, b) = if busy {
        (56, 189, 248)
    } else if any_error {
        (245, 158, 11)
    } else {
        (16, 185, 129)
    };
    ferrokit_gui::tiny_skia::Color::from_rgba8(r, g, b, 255)
}

/// The window style sheet of a UI mode (Python `_apply_theme`). Table mode's sheet names no
/// `font-family`; cards mode's does. Installing the cards sheet for both would force Segoe UI
/// onto the table's labels and shift every advance width.
fn style_sheet_for(ui_mode: &str, dark: bool) -> &'static str {
    if ui_mode == "table" {
        crate::ui::styles::get_hud_stylesheet(dark)
    } else {
        crate::ui::styles::get_cards_stylesheet(dark)
    }
}

/// Python `status_dot.setStyleSheet(f"color: {color}; font-size: 11px;")`.
fn set_status_dot_color(dot: &WidgetRef, color: ferrokit_gui::tiny_skia::Color) {
    let c = color.to_color_u8();
    let mut dot = dot.borrow_mut();
    if let Some(lbl) = dot.as_any_mut().downcast_mut::<Label>() {
        lbl.set_color(color);
    }
    dot.set_style_sheet(&format!(
        "color: rgba({}, {}, {}, {}); font-size: 11px;",
        c.red(),
        c.green(),
        c.blue(),
        c.alpha()
    ));
}

/// Background, border colour and corner radius of the window panel. In cards mode they come from
/// the application style sheet's `QWidget#CentralWidget` rule, as in the Python HUD. Table mode
/// uses the theme: the translucent panel only over an installed blur (Python
/// `get_hud_stylesheet(theme, vibrant)`, `styles.py:205`), the solid one otherwise.
fn panel_look(
    cards_mode: bool,
    vibrant: bool,
    theme: &Theme,
) -> (
    ferrokit_gui::tiny_skia::Color,
    ferrokit_gui::tiny_skia::Color,
    f32,
) {
    if !cards_mode {
        let bg = if vibrant {
            theme.panel_bg_vibrant
        } else {
            theme.panel_bg
        };
        return (bg, theme.panel_border, 12.0);
    }
    let themed = (theme.panel_bg, theme.panel_border, 10.0);
    let Some(sheet) = ferrokit_widgets::application::Application::style_sheet() else {
        return themed;
    };
    let ctx = ferrokit_widgets::style::stylesheet::WidgetStyleContext {
        type_name: "QWidget",
        object_name: "CentralWidget",
        pseudo_states: &[],
        sub_control: None,
        attributes: &[],
    };
    let style = sheet.resolve(&ctx);
    match (style.background_color, style.border_color) {
        (Some(bg), Some(border)) => (bg, border, style.border_radius.unwrap_or(9.0)),
        _ => themed,
    }
}

fn install_panel_painter(
    root: &WidgetRef,
    theme: &Theme,
    config: Arc<Mutex<Config>>,
    vibrant: Rc<Cell<bool>>,
) {
    let theme = theme.clone();
    if let Some(empty) = root.borrow_mut().as_any_mut().downcast_mut::<EmptyWidget>() {
        empty.set_paint_handler(move |painter| {
            let cards_mode = config.lock().ui_mode != "table";
            let (bg, border, radius) = panel_look(cards_mode, vibrant.get(), &theme);
            let w = painter.device().width();
            let h = painter.device().height();
            let rect_f = RectF::new(0.5, 0.5, w - 1.0, h - 1.0);
            painter.set_brush(Brush::Color(bg));
            painter.set_pen(crate::ui::round_pen(border, 1.0));
            // Python's QSS `border-radius` panel: Qt's stylesheet style draws the rounded border
            // with anti-aliasing (`qstylesheetstyle.cpp:1266-1285`), so AA is on for this shape.
            painter.set_render_hint(RenderHint::Antialiasing, true);
            painter.draw_rounded_rect(rect_f, radius, radius);
        });
    }
}

/// Python `gap > 15.0` in `_update_all_countdowns`: the tick gap that counts as a wake-up.
const WAKE_GAP_SECS: u64 = 15;

/// True when `now` is strictly more than 15 s after `previous` (wall clock, like `time.time()`).
/// No previous tick (the first one), exactly 15 s, or a backward clock jump give false.
pub fn tick_gap_exceeded(previous: Option<SystemTime>, now: SystemTime) -> bool {
    previous
        .and_then(|prev| now.duration_since(prev).ok())
        .is_some_and(|gap| gap > Duration::from_secs(WAKE_GAP_SECS))
}

/// Python `hideEvent`: `QTimer.singleShot(150, trim_memory)` (`hud_window.py:615-617`).
const HIDE_TRIM_DELAY_MS: u64 = 150;
/// Python `_on_busy_changed(False)`: `QTimer.singleShot(1000, trim_memory)` (`:455-459`).
const IDLE_TRIM_DELAY_MS: u64 = 1000;
/// Python `HUDWindow.__init__`: `QTimer.singleShot(2500, trim_memory)` (`:114`).
const INITIAL_TRIM_DELAY_MS: u64 = 2500;
fn schedule_trim(trim: &Arc<parking_lot::Mutex<TrimFn>>, delay_ms: u64) {
    let trim_holder = Arc::clone(trim);
    ferrokit_core::timer::Timer::single_shot(delay_ms, move || {
        let trim = Arc::clone(&trim_holder.lock());
        trim();
    });
}

/// The memory trim the HUD schedules (`crate::memory::trim_memory`; tests count calls instead).
type TrimFn = Arc<dyn Fn() + Send + Sync>;

/// What installs the window backdrop (`Window::set_backdrop`; tests substitute a failing one).
type BackdropFn = fn(&mut Window, BackdropType, bool) -> bool;

/// The backdrop half of Python `_apply_theme` (`hud_window.py:240-252`): table mode asks for the
/// Acrylic blur only while the window is visible and the `table_blur` switch is on
/// (`blur and self.isVisible() and vibrancy.apply(...)`) and is vibrant only if that succeeded;
/// without the switch, and in cards mode, the blur is cleared (`vibrancy.clear`).
fn install_backdrop(
    window: &mut Window,
    set: BackdropFn,
    ui_mode: &str,
    dark: bool,
    blur: bool,
) -> bool {
    if ui_mode == "table" && blur {
        window.is_visible() && set(window, BackdropType::Acrylic, dark)
    } else {
        set(window, BackdropType::None, dark);
        false
    }
}

/// Python `theme["radius"]` of the table panel, also the window mask's.
const TABLE_RADIUS: i32 = 12;
/// Python `_blur_resume_timer.setInterval(220)`.
const BLUR_RESUME_MS: u64 = 220;

/// Python `vibrancy.mask_needed`: only Windows' Acrylic needs the window clipped to rounded corners.
fn mask_needed(blur_applied: bool) -> bool {
    blur_applied && cfg!(windows)
}

/// Python `_refresh_mask`: clip the table window to rounded corners where native blur would otherwise
/// show square corners; `key` is `_mask_key`, the `(width, height, radius)` last applied. Works on the
/// window's root widget, as `Window::set_mask` does.
fn refresh_mask(
    root: &WidgetRef,
    in_table: bool,
    blur_active: bool,
    key: &Cell<Option<(i32, i32, i32)>>,
    size: (i32, i32),
) {
    if in_table && mask_needed(blur_active) {
        let wanted = (size.0, size.1, TABLE_RADIUS);
        if key.get() != Some(wanted) {
            let mut region = Region::default();
            for (x, y, w, h) in rounded_rects(wanted.0, wanted.1, wanted.2) {
                region = region.united_rect(&Rect::new(x, y, w, h));
            }
            root.borrow().set_mask(&region);
            key.set(Some(wanted));
        }
    } else if key.take().is_some() {
        root.borrow().clear_mask();
    }
}

thread_local! {
    /// The HUD as the window handlers reach it (they hold no `&mut Window`); set by `register_hud`.
    static HUD: RefCell<std::rc::Weak<RefCell<HUDWindow>>> = const { RefCell::new(std::rc::Weak::new()) };
}

/// Lets the window's press handler and the blur timer reach the HUD (the Qt `self`).
pub fn register_hud(hud: &Rc<RefCell<HUDWindow>>) {
    HUD.with(|h| *h.borrow_mut() = Rc::downgrade(hud));
}

/// Runs `f` on the registered HUD unless it is already borrowed (a handler re-entered from a HUD
/// call). Returns whether `f` ran; with no HUD registered there is nothing to retry, so that counts as run.
fn with_hud(f: impl FnOnce(&mut HUDWindow)) -> bool {
    let Some(hud) = HUD.with(|h| h.borrow().upgrade()) else {
        return true;
    };
    let ran = match hud.try_borrow_mut() {
        Ok(mut hud) => {
            f(&mut hud);
            true
        }
        Err(_) => false,
    };
    ran
}

/// The resume step of the blur pause: runs `resume_blur`, and while the HUD is busy tries again
/// `BLUR_RESUME_MS` later.
fn resume_blur_or_retry() {
    if blur_resume_needs_retry(with_hud(|hud| hud.resume_blur())) {
        ferrokit_core::timer::Timer::single_shot(BLUR_RESUME_MS, resume_blur_or_retry);
    }
}

/// The resume timer is single-shot: when the HUD was busy it must arm itself again, or the blur would
/// stay suspended for good.
fn blur_resume_needs_retry(hud_was_available: bool) -> bool {
    !hud_was_available
}

pub struct HUDWindow {
    pub config: Arc<Mutex<Config>>,
    pub refresh_ctrl: Arc<Mutex<RefreshController>>,
    pub providers: HashMap<String, Arc<dyn Provider + Send + Sync>>,
    pub window: Window,
    pub is_click_through: bool,
    pub is_dark: bool,
    pub theme: Theme,
    /// A provider fetch is running (drives the status dot colour).
    pub busy: bool,

    #[allow(dead_code)]
    pub status_dot: WidgetRef,
    pub title_label: WidgetRef,
    pub ghost_label: WidgetRef,
    pub layout_toggle_btn: WidgetRef,
    pub time_label: WidgetRef,

    pub stack: WidgetRef,
    pub cards_container: WidgetRef,
    pub cards: HashMap<String, ProviderCardWidget>,
    pub table: UsageTable,
    pub debouncer: Arc<ResizeDebouncer>,
    /// Wall-clock time of the previous clock tick (Python `_last_countdown_ts`); `None` before the first.
    last_tick: Option<SystemTime>,
    /// Diagnostic counter of long-gap refreshes; never read by any decision.
    wake_refresh_seq: u64,
    /// What the trim timers call; shared with the window event handler.
    trim: Arc<parking_lot::Mutex<TrimFn>>,
    /// The table panel sits on an installed blur (Python `vibrant`); read by the panel painter.
    vibrant: Rc<Cell<bool>>,
    set_backdrop: BackdropFn,
    /// What each provider last reported, before any row order is applied (Python `_raw_metrics`).
    raw_metrics: HashMap<String, UsageMetrics>,
    /// Python `self.sidebar`: the docked strip / panel (a page of `stack`).
    pub sidebar: SidebarWidget,
    /// Python `self.sidebar_cards`: the provider cards of the sidebar's panel and flyout.
    pub sidebar_cards: HashMap<String, Rc<RefCell<ProviderCardWidget>>>,
    /// The sidebar style's window state (`_docking`, `_resize`, `_snap_timer`, `_fade_timer`).
    sidebar_hud: Rc<SidebarHud>,
    /// Python `QApplication.instance().installEventFilter(self)`.
    hud_filter: Option<OwnedObject<HudFilter>>,
    /// The root layout is the sidebar's (no header, no margins) rather than the cards / table one.
    sidebar_layout: bool,
    chrome: TableChrome,
    /// Python `_mask_key`.
    mask_key: Rc<Cell<Option<(i32, i32, i32)>>>,
    /// Python `_blur_suspended`: the blur is paused while the window is moved or resized.
    blur_suspended: Rc<Cell<bool>>,
    /// Python `_blur_resume_timer`.
    blur_resume_timer: Rc<ferrokit_core::timer::Timer>,
}

/// The real providers. Test builds must not reach them: building a window launches a live fetch per
/// provider, so tests construct windows with `HUDWindow::with_providers` and stubs.
#[cfg(not(test))]
fn default_providers(
    config: &Arc<Mutex<Config>>,
) -> HashMap<String, Arc<dyn Provider + Send + Sync>> {
    use crate::providers::{agy::AgyProvider, claude::ClaudeProvider, codex::CodexProvider};
    HashMap::from([
        (
            "claude".to_string(),
            Arc::new(ClaudeProvider::with_config(Some(Arc::clone(config))))
                as Arc<dyn Provider + Send + Sync>,
        ),
        (
            "agy".to_string(),
            Arc::new(AgyProvider::new()) as Arc<dyn Provider + Send + Sync>,
        ),
        (
            "codex".to_string(),
            Arc::new(CodexProvider::new()) as Arc<dyn Provider + Send + Sync>,
        ),
    ])
}

#[cfg(test)]
fn default_providers(
    _config: &Arc<Mutex<Config>>,
) -> HashMap<String, Arc<dyn Provider + Send + Sync>> {
    panic!("HUDWindow::new would launch live provider fetches; tests must use HUDWindow::with_providers with providers::stub")
}

impl HUDWindow {
    pub fn new(
        config: Arc<Mutex<Config>>,
        refresh_ctrl: Arc<Mutex<RefreshController>>,
    ) -> Result<Self, &'static str> {
        let providers = default_providers(&config);
        Self::with_providers(config, refresh_ctrl, providers)
    }

    /// Same as `new`, with the providers supplied by the caller (one background fetch is launched per entry).
    pub fn with_providers(
        config: Arc<Mutex<Config>>,
        refresh_ctrl: Arc<Mutex<RefreshController>>,
        providers: HashMap<String, Arc<dyn Provider + Send + Sync>>,
    ) -> Result<Self, &'static str> {
        let (init_x, init_y, init_w, init_h, min_w, min_h, opacity, aot, ct, ui_mode, dark) = {
            let cfg = config.lock();
            let is_dark = crate::ui::resolve_is_dark(&cfg.appearance);
            let (w, h, min_w, min_h) = if cfg.ui_mode == "sidebar" {
                // docked by `dock_sidebar` right after construction; Python sets no minimum
                (72, 222, 0, 0)
            } else if cfg.ui_mode == "table" {
                let (w, h) = saved_table_size(&cfg, cfg.table_compact);
                let (min_w, min_h) = table_min_size();
                (w, h, min_w, min_h)
            } else if cfg.layout_mode == "horizontal" {
                (
                    cfg.horizontal_width.max(MIN_HORIZONTAL_WIDTH) as i32,
                    cfg.horizontal_height.max(MIN_HORIZONTAL_HEIGHT) as i32,
                    MIN_HORIZONTAL_WIDTH as i32,
                    MIN_HORIZONTAL_HEIGHT as i32,
                )
            } else {
                (
                    cfg.vertical_width.max(MIN_VERTICAL_WIDTH) as i32,
                    cfg.vertical_height.max(MIN_VERTICAL_HEIGHT) as i32,
                    MIN_VERTICAL_WIDTH as i32,
                    MIN_VERTICAL_HEIGHT as i32,
                )
            };
            // `_restore_or_default_position`: the saved position if it shows enough of some
            // screen, otherwise the default spot on the primary screen.
            let screens = super::placement::Screens::current();
            let (cl_x, cl_y) = super::placement::restore_or_default_position(
                cfg.window_x.zip(cfg.window_y),
                w,
                h,
                &screens.available,
                screens.primary,
            );
            (
                cl_x,
                cl_y,
                w,
                h,
                min_w,
                min_h,
                cfg.opacity,
                cfg.always_on_top,
                cfg.click_through,
                cfg.ui_mode.clone(),
                is_dark,
            )
        };

        let mut flags = WindowFlags::FRAMELESS
            | WindowFlags::CUSTOM_FRAMELESS
            | WindowFlags::LAYERED
            | WindowFlags::TOOL;
        if aot {
            flags |= WindowFlags::STAYS_ON_TOP;
        }
        if ct {
            flags |= WindowFlags::CLICK_THROUGH;
        }

        let debouncer = Arc::new(ResizeDebouncer::new(Arc::clone(&config)));
        debouncer.set_restoring(true);

        let geom = Rect::new(init_x, init_y, init_w, init_h);
        let mut window = Window::new("Claude HUD Monitor", geom, flags)?;
        // Python `_apply_ui_mode(..., initial=True)` sets the mode's minimum size on construction
        // (`hud_window.py:102, 296, 327, 347`; G12.8.q / RC-161).
        window.set_minimum_size(min_w, min_h);
        window.set_opacity(opacity);
        // The window is not shown yet, so table mode starts on the solid panel; `show` installs the
        // blur (Python: `showEvent` -> `_apply_theme`).
        let set_backdrop: BackdropFn = Window::set_backdrop;
        let table_blur = config.lock().table_blur;
        let vibrant = Rc::new(Cell::new(install_backdrop(
            &mut window,
            set_backdrop,
            &ui_mode,
            dark,
            table_blur,
        )));
        let sheet = style_sheet_for(&ui_mode, dark);
        window.set_style_sheet(sheet);
        ferrokit_widgets::application::Application::set_style_sheet(sheet);

        let theme = get_theme(dark);

        // Header bar widgets
        let status_dot = make_widget(Label::new("●"));
        set_status_dot_color(&status_dot, status_dot_color(false, false));

        let mut title = Label::new(if ui_mode == "table" {
            "AI AGENT HUD (TABLE)"
        } else {
            "AI AGENT HUD (3-IN-1)"
        });
        title.set_object_name("HeaderTitle");
        let title_label = make_widget(title);

        let ghost_label = make_widget(Label::new("👻"));
        ghost_label
            .borrow_mut()
            .set_tool_tip("滑鼠穿透中 (Alt+Shift+C 解除)");
        ghost_label.borrow_mut().set_visible(ct);

        let mut btn = Button::new("⇄");
        btn.set_object_name("LayoutToggleBtn");
        let layout_toggle_btn = make_widget(btn);
        layout_toggle_btn
            .borrow_mut()
            .set_tool_tip("切換 橫向並排 / 直式堆疊 佈局");
        layout_toggle_btn.borrow().set_visible(ui_mode != "table");

        let mut time_lbl = Label::new("--:--:--");
        time_lbl.set_object_name("HeaderStatus");
        let time_label = make_widget(time_lbl);
        // Header layout: `HeaderWidgets` builds it again whenever the root layout is rebuilt
        // (Python `_clear_layout(inner_layout)` + `_apply_ui_mode`).
        let header = HeaderWidgets {
            status_dot: status_dot.clone(),
            title_label: title_label.clone(),
            ghost_label: ghost_label.clone(),
            layout_toggle_btn: layout_toggle_btn.clone(),
            time_label: time_label.clone(),
        };

        // Page 0: Cards mode
        // Python sets no size policy and no stretch on the cards container (`hud_window.py:141-145`).
        let cards_container = make_widget(EmptyWidget::new());
        let mut cards = HashMap::new();
        cards.insert("claude".to_string(), ProviderCardWidget::new("claude"));
        cards.insert("agy".to_string(), ProviderCardWidget::new("agy"));
        cards.insert("codex".to_string(), ProviderCardWidget::new("codex"));

        let card_layout_mode = { config.lock().layout_mode.clone() };
        Self::apply_cards_layout_inner(&cards_container, &cards, &card_layout_mode);

        // Page 1: Table mode
        let scheme = { config.lock().color_scheme.clone() };
        let table = UsageTable::new(theme.clone(), &scheme);

        // Stacked container
        let stack: WidgetRef = make_widget(StackedWidget::new());
        stack
            .borrow_mut()
            .set_size_policy(Self::stack_policy(ui_mode == "table"));
        StackedWidget::add_widget(&stack, cards_container.clone());
        StackedWidget::add_widget(&stack, table.widget());

        // Page 2: the sidebar (Python `self.sidebar`, built hidden in `__init__`), its cards and the
        // tool window of its flyout (`_Flyout(owner)`, a child top-level of the HUD window).
        let sidebar_cards: Vec<(String, Rc<RefCell<ProviderCardWidget>>)> = DEFAULT_ORDER
            .iter()
            .map(|id| (id.to_string(), Rc::new(RefCell::new(ProviderCardWidget::new(id)))))
            .collect();
        let flyout = Window::with_parent(
            &window,
            "",
            Rect::new(0, 0, 260, 100),
            WindowFlags::FRAMELESS
                | WindowFlags::TOOL
                | WindowFlags::STAYS_ON_TOP
                | WindowFlags::LAYERED
                | WindowFlags::TOOLTIP,
        )?;
        let sidebar = SidebarWidget::new(sidebar_cards.clone(), flyout);
        StackedWidget::add_widget(&stack, sidebar.widget());
        let active_index = match ui_mode.as_str() {
            "table" => 1,
            "sidebar" => SIDEBAR_PAGE,
            _ => 0,
        };
        StackedWidget::set_current_index(&stack, active_index);

        // Root layout
        let compact_start = ui_mode == "table" && config.lock().table_compact;
        let mut root_layout = Self::build_root_layout(ui_mode == "sidebar", &header, &stack);
        if compact_start {
            let m = COMPACT_MARGIN;
            root_layout.set_margins(Margins::new(m, m, m, m));
            root_layout.set_spacing(0);
        }
        let root = window.root_widget();
        install_panel_painter(&root, &theme, Arc::clone(&config), Rc::clone(&vibrant));
        root.borrow_mut().set_layout(Box::new(root_layout));
        let chrome = TableChrome {
            config: Arc::clone(&config),
            views: table.views(),
            root: Rc::clone(&root),
            header: [status_dot.clone(), title_label.clone(), time_label.clone()],
            ghost: ghost_label.clone(),
        };
        if ui_mode == "table" {
            table.set_compact(compact_start);
        }
        chrome.sync_header();
        // Connect frameless window dragging and edge resizing
        let hwnd = window.native_handle();
        let cfg_move = Arc::clone(&config);
        window.set_mouse_move_handler(move |pos| {
            let (locked, sidebar) = {
                let cfg = cfg_move.lock();
                (cfg.locked, cfg.ui_mode == "sidebar")
            };
            if sidebar {
                // the sidebar has its own edge / corner resize (`HudFilter`), not the native one
                return;
            }
            let edges = ferrokit_platform::window::calc_frameless_edge(hwnd, pos, locked);
            let shape = if edges.is_empty() {
                ferrokit_platform::cursor::CursorShape::Arrow
            } else if edges == ferrokit_platform::platform_window::WindowEdges::LEFT
                || edges == ferrokit_platform::platform_window::WindowEdges::RIGHT
            {
                ferrokit_platform::cursor::CursorShape::SizeHor
            } else if edges == ferrokit_platform::platform_window::WindowEdges::TOP
                || edges == ferrokit_platform::platform_window::WindowEdges::BOTTOM
            {
                ferrokit_platform::cursor::CursorShape::SizeVer
            } else if edges == ferrokit_platform::platform_window::WindowEdges::TOP_LEFT
                || edges == ferrokit_platform::platform_window::WindowEdges::BOTTOM_RIGHT
            {
                ferrokit_platform::cursor::CursorShape::SizeFDiag
            } else {
                ferrokit_platform::cursor::CursorShape::SizeBDiag
            };
            #[cfg(windows)]
            ferrokit_platform::cursor::win32_cursor::Win32Cursor::set_shape(shape);
            #[cfg(not(windows))]
            let _ = shape;
        });

        let mask_key: Rc<Cell<Option<(i32, i32, i32)>>> = Rc::new(Cell::new(None));
        let blur_suspended = Rc::new(Cell::new(false));
        let blur_resume_timer = Rc::new(ferrokit_core::timer::Timer::new());
        blur_resume_timer.set_single_shot(true);
        blur_resume_timer.set_interval(BLUR_RESUME_MS);
        blur_resume_timer
            .timeout
            .connect(move |()| resume_blur_or_retry());

        let cfg_press = Arc::clone(&config);
        window.set_mouse_press_handler(move |pos, button| {
            if button == ferrokit_platform::MouseButton::Left {
                let (locked, sidebar) = {
                    let cfg = cfg_press.lock();
                    (cfg.locked, cfg.ui_mode == "sidebar")
                };
                // Python `_calc_edge` is `None` in the sidebar style: a press only moves the window
                let edges = if sidebar {
                    ferrokit_platform::platform_window::WindowEdges::empty()
                } else {
                    ferrokit_platform::window::calc_frameless_edge(hwnd, pos, locked)
                };
                if !edges.is_empty() {
                    with_hud(|hud| hud.suspend_blur());
                    ferrokit_platform::window::post_system_resize(hwnd, edges);
                    return true;
                } else if !locked {
                    with_hud(|hud| hud.suspend_blur());
                    ferrokit_platform::window::post_system_move(hwnd);
                    return true;
                }
            }
            false
        });
        let cfg_resize = Arc::clone(&config);
        let debouncer_resize = Arc::clone(&debouncer);
        let chrome_resize = chrome.clone();
        let root_resize = Rc::clone(&root);
        let vibrant_resize = Rc::clone(&vibrant);
        let mask_key_resize = Rc::clone(&mask_key);
        let suspended_resize = Rc::clone(&blur_suspended);
        let timer_resize = Rc::clone(&blur_resume_timer);
        // The window's current geometry as the events report it: the window event handler below
        // cannot reach `window.geometry()`, and Python reads `self.pos()`/`self.size()` instead.
        let geom_now = Rc::new(Cell::new(window.geometry()));
        // Python `_snap_timer` (single shot, 180 ms) and `_fade_timer` (single shot).
        let snap_timer = Timer::new();
        snap_timer.set_single_shot(true);
        snap_timer.set_interval(SIDEBAR_SNAP_DELAY_MS);
        snap_timer.timeout.connect(|()| {
            with_hud_retry(|hud| hud.finish_sidebar_move());
        });
        let fade_timer = Timer::new();
        fade_timer.set_single_shot(true);
        fade_timer.timeout.connect(|()| {
            with_hud_retry(|hud| hud.apply_opacity());
        });
        let sidebar_hud = Rc::new(SidebarHud {
            docking: Cell::new(false),
            resize: RefCell::new(None),
            grip_cursor: Cell::new(false),
            snap_timer,
            fade_timer,
            geom: Rc::clone(&geom_now),
            root: root.clone(),
            root_id: root.borrow().id(),
        });
        let geom_resize = Rc::clone(&geom_now);
        window.set_resize_handler(move |size| {
            let mut g = geom_resize.get();
            g.width = size.width;
            g.height = size.height;
            geom_resize.set(g);
            // Python `resizeEvent`: `_sync_table_state`, `_refresh_mask`, the blur pause timer, then the save. The handler runs before the layout pass, so the view the table collapses to is
            // the one that gets laid out.
            if !debouncer_resize.is_restoring() {
                chrome_resize.sync_state(size.width, size.height);
            }
            let in_table = cfg_resize.lock().ui_mode == "table";
            refresh_mask(
                &root_resize,
                in_table,
                vibrant_resize.get(),
                &mask_key_resize,
                (size.width, size.height),
            );
            if suspended_resize.get() {
                timer_resize.start();
            }
            if debouncer_resize.is_restoring() {
                return;
            }
            apply_size(&mut cfg_resize.lock(), size.width, size.height);
            debouncer_resize.request_save();
        });
        // Python `moveEvent` / `mouseReleaseEvent` / `closeEvent` / `hideEvent`
        // (`hud_window.py:595-617`). `hide()` and the close request both reach `hideEvent`.
        let trim: Arc<parking_lot::Mutex<TrimFn>> = Arc::new(parking_lot::Mutex::new(Arc::new(
            crate::memory::trim_memory,
        )));
        let trim_events = Arc::clone(&trim);
        let suspended_events = Rc::clone(&blur_suspended);
        let timer_events = Rc::clone(&blur_resume_timer);
        let cfg_events = Arc::clone(&config);
        let debouncer_events = Arc::clone(&debouncer);
        let hud_state_events = Rc::clone(&sidebar_hud);
        window.set_window_event_handler(move |event| match event.kind {
            EventKind::Move { x, y, .. } => {
                let mut g = geom_now.get();
                g.x = x;
                g.y = y;
                geom_now.set(g);
                // Python `moveEvent`: a paused blur waits for the movement to stop.
                if suspended_events.get() {
                    timer_events.start();
                }
                if debouncer_events.is_restoring() {
                    return;
                }
                if cfg_events.lock().ui_mode == "sidebar" {
                    // Python `moveEvent`: a move that is not the docking itself settles in
                    // `_finish_sidebar_move`; the position is saved there, not here
                    if !hud_state_events.docking.get() {
                        hud_state_events.snap_timer.start();
                    }
                    return;
                }
                {
                    let mut cfg = cfg_events.lock();
                    cfg.window_x = Some(x);
                    cfg.window_y = Some(y);
                }
                debouncer_events.request_save();
            }
            EventKind::MouseButtonRelease { .. } | EventKind::Close => {
                persist_rect(&debouncer_events, geom_now.get());
            }
            EventKind::Hide => schedule_trim(&trim_events, HIDE_TRIM_DELAY_MS),
            _ => {}
        });
        {
            let mut ctrl = refresh_ctrl.lock();
            for (id, provider) in &providers {
                ctrl.states.insert(id.clone(), Default::default());
                ctrl.launch(id, Arc::clone(provider));
            }
        }
        // Release initialization cold pages after UI and DirectWrite/fonts stabilize
        // (mirrors Python HUDWindow.__init__ QTimer.singleShot(2500, trim_memory), hud_window.py:114; G12.5.h).
        schedule_trim(&trim, INITIAL_TRIM_DELAY_MS);
        let mut hud = Self {
            config: Arc::clone(&config),
            refresh_ctrl,
            providers,
            last_tick: None,
            wake_refresh_seq: 0,
            window,
            is_click_through: ct,
            is_dark: dark,
            theme,
            status_dot,
            title_label,
            ghost_label,
            layout_toggle_btn,
            time_label,
            stack,
            cards_container,
            cards,
            table,
            debouncer: Arc::clone(&debouncer),
            busy: false,
            trim,
            vibrant,
            set_backdrop,
            raw_metrics: HashMap::new(),
            sidebar,
            sidebar_cards: sidebar_cards.into_iter().collect(),
            sidebar_hud: Rc::clone(&sidebar_hud),
            hud_filter: None,
            sidebar_layout: ui_mode == "sidebar",
            chrome,
            mask_key,
            blur_suspended,
            blur_resume_timer,
        };
        hud.connect_sidebar_signals();
        if ui_mode == "sidebar" {
            hud.apply_sidebar_mode();
            hud.window.set_opacity(opacity);
        }
        debouncer.set_restoring(false);

        Ok(hud)
    }

    /// Python puts the table or the cards container straight into the root layout, so the layout
    /// item behaves like that page: the cards container is `Preferred` (extra height goes to the
    /// header, `hud_window.py:141-145,308`), while the table's grid makes its item expand and take
    /// all the extra height (measured: 462 px of a 500 px window). The stack stands in for both.
    fn stack_policy(table: bool) -> ferrokit_widgets::QSizePolicy {
        let p = if table {
            ferrokit_widgets::Policy::Expanding
        } else {
            ferrokit_widgets::Policy::Preferred
        };
        ferrokit_widgets::QSizePolicy::new(p, p)
    }

    fn apply_cards_layout_inner(
        container: &WidgetRef,
        cards: &HashMap<String, ProviderCardWidget>,
        layout_mode: &str,
    ) {
        let is_horizontal = layout_mode == "horizontal";
        let mut layout = if is_horizontal {
            // `body_layout.setSpacing(8)` (`hud_window.py:337`); nested body, margin 0.
            let mut body = BoxLayout::horizontal();
            body.set_margins(Margins::new(0, 0, 0, 0));
            body.set_spacing(8);
            body
        } else {
            // `cards_layout` margin 0, spacing 6 (`hud_window.py:169-170`).
            let mut column = BoxLayout::vertical();
            column.set_margins(Margins::new(0, 0, 0, 0));
            column.set_spacing(6);
            column
        };

        // Card order of the Python HUD (`provider_ids` in `_apply_cards_layout`); the table
        // mode has its own order (`usage_table::PROVIDER_ORDER`).
        const CARDS_ORDER: [&str; 3] = ["claude", "agy", "codex"];
        for (idx, pid) in CARDS_ORDER.iter().enumerate() {
            if let Some(card) = cards.get(*pid) {
                if idx > 0 {
                    if is_horizontal {
                        let mut div = ferrokit_widgets::frame::Frame::new();
                        div.set_object_name("Divider");
                        div.set_frame_shape(ferrokit_widgets::frame::FrameShape::VLine);
                        layout.add_widget(make_widget(div));
                    } else {
                        let mut h_div = ferrokit_widgets::frame::Frame::new();
                        h_div.set_object_name("HorizontalDivider");
                        h_div.set_frame_shape(ferrokit_widgets::frame::FrameShape::HLine);
                        layout.add_widget(make_widget(h_div));
                    }
                }
                // `body_layout.addWidget(card, 1)` in the horizontal body only; the vertical column
                // adds the cards without a stretch (`hud_window.py:336-363`).
                if is_horizontal {
                    layout.add_widget_with_stretch(card.widget(), 1);
                } else {
                    layout.add_widget(card.widget());
                }
            }
        }
        container.borrow_mut().set_layout(Box::new(layout));
    }

    pub fn show(&mut self) {
        self.window.show();
        // Python `showEvent` -> `_apply_theme`: the blur can only be installed on a shown window.
        let was_vibrant = self.vibrant.get();
        let mode = self.config.lock().ui_mode.clone();
        self.refresh_backdrop(&mode);
        if self.vibrant.get() != was_vibrant {
            self.window.render_and_present();
        }
    }

    /// Installs or clears the blur for `ui_mode` and records whether the panel sits on it.
    fn refresh_backdrop(&mut self, ui_mode: &str) {
        let blur = self.config.lock().table_blur;
        let vibrant = install_backdrop(
            &mut self.window,
            self.set_backdrop,
            ui_mode,
            self.is_dark,
            blur,
        );
        self.vibrant.set(vibrant);
        // Python `_apply_theme`: `_blur_active, _blur_suspended = vibrant, False`, then `_refresh_mask()`.
        self.blur_suspended.set(false);
        self.refresh_mask(ui_mode == "table");
    }

    /// Python `_refresh_mask`.
    fn refresh_mask(&self, in_table: bool) {
        let g = self.window.geometry();
        refresh_mask(
            &self.window.root_widget(),
            in_table,
            self.vibrant.get(),
            &self.mask_key,
            (g.width, g.height),
        );
    }

    /// Python `_suspend_blur`: Acrylic makes dragging and resizing stutter, so drop the blur until the
    /// movement stops.
    fn suspend_blur(&mut self) {
        if !self.vibrant.get() || self.config.lock().ui_mode != "table" {
            return;
        }
        if !self.blur_suspended.get() {
            self.blur_suspended.set(self.window.suspend_backdrop());
        }
        if self.blur_suspended.get() {
            self.blur_resume_timer.start();
        }
    }

    /// Python `_resume_blur`.
    fn resume_blur(&mut self) {
        if self.blur_suspended.get() && self.vibrant.get() {
            self.window.resume_backdrop();
        }
        self.blur_suspended.set(false);
    }

    /// Python `toggle_table_blur`: the `table_blur` switch, applied at once.
    pub fn toggle_table_blur(&mut self) {
        crate::config::ConfigManager::update_and_save(&self.config, |cfg| {
            cfg.table_blur = !cfg.table_blur;
        });
        self.set_theme(self.is_dark);
        self.window.render_and_present();
    }

    pub fn hide(&mut self) {
        self.debouncer.flush();
        self.persist_geometry();
        self.window.hide();
        self.sidebar.close_flyout();
    }

    #[allow(dead_code)]
    pub fn close(&mut self) {
        self.debouncer.flush();
        self.persist_geometry();
        self.window.hide();
    }

    /// Replaces what the trim timers call, so tests can observe the scheduling.
    #[cfg(test)]
    fn set_trim(&self, trim: TrimFn) {
        *self.trim.lock() = trim;
    }

    /// Replaces what installs the backdrop, so tests can make it fail.
    #[cfg(test)]
    fn set_backdrop_fn(&mut self, set: BackdropFn) {
        self.set_backdrop = set;
    }

    /// A worker finished (the controller's notify callback): apply its results.
    pub fn drain_refresh_results(&mut self) {
        let drained = self.refresh_ctrl.lock().drain_results(&self.providers);
        self.apply_drained(&drained);
    }

    /// The 100 ms poll timer: apply finished results, launch due providers, update the status dot.
    pub fn poll_refresh(&mut self) {
        let drained = self.refresh_ctrl.lock().poll(&self.providers);
        self.apply_drained(&drained);
        let busy = self.refresh_ctrl.lock().is_busy();
        self.set_busy(busy);
    }

    /// Python `updated` per result, and `_on_busy_changed(False)`'s trim per completion that left
    /// every provider idle.
    fn apply_drained(&mut self, drained: &crate::refresh_controller::Drained) {
        for update in &drained.updates {
            self.on_data_fetched(update);
        }
        for _ in 0..drained.idle_completions {
            schedule_trim(&self.trim, IDLE_TRIM_DELAY_MS);
        }
    }

    pub fn persist_geometry(&self) {
        if self.debouncer.is_restoring() {
            return;
        }
        persist_rect(&self.debouncer, self.window.geometry());
    }

    pub fn toggle_visibility(&mut self) {
        if self.window.is_visible() {
            self.hide();
        } else {
            self.show();
        }
    }

    pub fn set_click_through(&mut self, enabled: bool) {
        self.is_click_through = enabled;
        self.window.set_click_through(enabled);
        self.config.lock().click_through = enabled;
        // Python `_sync_header`: the ghost shows only where the header does.
        self.chrome.sync_header();
        self.window.render_and_present();
    }

    pub fn toggle_click_through(&mut self) {
        self.set_click_through(!self.is_click_through);
    }

    pub fn toggle_always_on_top(&mut self) {
        let aot = {
            let mut cfg = self.config.lock();
            cfg.always_on_top = !cfg.always_on_top;
            cfg.always_on_top
        };
        self.window.set_stays_on_top(aot);
    }

    pub fn apply_ui_mode(&mut self, mode: &str) {
        let (old_mode, old_layout) = {
            let cfg = self.config.lock();
            (cfg.ui_mode.clone(), cfg.layout_mode.clone())
        };
        let geom = self.window.geometry();
        crate::config::ConfigManager::update_and_save(&self.config, |cfg| {
            if old_mode != "sidebar" {
                cfg.window_x = Some(geom.x);
                cfg.window_y = Some(geom.y);
                if old_mode == "table" {
                    apply_table_size(cfg, geom.width, geom.height);
                } else if old_layout == "horizontal" {
                    cfg.horizontal_width = (geom.width as u32).max(MIN_HORIZONTAL_WIDTH);
                    cfg.horizontal_height = (geom.height as u32).max(MIN_HORIZONTAL_HEIGHT);
                } else {
                    cfg.vertical_width = (geom.width as u32).max(MIN_VERTICAL_WIDTH);
                    cfg.vertical_height = (geom.height as u32).max(MIN_VERTICAL_HEIGHT);
                }
            }
            cfg.ui_mode = mode.to_string();
        });

        if mode == "cards" {
            let card_layout_mode = { self.config.lock().layout_mode.clone() };
            Self::apply_cards_layout_inner(&self.cards_container, &self.cards, &card_layout_mode);
        }

        self.apply_ui_mode_internal(mode);
    }

    fn apply_ui_mode_internal(&mut self, mode: &str) {
        // Python `_apply_theme`: the mode picks which style sheet the labels cascade from.
        // `HUDWindow.setStyleSheet` is the window's own sheet; `Window::set_style_sheet` is scoped
        // to it now (RC-10) and a stale window sheet would win over the application's.
        let sheet = style_sheet_for(mode, self.is_dark);
        self.window.set_style_sheet(sheet);
        ferrokit_widgets::application::Application::set_style_sheet(sheet);
        if mode == "sidebar" {
            // Python `_apply_ui_mode("sidebar")`: the flyout goes, the bar is sized by its content
            self.cancel_resize();
            self.sidebar.close_flyout();
            self.refresh_backdrop(mode);
            self.window.set_minimum_size(0, 0);
            self.apply_sidebar_mode();
            self.republish_all();
            self.apply_opacity();
            return;
        }
        if self.sidebar_layout {
            // leaving the sidebar: back to the header + page layout and a resizable window
            self.cancel_resize();
            self.sidebar.close_flyout();
            self.install_root_layout(false);
            self.window
                .set_maximum_size(QWIDGETSIZE_MAX, QWIDGETSIZE_MAX);
        }
        let (w, h) = if mode == "table" {
            StackedWidget::set_current_index(&self.stack, 1);
            self.stack
                .borrow_mut()
                .set_size_policy(Self::stack_policy(true));
            set_label_text(&self.title_label, "AI AGENT HUD (TABLE)");
            self.layout_toggle_btn.borrow().set_visible(false);
            // Python `_rebuild_table`: the collapsed or the full table, as last left.
            let (min_w, min_h) = table_min_size();
            self.window.set_minimum_size(min_w, min_h);
            let compact = self.config.lock().table_compact;
            self.chrome.set_compact(compact);
            let size = saved_table_size(&self.config.lock(), compact);
            size
        } else {
            StackedWidget::set_current_index(&self.stack, 0);
            self.stack
                .borrow_mut()
                .set_size_policy(Self::stack_policy(false));
            set_label_text(&self.title_label, "AI AGENT HUD (3-IN-1)");
            self.layout_toggle_btn.borrow().set_visible(true);
            self.chrome.leave_table();
            let cfg = self.config.lock();
            if cfg.layout_mode == "horizontal" {
                self.window
                    .set_minimum_size(MIN_HORIZONTAL_WIDTH as i32, MIN_HORIZONTAL_HEIGHT as i32);
                (
                    cfg.horizontal_width.max(MIN_HORIZONTAL_WIDTH) as i32,
                    cfg.horizontal_height.max(MIN_HORIZONTAL_HEIGHT) as i32,
                )
            } else {
                self.window
                    .set_minimum_size(MIN_VERTICAL_WIDTH as i32, MIN_VERTICAL_HEIGHT as i32);
                (
                    cfg.vertical_width.max(MIN_VERTICAL_WIDTH) as i32,
                    cfg.vertical_height.max(MIN_VERTICAL_HEIGHT) as i32,
                )
            }
        };
        self.refresh_backdrop(mode);

        // `_ensure_within_screen(w, h)` runs after the resize, so the screen is picked from the
        // centre of the window at its new size.
        let cur_geom = self.window.geometry();
        let resized = Rect::new(cur_geom.x, cur_geom.y, w, h);
        let screens = super::placement::Screens::current();
        let (nx, ny) = super::placement::ensure_within_screen(
            resized,
            &screens.available,
            screens.window_screen(resized),
            screens.primary,
        );
        self.window.set_geometry(Rect::new(nx, ny, w, h));
        self.cards_container.borrow().update_layout();
        self.chrome.relayout();
        self.republish_all();
        self.apply_opacity();
    }

    pub fn apply_cards_layout_mode(&mut self, mode: &str) {
        let (old_mode, old_layout) = {
            let cfg = self.config.lock();
            (cfg.ui_mode.clone(), cfg.layout_mode.clone())
        };
        let geom = self.window.geometry();
        crate::config::ConfigManager::update_and_save(&self.config, |cfg| {
            if old_mode != "sidebar" {
                cfg.window_x = Some(geom.x);
                cfg.window_y = Some(geom.y);
                if old_mode == "table" {
                    apply_table_size(cfg, geom.width, geom.height);
                } else if old_layout == "horizontal" {
                    cfg.horizontal_width = (geom.width as u32).max(MIN_HORIZONTAL_WIDTH);
                    cfg.horizontal_height = (geom.height as u32).max(MIN_HORIZONTAL_HEIGHT);
                } else {
                    cfg.vertical_width = (geom.width as u32).max(MIN_VERTICAL_WIDTH);
                    cfg.vertical_height = (geom.height as u32).max(MIN_VERTICAL_HEIGHT);
                }
            }
            cfg.layout_mode = mode.to_string();
            cfg.ui_mode = "cards".to_string();
        });
        Self::apply_cards_layout_inner(&self.cards_container, &self.cards, mode);
        self.apply_ui_mode_internal("cards");
    }

    #[allow(dead_code)]
    pub fn toggle_cards_layout(&mut self) {
        let new_mode = {
            let cfg = self.config.lock();
            if cfg.layout_mode == "horizontal" {
                "vertical".to_string()
            } else {
                "horizontal".to_string()
            }
        };

        self.apply_cards_layout_mode(&new_mode);
    }

    pub fn set_theme(&mut self, dark: bool) {
        self.is_dark = dark;
        self.theme = get_theme(dark);
        let ui_mode = { self.config.lock().ui_mode.clone() };
        self.refresh_backdrop(&ui_mode);
        let sheet = style_sheet_for(&ui_mode, dark);
        self.window.set_style_sheet(sheet);
        ferrokit_widgets::application::Application::set_style_sheet(sheet);

        for card in self.cards.values_mut() {
            card.set_appearance(dark);
        }
        if ui_mode == "sidebar" {
            // Python `_apply_theme`: status colours only; the duo scheme is for the table
            self.sidebar.set_appearance(dark, Scheme::Scale);
            for card in self.sidebar_cards.values() {
                card.borrow_mut().set_appearance(dark);
            }
        }

        let scheme = { self.config.lock().color_scheme.clone() };
        self.table.set_theme(self.theme.clone(), &scheme);
        let root = self.window.root_widget();
        install_panel_painter(
            &root,
            &self.theme,
            Arc::clone(&self.config),
            Rc::clone(&self.vibrant),
        );
    }

    /// Python `_on_data_fetched`: remember what the provider reported, show it arranged.
    pub fn on_data_fetched(&mut self, data: &UsageMetrics) {
        self.raw_metrics
            .insert(data.provider_id.clone(), data.clone());
        let arranged = self.arranged(data);
        self.publish(&arranged);
    }

    /// Python `_arranged`: agy's rows in the saved `agy_order`, another provider's in its saved
    /// `row_order` (the table's dials are fixed, so a custom order does not apply there).
    fn arranged(&self, metrics: &UsageMetrics) -> UsageMetrics {
        if metrics.provider_id == "agy" {
            return crate::agy_rows::arrange_agy(metrics, &self.agy_order());
        }
        let cfg = self.config.lock();
        match cfg.row_order.get(&metrics.provider_id) {
            Some(order) if !order.is_empty() && cfg.ui_mode != "table" => {
                crate::agy_rows::arrange_by_order(metrics, order)
            }
            _ => metrics.clone(),
        }
    }

    /// Python `_publish`.
    fn publish(&mut self, data: &UsageMetrics) {
        if let Some(card) = self.cards.get_mut(&data.provider_id) {
            card.update_metrics(data);
        }
        if let Some(card) = self.sidebar_cards.get(&data.provider_id) {
            card.borrow_mut().update_metrics(data);
        }
        self.sidebar.update_metrics(data);
        self.fit_sidebar();
        self.table.update_metrics(data);
        // Python `_on_data_fetched`: the header time is the time of the last fetch, and the
        // status dot is amber while any provider reports an error, green otherwise.
        set_label_text(
            &self.time_label,
            Local::now().format("%H:%M:%S").to_string(),
        );
        self.refresh_status_dot();
        self.cards_container.borrow().update_layout();
    }

    /// Python `_republish_all`.
    fn republish_all(&mut self) {
        let raws: Vec<UsageMetrics> = self.raw_metrics.values().cloned().collect();
        for raw in raws {
            let arranged = self.arranged(&raw);
            self.publish(&arranged);
        }
    }

    /// Python `provider_order`: tile / card / column order.
    pub fn provider_order(&self) -> Vec<String> {
        self.config.lock().provider_order_setting()
    }

    /// Python `agy_order`: the five agy rows, the first two being the main pair.
    pub fn agy_order(&self) -> Vec<String> {
        crate::agy_rows::normalize_order(&self.config.lock().agy_order)
    }

    /// Python `set_agy_order`.
    pub fn set_agy_order(&mut self, titles: &[String]) {
        let order = crate::agy_rows::normalize_order(titles);
        if order == self.agy_order() {
            return;
        }
        crate::config::ConfigManager::update_and_save(&self.config, |cfg| {
            cfg.agy_order = order.clone();
        });
        if let Some(raw) = self.raw_metrics.get("agy").cloned() {
            let arranged = self.arranged(&raw);
            self.publish(&arranged);
        }
    }

    /// Python `set_row_order(provider_id, titles)`: agy keeps its own order.
    pub fn set_row_order(&mut self, provider_id: &str, titles: &[String]) {
        if provider_id == "agy" {
            self.set_agy_order(titles);
            return;
        }
        let titles = titles.to_vec();
        if self.config.lock().row_order.get(provider_id) == Some(&titles) {
            return;
        }
        crate::config::ConfigManager::update_and_save(&self.config, |cfg| {
            cfg.row_order.insert(provider_id.to_owned(), titles.clone());
        });
        if let Some(raw) = self.raw_metrics.get(provider_id).cloned() {
            let arranged = self.arranged(&raw);
            self.publish(&arranged);
        }
    }

    /// Python `reset_agy_order`.
    pub fn reset_agy_order(&mut self) {
        let default: Vec<String> = crate::agy_rows::DEFAULT_ORDER
            .iter()
            .map(|s| s.to_string())
            .collect();
        self.set_agy_order(&default);
    }

    /// Python `_on_system_appearance_changed`: under `appearance = "auto"` the UI follows the
    /// operating system's colour scheme.
    pub fn follow_system_theme(&mut self) {
        let dark = {
            let cfg = self.config.lock();
            if cfg.appearance != "auto" {
                return;
            }
            crate::ui::resolve_is_dark(&cfg.appearance)
        };
        if dark != self.is_dark {
            self.set_theme(dark);
        }
    }

    /// Python `_on_busy_changed`: the status dot is blue while providers are being fetched.
    pub fn set_busy(&mut self, busy: bool) {
        if busy != self.busy {
            self.busy = busy;
            self.refresh_status_dot();
        }
    }

    fn refresh_status_dot(&self) {
        let any_error = self
            .cards
            .values()
            .any(|card| card.current_metrics.error.is_some());
        set_status_dot_color(&self.status_dot, status_dot_color(self.busy, any_error));
    }

    pub fn update_clock(&mut self) {
        for card in self.cards.values_mut() {
            card.update_countdown();
        }
        for card in self.sidebar_cards.values() {
            card.borrow_mut().update_countdown();
        }
        self.sidebar.update_countdown();
        self.table.update_countdowns();
    }

    /// One 1000 ms clock tick (Python `_update_all_countdowns`): refreshes the countdowns, and when the
    /// wall clock moved more than `WAKE_GAP_SECS` since the previous tick (system sleep, a stalled
    /// process, a clock jump) refreshes every provider once. `now` comes from the caller.
    pub fn on_clock_tick(&mut self, now: SystemTime) {
        let previous = self.last_tick;
        let gap = tick_gap_exceeded(previous, now);
        self.last_tick = Some(now);
        self.update_clock();
        if gap {
            // Diagnostic only: nothing reads the sequence to decide anything.
            self.wake_refresh_seq += 1;
            #[cfg(debug_assertions)]
            {
                let ms = |t: SystemTime| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_millis())
                };
                let gap_ms = previous
                    .and_then(|p| now.duration_since(p).ok())
                    .map_or(0, |d| d.as_millis());
                eprintln!(
                    "[rc18] wake refresh #{} previous_ms={} now_ms={} gap_ms={}",
                    self.wake_refresh_seq,
                    previous.map_or(0, ms),
                    ms(now),
                    gap_ms
                );
            }
            self.trigger_refresh();
        }
    }

    /// Number of long-gap refreshes `on_clock_tick` has triggered (tests).
    #[cfg(test)]
    pub fn wake_refresh_seq(&self) -> u64 {
        self.wake_refresh_seq
    }

    pub fn trigger_refresh(&mut self) {
        let mut ctrl = self.refresh_ctrl.lock();
        ctrl.refresh(&self.providers);
    }
    pub fn set_opacity(&mut self, opacity: f32) {
        let val = opacity.clamp(0.1, 1.0);
        crate::config::ConfigManager::update_and_save(&self.config, |cfg| {
            cfg.opacity = val;
        });
        if self.is_sidebar_mode() {
            // Python `_set_opacity`: the idle fade multiplies the chosen opacity
            self.apply_opacity();
        } else {
            self.window.set_opacity(val);
        }
    }

    pub fn set_refresh_interval(&mut self, seconds: u64) {
        crate::config::ConfigManager::update_and_save(&self.config, |cfg| {
            cfg.refresh_interval_sec = seconds;
        });
        self.refresh_ctrl.lock().set_interval(seconds);
    }

    pub fn set_claude_profile(&mut self, profile_id: &str) {
        crate::config::ConfigManager::update_and_save(&self.config, |cfg| {
            cfg.claude_profile = profile_id.to_string();
        });
        self.trigger_refresh();
    }

    pub fn reset_geometry(&mut self) {
        if self.is_sidebar_mode() {
            // Python `_reset_geometry` in the sidebar style
            ConfigManager::update_and_save(&self.config, |cfg| {
                cfg.sidebar_edge = "right".to_owned();
                cfg.sidebar_orientation = "vertical".to_owned();
                cfg.sidebar_offset = None;
                cfg.sidebar_x = None;
                cfg.sidebar_y = None;
                cfg.sidebar_scale = 1.0;
            });
            self.sidebar.set_scale(1.0);
            self.sidebar.set_orientation(Orientation::Vertical);
            self.sidebar.set_dock_edge(Some("right"));
            self.dock_sidebar();
            return;
        }
        // Python `_reset_geometry`: reset size expands a collapsed table first.
        let collapsed = {
            let cfg = self.config.lock();
            cfg.ui_mode == "table" && self.table.is_compact()
        };
        if collapsed {
            self.chrome.set_compact(false);
            self.chrome.relayout();
        }
        let (w, h) = {
            let cfg = self.config.lock();
            if cfg.ui_mode == "table" {
                (DEFAULT_TABLE_WIDTH as i32, DEFAULT_TABLE_HEIGHT as i32)
            } else if cfg.layout_mode == "horizontal" {
                (
                    DEFAULT_HORIZONTAL_WIDTH as i32,
                    DEFAULT_HORIZONTAL_HEIGHT as i32,
                )
            } else {
                (
                    DEFAULT_VERTICAL_WIDTH as i32,
                    DEFAULT_VERTICAL_HEIGHT as i32,
                )
            }
        };

        // `_reset_geometry` moves to the spot near the top right of the primary screen's work area
        // without clamping it.
        let screen = ferrokit_platform::platform().primary_screen();
        let (nx, ny) = super::placement::reset_position(w, screen.available_geometry());

        self.window.set_geometry(Rect::new(nx, ny, w, h));
        self.persist_geometry();
    }
}

// ---- Sidebar style (Python `hud_window.py`: the "Sidebar mode" and idle-fade parts) -------------

/// Python `SIDEBAR_SNAP_PX`, `RESIZE_GRIP_PX`, `IDLE_FADE_FACTOR`, `IDLE_FADE_DELAY_MS`.
pub const SIDEBAR_SNAP_PX: i32 = 24;
pub const RESIZE_GRIP_PX: i32 = 7;
/// Python `QWIDGETSIZE_MAX`.
const QWIDGETSIZE_MAX: i32 = 16_777_215;
/// The stack page of the sidebar (cards = 0, table = 1).
const SIDEBAR_PAGE: usize = 2;
/// Python `QTimer.start(1500)` in `_apply_sidebar_mode`: fully visible for a moment after the mode
/// is chosen.
const SIDEBAR_FADE_GRACE_MS: u64 = 1500;
/// Python `self._snap_timer.setInterval(180)`.
const SIDEBAR_SNAP_DELAY_MS: u64 = 180;

/// Python `self._resize`: the state of an edge / corner drag.
#[derive(Clone, Debug)]
struct ResizeState {
    start: Point,
    geom: Rect,
    scale: f64,
    zone: String,
}

/// What the sidebar style keeps on `HUDWindow` that its timers, the application event filter and
/// the window's event handler reach: one `Rc` shared with all of them.
pub struct SidebarHud {
    /// Python `_docking`: the window is being placed by `_dock_sidebar` / `_resize_to`.
    docking: Cell<bool>,
    resize: RefCell<Option<ResizeState>>,
    grip_cursor: Cell<bool>,
    /// Python `_snap_timer`.
    snap_timer: Timer,
    /// Python `_fade_timer`.
    fade_timer: Timer,
    /// The window geometry as the window's events report it.
    geom: Rc<Cell<Rect>>,
    root: WidgetRef,
    root_id: ObjectId,
}

/// Python `HUDWindow._resize_zone`: `"l"`, `"r"`, `"t"`, `"b"` or a corner (`"lt"`, `"rt"`, ...) when
/// `pos` (global) is within `RESIZE_GRIP_PX` of an edge of the window, `None` elsewhere. `QRect`
/// edges are inclusive (`right() == x + width - 1`).
pub fn resize_zone(g: Rect, pos: Point) -> Option<String> {
    let (left, top) = (g.x, g.y);
    let (right, bottom) = (g.x + g.width - 1, g.y + g.height - 1);
    if pos.x < left - 2 || pos.x > right + 2 || pos.y < top - 2 || pos.y > bottom + 2 {
        return None;
    }
    let m = RESIZE_GRIP_PX;
    let mut zone = String::new();
    if pos.x - left < m {
        zone.push('l');
    } else if right - pos.x < m {
        zone.push('r');
    }
    if pos.y - top < m {
        zone.push('t');
    } else if bottom - pos.y < m {
        zone.push('b');
    }
    (!zone.is_empty()).then_some(zone)
}

/// Python `_set_grip_cursor`'s shape for a zone.
pub fn zone_cursor(zone: &str) -> CursorShape {
    match zone {
        "l" | "r" => CursorShape::SizeHor,
        "t" | "b" => CursorShape::SizeVer,
        "lt" | "rb" => CursorShape::SizeFDiag,
        _ => CursorShape::SizeBDiag,
    }
}

/// Python `QRect.center()`.
fn qt_center(g: Rect) -> Point {
    Point::new((2 * g.x + g.width - 1) / 2, (2 * g.y + g.height - 1) / 2)
}

/// Python `_resize_to`'s scale: the start scale times the ratio the drag changed the grabbed
/// edge(s) by (the one furthest from 1), rounded to 2 decimals and held to 0.6 - 2.0.
pub fn resize_scale(zone: &str, g: Rect, start_scale: f64, dx: i32, dy: i32) -> f64 {
    let (gw, gh) = (g.width as f64, g.height as f64);
    let mut ratios = Vec::new();
    if zone.contains('r') {
        ratios.push((gw + dx as f64) / gw);
    }
    if zone.contains('l') {
        ratios.push((gw - dx as f64) / gw);
    }
    if zone.contains('b') {
        ratios.push((gh + dy as f64) / gh);
    }
    if zone.contains('t') {
        ratios.push((gh - dy as f64) / gh);
    }
    let ratio = ratios
        .into_iter()
        .fold(None::<f64>, |best, v| match best {
            Some(b) if (b - 1.0).abs() >= (v - 1.0).abs() => Some(b),
            _ => Some(v),
        })
        .unwrap_or(1.0);
    ((start_scale * ratio * 100.0).round() / 100.0).clamp(0.6, 2.0)
}

/// Python `_resize_to`'s new window origin: the fixed edge stays where it was.
pub fn resize_origin(zone: &str, g: Rect, w: i32, h: i32) -> (i32, i32) {
    let right = g.x + g.width - 1;
    let bottom = g.y + g.height - 1;
    let center = qt_center(g);
    let x = if zone.contains('r') {
        g.x
    } else if zone.contains('l') {
        right + 1 - w
    } else {
        center.x - w / 2
    };
    let y = if zone.contains('b') {
        g.y
    } else if zone.contains('t') {
        bottom + 1 - h
    } else {
        center.y - h / 2
    };
    (x, y)
}

/// Python `idle_fade_level` / the opacity the window gets: the chosen opacity, dimmed while the
/// sidebar is idle.
pub fn faded_opacity(base: f32, level: f64, active: bool) -> f32 {
    if active {
        base * level as f32
    } else {
        base
    }
}

/// How long and how often a task that found the HUD borrowed tries again (Python runs the slot
/// at once; here a nested call from inside a HUD method has to wait for that method to return).
const HUD_RETRY_MS: u64 = 20;
const HUD_RETRIES: u32 = 50;

/// Whether a task that did not run (the HUD was borrowed) is tried again.
fn should_retry(ran: bool, tries_left: u32) -> bool {
    !ran && tries_left > 0
}

/// Runs `f` on the HUD, trying again every `HUD_RETRY_MS` while it is borrowed. State the HUD
/// must leave (a resize grab, the sidebar's mode) is never skipped silently.
fn with_hud_retry(f: impl FnOnce(&mut HUDWindow) + 'static) {
    run_on_hud(Arc::new(UiThread::new(RefCell::new(Some(f)))), HUD_RETRIES);
}

fn run_on_hud<F: FnOnce(&mut HUDWindow) + 'static>(
    slot: Arc<UiThread<RefCell<Option<F>>>>,
    tries_left: u32,
) {
    let Some(cell) = slot.get() else { return };
    let ran = with_hud(|hud| {
        let task = cell.borrow_mut().take();
        if let Some(task) = task {
            task(hud);
        }
    });
    if should_retry(ran, tries_left) {
        ferrokit_core::timer::Timer::single_shot(HUD_RETRY_MS, move || {
            run_on_hud(slot, tries_left - 1)
        });
    }
}

/// `with_hud_retry` once the event that is being dispatched is done.
fn with_hud_later(f: impl FnOnce(&mut HUDWindow) + 'static) {
    ferrokit_widgets::WidgetCommandQueue::post_task(move || with_hud_retry(f));
}

/// Whether `id` is `root` or a widget below it. While a filter runs only the notified widget is
/// mutably borrowed, so a widget that cannot be read is that one and counts as found.
pub fn tree_contains(root: &WidgetRef, id: ObjectId) -> bool {
    let Ok(widget) = root.try_borrow() else {
        return true;
    };
    widget.id() == id || widget.children().iter().any(|child| tree_contains(child, id))
}

/// A resize drag whose button is no longer held lost its release event.
fn resize_lost_release(buttons_up: bool) -> bool {
    buttons_up
}

/// The events `HudFilter` looks at; everything else (paint, timers, layout) returns at once.
fn is_filtered_kind(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Enter { .. }
            | EventKind::Leave
            | EventKind::FocusOut { .. }
            | EventKind::MouseMove { .. }
            | EventKind::MouseButtonPress { .. }
            | EventKind::MouseButtonRelease { .. }
    )
}

impl SidebarHud {
    /// Python `_set_grip_cursor`.
    fn set_grip_cursor(&self, zone: Option<&str>) {
        use ferrokit_widgets::application::Application;
        match zone {
            None => {
                if self.grip_cursor.replace(false) {
                    Application::restore_override_cursor();
                }
            }
            Some(zone) => {
                let shape = zone_cursor(zone);
                if self.grip_cursor.get() {
                    Application::change_override_cursor(shape);
                } else {
                    Application::set_override_cursor(shape);
                    self.grip_cursor.set(true);
                }
            }
        }
    }
}

/// Python `HUDWindow.eventFilter` (installed application-wide) and `enterEvent` / `leaveEvent`.
struct HudFilter {
    data: ObjectData,
    state: UiThread<Weak<SidebarHud>>,
    config: Arc<Mutex<Config>>,
}

impl QObject for HudFilter {
    fn object_data(&self) -> &ObjectData {
        &self.data
    }
    fn object_data_mut(&mut self) -> ObjectDataMut<'_> {
        ObjectDataMut::new(&mut self.data)
    }

    fn event_filter(&mut self, watched: ObjectId, event: &mut Event) -> bool {
        if !is_filtered_kind(&event.kind) {
            return false;
        }
        let Some(hud) = self.state.get().and_then(|w| w.upgrade()) else {
            return false;
        };
        // `try_lock`: the config guard may be held by the code that dispatched this event
        let Some((sidebar_mode, locked)) = self
            .config
            .try_lock()
            .map(|cfg| (cfg.ui_mode == "sidebar", cfg.locked))
        else {
            return false;
        };
        if !sidebar_mode {
            return false;
        }
        // enterEvent / leaveEvent of the window: stop / start the idle-fade timer. Deferred: the
        // `WA_UnderMouse` bit that `_idle_fade_active` reads is toggled after the filters.
        if watched == hud.root_id {
            match &event.kind {
                EventKind::Enter { .. } => with_hud_later(|hud| {
                    hud.sidebar_hud.fade_timer.stop();
                    hud.apply_opacity();
                }),
                EventKind::Leave => with_hud_later(|hud| {
                    let delay = hud.idle_fade_delay() as u64;
                    hud.sidebar_hud.fade_timer.start_with_interval(delay);
                }),
                _ => {}
            }
        }
        if hud.resize.borrow().is_some() {
            let buttons_up = ferrokit_widgets::application::Application::mouse_buttons().is_empty();
            return match &event.kind {
                // a move with no button held: the release never arrived, so the drag is over
                EventKind::MouseMove { .. } if resize_lost_release(buttons_up) => {
                    with_hud_later(|hud| hud.end_resize());
                    true
                }
                EventKind::MouseMove { .. } => {
                    let pos = Cursor::pos();
                    with_hud_later(move |hud| hud.resize_to(pos));
                    true
                }
                EventKind::MouseButtonRelease { .. } => {
                    with_hud_later(|hud| hud.end_resize());
                    true
                }
                // the window lost the keyboard focus or the pointer mid-drag
                EventKind::FocusOut { .. } | EventKind::Leave if watched == hud.root_id => {
                    with_hud_later(|hud| hud.cancel_resize());
                    false
                }
                _ => false,
            };
        }
        if locked {
            return false;
        }
        match &event.kind {
            EventKind::Leave if watched == hud.root_id => {
                hud.set_grip_cursor(None);
                false
            }
            EventKind::MouseMove { .. } | EventKind::MouseButtonPress { .. } => {
                // Python: `isinstance(obj, QWidget) and (obj is self or self.isAncestorOf(obj))`
                let pos = Cursor::pos();
                let zone = resize_zone(hud.geom.get(), pos)
                    .filter(|_| tree_contains(&hud.root, watched));
                match &event.kind {
                    EventKind::MouseMove { .. } => {
                        if ferrokit_widgets::application::Application::mouse_buttons().is_empty() {
                            hud.set_grip_cursor(zone.as_deref());
                        }
                        false
                    }
                    EventKind::MouseButtonPress { button: 1, .. } if zone.is_some() => {
                        let zone = zone.unwrap_or_default();
                        let Some(scale) = self.config.try_lock().map(|c| c.sidebar_scale_setting()) else {
                            return false;
                        };
                        *hud.resize.borrow_mut() = Some(ResizeState {
                            start: pos,
                            geom: hud.geom.get(),
                            scale,
                            zone,
                        });
                        // `self._snap_timer.stop(); self.grabMouse()`
                        with_hud_later(|hud| hud.begin_resize());
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }
}

impl HUDWindow {
    // ================= Sidebar mode =================
    fn is_sidebar_mode(&self) -> bool {
        self.config.lock().ui_mode == "sidebar"
    }

    /// Python `_apply_sidebar_mode` (the inner-layout and window-size parts are in
    /// `install_root_layout` / `dock_sidebar`).
    fn apply_sidebar_mode(&mut self) {
        self.install_root_layout(true);
        StackedWidget::set_current_index(&self.stack, SIDEBAR_PAGE);
        self.sidebar_hud
            .fade_timer
            .start_with_interval(SIDEBAR_FADE_GRACE_MS); // stay fully visible for a moment
        let (scale, orientation, edge, state, dark) = {
            let cfg = self.config.lock();
            (
                cfg.sidebar_scale_setting(),
                self.sidebar_orientation_of(&cfg),
                self.sidebar_edge_of(&cfg),
                cfg.sidebar_state.clone(),
                self.is_dark,
            )
        };
        self.sidebar.set_scale(scale);
        self.sidebar
            .set_orientation(Orientation::parse(orientation).unwrap_or(Orientation::Vertical));
        self.sidebar.set_dock_edge(edge);
        self.sidebar
            .set_state(State::parse(&state).unwrap_or(State::Strip));
        // `_apply_theme`: status colours only; the duo scheme is for the table
        self.sidebar.set_appearance(dark, Scheme::Scale);
        for card in self.sidebar_cards.values() {
            card.borrow_mut().set_appearance(dark);
        }
        self.dock_sidebar();
    }

    fn sidebar_edge_of(&self, cfg: &Config) -> Option<&'static str> {
        EDGES.iter().copied().find(|e| *e == cfg.sidebar_edge)
    }

    fn sidebar_orientation_of(&self, cfg: &Config) -> &'static str {
        let current = if cfg.sidebar_orientation == "horizontal" { "horizontal" } else { "vertical" };
        orientation_for_edge(self.sidebar_edge_of(cfg), current)
    }

    /// Python `_sidebar_work_area`: the work area of the screen the window's centre is on.
    fn sidebar_work_area(&self) -> Option<GeoRect> {
        let screens = super::placement::Screens::current();
        let areas: Vec<GeoRect> = screens
            .available
            .iter()
            .map(|r| (r.x, r.y, r.width, r.height))
            .collect();
        let c = qt_center(self.window.geometry());
        pick_work_area((c.x, c.y), &areas)
    }

    /// Python `_dock_sidebar`: places the bar on its saved edge (or saved free position) of the
    /// current screen.
    fn dock_sidebar(&mut self) {
        let Some(work) = self.sidebar_work_area() else { return };
        let (w, h) = self.sidebar.preferred_size();
        let rect = {
            let cfg = self.config.lock();
            match self.sidebar_edge_of(&cfg) {
                Some(edge) => {
                    let offset = cfg.sidebar_offset.map(|o| match o {
                        crate::config::SidebarOffset::At(n) => Offset::Start(n),
                        crate::config::SidebarOffset::End => Offset::End,
                    });
                    dock_to_edge(work, edge, w, h, offset, DEFAULT_MARGIN)
                }
                None => {
                    let (x, y) = match (cfg.sidebar_x, cfg.sidebar_y) {
                        (Some(x), Some(y)) => (x, y),
                        _ => (work.0 + work.2, work.1 + work.3),
                    };
                    Some(snap_rect((x, y, w, h), work, -1_000_000).0)
                }
            }
        };
        let Some((x, y, w, h)) = rect else { return };
        self.sidebar_hud.docking.set(true);
        self.window.set_fixed_size(w, h);
        self.window.set_geometry(Rect::new(x, y, w, h));
        self.sidebar_hud.docking.set(false);
    }

    /// Python `_finish_sidebar_move`: after a drag, snap to the nearest screen edge (or stay free)
    /// and remember the position.
    pub fn finish_sidebar_move(&mut self) {
        if !self.is_sidebar_mode() || self.sidebar_hud.docking.get() {
            return;
        }
        if !ferrokit_widgets::application::Application::mouse_buttons().is_empty() {
            self.sidebar_hud.snap_timer.start();
            return;
        }
        let Some(work) = self.sidebar_work_area() else { return };
        let g = self.window.geometry();
        let current = {
            let cfg = self.config.lock();
            if cfg.sidebar_orientation == "horizontal" { "horizontal" } else { "vertical" }
        };
        // near a corner the bar keeps its orientation and sticks to the corner instead of turning sideways
        let (rect, edge, offset) = snap_rect_corner(
            (g.x, g.y, g.width, g.height),
            work,
            SIDEBAR_SNAP_PX,
            Some(current),
        );
        let orientation = orientation_for_edge(edge, current);
        ConfigManager::update_and_save(&self.config, |cfg| {
            cfg.sidebar_edge = edge.unwrap_or("none").to_owned();
            cfg.sidebar_orientation = orientation.to_owned();
            if edge.is_some() {
                cfg.sidebar_offset = offset.map(|o| match o {
                    Offset::Start(n) => crate::config::SidebarOffset::At(n),
                    Offset::End => crate::config::SidebarOffset::End,
                });
            } else {
                cfg.sidebar_x = Some(rect.0);
                cfg.sidebar_y = Some(rect.1);
            }
        });
        self.sidebar
            .set_orientation(Orientation::parse(orientation).unwrap_or(Orientation::Vertical));
        self.sidebar.set_dock_edge(edge);
        self.dock_sidebar();
    }

    /// Python `_start_sidebar_drag`.
    fn start_sidebar_drag(&self) {
        if self.config.lock().locked {
            return;
        }
        self.window.start_system_move();
    }

    /// Python `_begin_resize` after the press (the state is set by the filter).
    fn begin_resize(&mut self) {
        self.sidebar.close_flyout();
        self.sidebar_hud.snap_timer.stop();
        self.window.root_widget().borrow().grab_mouse();
    }

    /// Python `_resize_to`.
    fn resize_to(&mut self, pos: Point) {
        let Some(r) = self.sidebar_hud.resize.borrow().clone() else { return };
        let scale = resize_scale(&r.zone, r.geom, r.scale, pos.x - r.start.x, pos.y - r.start.y);
        if Some(scale) == Some(self.sidebar.scale()) {
            return;
        }
        self.sidebar.set_scale(scale);
        let (w, h) = self.sidebar.preferred_size();
        let (x, y) = resize_origin(&r.zone, r.geom, w, h);
        self.sidebar_hud.docking.set(true);
        self.window.set_fixed_size(w, h);
        self.window.set_geometry(Rect::new(x, y, w, h));
        self.sidebar_hud.docking.set(false);
    }

    /// A resize that ends without a release (mode switch, focus lost): the grab, the cursor and
    /// the state go, nothing is saved.
    fn cancel_resize(&mut self) {
        if self.sidebar_hud.resize.borrow_mut().take().is_some() {
            self.window.root_widget().borrow().release_mouse();
        }
        self.sidebar_hud.set_grip_cursor(None);
    }

    /// Python `_end_resize`.
    fn end_resize(&mut self) {
        self.window.root_widget().borrow().release_mouse();
        *self.sidebar_hud.resize.borrow_mut() = None;
        self.sidebar_hud.set_grip_cursor(None);
        let scale = (self.sidebar.scale() * 100.0).round() / 100.0;
        ConfigManager::update_and_save(&self.config, |cfg| cfg.sidebar_scale = scale);
        self.finish_sidebar_move();
    }

    /// Python `set_sidebar_scale`.
    pub fn set_sidebar_scale(&mut self, scale: f64) {
        let scale = if scale.is_finite() {
            ((scale * 100.0).round() / 100.0).clamp(0.6, 2.0)
        } else {
            1.0
        };
        ConfigManager::update_and_save(&self.config, |cfg| cfg.sidebar_scale = scale);
        if self.is_sidebar_mode() {
            let scale = self.config.lock().sidebar_scale_setting();
            self.sidebar.set_scale(scale);
            self.dock_sidebar();
        }
    }

    /// Python `_on_sidebar_scale_step`.
    fn on_sidebar_scale_step(&mut self, steps: i32) {
        let current = self.config.lock().sidebar_scale_setting();
        self.set_sidebar_scale(step_scale(current, steps));
    }

    /// Python `set_sidebar_state`.
    pub fn set_sidebar_state(&mut self, state: &str) {
        let Some(parsed) = State::parse(state) else { return };
        if !self.is_sidebar_mode() {
            return;
        }
        ConfigManager::update_and_save(&self.config, |cfg| cfg.sidebar_state = state.to_owned());
        self.sidebar.set_state(parsed);
        self.dock_sidebar();
    }

    /// Python `_fit_sidebar`: keeps the docked window exactly as big as the panel needs (agy shows
    /// five bars once its data arrives). New rows are measured before their style is applied, so
    /// the window is docked again once the event loop has polished them.
    fn fit_sidebar(&mut self) {
        if !self.is_sidebar_mode() || self.sidebar_hud.resize.borrow().is_some() {
            return;
        }
        if self.sidebar.state() == State::Panel {
            self.dock_sidebar();
            Timer::single_shot(0, || {
                with_hud_retry(|hud| hud.settle_sidebar());
            });
        }
    }

    /// Python `_settle_sidebar`.
    fn settle_sidebar(&mut self) {
        if !self.is_sidebar_mode()
            || self.sidebar_hud.resize.borrow().is_some()
            || self.sidebar.state() != State::Panel
        {
            return;
        }
        let g = self.window.geometry();
        if (g.width, g.height) != self.sidebar.preferred_size() {
            self.dock_sidebar();
        }
    }

    /// Python `set_provider_order`.
    pub fn set_provider_order(&mut self, order: &[String]) {
        if crate::ui::sidebar::valid_order(order).is_none() || order == self.provider_order() {
            return;
        }
        let order = order.to_vec();
        ConfigManager::update_and_save(&self.config, |cfg| cfg.provider_order = order.clone());
        self.sidebar.set_order(&order);
    }

    // ================= Idle fade =================
    /// Python `_idle_fade_active`.
    fn idle_fade_active(&self) -> bool {
        let cfg = self.config.lock();
        cfg.ui_mode == "sidebar"
            && cfg.sidebar_idle_fade
            && !cfg.click_through
            && !self.window.root_widget().borrow().under_mouse()
            && self.sidebar_hud.resize.borrow().is_none()
            && self.sidebar.active().is_none()
    }

    /// Python `_apply_opacity`: window opacity = the user's setting, dimmed while the sidebar is idle
    /// (no hover, no open detail page).
    pub fn apply_opacity(&mut self) {
        let base = self.config.lock().opacity;
        let level = self.idle_fade_level();
        let opacity = faded_opacity(base, level, self.idle_fade_active());
        self.window.set_opacity(opacity);
    }

    /// Python `idle_fade_level`: fraction of the chosen opacity the idle sidebar dims to (0.2 - 0.95).
    pub fn idle_fade_level(&self) -> f64 {
        self.config.lock().idle_fade_level_setting()
    }

    /// Python `idle_fade_delay`.
    pub fn idle_fade_delay(&self) -> i64 {
        self.config.lock().idle_fade_delay_setting()
    }

    /// Python `toggle_idle_fade`.
    pub fn toggle_idle_fade(&mut self) {
        ConfigManager::update_and_save(&self.config, |cfg| cfg.sidebar_idle_fade = !cfg.sidebar_idle_fade);
        self.apply_opacity();
    }

    /// Python `set_idle_fade_level(percent)`.
    pub fn set_idle_fade_level(&mut self, percent: i32) {
        let level = percent.clamp(20, 95) as f64 / 100.0;
        ConfigManager::update_and_save(&self.config, |cfg| cfg.sidebar_idle_level = level);
        self.apply_opacity();
    }

    /// Python `set_idle_fade_delay(milliseconds)`.
    pub fn set_idle_fade_delay(&mut self, milliseconds: i64) {
        let delay = milliseconds.clamp(200, 30000);
        ConfigManager::update_and_save(&self.config, |cfg| cfg.sidebar_idle_delay_ms = delay);
    }

    /// Python `__init__`: the sidebar's signals, the cards' `rowOrderChanged`, and the
    /// application-wide event filter. Every slot reaches the HUD through `MAIN_HUD` once the event
    /// that raised it is done.
    fn connect_sidebar_signals(&mut self) {
        let sidebar = self.sidebar.clone();
        sidebar.state_change_requested().connect(|state: &String| {
            let state = state.clone();
            with_hud_later(move |hud| hud.set_sidebar_state(&state));
        });
        sidebar.scale_step_requested().connect(|steps: &i32| {
            let steps = *steps;
            with_hud_later(move |hud| hud.on_sidebar_scale_step(steps));
        });
        sidebar.drag_requested().connect(|()| {
            with_hud_later(|hud| hud.start_sidebar_drag());
        });
        sidebar.order_changed().connect(|order: &Vec<String>| {
            let order = order.clone();
            with_hud_later(move |hud| hud.set_provider_order(&order));
        });
        for (pid, card) in &self.sidebar_cards {
            let pid = pid.clone();
            card.borrow().row_order_changed().connect(move |titles: &Vec<String>| {
                let (pid, titles) = (pid.clone(), titles.clone());
                with_hud_later(move |hud| hud.set_row_order(&pid, &titles));
            });
        }
        let order = self.provider_order();
        self.sidebar.set_order(&order);
        // QApplication.instance().installEventFilter(self)
        let filter = HudFilter {
            data: ObjectData::with_auto_id(),
            state: UiThread::new(Rc::downgrade(&self.sidebar_hud)),
            config: Arc::clone(&self.config),
        };
        // SAFETY: the owner stays in `self.hud_filter` on the UI thread and never reads the
        // object; its callback runs only from event dispatch.
        let owner = unsafe { register_owned_qobject(filter) };
        ferrokit_core::application::CoreApplication::install_event_filter(OwnedObject::object_id(&owner));
        self.hud_filter = Some(owner);
    }

    /// Python `_clear_layout(inner_layout)` + the header / margins of `_apply_ui_mode`: the root
    /// layout of the sidebar style (the stack only, no margins) or of cards / table (header + stack).
    fn build_root_layout(sidebar: bool, header: &HeaderWidgets, stack: &WidgetRef) -> BoxLayout {
        let mut root_layout = BoxLayout::vertical();
        if sidebar {
            root_layout.set_margins(Margins::new(0, 0, 0, 0));
            root_layout.set_spacing(0);
            root_layout.add_widget(stack.clone());
            return root_layout;
        }
        root_layout.set_margins(Margins::new(12, 8, 12, 10));
        root_layout.set_spacing(6);
        // Python `inner_layout.addLayout(header_layout)` (`hud_window.py:288`): the header is a nested
        // layout item, sized by its own children (`QBoxLayout::addLayout`).
        let mut header_layout = BoxLayout::horizontal();
        header_layout.set_spacing(6);
        header_layout.add_widget(header.status_dot.clone());
        header_layout.add_widget(header.title_label.clone());
        header_layout.add_widget(header.ghost_label.clone());
        header_layout.add_widget(header.layout_toggle_btn.clone());
        header_layout.add_stretch(1);
        header_layout.add_widget(header.time_label.clone());
        root_layout.add_layout(Box::new(header_layout), 0);
        root_layout.add_widget(stack.clone());
        root_layout
    }

    /// Rebuilds the root layout for the style that is about to show.
    fn install_root_layout(&mut self, sidebar: bool) {
        if self.sidebar_layout == sidebar {
            return;
        }
        self.sidebar_layout = sidebar;
        let header = HeaderWidgets {
            status_dot: self.status_dot.clone(),
            title_label: self.title_label.clone(),
            ghost_label: self.ghost_label.clone(),
            layout_toggle_btn: self.layout_toggle_btn.clone(),
            time_label: self.time_label.clone(),
        };
        let layout = Self::build_root_layout(sidebar, &header, &self.stack);
        self.window.root_widget().borrow_mut().set_layout(Box::new(layout));
    }
}

/// The widgets of the header bar (Python `status_dot`, `title_label`, `ghost_label`,
/// `layout_toggle_btn`, `time_label`).
struct HeaderWidgets {
    status_dot: WidgetRef,
    title_label: WidgetRef,
    ghost_label: WidgetRef,
    layout_toggle_btn: WidgetRef,
    time_label: WidgetRef,
}

/// Drop guarantees flush and worker thread join during normal object destruction and clean shutdown.
/// Note: Drop cannot run during abnormal termination (e.g. process::abort, SIGKILL, TerminateProcess,
/// unhandled access violations, OS crash, or power loss).
impl Drop for HUDWindow {
    fn drop(&mut self) {
        if let Some(filter) = self.hud_filter.take() {
            ferrokit_core::application::CoreApplication::remove_event_filter(OwnedObject::object_id(&filter));
        }
        self.sidebar.close_flyout();
        self.debouncer.shutdown();
    }
}

/// The pure size rules of the collapsible table (no window involved).
#[cfg(test)]
mod table_size_tests {
    use super::*;

    #[test]
    fn a_busy_hud_makes_the_resume_timer_arm_itself_again() {
        assert!(blur_resume_needs_retry(false));
        assert!(!blur_resume_needs_retry(true));
        // No HUD registered (or the borrow free) counts as run: `with_hud` returns true.
        assert!(with_hud(|_| unreachable!()));
    }

    fn table_cfg(compact: bool) -> Config {
        Config {
            ui_mode: "table".to_string(),
            table_compact: compact,
            ..Config::default()
        }
    }

    #[test]
    fn full_and_collapsed_sizes_are_stored_under_their_own_keys() {
        let mut cfg = table_cfg(false);
        apply_size(&mut cfg, 500, 400);
        assert_eq!((cfg.table_width, cfg.table_height), (500, 400));
        assert_eq!(
            (cfg.table_compact_width, cfg.table_compact_height),
            (240, 96)
        );

        cfg.table_compact = true;
        apply_size(&mut cfg, 300, 120);
        assert_eq!(
            (cfg.table_compact_width, cfg.table_compact_height),
            (300, 120)
        );
        assert_eq!((cfg.table_width, cfg.table_height), (500, 400), "survives");
    }

    #[test]
    fn stored_sizes_never_go_below_the_minimum_of_their_view() {
        let mut cfg = table_cfg(false);
        apply_size(&mut cfg, 10, 10);
        assert_eq!((cfg.table_width, cfg.table_height), (380, 280));
        cfg.table_compact = true;
        apply_size(&mut cfg, 10, 10);
        assert_eq!(
            (cfg.table_compact_width, cfg.table_compact_height),
            (174, 62)
        );
    }

    #[test]
    fn saved_sizes_fall_back_to_the_defaults_when_undersized() {
        let mut cfg = table_cfg(false);
        cfg.table_width = 100;
        cfg.table_height = 400;
        assert_eq!(saved_table_size(&cfg, false), (450, 400));
        cfg.table_width = 500;
        cfg.table_height = 200;
        assert_eq!(saved_table_size(&cfg, false), (500, 350));

        cfg.table_compact_width = 100;
        cfg.table_compact_height = 70;
        assert_eq!(saved_table_size(&cfg, true), (240, 70));
        cfg.table_compact_width = 200;
        cfg.table_compact_height = 30;
        assert_eq!(saved_table_size(&cfg, true), (200, 96));
    }

    #[test]
    fn the_table_window_may_shrink_to_the_collapsed_minimum() {
        assert_eq!(table_min_size(), (174, 62));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refresh_controller::RefreshController;

    /// G12.8.f / RC-23. PySide6 oracle (`hud_window.py:31-35`, run with a fresh config at DPR 1.25,
    /// `size()` / `minimumSize()` after `show()`): horizontal 690x145, minimum 540x125; vertical
    /// 280x410, minimum 250x320. The minimum is the explicit `setMinimumSize` and wins over the
    /// layout's own minimum (151 / 395), so the window may be smaller than its content needs.
    #[cfg(windows)]
    #[test]
    fn test_window_default_and_minimum_sizes_match_pyside6() {
        let _setup = crate::ui::test_support::CardsOracleSetup::new();
        for (mode, size, min) in [
            ("horizontal", (690, 145), (540, 125)),
            ("vertical", (280, 410), (250, 320)),
        ] {
            let mut cfg = Config::default();
            cfg.ui_mode = "cards".into();
            cfg.layout_mode = mode.into();
            cfg.window_x = Some(0);
            cfg.window_y = Some(0);
            let cfg = Arc::new(Mutex::new(cfg));
            let ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
            let mut hud =
                HUDWindow::with_providers(cfg, ctrl, crate::providers::stub::stub_providers())
                    .unwrap();
            let g = hud.window.geometry();
            assert_eq!((g.width, g.height), size, "{mode} default size (PySide6)");
            assert_eq!(
                hud.window.minimum_size(),
                min,
                "{mode} initial minimum size on construction (G12.8.q)"
            );
            // RC-161: The minimum size is now applied immediately on construction (G12.8.q).
            hud.apply_cards_layout_mode(mode);
            assert_eq!(hud.window.minimum_size(), min, "{mode} minimum (PySide6)");
            let g = hud.window.geometry();
            assert_eq!(
                (g.width, g.height),
                size,
                "{mode} size after the mode is applied"
            );
        }
    }

    /// RC-24 guard. The HUD keeps the table page in its `StackedWidget` while the cards page is
    /// shown, and `QStackedLayout::sizeHint`/`minimumSize` cover every page, so the stack's hint
    /// now includes the table. PySide6 (no stack, `py_geom.py vertical`, `VH=410`, DPR 1.25) at the
    /// default 280x410: header [12, 8, 256, 33], cards at y 47/169/291, each 256x109.
    #[cfg(windows)]
    #[test]
    fn test_default_vertical_window_layout_matches_pyside6_with_the_table_page_in_the_stack() {
        let _setup = crate::ui::test_support::CardsOracleSetup::new();
        let mut cfg = Config::default();
        cfg.ui_mode = "cards".into();
        cfg.layout_mode = "vertical".into();
        cfg.appearance = "dark".into();
        cfg.window_x = Some(0);
        cfg.window_y = Some(0);
        let cfg = Arc::new(Mutex::new(cfg));
        let ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
        let hud =
            HUDWindow::with_providers(cfg, ctrl, crate::providers::stub::stub_providers()).unwrap();
        hud.window.root_widget().borrow().update_layout();
        let header = hud.window.root_widget().borrow().children()[0]
            .borrow()
            .geometry();
        assert_eq!(
            (header.x, header.y, header.width, header.height),
            (12, 8, 256, 33),
            "header (PySide6 [12, 8, 256, 33])"
        );
        let cards: Vec<(i32, i32, i32, i32)> = ["claude", "agy", "codex"]
            .iter()
            .map(|id| {
                let g = hud.cards[*id].container.borrow().geometry();
                (g.x, g.y, g.width, g.height)
            })
            .collect();
        // Card geometry is relative to the cards container, so compare sizes and the 122 px pitch.
        assert!(
            cards.iter().all(|c| c.3 == 109 && c.2 == 256),
            "cards {cards:?} (PySide6 256x109)"
        );
        assert_eq!(
            (cards[1].1 - cards[0].1, cards[2].1 - cards[1].1),
            (122, 122)
        );
    }

    /// G12.8.b. PySide6 (`py_horizontal.json`, DPR 1.25): in a 666 px wide body the three cards are
    /// 211, 210 and 211 px wide, because the body `QHBoxLayout` has spacing 8 (`hud_window.py:337`).
    #[cfg(windows)]
    #[test]
    fn test_horizontal_cards_body_matches_pyside6_widths() {
        let _setup = crate::ui::test_support::CardsOracleSetup::new();
        let cards: HashMap<String, ProviderCardWidget> = ["claude", "agy", "codex"]
            .iter()
            .map(|id| (id.to_string(), ProviderCardWidget::new(id)))
            .collect();
        let container = make_widget(ferrokit_widgets::EmptyWidget::new());
        HUDWindow::apply_cards_layout_inner(&container, &cards, "horizontal");
        container
            .borrow_mut()
            .set_geometry(ferrokit_gui::geometry::primitives::Rect::new(
                12, 32, 666, 103,
            ));
        container.borrow().update_layout();

        let widths: Vec<i32> = ["claude", "agy", "codex"]
            .iter()
            .map(|id| cards[*id].widget().borrow().geometry().width)
            .collect();
        assert_eq!(widths, [211, 210, 211]);
    }

    /// RC-16, G12.8.c. PySide6 (`py_vertical.json`, DPR 1.25, 280x463): the card column keeps each
    /// card at its size hint (109) and the extra height goes to the header row, which becomes 86
    /// high with the title label filling it, because the header is a plain layout and the card
    /// container has no stretch and no `Expanding` policy.
    #[cfg(windows)]
    #[test]
    fn test_vertical_extra_height_goes_to_header_like_pyside6() {
        let _setup = crate::ui::test_support::CardsOracleSetup::new();
        let mut cfg = Config::default();
        cfg.ui_mode = "cards".into();
        cfg.layout_mode = "vertical".into();
        cfg.appearance = "dark".into();
        cfg.window_x = Some(0);
        cfg.window_y = Some(0);
        cfg.vertical_width = 280;
        cfg.vertical_height = 463;
        let cfg = Arc::new(Mutex::new(cfg));
        let ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
        let hud =
            HUDWindow::with_providers(cfg, ctrl, crate::providers::stub::stub_providers()).unwrap();
        hud.window.root_widget().borrow().update_layout();

        let root = hud.window.root_widget();
        let header = root.borrow().children()[0].clone();
        assert_eq!(
            header.borrow().geometry().height,
            86,
            "header row height (PySide6: 86)"
        );
        assert_eq!(
            hud.title_label.borrow().geometry().height,
            86,
            "title label height (PySide6: 86)"
        );
        // RC-19 / G12.8.g: `max-height: 18px` is the content box, so the button is 22 high (PySide6:
        // `layout_toggle_btn` [175, 40, 28, 22] in the window, i.e. y 32 inside the header at y 8).
        let toggle = hud.layout_toggle_btn.borrow().geometry();
        assert_eq!(
            (toggle.y, toggle.width, toggle.height),
            (32, 28, 22),
            "layout toggle button (PySide6: y 32, 28x22)"
        );
        let heights: Vec<i32> = ["claude", "agy", "codex"]
            .iter()
            .map(|id| hud.cards[*id].widget().borrow().geometry().height)
            .collect();
        assert_eq!(heights, [109, 109, 109], "card heights (PySide6: 109 each)");
        let ys: Vec<i32> = ["claude", "agy", "codex"]
            .iter()
            .map(|id| hud.cards[*id].widget().borrow().geometry().y)
            .collect();
        assert_eq!(
            ys,
            [0, 122, 244],
            "card y inside the container (PySide6: 100/222/344 in the window)"
        );
    }

    /// RC-16 side check. PySide6 (DPR 1.25, table mode 400x500): the title label stays 14 high and
    /// the table takes all the extra height (462), although the table's own policy is Preferred.
    #[cfg(windows)]
    #[test]
    fn test_table_mode_table_takes_extra_height_like_pyside6() {
        let _setup = crate::ui::test_support::CardsOracleSetup::new();
        let mut cfg = Config::default();
        cfg.ui_mode = "table".into();
        cfg.appearance = "dark".into();
        cfg.window_x = Some(0);
        cfg.window_y = Some(0);
        cfg.table_width = 400;
        cfg.table_height = 500;
        let cfg = Arc::new(Mutex::new(cfg));
        let ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
        let hud =
            HUDWindow::with_providers(cfg, ctrl, crate::providers::stub::stub_providers()).unwrap();
        hud.window.root_widget().borrow().update_layout();

        assert_eq!(
            hud.title_label.borrow().geometry().height,
            14,
            "title label height (PySide6: 14)"
        );
        assert_eq!(
            hud.stack.borrow().geometry().height,
            462,
            "table height (PySide6: 462)"
        );
    }

    /// RC-16: the stack policy follows the page, so switching cards -> table -> cards keeps both
    /// PySide6 geometries (table takes the height; the vertical header takes it in cards mode).
    #[cfg(windows)]
    #[test]
    fn test_ui_mode_switch_keeps_pyside6_extra_height_owner() {
        let _setup = crate::ui::test_support::CardsOracleSetup::new();
        let mut cfg = Config::default();
        cfg.ui_mode = "cards".into();
        cfg.layout_mode = "vertical".into();
        cfg.appearance = "dark".into();
        cfg.window_x = Some(0);
        cfg.window_y = Some(0);
        cfg.vertical_width = 280;
        cfg.vertical_height = 463;
        cfg.table_width = 400;
        cfg.table_height = 500;
        let cfg = Arc::new(Mutex::new(cfg));
        let ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
        let mut hud =
            HUDWindow::with_providers(cfg, ctrl, crate::providers::stub::stub_providers()).unwrap();

        hud.apply_ui_mode("table");
        hud.window.root_widget().borrow().update_layout();
        assert_eq!(
            hud.title_label.borrow().geometry().height,
            14,
            "table mode title height (PySide6: 14)"
        );
        assert_eq!(
            hud.stack.borrow().geometry().height,
            462,
            "table mode table height (PySide6: 462)"
        );
        hud.apply_ui_mode("cards");
        hud.window.root_widget().borrow().update_layout();
        assert_eq!(
            hud.title_label.borrow().geometry().height,
            86,
            "cards mode title height (PySide6: 86)"
        );
        assert_eq!(
            hud.stack.borrow().geometry().height,
            353,
            "cards mode stack height"
        );
        assert!(
            hud.layout_toggle_btn.borrow().is_visible(),
            "toggle button visible in cards mode"
        );
        assert_eq!(
            hud.title_label
                .borrow()
                .as_any()
                .downcast_ref::<Label>()
                .unwrap()
                .text(),
            "AI AGENT HUD (3-IN-1)"
        );
    }

    /// G12.5.r / DV-04: Test full roundtrip switching `cards -> table -> cards`
    /// asserts visibility, geometry preservation, toggle button, and title state without degradation.
    #[cfg(windows)]
    #[test]
    fn test_cards_table_cards_roundtrip_preserves_state() {
        let _setup = crate::ui::test_support::CardsOracleSetup::new();
        let mut cfg = Config::default();
        cfg.ui_mode = "cards".into();
        cfg.layout_mode = "vertical".into();
        cfg.appearance = "dark".into();
        cfg.window_x = Some(0);
        cfg.window_y = Some(0);
        cfg.vertical_width = 280;
        cfg.vertical_height = 463;
        cfg.table_width = 400;
        cfg.table_height = 500;
        let cfg = Arc::new(Mutex::new(cfg));
        let ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
        let mut hud =
            HUDWindow::with_providers(cfg, ctrl, crate::providers::stub::stub_providers()).unwrap();

        // 1. Initial cards mode
        hud.window.root_widget().borrow().update_layout();
        let init_cards_geom = hud.cards_container.borrow().geometry();
        let init_toggle_visible = hud.layout_toggle_btn.borrow().is_visible();
        assert!(init_toggle_visible);
        assert_eq!(
            hud.title_label
                .borrow()
                .as_any()
                .downcast_ref::<Label>()
                .unwrap()
                .text(),
            "AI AGENT HUD (3-IN-1)"
        );

        // 2. Switch to table mode
        hud.apply_ui_mode("table");
        hud.window.root_widget().borrow().update_layout();
        assert!(
            !hud.layout_toggle_btn.borrow().is_visible(),
            "toggle button hidden in table mode"
        );
        assert_eq!(
            hud.title_label
                .borrow()
                .as_any()
                .downcast_ref::<Label>()
                .unwrap()
                .text(),
            "AI AGENT HUD (TABLE)"
        );
        assert_eq!(hud.stack.borrow().geometry().height, 462);

        // 3. Switch back to cards mode
        hud.apply_ui_mode("cards");
        hud.window.root_widget().borrow().update_layout();
        let final_cards_geom = hud.cards_container.borrow().geometry();
        assert!(
            hud.layout_toggle_btn.borrow().is_visible(),
            "toggle button restored visible in cards mode"
        );
        assert_eq!(
            hud.title_label
                .borrow()
                .as_any()
                .downcast_ref::<Label>()
                .unwrap()
                .text(),
            "AI AGENT HUD (3-IN-1)"
        );
        assert_eq!(
            final_cards_geom, init_cards_geom,
            "cards container geometry preserved across roundtrip"
        );
        assert_eq!(hud.stack.borrow().geometry().height, 353);
    }

    /// TI-01: a window built with injected providers fetches from exactly those providers — one
    /// background fetch each — so tests never reach the live endpoints.
    #[test]
    fn test_hud_window_launches_only_the_injected_providers() {
        let cfg = Arc::new(Mutex::new(Config::default()));
        let refresh_ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
        let (providers, fetched) = crate::providers::stub::stub_providers_with_receiver();

        let hud =
            HUDWindow::with_providers(cfg, refresh_ctrl, providers).expect("with_providers failed");

        let mut ids: Vec<String> = (0..crate::providers::PROVIDER_IDS.len())
            .map(|_| {
                fetched
                    .recv_timeout(std::time::Duration::from_secs(10))
                    .expect("stub was not fetched")
            })
            .collect();
        ids.sort();
        assert_eq!(ids, ["agy", "claude", "codex"]);
        let mut held: Vec<&String> = hud.providers.keys().collect();
        held.sort();
        assert_eq!(held, ["agy", "claude", "codex"]);
    }

    fn after(t: SystemTime, secs: f64) -> SystemTime {
        t + Duration::from_secs_f64(secs)
    }

    #[test]
    fn tick_gap_is_false_for_the_first_tick_and_up_to_exactly_15_seconds() {
        let t0 = SystemTime::now();
        assert!(
            !tick_gap_exceeded(None, t0),
            "first tick only records a timestamp"
        );
        assert!(!tick_gap_exceeded(Some(t0), after(t0, 1.0)));
        assert!(!tick_gap_exceeded(Some(t0), after(t0, 10.0)));
        assert!(!tick_gap_exceeded(Some(t0), after(t0, 14.999)));
        assert!(
            !tick_gap_exceeded(Some(t0), after(t0, 15.0)),
            "Python: gap > 15.0, so exactly 15 s is no wake-up"
        );
    }

    #[test]
    fn tick_gap_is_true_just_above_15_seconds() {
        let t0 = SystemTime::now();
        assert!(tick_gap_exceeded(Some(t0), after(t0, 15.001)));
        assert!(tick_gap_exceeded(Some(t0), after(t0, 3600.0)));
    }

    #[test]
    fn tick_gap_is_false_when_the_clock_moved_backwards() {
        let t0 = SystemTime::now();
        assert!(!tick_gap_exceeded(Some(t0), t0 - Duration::from_secs(60)));
    }

    fn hud_with_stubs() -> (HUDWindow, std::sync::mpsc::Receiver<String>) {
        let cfg = Arc::new(Mutex::new(Config::default()));
        let ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
        let (providers, fetched) = crate::providers::stub::stub_providers_with_receiver();
        let hud = HUDWindow::with_providers(cfg, ctrl, providers).expect("with_providers failed");
        // Let the initial fetches finish and be collected, so later fetches are only refresh() ones.
        for _ in 0..crate::providers::PROVIDER_IDS.len() {
            fetched
                .recv_timeout(Duration::from_secs(10))
                .expect("initial fetch");
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while hud.refresh_ctrl.lock().is_busy() {
            hud.refresh_ctrl.lock().drain_results(&hud.providers);
            assert!(
                std::time::Instant::now() < deadline,
                "initial fetches never completed"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        (hud, fetched)
    }

    fn fetches_within(rx: &std::sync::mpsc::Receiver<String>, ms: u64) -> usize {
        let deadline = std::time::Instant::now() + Duration::from_millis(ms);
        let mut n = 0;
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            if rx.recv_timeout(left).is_ok() {
                n += 1;
            }
        }
        n
    }

    /// Counts trim calls and when they happened, relative to `install`.
    struct TrimLog(Arc<Mutex<Vec<Duration>>>, std::time::Instant);

    impl TrimLog {
        fn install(hud: &HUDWindow) -> Self {
            let calls = Arc::new(Mutex::new(Vec::new()));
            let start = std::time::Instant::now();
            let sink = Arc::clone(&calls);
            hud.set_trim(Arc::new(move || sink.lock().push(start.elapsed())));
            TrimLog(calls, start)
        }

        fn calls(&self) -> Vec<Duration> {
            self.0.lock().clone()
        }

        /// Runs the event loop until `ms` after `install`.
        fn pump_until(&self, el: &mut ferrokit_core::event_loop::EventLoop, ms: u64) {
            while self.1.elapsed() < Duration::from_millis(ms) {
                el.process_events(false);
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }

    /// The timers are coarse (`Timer::single_shot`), which may fire up to 5 % early; 10 % is the
    /// floor asserted here. Timer precision itself is outside this check (C5.5).
    fn assert_one_trim_after(log: &TrimLog, delay_ms: u64, what: &str) {
        let calls = log.calls();
        assert_eq!(calls.len(), 1, "{what}: trims {calls:?}");
        assert!(
            calls[0] >= Duration::from_millis(delay_ms * 9 / 10),
            "{what}: trimmed at {:?}, Python waits {delay_ms} ms",
            calls[0]
        );
    }

    /// Runs one fetch of `id` to completion; its result is then waiting in the channel.
    fn finish_fetch(hud: &HUDWindow, rx: &std::sync::mpsc::Receiver<String>, id: &str) {
        let provider = Arc::clone(&hud.providers[id]);
        hud.refresh_ctrl.lock().launch(id, provider);
        rx.recv_timeout(Duration::from_secs(10)).expect("fetch");
        std::thread::sleep(Duration::from_millis(50));
    }

    /// Python `hideEvent` -> `QTimer.singleShot(150, trim_memory)` (`hud_window.py:615-617`).
    #[test]
    fn hide_trims_150ms_later_not_during_the_hide() {
        let mut el = ferrokit_core::event_loop::EventLoop::new();
        let (mut hud, _rx) = hud_with_stubs();
        hud.show();
        let log = TrimLog::install(&hud);
        hud.hide();
        assert!(log.calls().is_empty(), "hide() trimmed synchronously");
        log.pump_until(&mut el, 1000);
        assert_one_trim_after(&log, 150, "hide");
    }

    /// Alt+F4: the close request hides the HUD (RC-06), and Python's `hideEvent` then trims.
    #[cfg(windows)]
    #[test]
    fn close_request_trims_150ms_after_the_hide() {
        #[link(name = "user32")]
        extern "system" {
            fn SendMessageW(hwnd: *mut core::ffi::c_void, msg: u32, w: usize, l: isize) -> isize;
        }
        const WM_CLOSE: u32 = 0x0010;
        let mut el = ferrokit_core::event_loop::EventLoop::new();
        let (mut hud, _rx) = hud_with_stubs();
        hud.show();
        let log = TrimLog::install(&hud);
        unsafe { SendMessageW(hud.window.native_handle() as _, WM_CLOSE, 0, 0) };
        assert!(!hud.window.is_visible(), "WM_CLOSE hides the HUD");
        log.pump_until(&mut el, 1000);
        assert_one_trim_after(&log, 150, "close request");
    }

    /// Python `_complete` emits `busy_changed(False)` when the last running provider finishes, and
    /// `_on_busy_changed(False)` trims 1000 ms later (`refresh_controller.py:125-126`,
    /// `hud_window.py:455-459`). Here through the worker notify path.
    #[test]
    fn last_provider_finishing_trims_a_second_later() {
        let mut el = ferrokit_core::event_loop::EventLoop::new();
        let (mut hud, rx) = hud_with_stubs();
        finish_fetch(&hud, &rx, "claude");
        let log = TrimLog::install(&hud);
        hud.drain_refresh_results();
        log.pump_until(&mut el, 1500);
        assert_one_trim_after(&log, 1000, "idle completion (notify path)");
    }

    /// `poll` drains results and then launches due providers in one call. Python's completion
    /// event happens before its separate poll timer launches anything, so the HUD being busy again
    /// after `poll` must not swallow the idle completion.
    #[test]
    fn idle_completion_survives_a_due_provider_launched_in_the_same_poll() {
        let mut el = ferrokit_core::event_loop::EventLoop::new();
        let (mut hud, rx) = hud_with_stubs();
        finish_fetch(&hud, &rx, "claude");
        hud.refresh_ctrl.lock().states.get_mut("codex").unwrap().due =
            Some(std::time::Instant::now());
        let log = TrimLog::install(&hud);
        hud.poll_refresh();
        assert!(
            hud.refresh_ctrl.lock().is_busy(),
            "codex was due and launched"
        );
        rx.recv_timeout(Duration::from_secs(10))
            .expect("codex fetch");
        log.pump_until(&mut el, 1500);
        assert_one_trim_after(&log, 1000, "idle completion (poll path)");
    }

    /// Python emits `busy_changed(True)` while another provider still runs: no trim.
    #[test]
    fn completion_while_another_provider_runs_does_not_trim() {
        let mut el = ferrokit_core::event_loop::EventLoop::new();
        let (mut hud, rx) = hud_with_stubs();
        finish_fetch(&hud, &rx, "claude");
        hud.refresh_ctrl
            .lock()
            .states
            .get_mut("codex")
            .unwrap()
            .running = true;
        let log = TrimLog::install(&hud);
        hud.drain_refresh_results();
        log.pump_until(&mut el, 1500);
        assert!(log.calls().is_empty(), "trimmed with codex running");
    }

    /// Python `HUDWindow.__init__` -> `QTimer.singleShot(2500, trim_memory)` (`hud_window.py:114`).
    #[test]
    fn construction_schedules_initial_trim_at_2500ms() {
        let mut el = ferrokit_core::event_loop::EventLoop::new();
        let cfg = Arc::new(Mutex::new(Config::default()));
        let ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
        let (providers, _fetched) = crate::providers::stub::stub_providers_with_receiver();
        let hud = HUDWindow::with_providers(cfg, ctrl, providers).expect("with_providers");
        let log = TrimLog::install(&hud);

        // Within 1000ms, initial 2500ms trim should not have fired yet
        log.pump_until(&mut el, 1000);
        assert!(log.calls().is_empty(), "trimmed prematurely before 2500ms");

        // After pumping past 2700ms, initial trim should have fired exactly once
        log.pump_until(&mut el, 2700);
        assert_one_trim_after(&log, 2500, "construction 2500ms trim");
    }
    #[test]
    fn clock_tick_refreshes_every_provider_exactly_once_after_a_long_gap() {
        let (mut hud, fetched) = hud_with_stubs();
        let t0 = SystemTime::now();
        let providers = crate::providers::PROVIDER_IDS.len();

        hud.on_clock_tick(t0);
        hud.on_clock_tick(after(t0, 1.0));
        assert_eq!(
            fetches_within(&fetched, 400),
            0,
            "normal ticks must not refresh"
        );

        hud.on_clock_tick(after(t0, 17.0));
        assert_eq!(
            fetches_within(&fetched, 800),
            providers,
            "one long gap = one refresh() = one fetch per provider"
        );

        // previous timestamp was updated to t0+17: the next ordinary tick is not another wake-up
        hud.refresh_ctrl.lock().drain_results(&hud.providers);
        hud.on_clock_tick(after(t0, 18.0));
        assert_eq!(
            fetches_within(&fetched, 400),
            0,
            "the gap is measured from the previous tick, not from t0"
        );
    }

    #[test]
    fn wake_refresh_sequence_advances_once_per_long_gap_only() {
        let (mut hud, _fetched) = hud_with_stubs();
        let t0 = SystemTime::now();
        assert_eq!(hud.wake_refresh_seq(), 0);

        hud.on_clock_tick(t0);
        hud.on_clock_tick(after(t0, 1.0));
        hud.on_clock_tick(after(t0, 2.0));
        assert_eq!(
            hud.wake_refresh_seq(),
            0,
            "normal ticks leave the sequence alone"
        );

        hud.on_clock_tick(after(t0, 19.0));
        assert_eq!(
            hud.wake_refresh_seq(),
            1,
            "a 17 s gap is exactly one refresh"
        );

        hud.on_clock_tick(after(t0, 20.0));
        assert_eq!(
            hud.wake_refresh_seq(),
            1,
            "the next 1 s tick is not a second wake-up"
        );

        hud.on_clock_tick(after(t0, 20.0 + 15.0));
        assert_eq!(hud.wake_refresh_seq(), 1, "exactly 15 s is not a wake-up");
        hud.on_clock_tick(after(t0, 20.0 + 15.0 + 16.0));
        assert_eq!(hud.wake_refresh_seq(), 2);
    }

    #[test]
    fn test_hud_window_init_does_not_trigger_save() {
        let cfg = Arc::new(Mutex::new(Config::default()));
        let refresh_ctrl = Arc::new(Mutex::new(RefreshController::new(60)));

        let hud = HUDWindow::with_providers(
            Arc::clone(&cfg),
            refresh_ctrl,
            crate::providers::stub::stub_providers(),
        )
        .expect("HUDWindow::new failed");

        // Debouncer was in restoring mode during HUDWindow::new
        // It must have prevented any saves during init
        assert_eq!(hud.debouncer.save_count(), 0);
        assert!(!hud.debouncer.is_restoring());
    }

    #[test]
    fn test_hud_window_close_flushes_pending_resize() {
        let cfg = Arc::new(Mutex::new(Config::default()));
        let refresh_ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
        let mut hud = HUDWindow::with_providers(
            Arc::clone(&cfg),
            refresh_ctrl,
            crate::providers::stub::stub_providers(),
        )
        .expect("HUDWindow::new failed");

        // Simulate resize request
        hud.debouncer.request_save();
        assert!(hud.debouncer.is_pending());

        // Calling close should flush pending resize immediately
        hud.close();
        assert!(!hud.debouncer.is_pending());
    }

    #[test]
    fn test_hud_window_drop_triggers_shutdown_and_worker_join() {
        let cfg = Arc::new(Mutex::new(Config::default()));
        let refresh_ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
        let hud = HUDWindow::with_providers(
            Arc::clone(&cfg),
            refresh_ctrl,
            crate::providers::stub::stub_providers(),
        )
        .expect("HUDWindow::new failed");
        let debouncer = Arc::clone(&hud.debouncer);

        assert!(!debouncer.is_worker_joined());

        drop(hud);

        assert!(debouncer.is_worker_joined());
    }

    /// Python `_apply_theme` re-sets `HUDWindow.setStyleSheet` for the mode. A window sheet is
    /// scoped to the window (RC-10) and outranks the application's, so a stale one from the other
    /// mode would keep styling the labels.
    #[test]
    fn test_switching_ui_mode_restyles_labels_from_the_new_sheet() {
        use ferrokit_widgets::style::stylesheet::{QStyleSheetStyle, WidgetStyleContext};
        use ferrokit_widgets::Label;

        let cfg = Arc::new(Mutex::new(Config::default()));
        let refresh_ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
        let mut hud = HUDWindow::with_providers(
            Arc::clone(&cfg),
            refresh_ctrl,
            crate::providers::stub::stub_providers(),
        )
        .expect("HUDWindow::new failed");

        let ctx = WidgetStyleContext {
            type_name: "QLabel",
            object_name: "HeaderTitle",
            pseudo_states: &[],
            sub_control: None,
            attributes: &[],
        };
        let mut colours = Vec::new();
        for mode in ["table", "cards", "table"] {
            hud.apply_ui_mode(mode);
            let expected = QStyleSheetStyle::parse(style_sheet_for(mode, hud.is_dark))
                .resolve(&ctx)
                .color;
            let actual = hud
                .title_label
                .borrow()
                .as_any()
                .downcast_ref::<Label>()
                .expect("title is a Label")
                .resolved_style()
                .color;
            assert_eq!(actual, expected, "title colour in {mode} mode");
            colours.push(expected);
        }
        assert_ne!(
            colours[0], colours[1],
            "the two sheets must style the title differently"
        );
    }

    fn blur_installs(_: &mut Window, _: BackdropType, _: bool) -> bool {
        true
    }

    fn blur_fails(_: &mut Window, _: BackdropType, _: bool) -> bool {
        false
    }

    /// A shown table-mode HUD whose blur install succeeds or fails as `set` says.
    fn shown_table_hud(set: BackdropFn) -> HUDWindow {
        let mut cfg = Config::default();
        cfg.ui_mode = "table".into();
        let cfg = Arc::new(Mutex::new(cfg));
        let ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
        let mut hud =
            HUDWindow::with_providers(cfg, ctrl, crate::providers::stub::stub_providers()).unwrap();
        hud.set_backdrop_fn(set);
        hud.show();
        hud
    }

    /// Alpha of the panel at its centre, as the window's root paints it.
    fn panel_alpha(hud: &HUDWindow) -> u8 {
        let mut pixmap = ferrokit_gui::paint::pixmap::Pixmap::new(40, 40).unwrap();
        {
            let mut painter = ferrokit_gui::paint::Painter::begin(&mut pixmap);
            hud.window
                .root_widget()
                .borrow_mut()
                .paint_event(&mut painter);
        }
        pixmap.pixel(20, 20).unwrap().alpha()
    }

    fn alpha(c: ferrokit_gui::tiny_skia::Color) -> u8 {
        c.to_color_u8().alpha()
    }

    /// G12.5.c: Python paints the solid panel when `vibrancy.apply` fails (`hud_window.py:245-246`,
    /// `styles.py:205`), so the panel never becomes see-through without a blur behind it.
    #[test]
    fn table_panel_is_solid_when_the_blur_cannot_be_installed() {
        let hud = shown_table_hud(blur_fails);
        assert_eq!(panel_alpha(&hud), alpha(hud.theme.panel_bg));
    }

    /// An idle countdown pass in cards mode marks nothing for repaint: the countdowns are
    /// day-granular so no text changes, and the hidden table page's dials are not visible, so
    /// their `update()` is ignored as in Qt (`QWidgetPrivate::update`, qwidget.cpp:11367-11368).
    #[test]
    fn idle_countdown_pass_in_cards_mode_marks_nothing_for_repaint() {
        let (mut hud, _fetched) = hud_with_stubs();
        assert_eq!(hud.config.lock().ui_mode, "cards");
        let reset = chrono::Utc::now() + chrono::Duration::days(3) + chrono::Duration::hours(2);
        for id in crate::providers::PROVIDER_IDS {
            hud.on_data_fetched(&UsageMetrics {
                provider_id: id.to_string(),
                metric1_reset: Some(reset),
                metric2_reset: Some(reset),
                ..Default::default()
            });
        }
        let root = hud.window.root_widget();
        let origin = ferrokit_gui::geometry::primitives::Point::new(0, 0);
        // Settle: drop everything the data pass marked.
        let _ = ferrokit_widgets::window::collect_dirty_region(&root, origin);
        let direct_before = hud.window.render_stats().direct_render_count;
        hud.update_clock();
        assert_eq!(
            hud.window.render_stats().direct_render_count,
            direct_before,
            "a clock tick must reach the window through widget update, not render_and_present"
        );
        let dirty = ferrokit_widgets::window::collect_dirty_region(&root, origin);
        assert_eq!(dirty, None, "an idle countdown pass marked a repaint");
    }

    /// G10.1.a: an idle clock tick in table mode repaints through widget `update()` only, and
    /// only the widgets Python's `update_countdown` touches every second: the dial (`update()`,
    /// `usage_table.py:144`) and the weekly value label (`setStyleSheet`, `usage_table.py:368`,
    /// which Qt repolishes on every call). No full-window `render_and_present`.
    #[test]
    fn idle_clock_tick_in_table_mode_dirties_only_dials_and_weekly_values() {
        let (mut hud, _fetched) = hud_with_stubs();
        hud.apply_ui_mode("table");
        let reset = chrono::Utc::now() + chrono::Duration::days(3) + chrono::Duration::hours(2);
        for id in crate::providers::PROVIDER_IDS {
            hud.on_data_fetched(&UsageMetrics {
                provider_id: id.to_string(),
                metric1_val: Some(40.0),
                metric1_reset: Some(reset),
                metric2_val: Some(60.0),
                metric2_reset: Some(reset),
                ..Default::default()
            });
        }
        let root = hud.window.root_widget();
        // Settle: drop everything the data pass marked.
        let _ = ferrokit_widgets::window::collect_dirty_region(
            &root,
            ferrokit_gui::geometry::primitives::Point::new(0, 0),
        );
        let direct_before = hud.window.render_stats().direct_render_count;

        hud.update_clock();

        assert_eq!(
            hud.window.render_stats().direct_render_count,
            direct_before,
            "a clock tick must reach the window through widget update, not render_and_present"
        );
        fn dirty_widgets(w: &WidgetRef, out: &mut Vec<WidgetRef>) {
            let b = w.borrow();
            if b.dirty_rect().is_some() {
                out.push(Rc::clone(w));
            }
            for child in b.children() {
                dirty_widgets(&child, out);
            }
        }
        let mut dirty = Vec::new();
        dirty_widgets(&root, &mut dirty);
        let expected: Vec<WidgetRef> = hud
            .table
            .columns
            .values()
            .flat_map(|c| [Rc::clone(&c.dial), Rc::clone(&c.m2_val)])
            .collect();
        assert!(
            !dirty.is_empty(),
            "the dial repaints every tick, as in Python"
        );
        for w in &dirty {
            assert!(
                expected.iter().any(|e| Rc::ptr_eq(e, w)),
                "an idle tick dirtied a widget other than a dial or weekly value: {:?}",
                w.borrow().geometry()
            );
        }
    }

    #[test]
    fn table_panel_is_translucent_over_an_installed_blur() {
        let hud = shown_table_hud(blur_installs);
        assert_eq!(panel_alpha(&hud), alpha(hud.theme.panel_bg_vibrant));
    }

    /// `_apply_theme` runs again on a theme change; a blur that no longer installs drops the panel
    /// to solid there too.
    #[test]
    fn theme_change_whose_blur_fails_switches_to_the_solid_panel() {
        let mut hud = shown_table_hud(blur_installs);
        hud.set_backdrop_fn(blur_fails);
        let dark = !hud.is_dark;
        hud.set_theme(dark);
        assert_eq!(panel_alpha(&hud), alpha(hud.theme.panel_bg));
    }
}

/// The pure maths of the sidebar's edge / corner resize and idle fade (Python `_resize_zone`,
/// `_resize_to`, `_apply_opacity`); nothing here creates a window.
#[cfg(test)]
mod sidebar_geometry_tests {
    use super::*;

    const G: Rect = Rect {
        x: 100,
        y: 100,
        width: 64,
        height: 200,
    };

    #[test]
    fn grips_are_seven_pixels_wide_and_corners_combine() {
        // QRect edges are inclusive: right() == 163, bottom() == 299
        assert_eq!(resize_zone(G, Point::new(100, 200)).as_deref(), Some("l"));
        assert_eq!(resize_zone(G, Point::new(106, 200)).as_deref(), Some("l"));
        assert_eq!(resize_zone(G, Point::new(107, 200)), None);
        assert_eq!(resize_zone(G, Point::new(163, 200)).as_deref(), Some("r"));
        assert_eq!(resize_zone(G, Point::new(157, 200)).as_deref(), Some("r"));
        assert_eq!(resize_zone(G, Point::new(130, 100)).as_deref(), Some("t"));
        assert_eq!(resize_zone(G, Point::new(130, 299)).as_deref(), Some("b"));
        assert_eq!(resize_zone(G, Point::new(100, 100)).as_deref(), Some("lt"));
        assert_eq!(resize_zone(G, Point::new(163, 100)).as_deref(), Some("rt"));
        assert_eq!(resize_zone(G, Point::new(100, 299)).as_deref(), Some("lb"));
        assert_eq!(resize_zone(G, Point::new(163, 299)).as_deref(), Some("rb"));
    }

    #[test]
    fn the_grip_reaches_two_pixels_outside_the_window_only() {
        assert_eq!(resize_zone(G, Point::new(98, 200)).as_deref(), Some("l"));
        assert_eq!(resize_zone(G, Point::new(97, 200)), None);
        assert_eq!(resize_zone(G, Point::new(165, 200)).as_deref(), Some("r"));
        assert_eq!(resize_zone(G, Point::new(166, 200)), None);
        assert_eq!(resize_zone(G, Point::new(130, 301)).as_deref(), Some("b"));
        assert_eq!(resize_zone(G, Point::new(130, 302)), None);
        assert_eq!(resize_zone(G, Point::new(130, 200)), None);
    }

    #[test]
    fn cursors_follow_the_zone() {
        assert_eq!(zone_cursor("l"), CursorShape::SizeHor);
        assert_eq!(zone_cursor("r"), CursorShape::SizeHor);
        assert_eq!(zone_cursor("t"), CursorShape::SizeVer);
        assert_eq!(zone_cursor("b"), CursorShape::SizeVer);
        assert_eq!(zone_cursor("lt"), CursorShape::SizeFDiag);
        assert_eq!(zone_cursor("rb"), CursorShape::SizeFDiag);
        assert_eq!(zone_cursor("rt"), CursorShape::SizeBDiag);
        assert_eq!(zone_cursor("lb"), CursorShape::SizeBDiag);
    }

    #[test]
    fn dragging_an_edge_scales_by_the_ratio_of_the_size() {
        assert_eq!(resize_scale("r", G, 1.0, 32, 0), 1.5);
        assert_eq!(resize_scale("l", G, 1.0, -32, 0), 1.5);
        assert_eq!(resize_scale("b", G, 1.0, 0, 100), 1.5);
        assert_eq!(resize_scale("t", G, 1.0, 0, -100), 1.5);
        // the start scale is the base
        assert_eq!(resize_scale("r", G, 1.2, 32, 0), 1.8);
        // held to 0.6 - 2.0
        assert_eq!(resize_scale("b", G, 1.0, 0, -150), 0.6);
        assert_eq!(resize_scale("r", G, 1.5, 640, 0), 2.0);
    }

    #[test]
    fn a_corner_uses_the_edge_that_moved_the_furthest() {
        // right grew by 5 %, bottom by 50 %
        assert_eq!(resize_scale("rb", G, 1.0, 3, 100), 1.5);
        // width shrank (0.5) further from 1 than the height grew (1.1)
        assert_eq!(resize_scale("rb", G, 1.0, -32, 20), 0.6);
    }

    #[test]
    fn the_opposite_edge_or_centre_stays_put() {
        // 128 x 400 after doubling
        // a right grip keeps the left edge and centres vertically (centre y 199 - 400 / 2)
        assert_eq!(resize_origin("r", G, 128, 400), (100, -1));
        assert_eq!(resize_origin("rb", G, 128, 400), (100, 100));
        // left grip: the right edge (x + width) stays
        assert_eq!(resize_origin("l", G, 128, 200).0, 164 - 128);
        assert_eq!(resize_origin("t", G, 64, 400).1, 300 - 400);
        // a grip without a horizontal part keeps the centre
        assert_eq!(resize_origin("b", G, 128, 200).0, 131 - 64);
    }

    #[test]
    fn a_task_that_found_the_hud_busy_is_tried_again_until_it_runs() {
        assert!(should_retry(false, 5));
        assert!(!should_retry(true, 5), "it ran");
        assert!(!should_retry(false, 0), "out of tries");
        assert!(HUD_RETRIES as u64 * HUD_RETRY_MS >= 500, "long enough to outlast a layout pass");
    }

    #[test]
    fn a_drag_without_a_held_button_lost_its_release() {
        assert!(resize_lost_release(true));
        assert!(!resize_lost_release(false));
    }

    #[test]
    fn only_input_events_reach_the_filter_logic() {
        assert!(is_filtered_kind(&EventKind::MouseMove { x: 0, y: 0 }));
        assert!(is_filtered_kind(&EventKind::MouseButtonPress { x: 0, y: 0, button: 1 }));
        assert!(is_filtered_kind(&EventKind::MouseButtonRelease { x: 0, y: 0, button: 1 }));
        assert!(is_filtered_kind(&EventKind::Leave));
        assert!(is_filtered_kind(&EventKind::Enter { x: 0, y: 0 }));
        assert!(!is_filtered_kind(&EventKind::Close));
        assert!(!is_filtered_kind(&EventKind::Resize { width: 1, height: 1, old_width: 0, old_height: 0 }));
    }

    #[test]
    fn the_filter_only_accepts_widgets_of_the_hud_tree() {
        let mk = || -> WidgetRef { make_widget(EmptyWidget::new()) };
        let (root, child, grandchild, stranger) = (mk(), mk(), mk(), mk());
        let mut inner = BoxLayout::vertical();
        inner.add_widget(grandchild.clone());
        child.borrow_mut().set_layout(Box::new(inner));
        let mut outer = BoxLayout::vertical();
        outer.add_widget(child.clone());
        root.borrow_mut().set_layout(Box::new(outer));
        let id = |w: &WidgetRef| w.borrow().id();
        assert!(tree_contains(&root, id(&root)));
        assert!(tree_contains(&root, id(&child)));
        assert!(tree_contains(&root, id(&grandchild)));
        assert!(!tree_contains(&root, id(&stranger)), "another window's widget");
        // the widget that is being notified is mutably borrowed and cannot be read: it is found
        let child_id = id(&child);
        let held = child.borrow_mut();
        assert!(tree_contains(&root, child_id));
        drop(held);
    }

    #[test]
    fn the_idle_fade_multiplies_the_chosen_opacity() {
        assert_eq!(faded_opacity(0.88, 0.6, false), 0.88);
        assert!((faded_opacity(0.88, 0.6, true) - 0.528).abs() < 1e-6);
        assert!((faded_opacity(1.0, 0.2, true) - 0.2).abs() < 1e-6);
    }
}
