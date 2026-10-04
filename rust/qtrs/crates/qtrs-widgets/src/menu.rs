//! Popup menus with keyboard navigation and cascading sub-menus (`QMenu`).

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use qtrs_core::event::{Event, EventKind};
use qtrs_core::object::{ObjectData, ObjectId, QObject};
use qtrs_core::signal::Signal;
use qtrs_gui::geometry::primitives::{Point, PointF, Rect, RectF, Size};
use qtrs_gui::image::{Icon, IconMode, IconState};
use qtrs_gui::paint::brush::Brush;
use qtrs_gui::paint::painter::{Painter, Pen};
use qtrs_gui::text::{Font, FontMetrics};
use qtrs_gui::tiny_skia::Color;

use crate::action::{keys, Action, ActionRef, ActionWeak};
use crate::focus::FocusPolicy;
use crate::layout::Layout;
use crate::size_policy::{Policy, QSizePolicy};
use crate::widget::{Widget, WidgetBase, WidgetRef, WidgetWeak};

/// Shared handle to a [`Menu`]; sub-menus, menu bars and tool bar extensions hold menus this way.
pub type MenuRef = Rc<RefCell<Menu>>;

const FRAME: i32 = 1;
const V_PADDING: i32 = 4;
const SEPARATOR_HEIGHT: i32 = 9;
const CHECK_COLUMN: i32 = 28;
const SHORTCUT_GAP: i32 = 28;
const ARROW_COLUMN: i32 = 20;
const RIGHT_PADDING: i32 = 8;
const MIN_WIDTH: i32 = 120;
const SUBMENU_OVERLAP: i32 = 2;
const ICON_SIZE: i32 = 16;
/// Width and height of the check indicator box (`QWindows11Style::pixelMetric(PM_IndicatorWidth)`).
/// A styled menu with checkable items reserves this plus [`CHECK_GAP`] before the text
/// (`QStyleSheetStyle::sizeFromContents`, `CT_MenuItem`), and draws the text this far from the
/// item's padding edge (`drawControl`, `CE_MenuItem`: `textRectOffset`).
const CHECK_INDICATOR: i32 = 16;
/// Extra width `QStyleSheetStyle` adds after the check indicator in `CT_MenuItem`.
const CHECK_GAP: i32 = 4;

/// Box model and palette of a style-sheet-driven menu (`QMenu`, `QMenu::item`,
/// `QMenu::separator` rules). Without it a menu keeps its built-in look.
///
/// Geometry follows `QStyleSheetStyle`: a row is as tall as the *menu font* (`row_font`,
/// not the item's `font-size`) plus the item padding, a menu is as wide as the widest text
/// in `row_font` plus left/right padding and 20 px for the check column when any item is
/// checkable, and the first row starts below the 1 px frame and the `QMenu` padding.
#[derive(Clone, Debug)]
pub struct MenuStyle {
    /// Font the item text is drawn with (the `QMenu::item` `font-size`).
    pub font: Font,
    /// The menu's own font, which sizes rows and text widths.
    pub row_font: Font,
    pub background: Color,
    pub border: Color,
    pub text: Color,
    pub disabled_text: Color,
    pub hover_background: Color,
    pub hover_text: Color,
    pub separator: Color,
    pub radius: f32,
    /// `QMenu { padding }` above the first and below the last row.
    pub padding_v: i32,
    /// `QMenu::item { padding }` as top, right, bottom, left.
    pub item_padding: [i32; 4],
    /// `QMenu::separator { margin }` as vertical, horizontal; the line itself is 1 px.
    pub separator_margin: [i32; 2],
}

/// Result of routing one input event through a chain of open menus.
pub(crate) enum MenuOutcome {
    /// The event was not used by the menu chain.
    Ignored,
    /// The event was consumed.
    Handled,
    /// The receiving menu closed itself (Escape or click outside).
    Closed,
    /// Left was pressed in the deepest menu with no sub-menu to close.
    NavigateLeft,
    /// Right was pressed on an item without a sub-menu.
    NavigateRight,
    /// An action was chosen. All menus up to the receiver are already hidden; the
    /// signals are the `triggered` signals of those menus, deepest first.
    Activate(ActionRef, Vec<Signal<ActionRef>>),
}

impl MenuOutcome {
    /// Triggers `action` and then emits every collected menu `triggered` signal.
    pub(crate) fn fire(action: &ActionRef, signals: &[Signal<ActionRef>]) {
        Action::trigger(action);
        for signal in signals {
            signal.emit(action);
        }
    }
}

/// Popup menu (`QMenu`).
///
/// A menu paints and routes events for its open sub-menu itself, so a whole cascade
/// behaves as one widget. Sub-menu geometry is relative to the parent menu's origin.
pub struct Menu {
    base: WidgetBase,
    title: String,
    icon: Icon,
    actions: Vec<ActionRef>,
    active: Option<usize>,
    open_submenu: Option<(usize, MenuRef)>,
    menu_action: ActionWeak,
    popup_bounds: Option<Rect>,
    last_triggered: Option<ActionRef>,
    /// Previous pointer position over this menu, used to tell whether the pointer is heading
    /// for the open submenu. Cleared when the pointer leaves or enters a submenu.
    last_mouse_pos: Option<Point>,
    last_covered: std::cell::Cell<Rect>,
    font: Font,
    style: Option<MenuStyle>,

    background_color: Color,
    border_color: Color,
    text_color: Color,
    disabled_text_color: Color,
    highlight_color: Color,
    highlight_text_color: Color,
    separator_color: Color,

    /// Emitted with the action chosen in this menu or one of its sub-menus.
    pub triggered: Signal<ActionRef>,
    /// Emitted with the action highlighted by mouse or keyboard.
    pub hovered: Signal<ActionRef>,
    /// Emitted just before the menu is shown.
    pub about_to_show: Signal<()>,
    /// Emitted just before the menu is hidden.
    pub about_to_hide: Signal<()>,
}

pub type QMenu = Menu;

impl Menu {
    /// Creates a hidden menu with the given title (used by menu bars and parent menus).
    pub fn new(title: impl Into<String>) -> Self {
        let base = WidgetBase::new();
        base.set_visible(false);
        base.clear_dirty();
        base.set_focus_policy(FocusPolicy::StrongFocus);
        base.set_size_policy(QSizePolicy::new(Policy::Fixed, Policy::Fixed));
        base.set_geometry(Rect::new(0, 0, MIN_WIDTH, 2 * V_PADDING));
        Self {
            base,
            title: title.into(),
            icon: Icon::default(),
            actions: Vec::new(),
            active: None,
            open_submenu: None,
            menu_action: Weak::new(),
            popup_bounds: None,
            last_triggered: None,
            last_mouse_pos: None,
            last_covered: std::cell::Cell::new(Rect::default()),
            font: Font::new("Segoe UI, Microsoft JhengHei, Segoe UI Emoji", 12.0),
            style: None,

            background_color: Color::from_rgba8(22, 25, 32, 250),
            border_color: Color::from_rgba8(255, 255, 255, 46),
            text_color: Color::from_rgba8(226, 232, 240, 255),
            disabled_text_color: Color::from_rgba8(100, 116, 139, 255),
            highlight_color: Color::from_rgba8(39, 47, 61, 255),
            highlight_text_color: Color::from_rgba8(56, 189, 248, 255),
            separator_color: Color::from_rgba8(255, 255, 255, 31),

            triggered: Signal::new(),
            hovered: Signal::new(),
            about_to_show: Signal::new(),
            about_to_hide: Signal::new(),
        }
    }

    /// Creates a new shared menu.
    pub fn new_ref(title: impl Into<String>) -> MenuRef {
        Rc::new(RefCell::new(Self::new(title)))
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// Sets the title and keeps the menu action text in sync.
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
        if let Some(action) = self.menu_action.upgrade() {
            if let Ok(mut a) = action.try_borrow_mut() {
                a.set_text(self.title.clone());
            }
        }
        self.update();
    }
    /// Sets dark or light mode palette matching Qt / Python style.
    pub fn set_dark_mode(&mut self, dark: bool) {
        if dark {
            self.background_color = Color::from_rgba8(22, 25, 32, 250);
            self.border_color = Color::from_rgba8(255, 255, 255, 46);
            self.text_color = Color::from_rgba8(226, 232, 240, 255);
            self.disabled_text_color = Color::from_rgba8(100, 116, 139, 255);
            self.highlight_color = Color::from_rgba8(39, 47, 61, 255);
            self.highlight_text_color = Color::from_rgba8(56, 189, 248, 255);
            self.separator_color = Color::from_rgba8(255, 255, 255, 31);
        } else {
            self.background_color = Color::from_rgba8(250, 250, 250, 255);
            self.border_color = Color::from_rgba8(160, 160, 160, 255);
            self.text_color = Color::from_rgba8(20, 20, 20, 255);
            self.disabled_text_color = Color::from_rgba8(160, 160, 160, 255);
            self.highlight_color = Color::from_rgba8(0, 120, 215, 255);
            self.highlight_text_color = Color::from_rgba8(255, 255, 255, 255);
            self.separator_color = Color::from_rgba8(210, 210, 210, 255);
        }
        for action in &self.actions {
            if let Some(sub) = action.borrow().menu() {
                if let Ok(mut s) = sub.try_borrow_mut() {
                    s.set_dark_mode(dark);
                }
            }
        }
        self.update();
    }

    pub fn is_dark_mode(&self) -> bool {
        self.background_color.red() < 0.5
    }
    pub fn icon(&self) -> &Icon {
        &self.icon
    }

    pub fn set_icon(&mut self, icon: Icon) {
        self.icon = icon.clone();
        if let Some(action) = self.menu_action.upgrade() {
            if let Ok(mut a) = action.try_borrow_mut() {
                a.set_icon(icon);
            }
        }
    }

    pub fn font(&self) -> &Font {
        &self.font
    }

    pub fn set_font(&mut self, font: Font) {
        self.font = font;
        self.update();
    }

    /// Applies a style-sheet-like look (colours, fonts and box model) to this menu and, when
    /// they open, its sub-menus.
    pub fn set_style(&mut self, style: MenuStyle) {
        self.font = style.font.clone();
        self.background_color = style.background;
        self.border_color = style.border;
        self.text_color = style.text;
        self.disabled_text_color = style.disabled_text;
        self.highlight_color = style.hover_background;
        self.highlight_text_color = style.hover_text;
        self.separator_color = style.separator;
        self.style = Some(style);
        self.update();
    }

    fn top_inset(&self) -> i32 {
        self.style.as_ref().map_or(V_PADDING, |s| FRAME + s.padding_v)
    }

    fn separator_height(&self) -> i32 {
        self.style
            .as_ref()
            .map_or(SEPARATOR_HEIGHT, |s| 1 + 2 * s.separator_margin[0])
    }

    fn has_checkable(&self) -> bool {
        self.actions.iter().any(|a| {
            let a = a.borrow();
            a.is_visible() && !a.is_separator() && a.is_checkable()
        })
    }

    /// X of the item text in menu coordinates.
    fn text_x(&self) -> f32 {
        match &self.style {
            Some(s) => {
                (FRAME + s.item_padding[3] + if self.has_checkable() { CHECK_INDICATOR } else { 0 }) as f32
            }
            None => CHECK_COLUMN as f32,
        }
    }

    fn measure_font(&self) -> &Font {
        self.style.as_ref().map_or(&self.font, |s| &s.row_font)
    }

    fn width_for(&self, max_text: i32, shortcut_w: i32) -> i32 {
        match &self.style {
            Some(s) => {
                2 * FRAME
                    + s.item_padding[1]
                    + s.item_padding[3]
                    + if self.has_checkable() { CHECK_INDICATOR + CHECK_GAP } else { 0 }
                    + max_text
                    + shortcut_w
            }
            None => (CHECK_COLUMN + max_text + shortcut_w + ARROW_COLUMN + RIGHT_PADDING)
                .max(MIN_WIDTH),
        }
    }

    /// Returns the action that represents `menu` inside a menu bar or parent menu
    /// (`QMenu::menuAction`), creating it on first use.
    ///
    /// The owner of the returned action keeps it alive; the menu holds only a weak link.
    pub fn menu_action(menu: &MenuRef) -> ActionRef {
        if let Some(action) = menu.borrow().menu_action.upgrade() {
            return action;
        }
        let (title, icon) = {
            let m = menu.borrow();
            (m.title.clone(), m.icon.clone())
        };
        let action = Action::new_ref(title);
        {
            let mut a = action.borrow_mut();
            a.set_icon(icon);
            a.set_menu(Some(menu.clone()));
        }
        menu.borrow_mut().menu_action = Rc::downgrade(&action);
        action
    }

    /// Appends an action; an action already in the menu is moved to the end.
    pub fn add_action(&mut self, action: ActionRef) -> ActionRef {
        self.remove_action(&action);
        self.actions.push(action.clone());
        self.update();
        action
    }

    /// Creates an action with `text` and appends it (`QMenu::addAction(text)`).
    pub fn add_new_action(&mut self, text: impl Into<String>) -> ActionRef {
        self.add_action(Action::new_ref(text))
    }

    /// Appends a separator action.
    pub fn add_separator(&mut self) -> ActionRef {
        self.add_action(Action::separator_ref())
    }

    /// Creates a sub-menu with `title`, appends its menu action and returns it.
    pub fn add_menu(&mut self, title: impl Into<String>) -> MenuRef {
        let submenu = Menu::new_ref(title);
        self.add_menu_ref(&submenu);
        submenu
    }

    /// Appends the menu action of an existing menu.
    pub fn add_menu_ref(&mut self, menu: &MenuRef) -> ActionRef {
        let action = Menu::menu_action(menu);
        self.add_action(action)
    }

    /// Inserts an action before `index` (clamped to the end).
    pub fn insert_action(&mut self, index: usize, action: ActionRef) -> ActionRef {
        self.remove_action(&action);
        let index = index.min(self.actions.len());
        self.actions.insert(index, action.clone());
        self.close_submenu();
        self.active = None;
        self.update();
        action
    }

    /// Removes an action from the menu.
    pub fn remove_action(&mut self, action: &ActionRef) {
        if let Some(pos) = self.actions.iter().position(|a| Rc::ptr_eq(a, action)) {
            if self.open_submenu.as_ref().is_some_and(|(i, _)| *i == pos) {
                self.close_submenu();
            }
            self.actions.remove(pos);
            if let Some((i, _)) = self.open_submenu.as_mut() {
                if *i > pos {
                    *i -= 1;
                }
            }
            self.active = match self.active {
                Some(a) if a == pos => None,
                Some(a) if a > pos => Some(a - 1),
                other => other,
            };
            self.update();
        }
    }

    /// Removes all actions.
    pub fn clear(&mut self) {
        self.close_submenu();
        self.actions.clear();
        self.active = None;
        self.update();
    }

    pub fn actions(&self) -> Vec<ActionRef> {
        self.actions.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Currently highlighted action.
    pub fn active_action(&self) -> Option<ActionRef> {
        self.active.and_then(|i| self.actions.get(i).cloned())
    }

    /// Highlights an action (or clears the highlight), closing an unrelated open sub-menu.
    pub fn set_active_action(&mut self, action: Option<&ActionRef>) {
        let idx = action.and_then(|a| self.actions.iter().position(|x| Rc::ptr_eq(x, a)));
        if self
            .open_submenu
            .as_ref()
            .is_some_and(|(i, _)| Some(*i) != idx)
        {
            self.close_submenu();
        }
        self.set_active_index(idx);
    }

    /// Currently open sub-menu, if any.
    pub fn open_submenu(&self) -> Option<MenuRef> {
        self.open_submenu.as_ref().map(|(_, m)| m.clone())
    }

    /// Action chosen the last time this menu (or a sub-menu) was activated.
    pub fn last_triggered_action(&self) -> Option<ActionRef> {
        self.last_triggered.clone()
    }

    /// Area, in the coordinate space of this menu's geometry, the popup must stay inside.
    pub fn popup_bounds(&self) -> Option<Rect> {
        self.popup_bounds
    }

    pub fn set_popup_bounds(&mut self, bounds: Option<Rect>) {
        self.popup_bounds = bounds;
    }

    /// Computes the geometry `popup(pos)` would use, keeping the menu inside the popup bounds.
    ///
    /// Like `QMenu::popup`, a menu overflowing the bottom edge opens upwards from `pos`
    /// when there is room, and a menu overflowing the right edge is shifted left.
    pub fn popup_geometry(&self, pos: Point) -> Rect {
        let size = self.size_hint();
        let mut x = pos.x;
        let mut y = pos.y;
        if let Some(b) = self.popup_bounds {
            if x + size.width > b.right() {
                x = b.right() - size.width;
            }
            if y + size.height > b.bottom() {
                y = if pos.y - size.height >= b.y {
                    pos.y - size.height
                } else {
                    b.bottom() - size.height
                };
            }
            x = x.max(b.x);
            y = y.max(b.y);
        }
        Rect::new(x, y, size.width, size.height)
    }

    /// Shows the menu at `pos` (`QMenu::popup`).
    pub fn popup(&mut self, pos: Point) {
        self.popup_at(pos, None);
    }

    /// Shows the menu at `pos` with `at` highlighted.
    pub fn popup_at(&mut self, pos: Point, at: Option<&ActionRef>) {
        self.about_to_show.emit(&());
        let geometry = self.popup_geometry(pos);
        self.show_with_geometry(geometry);
        if let Some(action) = at {
            let idx = self.actions.iter().position(|a| Rc::ptr_eq(a, action));
            self.set_active_index(idx);
        }
    }

    /// Runs the menu modally (`QMenu::exec`): pops it up at `pos`, feeds it `events`
    /// until it closes, and returns the triggered action.
    ///
    /// `events` may be any (possibly blocking) event source, e.g. a platform event pump.
    pub fn exec<I>(&mut self, pos: Point, events: I) -> Option<ActionRef>
    where
        I: IntoIterator<Item = Event>,
    {
        self.last_triggered = None;
        self.popup(pos);
        for mut event in events {
            self.event(&mut event);
            if !self.base.is_visible() {
                break;
            }
        }
        if self.base.is_visible() {
            self.hide_menu();
        }
        self.last_triggered.clone()
    }
    /// Pops up the menu as a top-level transparent frameless layered window and runs
    /// modally until an action is selected or dismissed (mirrors `QMenu::exec`).
    #[cfg(windows)]
    pub fn exec_popup(&mut self, pos: Point) -> Option<ActionRef> {
        use crate::window::Window;
        use qtrs_platform::{platform, WindowFlags};
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            ReleaseCapture, SetCapture, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN, VK_RIGHT, VK_UP,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, LoadCursorW, PeekMessageW, SetCursor,
            SetForegroundWindow, TranslateMessage, IDC_ARROW, MSG, PM_REMOVE,
            WM_ACTIVATEAPP, WM_CANCELMODE, WM_CAPTURECHANGED, WM_KEYDOWN,
            WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
            WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR,
        };

        self.last_triggered = None;
        let size = self.size_hint();
        let screen = platform().primary_screen();
        let s_geom = screen.available_geometry();
        const DESKTOP_MARGIN: i32 = 8;

        let mut x = pos.x;
        let mut y = pos.y;
        if x + size.width > s_geom.right() - DESKTOP_MARGIN {
            x = (s_geom.right() - size.width - DESKTOP_MARGIN).max(s_geom.x + DESKTOP_MARGIN);
        }
        if y + size.height > s_geom.bottom() - DESKTOP_MARGIN {
            y = if pos.y - size.height >= s_geom.y + DESKTOP_MARGIN {
                (pos.y - size.height).min(s_geom.bottom() - size.height - DESKTOP_MARGIN)
            } else {
                (s_geom.bottom() - size.height - DESKTOP_MARGIN).max(s_geom.y + DESKTOP_MARGIN)
            };
        }
        x = x.max(s_geom.x + DESKTOP_MARGIN);
        y = y.max(s_geom.y + DESKTOP_MARGIN);

        let root_origin = Point::new(x, y);
        let initial_rect = Rect::new(x, y, size.width, size.height);
        self.popup_bounds = Some(s_geom.translated(-root_origin.x, -root_origin.y));
        self.popup_at(Point::new(0, 0), None);

        let mut window = match Window::new(
            "",
            initial_rect,
            WindowFlags::FRAMELESS | WindowFlags::TOOL | WindowFlags::STAYS_ON_TOP | WindowFlags::LAYERED,
        ) {
            Ok(w) => w,
            Err(_) => return None,
        };

        let hwnd = window.native_handle() as HWND;
        unsafe {
            SetForegroundWindow(hwnd);
            SetCapture(hwnd);
        }

        window.show();
        window.present_custom(|painter| {
            self.paint_event(painter);
        });

        let mut chosen = None;
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) != 0 {
                let dpr = platform().primary_screen().device_pixel_ratio();
                let mouse_pt = Point::new(msg.pt.x, msg.pt.y);
                let mouse_screen = if dpr > 1.0 {
                    qtrs_platform::high_dpi::from_native_point(mouse_pt, dpr)
                } else {
                    mouse_pt
                };
                let local_pt = Point::new(mouse_screen.x - root_origin.x, mouse_screen.y - root_origin.y);
                match msg.message {
                    WM_SETCURSOR => {
                        SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_ARROW));
                    }
                    WM_RBUTTONUP => {
                        if !self.covers(local_pt) {
                            break;
                        }
                    }
                    WM_MOUSEMOVE => {
                        let old_signature = self.hover_signature();
                        let outcome = self.handle_mouse_move(local_pt);
                        let new_signature = self.hover_signature();

                        if old_signature != new_signature {
                            let covered = self.covered_rect();
                            let win_x = root_origin.x + covered.x;
                            let win_y = root_origin.y + covered.y;
                            let win_w = covered.width.max(size.width);
                            let win_h = covered.height.max(size.height);
                            let current_log = window.geometry();
                            if current_log.x != win_x || current_log.y != win_y || current_log.width != win_w || current_log.height != win_h {
                                window.set_geometry_silent(Rect::new(win_x, win_y, win_w, win_h));
                            }
                            window.present_custom(|painter| {
                                painter.save();
                                painter.translate(-covered.x as f32, -covered.y as f32);
                                self.paint_event(painter);
                                painter.restore();
                            });
                        }

                        if let MenuOutcome::Activate(act, _) = outcome {
                            chosen = Some(act);
                            break;
                        }
                    }
                    WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
                        if !self.covers(local_pt) {
                            break;
                        }
                        let outcome = self.handle_mouse_press(local_pt, 1);
                        if let MenuOutcome::Closed = outcome {
                            break;
                        }
                        let covered = self.covered_rect();
                        let win_x = root_origin.x + covered.x;
                        let win_y = root_origin.y + covered.y;
                        let win_w = covered.width.max(size.width);
                        let win_h = covered.height.max(size.height);
                        let current_log = window.geometry();
                        if current_log.x != win_x || current_log.y != win_y || current_log.width != win_w || current_log.height != win_h {
                            window.set_geometry_silent(Rect::new(win_x, win_y, win_w, win_h));
                        }
                        window.present_custom(|painter| {
                            painter.save();
                            painter.translate(-covered.x as f32, -covered.y as f32);
                            self.paint_event(painter);
                            painter.restore();
                        });
                    }
                    WM_LBUTTONUP => {
                        if !self.covers(local_pt) {
                            break;
                        }
                        let outcome = self.handle_mouse_release(local_pt, 1);
                        match outcome {
                            MenuOutcome::Activate(act, _) => {
                                chosen = Some(act);
                                break;
                            }
                            MenuOutcome::Closed => break,
                            _ => {
                                let covered = self.covered_rect();
                                window.present_custom(|painter| {
                                    painter.save();
                                    painter.translate(-covered.x as f32, -covered.y as f32);
                                    self.paint_event(painter);
                                    painter.restore();
                                });
                            }
                        }
                    }
                    WM_KEYDOWN => {
                        match msg.wParam as u16 {
                            VK_ESCAPE => break,
                            VK_UP => {
                                self.handle_key(keys::UP, 0);
                            }
                            VK_DOWN => {
                                self.handle_key(keys::DOWN, 0);
                            }
                            VK_LEFT => {
                                self.handle_key(keys::LEFT, 0);
                            }
                            VK_RIGHT => {
                                self.handle_key(keys::RIGHT, 0);
                            }
                            VK_RETURN => {
                                let outcome = self.handle_key(keys::RETURN, 0);
                                if let MenuOutcome::Activate(act, _) = outcome {
                                    chosen = Some(act);
                                    break;
                                }
                            }
                            _ => {}
                        }
                        let covered = self.covered_rect();
                        let win_x = root_origin.x + covered.x;
                        let win_y = root_origin.y + covered.y;
                        let win_w = covered.width.max(size.width);
                        let win_h = covered.height.max(size.height);
                        let current_log = window.geometry();
                        if current_log.x != win_x || current_log.y != win_y || current_log.width != win_w || current_log.height != win_h {
                            window.set_geometry_silent(Rect::new(win_x, win_y, win_w, win_h));
                        }
                        window.present_custom(|painter| {
                            painter.save();
                            painter.translate(-covered.x as f32, -covered.y as f32);
                            self.paint_event(painter);
                            painter.restore();
                        });
                    }
                    WM_KILLFOCUS => {
                        // Window lost keyboard/window focus (Win+L, Alt+Tab, clicked outside)
                        break;
                    }
                    WM_ACTIVATEAPP => {
                        // Entire application deactivated (wParam == 0 / FALSE)
                        if msg.wParam == 0 {
                            break;
                        }
                    }
                    WM_CAPTURECHANGED => {
                        // Mouse capture revoked by OS (UAC secure desktop, external capture steal)
                        if msg.lParam as HWND != hwnd {
                            break;
                        }
                    }
                    WM_CANCELMODE => {
                        // Windows or host requests cancellation of internal modal loops
                        break;
                    }
                    _ => {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }

                if !self.base.is_visible() {
                    break;
                }
            }
            ReleaseCapture();
            // Discard any residual mouse button messages queued during the modal menu
            let mut discard_msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut discard_msg, std::ptr::null_mut(), WM_LBUTTONDOWN, WM_RBUTTONUP, PM_REMOVE) != 0 {}
        }
        self.hide_menu();
        chosen
    }
    #[cfg(not(windows))]
    pub fn exec_popup(&mut self, pos: Point) -> Option<ActionRef> {
        self.popup(pos);
        self.last_triggered.clone()
    }

    /// Hides the menu and every open sub-menu.
    pub fn hide_menu(&mut self) {
        if let Some((_, submenu)) = self.open_submenu.take() {
            if let Ok(mut s) = submenu.try_borrow_mut() {
                s.hide_menu();
            }
        }
        if !self.base.is_visible() {
            return;
        }
        self.about_to_hide.emit(&());
        self.base.set_visible(false);
        self.active = None;
        self.update();
    }

    /// Geometry of an action's item in menu coordinates.
    pub fn action_geometry(&self, action: &ActionRef) -> Option<Rect> {
        let idx = self.actions.iter().position(|a| Rc::ptr_eq(a, action))?;
        self.item_rects().get(idx).copied()
    }

    /// Action whose item contains `pos` (menu coordinates).
    pub fn action_at(&self, pos: Point) -> Option<ActionRef> {
        self.index_at(pos)
            .and_then(|i| self.actions.get(i).cloned())
    }

    fn item_height(&self) -> i32 {
        if let Some(s) = &self.style {
            let h = FontMetrics::from_font(&s.row_font).height.ceil() as i32;
            return h + s.item_padding[0] + s.item_padding[2];
        }
        let metrics = FontMetrics::from_font(&self.font);
        (metrics.height.ceil() as i32 + 8).max(24)
    }

    fn item_rects(&self) -> Vec<Rect> {
        let width = self.base.geometry().width - 2 * FRAME;
        let item_h = self.item_height();
        let mut y = self.top_inset();
        self.actions
            .iter()
            .map(|action| {
                let a = action.borrow();
                if !a.is_visible() {
                    return Rect::new(FRAME, y, 0, 0);
                }
                let h = if a.is_separator() {
                    self.separator_height()
                } else {
                    item_h
                };
                let rect = Rect::new(FRAME, y, width, h);
                y += h;
                rect
            })
            .collect()
    }

    fn index_at(&self, pos: Point) -> Option<usize> {
        self.item_rects().iter().position(|r| r.contains(pos))
    }

    fn is_selectable(&self, idx: usize) -> bool {
        self.actions.get(idx).is_some_and(|a| {
            let a = a.borrow();
            a.is_visible() && !a.is_separator() && a.is_enabled()
        })
    }

    fn has_submenu(&self, idx: usize) -> bool {
        self.actions
            .get(idx)
            .is_some_and(|a| a.borrow().menu().is_some())
    }

    fn set_active_index(&mut self, idx: Option<usize>) {
        if self.active == idx {
            return;
        }
        self.active = idx;
        if let Some(action) = idx.and_then(|i| self.actions.get(i).cloned()) {
            Action::hover(&action);
            self.hovered.emit(&action);
        }
        self.update();
    }

    fn step_active(&mut self, forward: bool) {
        let n = self.actions.len();
        if n == 0 {
            return;
        }
        let start = self.active.unwrap_or(if forward { n - 1 } else { 0 });
        for step in 1..=n {
            let i = if forward {
                (start + step) % n
            } else {
                (start + n - step) % n
            };
            if self.is_selectable(i) {
                self.set_active_index(Some(i));
                return;
            }
        }
    }

    /// Highlights the first selectable item (used when opening via keyboard).
    pub(crate) fn select_first(&mut self) {
        if let Some(i) = (0..self.actions.len()).find(|&i| self.is_selectable(i)) {
            self.set_active_index(Some(i));
        }
    }

    fn select_last(&mut self) {
        if let Some(i) = (0..self.actions.len())
            .rev()
            .find(|&i| self.is_selectable(i))
        {
            self.set_active_index(Some(i));
        }
    }

    /// Shows the menu at `geometry` without emitting `about_to_show` (callers do).
    pub(crate) fn show_with_geometry(&mut self, geometry: Rect) {
        self.close_submenu();
        self.active = None;
        self.base.set_geometry(geometry);
        self.base.set_visible(true);
        self.update();
    }

    fn open_submenu_at(&mut self, idx: usize, select_first: bool) {
        let Some(action) = self.actions.get(idx).cloned() else {
            return;
        };
        let (submenu, enabled) = {
            let a = action.borrow();
            (a.menu(), a.is_enabled())
        };
        let Some(submenu) = submenu else {
            return;
        };
        if let Some((open_idx, open)) = &self.open_submenu {
            if *open_idx == idx {
                if select_first {
                    open.borrow_mut().select_first();
                }
                return;
            }
        }
        self.close_submenu();
        if !enabled {
            return;
        }
        let item = self.item_rects()[idx];
        let geom = self.base.geometry();
        let local_bounds = self.popup_bounds.map(|b| b.translated(-geom.x, -geom.y));
        {
            let mut sub = submenu.borrow_mut();
            sub.background_color = self.background_color;
            sub.border_color = self.border_color;
            sub.text_color = self.text_color;
            sub.disabled_text_color = self.disabled_text_color;
            sub.highlight_color = self.highlight_color;
            sub.highlight_text_color = self.highlight_text_color;
            sub.separator_color = self.separator_color;
            sub.font = self.font.clone();
            sub.style = self.style.clone();
            sub.about_to_show.emit(&());
            sub.popup_bounds = local_bounds;
            let size = sub.size_hint();
            let mut x = geom.width - SUBMENU_OVERLAP;
            let mut y = item.y - self.top_inset();
            if let Some(b) = local_bounds {
                if x + size.width > b.right() {
                    x = (SUBMENU_OVERLAP - size.width).max(b.x);
                }
                if y + size.height > b.bottom() {
                    y = (b.bottom() - size.height).max(b.y);
                }
                y = y.max(b.y);
            }
            sub.popup_bounds = local_bounds.map(|b| b.translated(-x, -y));
            sub.show_with_geometry(Rect::new(x, y, size.width, size.height));
            if select_first {
                sub.select_first();
            }
        }
        self.open_submenu = Some((idx, submenu));
        self.update();
    }

    fn close_submenu(&mut self) {
        if let Some((_, submenu)) = self.open_submenu.take() {
            if let Ok(mut s) = submenu.try_borrow_mut() {
                s.hide_menu();
            }
            self.update();
        }
    }

    /// Chooses the item at `idx`: opens its sub-menu or hides the menu and reports activation.
    fn activate_index(&mut self, idx: usize) -> MenuOutcome {
        let Some(action) = self.actions.get(idx).cloned() else {
            return MenuOutcome::Handled;
        };
        let (enabled, has_menu, separator) = {
            let a = action.borrow();
            (a.is_enabled(), a.menu().is_some(), a.is_separator())
        };
        if !enabled || separator {
            return MenuOutcome::Handled;
        }
        if has_menu {
            self.set_active_index(Some(idx));
            self.open_submenu_at(idx, true);
            return MenuOutcome::Handled;
        }
        self.hide_menu();
        self.last_triggered = Some(action.clone());
        MenuOutcome::Activate(action, vec![self.triggered.clone()])
    }

    /// Converts the outcome reported by the open sub-menu into this menu's outcome.
    fn outcome_from_child(&mut self, outcome: MenuOutcome) -> MenuOutcome {
        match outcome {
            MenuOutcome::Ignored => MenuOutcome::Ignored,
            MenuOutcome::Handled => {
                self.update();
                MenuOutcome::Handled
            }
            MenuOutcome::Closed | MenuOutcome::NavigateLeft => {
                self.close_submenu();
                MenuOutcome::Handled
            }
            MenuOutcome::NavigateRight => MenuOutcome::NavigateRight,
            MenuOutcome::Activate(action, mut signals) => {
                self.open_submenu = None;
                self.hide_menu();
                self.last_triggered = Some(action.clone());
                signals.push(self.triggered.clone());
                MenuOutcome::Activate(action, signals)
            }
        }
    }

    /// Returns true if `pos` (menu coordinates) is inside this menu or an open sub-menu.
    pub(crate) fn covers(&self, pos: Point) -> bool {
        if !self.base.is_visible() {
            return false;
        }
        let g = self.base.geometry();
        if Rect::new(0, 0, g.width, g.height).contains(pos) {
            return true;
        }
        self.open_submenu.as_ref().is_some_and(|(_, submenu)| {
            submenu.try_borrow().is_ok_and(|s| {
                let sg = s.base.geometry();
                s.covers(Point::new(pos.x - sg.x, pos.y - sg.y))
            })
        })
    }

    /// Bounding rectangle of this menu and its open sub-menus, in menu coordinates.
    pub fn covered_rect(&self) -> Rect {
        let g = self.base.geometry();
        let mut rect = Rect::new(0, 0, g.width, g.height);
        if let Some((_, submenu)) = &self.open_submenu {
            if let Ok(s) = submenu.try_borrow() {
                let sg = s.base.geometry();
                rect = rect.united(&s.covered_rect().translated(sg.x, sg.y));
            }
        }
        rect
    }
    /// Visual hover signature tracking the active item and open submenu at every hierarchy level.
    pub fn hover_signature(&self) -> Vec<(Option<usize>, Option<usize>)> {
        let mut sig = vec![(self.active, self.open_submenu.as_ref().map(|(i, _)| *i))];
        let mut curr = self.open_submenu.as_ref().map(|(_, s)| s.clone());
        while let Some(sub_rc) = curr {
            if let Ok(s) = sub_rc.try_borrow() {
                sig.push((s.active, s.open_submenu.as_ref().map(|(i, _)| *i)));
                curr = s.open_submenu.as_ref().map(|(_, next)| next.clone());
            } else {
                break;
            }
        }
        sig
    }

    /// Handles mouse motion at `pos` in menu coordinates.
    pub fn handle_mouse_move_at(&mut self, pos: Point) {
        self.handle_mouse_move(pos);
    }

    fn route_to_submenu<F>(&mut self, pos: Point, f: F) -> Option<MenuOutcome>
    where
        F: FnOnce(&mut Menu, Point) -> MenuOutcome,
    {
        let (_, submenu) = self.open_submenu.clone()?;
        let origin = {
            let s = submenu.borrow();
            let sg = s.base.geometry();
            if !s.covers(Point::new(pos.x - sg.x, pos.y - sg.y)) {
                return None;
            }
            sg
        };
        let outcome = f(
            &mut submenu.borrow_mut(),
            Point::new(pos.x - origin.x, pos.y - origin.y),
        );
        Some(self.outcome_from_child(outcome))
    }

    /// Keyboard handling (`QMenu::keyPressEvent`); the deepest open sub-menu gets the key first.
    pub(crate) fn handle_key(&mut self, key: u32, modifiers: u32) -> MenuOutcome {
        let key = keys::normalize_key(key);
        let modifiers = keys::normalize_modifiers(modifiers);
        if let Some((_, submenu)) = self.open_submenu.clone() {
            let outcome = submenu.borrow_mut().handle_key(key, modifiers);
            return self.outcome_from_child(outcome);
        }
        match key {
            keys::UP => {
                self.step_active(false);
                MenuOutcome::Handled
            }
            keys::DOWN => {
                self.step_active(true);
                MenuOutcome::Handled
            }
            keys::HOME | keys::PAGE_UP => {
                self.select_first();
                MenuOutcome::Handled
            }
            keys::END | keys::PAGE_DOWN => {
                self.select_last();
                MenuOutcome::Handled
            }
            keys::LEFT => MenuOutcome::NavigateLeft,
            keys::RIGHT => match self.active {
                Some(idx) if self.has_submenu(idx) && self.is_selectable(idx) => {
                    self.open_submenu_at(idx, true);
                    MenuOutcome::Handled
                }
                _ => MenuOutcome::NavigateRight,
            },
            keys::RETURN | keys::ENTER | keys::SPACE => match self.active {
                Some(idx) => self.activate_index(idx),
                None => MenuOutcome::Handled,
            },
            keys::ESCAPE => {
                self.hide_menu();
                MenuOutcome::Closed
            }
            _ => self.handle_mnemonic(key, modifiers),
        }
    }

    /// Mnemonic handling: a unique match is activated, clashing matches are cycled.
    fn handle_mnemonic(&mut self, key: u32, modifiers: u32) -> MenuOutcome {
        let allowed = modifiers & !(keys::MOD_ALT | keys::MOD_SHIFT) == 0;
        let Some(c) = keys::key_char(key).filter(|_| allowed) else {
            return MenuOutcome::Ignored;
        };
        let matches: Vec<usize> = (0..self.actions.len())
            .filter(|&i| {
                let a = self.actions[i].borrow();
                a.is_visible() && !a.is_separator() && a.mnemonic() == Some(c)
            })
            .collect();
        let Some(&first) = matches.first() else {
            return MenuOutcome::Ignored;
        };
        let next = match self.active {
            Some(current) if matches.len() > 1 => matches
                .iter()
                .copied()
                .find(|&i| i > current)
                .unwrap_or(first),
            _ => first,
        };
        if matches.len() == 1 {
            return self.activate_index(next);
        }
        self.set_active_index(Some(next));
        MenuOutcome::Handled
    }

    /// True when the pointer moved from `from` to `pos` heading for the open submenu, i.e. `pos`
    /// lies inside the wedge spanned by `from` and the submenu's near edge.
    ///
    /// Mirrors Qt's `QMenuSloppyState` (qmenu.cpp): the submenu is not abandoned while the
    /// pointer crosses neighbouring items on its way to it. The decision depends on the direction
    /// of travel, so moving straight along the parent menu, or away from the submenu, selects the
    /// item under the pointer as usual (a plain bounding rectangle would swallow every row that
    /// lies within the submenu's vertical extent).
    fn is_moving_towards_submenu(&self, from: Point, pos: Point) -> bool {
        let Some((idx, submenu)) = &self.open_submenu else {
            return false;
        };
        let Ok(sub) = submenu.try_borrow() else {
            return false;
        };
        let rects = self.item_rects();
        let Some(&item) = rects.get(*idx) else {
            return false;
        };
        let sg = sub.base.geometry();

        // `side` is +1 when the submenu opens to the right, -1 when it opens to the left; all
        // x distances below are measured towards the submenu.
        let (side, edge_x) = if sg.x >= item.right() - SUBMENU_OVERLAP - 4 {
            (1i64, sg.x)
        } else {
            (-1i64, sg.x + sg.width)
        };
        let to_edge = side * (edge_x - from.x) as i64;
        let dx = side * (pos.x - from.x) as i64;
        if to_edge <= 0 || dx <= 0 {
            return false;
        }
        let dy = (pos.y - from.y) as i64;
        let top = (sg.y - from.y) as i64;
        let bottom = (sg.y + sg.height - from.y) as i64;
        // cross((to_edge, top), (dx, dy)) >= 0 && cross((dx, dy), (to_edge, bottom)) >= 0
        to_edge * dy - top * dx >= 0 && dx * bottom - dy * to_edge >= 0
    }

    pub(crate) fn handle_mouse_move(&mut self, pos: Point) -> MenuOutcome {
        if let Some(outcome) = self.route_to_submenu(pos, |s, p| s.handle_mouse_move(p)) {
            self.last_mouse_pos = None;
            return outcome;
        }
        // Parity with Qt QMenu::mouseMoveEvent (qmenu.cpp:3511):
        // When mouse is inside this menu, clear active hover selection on open submenus
        if let Some((_, submenu)) = &self.open_submenu {
            if let Ok(mut sub) = submenu.try_borrow_mut() {
                sub.set_active_index(None);
            }
        }
        // While the pointer travels diagonally towards the open submenu, keep the submenu and
        // the active item (mirrors Qt's sloppyState). Any other move selects the item under it.
        let from = self.last_mouse_pos.replace(pos);
        if from.is_some_and(|from| self.is_moving_towards_submenu(from, pos)) {
            return MenuOutcome::Handled;
        }
        if let Some(idx) = self.index_at(pos) {
            if self.is_selectable(idx) && self.active != Some(idx) {
                self.set_active_index(Some(idx));
                if self.has_submenu(idx) {
                    self.open_submenu_at(idx, false);
                } else {
                    self.close_submenu();
                }
            }
            return MenuOutcome::Handled;
        }
        if self.covers(pos) {
            MenuOutcome::Handled
        } else {
            MenuOutcome::Ignored
        }
    }

    pub(crate) fn handle_mouse_press(&mut self, pos: Point, _button: u32) -> MenuOutcome {
        if let Some(outcome) = self.route_to_submenu(pos, |s, p| s.handle_mouse_press(p, _button)) {
            return outcome;
        }
        if !self.covers(pos) {
            self.hide_menu();
            return MenuOutcome::Closed;
        }
        if let Some(idx) = self.index_at(pos) {
            if self.is_selectable(idx) && self.has_submenu(idx) {
                if self.open_submenu.as_ref().is_some_and(|(i, _)| *i == idx) {
                    self.close_submenu();
                } else {
                    self.set_active_index(Some(idx));
                    self.open_submenu_at(idx, false);
                }
            }
        }
        MenuOutcome::Handled
    }

    pub(crate) fn handle_mouse_release(&mut self, pos: Point, _button: u32) -> MenuOutcome {
        if let Some(outcome) = self.route_to_submenu(pos, |s, p| s.handle_mouse_release(p, _button))
        {
            return outcome;
        }
        if let Some(idx) = self.index_at(pos) {
            if self.is_selectable(idx) && !self.has_submenu(idx) {
                return self.activate_index(idx);
            }
            return MenuOutcome::Handled;
        }
        if self.covers(pos) {
            MenuOutcome::Handled
        } else {
            MenuOutcome::Ignored
        }
    }

    pub(crate) fn handle_leave(&mut self) {
        self.last_mouse_pos = None;
        if self.open_submenu.is_none() {
            self.set_active_index(None);
        }
    }

    fn finish_top_level(&mut self, outcome: MenuOutcome) -> bool {
        match outcome {
            MenuOutcome::Ignored => false,
            MenuOutcome::Activate(action, signals) => {
                MenuOutcome::fire(&action, &signals);
                true
            }
            MenuOutcome::Handled
            | MenuOutcome::Closed
            | MenuOutcome::NavigateLeft
            | MenuOutcome::NavigateRight => true,
        }
    }

    fn paint_item(&self, painter: &mut Painter, action: &Action, rect: Rect, active: bool) {
        let metrics = FontMetrics::from_font(&self.font);
        let width = self.base.geometry().width;
        if action.is_separator() {
            let (x0, x1, line_y) = match &self.style {
                Some(s) => (
                    (FRAME + s.separator_margin[1]) as f32,
                    (width - FRAME - s.separator_margin[1]) as f32,
                    (rect.y + s.separator_margin[0]) as f32 + 0.5,
                ),
                None => (
                    CHECK_COLUMN as f32 - 4.0,
                    (width - RIGHT_PADDING) as f32,
                    rect.y as f32 + rect.height as f32 / 2.0,
                ),
            };
            painter.set_pen(Pen::new(self.separator_color, 1.0));
            painter.draw_line(PointF::new(x0, line_y), PointF::new(x1, line_y));
            return;
        }
        let enabled = action.is_enabled();
        if active && enabled {
            painter.set_brush(Brush::Color(self.highlight_color));
            painter.set_pen(None);
            let (hl, radius) = if self.style.is_some() {
                (rect, 0.0)
            } else {
                (Rect::new(rect.x + 3, rect.y, rect.width - 6, rect.height), 4.0)
            };
            painter.draw_rounded_rect(
                RectF::new(hl.x as f32, hl.y as f32, hl.width as f32, hl.height as f32),
                radius,
                radius,
            );
        }
        let color = if !enabled {
            self.disabled_text_color
        } else if active {
            self.highlight_text_color
        } else {
            self.text_color
        };
        let mid_y = rect.y as f32 + rect.height as f32 / 2.0;
        let text_x = self.text_x();

        if action.is_checkable() && action.is_checked() {
            if self.style.is_some() && !action.is_exclusive_in_group() {
                // The glyph is centred in the indicator box at the item's left edge
                // (`positionRect`: AlignLeft | AlignVCenter, padding origin). Qt draws the
                // Fluent check-mark font glyph; this stroke imitates its shape and weight.
                let u = CHECK_INDICATOR as f32 / 16.0;
                let cx = FRAME as f32 + CHECK_INDICATOR as f32 / 2.0;
                painter.set_pen(Pen::new(color, 1.6));
                painter.draw_line(
                    PointF::new(cx - 3.5 * u, mid_y + 0.5 * u),
                    PointF::new(cx - 1.0 * u, mid_y + 3.0 * u),
                );
                painter.draw_line(
                    PointF::new(cx - 1.0 * u, mid_y + 3.0 * u),
                    PointF::new(cx + 3.5 * u, mid_y - 3.0 * u),
                );
            } else {
                let check_color = if active {
                    self.highlight_text_color
                } else {
                    Color::from_rgba8(56, 189, 248, 255)
                };
                if action.is_exclusive_in_group() {
                    painter.set_brush(Brush::Color(check_color));
                    painter.set_pen(None);
                    painter.draw_ellipse(RectF::new(10.0, mid_y - 4.0, 8.0, 8.0));
                } else {
                    painter.set_pen(Pen::new(check_color, 2.0));
                    painter.draw_line(PointF::new(9.0, mid_y), PointF::new(12.5, mid_y + 3.5));
                    painter.draw_line(
                        PointF::new(12.5, mid_y + 3.5),
                        PointF::new(19.0, mid_y - 4.0),
                    );
                }
            }
        } else if !action.icon().is_null() {
            let mode = if enabled {
                IconMode::Normal
            } else {
                IconMode::Disabled
            };
            let pixmap =
                action
                    .icon()
                    .pixmap(Size::new(ICON_SIZE, ICON_SIZE), mode, IconState::Off);
            painter.draw_pixmap(
                RectF::new(
                    6.0,
                    mid_y - ICON_SIZE as f32 / 2.0,
                    ICON_SIZE as f32,
                    ICON_SIZE as f32,
                ),
                &pixmap,
                None,
            );
        }

        let text = action.display_text();
        let baseline = mid_y - metrics.height / 2.0 + metrics.ascent;
        painter.draw_text_colored(
            PointF::new(text_x, baseline),
            &text,
            &self.font,
            color,
        );
        if let Some(offset) = keys::mnemonic_offset(action.text()) {
            let prefix_w = metrics.horizontal_advance(&text[..offset], &self.font);
            let ch: String = text[offset..].chars().take(1).collect();
            let ch_w = metrics.horizontal_advance(&ch, &self.font);
            let underline_y = baseline + 2.0;
            painter.set_pen(Pen::new(color, 1.0));
            painter.draw_line(
                PointF::new(text_x + prefix_w, underline_y),
                PointF::new(text_x + prefix_w + ch_w, underline_y),
            );
        }

        let shortcut = action.shortcut();
        if !shortcut.is_empty() {
            let sc_text = shortcut.to_string();
            let sc_w = metrics.horizontal_advance(&sc_text, &self.font);
            let x = (width - ARROW_COLUMN) as f32 - sc_w;
            painter.draw_text_colored(PointF::new(x, baseline), &sc_text, &self.font, color);
        }

        if action.menu().is_some() {
            if self.style.is_some() {
                // `QStyleSheetStyle` derives the arrow box from the row: a square of half the
                // row height, centred `dim` px left of the item's right edge. Qt draws the
                // Fluent chevron glyph in it; this stroke imitates its shape and weight.
                let dim = (rect.height / 2) as f32;
                let cx = (width - FRAME - 1) as f32 - dim;
                let half = dim * 0.3;
                painter.set_pen(Pen::new(color, 1.3));
                painter.draw_line(
                    PointF::new(cx - half / 2.0, mid_y - half),
                    PointF::new(cx + half / 2.0, mid_y),
                );
                painter.draw_line(
                    PointF::new(cx + half / 2.0, mid_y),
                    PointF::new(cx - half / 2.0, mid_y + half),
                );
            } else {
                let ax = (width - 14) as f32;
                painter.set_pen(Pen::new(color, 1.5));
                painter.draw_line(PointF::new(ax, mid_y - 4.0), PointF::new(ax + 4.0, mid_y));
                painter.draw_line(PointF::new(ax + 4.0, mid_y), PointF::new(ax, mid_y + 4.0));
            }
        }
    }
}

impl Default for Menu {
    fn default() -> Self {
        Self::new("")
    }
}

impl QObject for Menu {
    fn object_data(&self) -> &ObjectData {
        &self.base.object_data
    }

    fn object_data_mut(&mut self) -> &mut ObjectData {
        &mut self.base.object_data
    }

    fn event(&mut self, event: &mut Event) -> bool {
        if !self.base.is_visible() {
            return false;
        }
        let outcome = match &event.kind {
            EventKind::KeyPress { key, modifiers, .. } => self.handle_key(*key, *modifiers),
            EventKind::MouseMove { x, y } => self.handle_mouse_move(Point::new(*x, *y)),
            EventKind::MouseButtonPress { x, y, button } => {
                self.handle_mouse_press(Point::new(*x, *y), *button)
            }
            EventKind::MouseButtonRelease { x, y, button } => {
                self.handle_mouse_release(Point::new(*x, *y), *button)
            }
            EventKind::Leave => {
                self.handle_leave();
                MenuOutcome::Handled
            }
            _ => return false,
        };
        self.finish_top_level(outcome)
    }

    fn as_qobject_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_qobject_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

impl Widget for Menu {
    fn id(&self) -> ObjectId {
        self.base.object_data.id
    }

    fn geometry(&self) -> Rect {
        self.base.geometry()
    }

    fn set_geometry(&self, rect: Rect) {
        self.base.set_geometry(rect);
    }

    fn size_hint(&self) -> Size {
        let measure = self.measure_font().clone();
        let metrics = FontMetrics::from_font(&measure);
        let item_h = self.item_height();
        let mut height = 2 * self.top_inset();
        let mut max_text = 0i32;
        let mut max_shortcut = 0i32;
        for action in &self.actions {
            let a = action.borrow();
            if !a.is_visible() {
                continue;
            }
            if a.is_separator() {
                height += self.separator_height();
                continue;
            }
            height += item_h;
            let text_w = metrics
                .horizontal_advance_exact(&a.display_text(), &measure)
                .ceil() as i32;
            max_text = max_text.max(text_w);
            if !a.shortcut().is_empty() {
                let sc_w = metrics
                    .horizontal_advance_exact(&a.shortcut().to_string(), &measure)
                    .ceil() as i32;
                max_shortcut = max_shortcut.max(sc_w);
            }
        }
        let shortcut_w = if max_shortcut > 0 {
            SHORTCUT_GAP + max_shortcut
        } else {
            0
        };
        let width = self.width_for(max_text, shortcut_w);
        Size::new(width, height)
    }

    fn minimum_size_hint(&self) -> Size {
        self.size_hint()
    }

    fn size_policy(&self) -> QSizePolicy {
        self.base.size_policy()
    }

    fn set_size_policy(&self, policy: QSizePolicy) {
        self.base.set_size_policy(policy);
    }

    fn is_visible(&self) -> bool {
        self.base.is_visible()
    }

    fn set_visible(&self, visible: bool) {
        if visible == self.base.is_visible() {
            return;
        }
        if visible {
            self.about_to_show.emit(&());
            let g = self.base.geometry();
            let size = self.size_hint();
            self.base.set_geometry(Rect::new(g.x, g.y, size.width, size.height));
            self.base.set_visible(true);
        } else {
            self.about_to_hide.emit(&());
            self.base.set_visible(false);
        }
        self.update();
    }

    fn is_enabled(&self) -> bool {
        self.base.is_enabled()
    }

    fn set_enabled(&self, enabled: bool) {
        self.base.set_enabled(enabled);
        self.update();
    }

    fn update(&self) {
        let covered = self.covered_rect();
        let old_covered = self.last_covered.get();
        self.base.dirty.set(Some(covered.united(&old_covered)));
        self.last_covered.set(covered);
        let target = self.base.window_id().unwrap_or(self.base.object_data.id);
        let _ = qtrs_core::event_loop::post_event_to_thread(
            qtrs_core::object::ThreadId::current(),
            target,
            Event::new(EventKind::UpdateRequest),
        );
    }

    fn dirty_rect(&self) -> Option<Rect> {
        self.base.dirty_rect()
    }

    fn clear_dirty(&self) {
        self.base.clear_dirty();
    }

    fn layout(&self) -> Option<&dyn Layout> {
        None
    }

    fn layout_mut(&mut self) -> Option<&mut Box<dyn Layout>> {
        None
    }

    fn set_layout(&mut self, _layout: Box<dyn Layout>) {}

    fn parent_widget(&self) -> Option<WidgetWeak> {
        self.base.parent_widget()
    }

    fn set_parent_widget(&self, parent: Option<WidgetWeak>) {
        self.base.set_parent_widget(parent);
    }

    fn window_id(&self) -> Option<ObjectId> {
        self.base.window_id()
    }

    fn set_window_id(&self, window_id: Option<ObjectId>) {
        self.base.set_window_id(window_id);
    }
    fn children(&self) -> Vec<WidgetRef> {
        Vec::new()
    }

    fn add_child(&mut self, _child: WidgetRef) {}

    fn remove_child(&mut self, _child_id: ObjectId) {}

    fn focus_policy(&self) -> FocusPolicy {
        self.base.focus_policy()
    }

    fn set_focus_policy(&self, policy: FocusPolicy) {
        self.base.set_focus_policy(policy);
    }

    fn has_focus(&self) -> bool {
        self.base.has_focus()
    }

    fn set_has_focus(&self, focus: bool) {
        self.base.set_has_focus(focus);
    }

    fn mouse_press_event(&mut self, pos: Point, button: u32, _modifiers: u32) {
        let outcome = self.handle_mouse_press(pos, button);
        self.finish_top_level(outcome);
    }

    fn mouse_release_event(&mut self, pos: Point, button: u32, _modifiers: u32) {
        let outcome = self.handle_mouse_release(pos, button);
        self.finish_top_level(outcome);
    }

    fn mouse_move_event(&mut self, pos: Point) {
        let outcome = self.handle_mouse_move(pos);
        self.finish_top_level(outcome);
    }

    fn leave_event(&mut self) {
        self.handle_leave();
    }

    fn key_press_event(&mut self, key: u32, modifiers: u32, _is_repeat: bool) {
        if self.base.is_visible() {
            let outcome = self.handle_key(key, modifiers);
            self.finish_top_level(outcome);
        }
    }

    fn paint_event(&mut self, painter: &mut Painter) {
        if !self.base.is_visible() {
            return;
        }
        let g = self.base.geometry();
        painter.set_brush(Brush::Color(self.background_color));
        painter.set_pen(Pen::new(self.border_color, 1.0));
        painter.draw_rounded_rect(
            RectF::new(
                0.5,
                0.5,
                g.width as f32 - 1.0,
                g.height as f32 - 1.0,
            ),
            self.style.as_ref().map_or(6.0, |s| s.radius),
            self.style.as_ref().map_or(6.0, |s| s.radius),
        );

        let rects = self.item_rects();
        for (i, action) in self.actions.iter().enumerate() {
            let rect = rects[i];
            if rect.width <= 0 || rect.height <= 0 {
                continue;
            }
            let a = action.borrow();
            self.paint_item(painter, &a, rect, self.active == Some(i));
        }

        if let Some((_, submenu)) = &self.open_submenu {
            if let Ok(mut sub) = submenu.try_borrow_mut() {
                let sg = sub.base.geometry();
                painter.save();
                painter.translate(sg.x as f32, sg.y as f32);
                sub.paint_event(painter);
                painter.restore();
            }
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
