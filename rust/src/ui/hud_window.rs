//! Main HUD window component matching Python ui/hud_window.py.
//!
//! Hosts the header bar, StackedWidget (Cards vs Table mode),
//! frameless window management, Acrylic backdrop blur, and click-through ghost mode.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use chrono::Local;
use parking_lot::Mutex;
use qtrs_core::QObject;
use qtrs_gui::geometry::primitives::{Margins, Rect, RectF};
use qtrs_gui::paint::{Brush, Pen};
use qtrs_platform::backdrop::BackdropType;
use qtrs_platform::WindowFlags;
use qtrs_widgets::{
    BoxLayout, Button, EmptyWidget, Label, Layout, StackedWidget, Widget, WidgetRef, Window,
};

use super::provider_card::ProviderCardWidget;
use super::styles::{get_theme, Theme};
use super::usage_table::UsageTable;
use crate::config::{
    Config, ResizeDebouncer, DEFAULT_HORIZONTAL_HEIGHT, DEFAULT_HORIZONTAL_WIDTH,
    DEFAULT_TABLE_HEIGHT, DEFAULT_TABLE_WIDTH, DEFAULT_VERTICAL_HEIGHT, DEFAULT_VERTICAL_WIDTH,
    MIN_HORIZONTAL_HEIGHT, MIN_HORIZONTAL_WIDTH, MIN_TABLE_HEIGHT, MIN_TABLE_WIDTH,
    MIN_VERTICAL_HEIGHT, MIN_VERTICAL_WIDTH,
};
use crate::providers::base::UsageMetrics;
use crate::providers::Provider;
use crate::refresh_controller::RefreshController;

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
fn status_dot_color(busy: bool, any_error: bool) -> qtrs_gui::tiny_skia::Color {
    let (r, g, b) = if busy {
        (56, 189, 248)
    } else if any_error {
        (245, 158, 11)
    } else {
        (16, 185, 129)
    };
    qtrs_gui::tiny_skia::Color::from_rgba8(r, g, b, 255)
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
fn set_status_dot_color(dot: &WidgetRef, color: qtrs_gui::tiny_skia::Color) {
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
/// the application style sheet's `QWidget#CentralWidget` rule, as in the Python HUD; the table
/// mode (and a sheet without that rule) uses the theme.
fn panel_look(cards_mode: bool, theme: &Theme) -> (qtrs_gui::tiny_skia::Color, qtrs_gui::tiny_skia::Color, f32) {
    if !cards_mode {
        // Table mode uses vibrant panel on top of Acrylic backdrop blur, matching Python (radius 12.0)
        return (theme.panel_bg_vibrant, theme.panel_border, 12.0);
    }
    let themed = (theme.panel_bg, theme.panel_border, 10.0);
    let Some(sheet) = qtrs_widgets::application::Application::style_sheet() else {
        return themed;
    };
    let ctx = qtrs_widgets::style::stylesheet::WidgetStyleContext {
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

fn install_panel_painter(root: &WidgetRef, theme: &Theme, config: Arc<Mutex<Config>>) {
    let theme = theme.clone();
    if let Some(empty) = root.borrow_mut().as_any_mut().downcast_mut::<EmptyWidget>() {
        empty.set_paint_handler(move |painter| {
            let (bg, border, radius) = panel_look(config.lock().ui_mode != "table", &theme);
            let w = painter.device().width();
            let h = painter.device().height();
            let rect_f = RectF::new(0.5, 0.5, w - 1.0, h - 1.0);
            painter.set_brush(Brush::Color(bg));
            painter.set_pen(Pen::new(border, 1.0));
            painter.draw_rounded_rect(rect_f, radius, radius);
        });
    }
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
}

/// The real providers. Test builds must not reach them: building a window launches a live fetch per
/// provider, so tests construct windows with `HUDWindow::with_providers` and stubs.
#[cfg(not(test))]
fn default_providers(config: &Arc<Mutex<Config>>) -> HashMap<String, Arc<dyn Provider + Send + Sync>> {
    use crate::providers::{agy::AgyProvider, claude::ClaudeProvider, codex::CodexProvider};
    HashMap::from([
        (
            "claude".to_string(),
            Arc::new(ClaudeProvider::with_config(Some(Arc::clone(config)))) as Arc<dyn Provider + Send + Sync>,
        ),
        ("agy".to_string(), Arc::new(AgyProvider::new()) as Arc<dyn Provider + Send + Sync>),
        ("codex".to_string(), Arc::new(CodexProvider::new()) as Arc<dyn Provider + Send + Sync>),
    ])
}

#[cfg(test)]
fn default_providers(_config: &Arc<Mutex<Config>>) -> HashMap<String, Arc<dyn Provider + Send + Sync>> {
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
        let (init_x, init_y, init_w, init_h, opacity, aot, ct, ui_mode, dark) = {
            let cfg = config.lock();
            let is_dark = crate::ui::resolve_is_dark(&cfg.appearance);
            let (w, h) = if cfg.ui_mode == "table" {
                (
                    cfg.table_width.max(MIN_TABLE_WIDTH) as i32,
                    cfg.table_height.max(MIN_TABLE_HEIGHT) as i32,
                )
            } else if cfg.layout_mode == "horizontal" {
                (
                    cfg.horizontal_width.max(MIN_HORIZONTAL_WIDTH) as i32,
                    cfg.horizontal_height.max(MIN_HORIZONTAL_HEIGHT) as i32,
                )
            } else {
                (
                    cfg.vertical_width.max(MIN_VERTICAL_WIDTH) as i32,
                    cfg.vertical_height.max(MIN_VERTICAL_HEIGHT) as i32,
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
        window.set_opacity(opacity);
        let backdrop = if ui_mode == "table" {
            BackdropType::Acrylic
        } else {
            BackdropType::None
        };
        window.set_backdrop(backdrop, dark);
        let sheet = style_sheet_for(&ui_mode, dark);
        window.set_style_sheet(sheet);
        qtrs_widgets::application::Application::set_style_sheet(sheet);

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
        ghost_label.borrow_mut().set_visible(ct);

        let mut btn = Button::new("⇄");
        btn.set_object_name("LayoutToggleBtn");
        let layout_toggle_btn = make_widget(btn);
        layout_toggle_btn.borrow().set_visible(ui_mode != "table");

        let mut time_lbl = Label::new("--:--:--");
        time_lbl.set_object_name("HeaderStatus");
        let time_label = make_widget(time_lbl);
        // Header layout
        let mut header_layout = BoxLayout::horizontal();
        header_layout.set_spacing(6);
        header_layout.add_widget(status_dot.clone());
        header_layout.add_widget(title_label.clone());
        header_layout.add_widget(ghost_label.clone());
        header_layout.add_widget(layout_toggle_btn.clone());
        header_layout.add_stretch(1);
        header_layout.add_widget(time_label.clone());

        // Python's header is a bare `QHBoxLayout` with no size policy; the default (Preferred) lets
        // the root layout hand any extra height to it, as `qGeomCalc` does for the Python layout.
        let header_widget = make_widget(EmptyWidget::new());
        header_widget
            .borrow_mut()
            .set_layout(Box::new(header_layout));

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
        stack.borrow_mut().set_size_policy(Self::stack_policy(ui_mode == "table"));
        if let Some(s) = stack
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<StackedWidget>()
        {
            s.add_widget(cards_container.clone());
            s.add_widget(table.widget());
            let active_index = if ui_mode == "table" { 1 } else { 0 };
            s.set_current_index(active_index);
        }

        // Root layout
        let mut root_layout = BoxLayout::vertical();
        root_layout.set_margins(Margins::new(12, 8, 12, 10));
        root_layout.set_spacing(6);
        root_layout.add_widget(header_widget);
        root_layout.add_widget(stack.clone());
        let root = window.root_widget();
        install_panel_painter(&root, &theme, Arc::clone(&config));
        root.borrow_mut().set_layout(Box::new(root_layout));
        // Connect frameless window dragging and edge resizing
        let hwnd = window.native_handle();
        let cfg_move = Arc::clone(&config);
        window.set_mouse_move_handler(move |pos| {
            let locked = cfg_move.lock().locked;
            let edges = qtrs_platform::window::calc_frameless_edge(hwnd, pos, locked);
            let shape = if edges.is_empty() {
                qtrs_platform::cursor::CursorShape::Arrow
            } else if edges == qtrs_platform::platform_window::WindowEdges::LEFT
                || edges == qtrs_platform::platform_window::WindowEdges::RIGHT
            {
                qtrs_platform::cursor::CursorShape::SizeHor
            } else if edges == qtrs_platform::platform_window::WindowEdges::TOP
                || edges == qtrs_platform::platform_window::WindowEdges::BOTTOM
            {
                qtrs_platform::cursor::CursorShape::SizeVer
            } else if edges == qtrs_platform::platform_window::WindowEdges::TOP_LEFT
                || edges == qtrs_platform::platform_window::WindowEdges::BOTTOM_RIGHT
            {
                qtrs_platform::cursor::CursorShape::SizeFDiag
            } else {
                qtrs_platform::cursor::CursorShape::SizeBDiag
            };
            #[cfg(windows)]
            qtrs_platform::cursor::win32_cursor::Win32Cursor::set_shape(shape);
            #[cfg(not(windows))]
            let _ = shape;
        });

        let cfg_press = Arc::clone(&config);
        window.set_mouse_press_handler(move |pos, button| {
            if button == qtrs_platform::MouseButton::Left {
                let locked = cfg_press.lock().locked;
                let edges = qtrs_platform::window::calc_frameless_edge(hwnd, pos, locked);
                if !edges.is_empty() {
                    qtrs_platform::window::post_system_resize(hwnd, edges);
                    return true;
                } else if !locked {
                    qtrs_platform::window::post_system_move(hwnd);
                    return true;
                }
            }
            false
        });
        let cfg_resize = Arc::clone(&config);
        let debouncer_resize = Arc::clone(&debouncer);
        window.set_resize_handler(move |size| {
            if debouncer_resize.is_restoring() {
                return;
            }
            {
                let mut cfg = cfg_resize.lock();
                if cfg.ui_mode == "table" {
                    cfg.table_width = size.width as u32;
                    cfg.table_height = size.height as u32;
                } else if cfg.layout_mode == "horizontal" {
                    cfg.horizontal_width = size.width as u32;
                    cfg.horizontal_height = size.height as u32;
                } else {
                    cfg.vertical_width = size.width as u32;
                    cfg.vertical_height = size.height as u32;
                }
            }
            debouncer_resize.request_save();
        });
        {
            let mut ctrl = refresh_ctrl.lock();
            for (id, provider) in &providers {
                ctrl.states.insert(id.clone(), Default::default());
                ctrl.launch(id, Arc::clone(provider));
            }
        }

        let hud = Self {
            config,
            refresh_ctrl,
            providers,
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
        };
        debouncer.set_restoring(false);

        Ok(hud)
    }

    /// Python puts the table or the cards container straight into the root layout, so the layout
    /// item behaves like that page: the cards container is `Preferred` (extra height goes to the
    /// header, `hud_window.py:141-145,308`), while the table's grid makes its item expand and take
    /// all the extra height (measured: 462 px of a 500 px window). The stack stands in for both.
    fn stack_policy(table: bool) -> qtrs_widgets::QSizePolicy {
        let p = if table { qtrs_widgets::Policy::Expanding } else { qtrs_widgets::Policy::Preferred };
        qtrs_widgets::QSizePolicy::new(p, p)
    }

    fn apply_cards_layout_inner(
        container: &WidgetRef,
        cards: &HashMap<String, ProviderCardWidget>,
        layout_mode: &str,
    ) {
        let is_horizontal = layout_mode == "horizontal";
        let mut layout: Box<dyn Layout> = if is_horizontal {
            // `body_layout.setSpacing(8)` (`hud_window.py:337`); the vertical layout keeps Qt's default.
            let mut body = BoxLayout::horizontal();
            body.set_spacing(8);
            Box::new(body)
        } else {
            Box::new(BoxLayout::vertical())
        };

        // Card order of the Python HUD (`provider_ids` in `_apply_cards_layout`); the table
        // mode has its own order (`usage_table::PROVIDER_ORDER`).
        const CARDS_ORDER: [&str; 3] = ["claude", "agy", "codex"];
        for (idx, pid) in CARDS_ORDER.iter().enumerate() {
            if let Some(card) = cards.get(*pid) {
                if idx > 0 {
                    if is_horizontal {
                        let mut div = qtrs_widgets::frame::Frame::new();
                        div.set_object_name("Divider");
                        div.set_frame_shape(qtrs_widgets::frame::FrameShape::VLine);
                        layout.add_widget(make_widget(div));
                    } else {
                        let mut h_div = qtrs_widgets::frame::Frame::new();
                        h_div.set_object_name("HorizontalDivider");
                        h_div.set_frame_shape(qtrs_widgets::frame::FrameShape::HLine);
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
        container.borrow_mut().set_layout(layout);
    }

    pub fn show(&mut self) {
        self.window.show();
    }

    pub fn hide(&mut self) {
        self.debouncer.flush();
        self.persist_geometry();
        self.window.hide();
        crate::memory::trim_memory();
    }

    #[allow(dead_code)]
    pub fn close(&mut self) {
        self.debouncer.flush();
        self.persist_geometry();
        self.window.hide();
        crate::memory::trim_memory();
    }

    pub fn persist_geometry(&self) {
        if self.debouncer.is_restoring() {
            return;
        }
        let geom = self.window.geometry();
        // SAVE_LOCK -> Config lock; Config lock is released before the file write.
        self.debouncer.update_and_save_now(|cfg| {
            cfg.window_x = Some(geom.x);
            cfg.window_y = Some(geom.y);
            if cfg.ui_mode == "table" {
                cfg.table_width = (geom.width as u32).max(MIN_TABLE_WIDTH);
                cfg.table_height = (geom.height as u32).max(MIN_TABLE_HEIGHT);
            } else if cfg.layout_mode == "horizontal" {
                cfg.horizontal_width = (geom.width as u32).max(MIN_HORIZONTAL_WIDTH);
                cfg.horizontal_height = (geom.height as u32).max(MIN_HORIZONTAL_HEIGHT);
            } else {
                cfg.vertical_width = (geom.width as u32).max(MIN_VERTICAL_WIDTH);
                cfg.vertical_height = (geom.height as u32).max(MIN_VERTICAL_HEIGHT);
            }
        });
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
        self.ghost_label.borrow_mut().set_visible(enabled);
        self.config.lock().click_through = enabled;
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
            cfg.window_x = Some(geom.x);
            cfg.window_y = Some(geom.y);
            if old_mode == "table" {
                cfg.table_width = (geom.width as u32).max(MIN_TABLE_WIDTH);
                cfg.table_height = (geom.height as u32).max(MIN_TABLE_HEIGHT);
            } else if old_layout == "horizontal" {
                cfg.horizontal_width = (geom.width as u32).max(MIN_HORIZONTAL_WIDTH);
                cfg.horizontal_height = (geom.height as u32).max(MIN_HORIZONTAL_HEIGHT);
            } else {
                cfg.vertical_width = (geom.width as u32).max(MIN_VERTICAL_WIDTH);
                cfg.vertical_height = (geom.height as u32).max(MIN_VERTICAL_HEIGHT);
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
        qtrs_widgets::application::Application::set_style_sheet(sheet);
        let (w, h) = if mode == "table" {
            if let Some(s) = self
                .stack
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<StackedWidget>()
            {
                s.set_current_index(1);
            }
            self.stack.borrow_mut().set_size_policy(Self::stack_policy(true));
            set_label_text(&self.title_label, "AI AGENT HUD (TABLE)");
            self.layout_toggle_btn.borrow().set_visible(false);
            self.window
                .set_minimum_size(MIN_TABLE_WIDTH as i32, MIN_TABLE_HEIGHT as i32);
            let cfg = self.config.lock();
            (
                cfg.table_width.max(MIN_TABLE_WIDTH) as i32,
                cfg.table_height.max(MIN_TABLE_HEIGHT) as i32,
            )
        } else {
            if let Some(s) = self
                .stack
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<StackedWidget>()
            {
                s.set_current_index(0);
            }
            self.stack.borrow_mut().set_size_policy(Self::stack_policy(false));
            set_label_text(&self.title_label, "AI AGENT HUD (3-IN-1)");
            self.layout_toggle_btn.borrow().set_visible(true);
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
        let backdrop = if mode == "table" {
            BackdropType::Acrylic
        } else {
            BackdropType::None
        };
        self.window.set_backdrop(backdrop, self.is_dark);

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
        self.window.render_and_present();
    }

    pub fn apply_cards_layout_mode(&mut self, mode: &str) {
        let (old_mode, old_layout) = {
            let cfg = self.config.lock();
            (cfg.ui_mode.clone(), cfg.layout_mode.clone())
        };
        let geom = self.window.geometry();
        crate::config::ConfigManager::update_and_save(&self.config, |cfg| {
            cfg.window_x = Some(geom.x);
            cfg.window_y = Some(geom.y);
            if old_mode == "table" {
                cfg.table_width = (geom.width as u32).max(MIN_TABLE_WIDTH);
                cfg.table_height = (geom.height as u32).max(MIN_TABLE_HEIGHT);
            } else if old_layout == "horizontal" {
                cfg.horizontal_width = (geom.width as u32).max(MIN_HORIZONTAL_WIDTH);
                cfg.horizontal_height = (geom.height as u32).max(MIN_HORIZONTAL_HEIGHT);
            } else {
                cfg.vertical_width = (geom.width as u32).max(MIN_VERTICAL_WIDTH);
                cfg.vertical_height = (geom.height as u32).max(MIN_VERTICAL_HEIGHT);
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
        let backdrop = if ui_mode == "table" {
            BackdropType::Acrylic
        } else {
            BackdropType::None
        };
        self.window.set_backdrop(backdrop, dark);
        let sheet = style_sheet_for(&ui_mode, dark);
        self.window.set_style_sheet(sheet);
        qtrs_widgets::application::Application::set_style_sheet(sheet);

        for card in self.cards.values_mut() {
            card.set_appearance(dark);
        }

        let scheme = { self.config.lock().color_scheme.clone() };
        self.table.set_theme(self.theme.clone(), &scheme);
        let root = self.window.root_widget();
        install_panel_painter(&root, &self.theme, Arc::clone(&self.config));
        self.window.render_and_present();
    }

    pub fn on_data_fetched(&mut self, data: &UsageMetrics) {
        if let Some(card) = self.cards.get_mut(&data.provider_id) {
            card.update_metrics(data);
        }
        self.table.update_metrics(data);
        // Python `_on_data_fetched`: the header time is the time of the last fetch, and the
        // status dot is amber while any provider reports an error, green otherwise.
        set_label_text(&self.time_label, Local::now().format("%H:%M:%S").to_string());
        self.refresh_status_dot();
        self.cards_container.borrow().update_layout();
        self.window.render_and_present();
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
            self.window.render_and_present();
        }
    }

    fn refresh_status_dot(&self) {
        let any_error = self.cards.values().any(|card| card.current_metrics.error.is_some());
        set_status_dot_color(&self.status_dot, status_dot_color(self.busy, any_error));
    }

    pub fn update_clock(&mut self) {
        for card in self.cards.values_mut() {
            card.update_countdown();
        }
        self.table.update_countdowns();
        self.window.render_and_present();
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
        self.window.set_opacity(val);
        self.window.render_and_present();
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
        let screen = qtrs_platform::platform().primary_screen();
        let (nx, ny) = super::placement::reset_position(w, screen.available_geometry());

        self.window.set_geometry(Rect::new(nx, ny, w, h));
        self.persist_geometry();
        self.window.render_and_present();
    }
}

/// Drop guarantees flush and worker thread join during normal object destruction and clean shutdown.
/// Note: Drop cannot run during abnormal termination (e.g. process::abort, SIGKILL, TerminateProcess,
/// unhandled access violations, OS crash, or power loss).
impl Drop for HUDWindow {
    fn drop(&mut self) {
        self.debouncer.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refresh_controller::RefreshController;

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
        let container = make_widget(qtrs_widgets::EmptyWidget::new());
        HUDWindow::apply_cards_layout_inner(&container, &cards, "horizontal");
        container.borrow_mut().set_geometry(qtrs_gui::geometry::primitives::Rect::new(12, 32, 666, 103));
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
        let hud = HUDWindow::with_providers(cfg, ctrl, crate::providers::stub::stub_providers()).unwrap();
        hud.window.root_widget().borrow().update_layout();

        let root = hud.window.root_widget();
        let header = root.borrow().children()[0].clone();
        assert_eq!(header.borrow().geometry().height, 86, "header row height (PySide6: 86)");
        assert_eq!(hud.title_label.borrow().geometry().height, 86, "title label height (PySide6: 86)");
        // RC-19 / G12.8.g: `max-height: 18px` is the content box, so the button is 22 high (PySide6:
        // `layout_toggle_btn` [175, 40, 28, 22] in the window, i.e. y 32 inside the header at y 8).
        let toggle = hud.layout_toggle_btn.borrow().geometry();
        assert_eq!((toggle.y, toggle.width, toggle.height), (32, 28, 22), "layout toggle button (PySide6: y 32, 28x22)");
        let heights: Vec<i32> = ["claude", "agy", "codex"]
            .iter()
            .map(|id| hud.cards[*id].widget().borrow().geometry().height)
            .collect();
        assert_eq!(heights, [109, 109, 109], "card heights (PySide6: 109 each)");
        let ys: Vec<i32> = ["claude", "agy", "codex"]
            .iter()
            .map(|id| hud.cards[*id].widget().borrow().geometry().y)
            .collect();
        assert_eq!(ys, [0, 122, 244], "card y inside the container (PySide6: 100/222/344 in the window)");
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
        let hud = HUDWindow::with_providers(cfg, ctrl, crate::providers::stub::stub_providers()).unwrap();
        hud.window.root_widget().borrow().update_layout();

        assert_eq!(hud.title_label.borrow().geometry().height, 14, "title label height (PySide6: 14)");
        assert_eq!(hud.stack.borrow().geometry().height, 462, "table height (PySide6: 462)");
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
        let mut hud = HUDWindow::with_providers(cfg, ctrl, crate::providers::stub::stub_providers()).unwrap();

        hud.apply_ui_mode("table");
        hud.window.root_widget().borrow().update_layout();
        assert_eq!(hud.title_label.borrow().geometry().height, 14, "table mode title height (PySide6: 14)");
        assert_eq!(hud.stack.borrow().geometry().height, 462, "table mode table height (PySide6: 462)");

        hud.apply_ui_mode("cards");
        hud.window.root_widget().borrow().update_layout();
        assert_eq!(hud.title_label.borrow().geometry().height, 86, "cards mode title height (PySide6: 86)");
    }

    /// TI-01: a window built with injected providers fetches from exactly those providers — one
    /// background fetch each — so tests never reach the live endpoints.
    #[test]
    fn test_hud_window_launches_only_the_injected_providers() {
        let cfg = Arc::new(Mutex::new(Config::default()));
        let refresh_ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
        let (providers, fetched) = crate::providers::stub::stub_providers_with_receiver();

        let hud = HUDWindow::with_providers(cfg, refresh_ctrl, providers).expect("with_providers failed");

        let mut ids: Vec<String> = (0..crate::providers::PROVIDER_IDS.len())
            .map(|_| fetched.recv_timeout(std::time::Duration::from_secs(10)).expect("stub was not fetched"))
            .collect();
        ids.sort();
        assert_eq!(ids, ["agy", "claude", "codex"]);
        let mut held: Vec<&String> = hud.providers.keys().collect();
        held.sort();
        assert_eq!(held, ["agy", "claude", "codex"]);
    }

    #[test]
    fn test_hud_window_init_does_not_trigger_save() {
        let cfg = Arc::new(Mutex::new(Config::default()));
        let refresh_ctrl = Arc::new(Mutex::new(RefreshController::new(60)));

        let hud = HUDWindow::with_providers(Arc::clone(&cfg), refresh_ctrl, crate::providers::stub::stub_providers()).expect("HUDWindow::new failed");

        // Debouncer was in restoring mode during HUDWindow::new
        // It must have prevented any saves during init
        assert_eq!(hud.debouncer.save_count(), 0);
        assert!(!hud.debouncer.is_restoring());
    }

    #[test]
    fn test_hud_window_close_flushes_pending_resize() {
        let cfg = Arc::new(Mutex::new(Config::default()));
        let refresh_ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
        let mut hud =
            HUDWindow::with_providers(Arc::clone(&cfg), refresh_ctrl, crate::providers::stub::stub_providers()).expect("HUDWindow::new failed");

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
        let hud = HUDWindow::with_providers(Arc::clone(&cfg), refresh_ctrl, crate::providers::stub::stub_providers()).expect("HUDWindow::new failed");
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
        use qtrs_widgets::style::stylesheet::{QStyleSheetStyle, WidgetStyleContext};
        use qtrs_widgets::Label;

        let cfg = Arc::new(Mutex::new(Config::default()));
        let refresh_ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
        let mut hud = HUDWindow::with_providers(Arc::clone(&cfg), refresh_ctrl, crate::providers::stub::stub_providers()).expect("HUDWindow::new failed");

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
        assert_ne!(colours[0], colours[1], "the two sheets must style the title differently");
    }
}
