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
use crate::providers::{AgyProvider, ClaudeProvider, CodexProvider, Provider};
use crate::refresh_controller::RefreshController;

fn make_widget<W: Widget + 'static>(w: W) -> WidgetRef {
    Rc::new(RefCell::new(Box::new(w)))
}

fn set_label_text(w: &WidgetRef, text: impl Into<String>) {
    if let Some(lbl) = w.borrow_mut().as_any_mut().downcast_mut::<Label>() {
        lbl.set_text(text);
    }
}

use crate::ui::set_label_color;

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

/// Background, border colour and corner radius of the window panel. In cards mode they come from
/// the application style sheet's `QWidget#CentralWidget` rule, as in the Python HUD; the table
/// mode (and a sheet without that rule) uses the theme.
fn panel_look(cards_mode: bool, theme: &Theme) -> (qtrs_gui::tiny_skia::Color, qtrs_gui::tiny_skia::Color, f32) {
    let themed = (theme.panel_bg, theme.panel_border, 10.0);
    if !cards_mode {
        return themed;
    }
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
    pub is_visible: bool,
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

impl HUDWindow {
    pub fn new(
        config: Arc<Mutex<Config>>,
        refresh_ctrl: Arc<Mutex<RefreshController>>,
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
            let raw_x = cfg.window_x.unwrap_or(100);
            let raw_y = cfg.window_y.unwrap_or(100);
            let screen = qtrs_platform::platform().primary_screen();
            let avail = screen.geometry();
            let mut cl_x = raw_x;
            let mut cl_y = raw_y;
            if cl_x + w > avail.right() {
                cl_x = (avail.right() - w).max(avail.x);
            }
            if cl_x < avail.x {
                cl_x = avail.x;
            }
            if cl_y + h > avail.bottom() {
                cl_y = (avail.bottom() - h).max(avail.y);
            }
            if cl_y < avail.y {
                cl_y = avail.y;
            }
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
        window.set_backdrop(BackdropType::Acrylic, dark);
        let sheet = crate::ui::styles::get_cards_stylesheet(dark);
        window.set_style_sheet(sheet);
        qtrs_widgets::application::Application::set_style_sheet(sheet);

        let theme = get_theme(dark);

        // Header bar widgets
        let dot = Label::new("●");
        let status_dot = make_widget(dot);
        set_label_color(&status_dot, status_dot_color(false, false));

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
        btn.set_font(qtrs_gui::text::font::Font::new("Segoe UI Symbol", 11.0));
        let layout_toggle_btn = make_widget(btn);

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

        let header_widget = make_widget(EmptyWidget::new());
        header_widget
            .borrow_mut()
            .set_size_policy(qtrs_widgets::QSizePolicy::new(
                qtrs_widgets::Policy::Expanding,
                qtrs_widgets::Policy::Fixed,
            ));
        header_widget
            .borrow_mut()
            .set_layout(Box::new(header_layout));

        // Page 0: Cards mode
        let cards_container = make_widget(EmptyWidget::new());
        cards_container
            .borrow_mut()
            .set_size_policy(qtrs_widgets::QSizePolicy::new(
                qtrs_widgets::Policy::Expanding,
                qtrs_widgets::Policy::Expanding,
            ));
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
        root_layout.set_margins(Margins::new(10, 8, 10, 8));
        root_layout.set_spacing(6);
        root_layout.add_widget(header_widget);
        root_layout.add_widget_with_stretch(stack.clone(), 1);
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
        // Initialize providers
        let providers: HashMap<String, Arc<dyn Provider + Send + Sync>> = HashMap::from([
            (
                "claude".to_string(),
                Arc::new(ClaudeProvider::with_config(Some(Arc::clone(&config))))
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
        ]);

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
            is_visible: true,
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

    fn apply_cards_layout_inner(
        container: &WidgetRef,
        cards: &HashMap<String, ProviderCardWidget>,
        layout_mode: &str,
    ) {
        let is_horizontal = layout_mode == "horizontal";
        let mut layout: Box<dyn Layout> = if is_horizontal {
            Box::new(BoxLayout::horizontal())
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
                layout.add_widget_with_stretch(card.widget(), 1);
            }
        }
        container.borrow_mut().set_layout(layout);
    }

    pub fn show(&mut self) {
        self.is_visible = true;
        self.window.show();
    }

    pub fn hide(&mut self) {
        self.is_visible = false;
        self.debouncer.flush();
        self.persist_geometry();
        self.window.hide();
        crate::memory::trim_memory();
    }

    #[allow(dead_code)]
    pub fn close(&mut self) {
        self.is_visible = false;
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
        if self.is_visible {
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
        let (w, h) = if mode == "table" {
            if let Some(s) = self
                .stack
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<StackedWidget>()
            {
                s.set_current_index(1);
            }
            set_label_text(&self.title_label, "AI AGENT HUD (TABLE)");
            self.layout_toggle_btn.borrow_mut().set_visible(false);
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
            set_label_text(&self.title_label, "AI AGENT HUD (3-IN-1)");
            self.layout_toggle_btn.borrow_mut().set_visible(true);
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

        let cur_geom = self.window.geometry();
        let (nx, ny) = self.ensure_within_screen(cur_geom.x, cur_geom.y, w, h);
        self.window.set_geometry(Rect::new(nx, ny, w, h));
        self.cards_container.borrow().update_layout();
        self.window.render_and_present();
    }

    pub fn ensure_within_screen(&self, x: i32, y: i32, w: i32, h: i32) -> (i32, i32) {
        let screen = qtrs_platform::platform().primary_screen();
        let avail = screen.geometry();

        let mut cur_x = x;
        let mut cur_y = y;

        if cur_x + w > avail.right() {
            cur_x = (avail.right() - w).max(avail.x);
        }
        if cur_x < avail.x {
            cur_x = avail.x;
        }

        if cur_y + h > avail.bottom() {
            cur_y = (avail.bottom() - h).max(avail.y);
        }
        if cur_y < avail.y {
            cur_y = avail.y;
        }

        (cur_x, cur_y)
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
        self.window.set_backdrop(BackdropType::Acrylic, dark);
        let sheet = crate::ui::styles::get_cards_stylesheet(dark);
        self.window.set_style_sheet(sheet);
        qtrs_widgets::application::Application::set_style_sheet(sheet);

        set_label_color(&self.title_label, self.theme.text);
        set_label_color(&self.time_label, self.theme.text2);

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
        set_label_color(&self.status_dot, status_dot_color(self.busy, any_error));
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

        let screen = qtrs_platform::platform().primary_screen();
        let screen_geom = screen.available_geometry();
        let target_x = screen_geom.x + screen_geom.width - w - 40;
        let target_y = screen_geom.y + 50;
        let (nx, ny) = self.ensure_within_screen(target_x, target_y, w, h);

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

    #[test]
    fn test_hud_window_init_does_not_trigger_save() {
        let cfg = Arc::new(Mutex::new(Config::default()));
        let refresh_ctrl = Arc::new(Mutex::new(RefreshController::new(60)));

        let hud = HUDWindow::new(Arc::clone(&cfg), refresh_ctrl).expect("HUDWindow::new failed");

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
            HUDWindow::new(Arc::clone(&cfg), refresh_ctrl).expect("HUDWindow::new failed");

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
        let hud = HUDWindow::new(Arc::clone(&cfg), refresh_ctrl).expect("HUDWindow::new failed");
        let debouncer = Arc::clone(&hud.debouncer);

        assert!(!debouncer.is_worker_joined());

        drop(hud);

        assert!(debouncer.is_worker_joined());
    }
}
