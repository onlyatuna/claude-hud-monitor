//! Tool tips: `QToolTip` and the application-level protocol that wakes one up.
//!
//! Qt shows a tool tip in two steps, and qtrs keeps them apart as Qt does.
//!
//! 1. `QApplication::notify` (qapplication.cpp:2712-2737): a spontaneous mouse move with no button
//!    down remembers the widget under the cursor and starts the *wake-up* timer, 700 ms
//!    (`SH_ToolTip_WakeUpDelay`, qcommonstyle.cpp:5348), or 20 ms while the *fall-asleep* timer
//!    (2000 ms, `SH_ToolTip_FallAsleepDelay`) is still running, so moving to the next widget after
//!    a tip was shown does not wait again. User input, window activation and `Leave` stop the
//!    timers (qapplication.cpp:2622-2637).
//! 2. When the wake-up timer fires (`QApplication::event`, qapplication.cpp:1710-1740) and the
//!    widget's window is the active window (or has `WA_AlwaysShowToolTips`), a `QHelpEvent`
//!    `ToolTip` is sent to the widget. It bubbles to the parents until one handles and accepts it
//!    (qapplication.cpp:2940-2962). `QWidget::event` answers by `QToolTip::showText` with the
//!    widget's own `toolTip`, or ignores the event when there is none (qwidget.cpp:9381-9386). An
//!    accepted tip starts the fall-asleep timer.
//!
//! `QToolTip::showText` (qtooltip.cpp) puts the text in a top-level, non-activating window
//! (`Qt::ToolTip`), positioned by [`place_tip`] and removed by an expire timer, by user input or
//! shortly after the cursor leaves.
//!
//! The 700 ms / 2 s / 300 ms / 10 s timers are the Qt tool tip protocol, not application debounce.
//! Application state is per thread, like `QApplicationPrivate`.

use crate::label::Label;
use crate::widget::{Widget, WidgetRef, WidgetWeak};
use crate::window::{window_shows_tool_tips, Window};
use qtrs_core::event::{Event, EventKind};
use qtrs_core::object::ObjectId;
use qtrs_core::timer::Timer;
use qtrs_gui::geometry::primitives::{Point, Rect, Size};
use qtrs_gui::paint::palette::{ColorGroup, ColorRole};
use qtrs_platform::window::WindowFlags;
use std::cell::{Cell, RefCell};

/// `SH_ToolTip_WakeUpDelay` (qcommonstyle.cpp:5348-5350).
pub const WAKE_UP_DELAY_MS: u64 = 700;
/// `SH_ToolTip_FallAsleepDelay` (qcommonstyle.cpp:5351-5353).
pub const FALL_ASLEEP_DELAY_MS: u64 = 2000;
/// The wake-up delay while the fall-asleep timer still runs (qapplication.cpp:2731).
pub const QUICK_WAKE_UP_DELAY_MS: u64 = 20;
/// `QTipLabel::hideTip` waits this long before hiding (qtooltip.cpp:176-180).
pub const HIDE_DELAY_MS: u64 = 300;
/// `QPlatformCursor::size` default (qplatformcursor.h), used for the tip's offset from the cursor.
pub const CURSOR_SIZE: Size = Size { width: 16, height: 16 };

/// `QTipLabel::restartExpireTimer` (qtooltip.cpp:117-126): how long a tip stays, in ms. `msec > 0`
/// is the caller's duration; otherwise `10000 + 40 * max(0, length - 100)`, the length being the
/// text's UTF-16 length like `QString::size`.
pub fn expire_time_ms(text: &str, msec: i32) -> u64 {
    if msec > 0 {
        return msec as u64;
    }
    let length = text.encode_utf16().count() as i64;
    10_000 + 40 * (length - 100).max(0) as u64
}

/// `QTipLabel::placeTip` (qtooltip.cpp:296-370): where the top-left corner of a tip of `tip_size`
/// goes for a cursor at the global position `pos` on a screen covering `screen`.
///
/// The tip sits right of and below the cursor (`2`, cursor height; beside the cursor when the
/// cursor is more than twice the tip's height). A tip that overflows the right edge flips to the
/// left of the cursor (`4 + width`) and one that overflows the bottom flips above it
/// (`24 + height`); what still overflows is then clamped into the screen.
pub fn place_tip(pos: Point, tip_size: Size, cursor_size: Size, screen: Rect) -> Point {
    let (w, h) = (tip_size.width, tip_size.height);
    let mut offset = Point::new(2, cursor_size.height);
    if cursor_size.height > 2 * h {
        offset = Point::new(cursor_size.width / 2, 0);
    }
    let mut p = Point::new(pos.x + offset.x, pos.y + offset.y);
    let right = screen.x + screen.width;
    let bottom = screen.y + screen.height;
    if p.x + w > right {
        p.x -= 4 + w;
    }
    if p.y + h > bottom {
        p.y -= 24 + h;
    }
    if p.y < screen.y {
        p.y = screen.y;
    }
    if p.x + w > right {
        p.x = right - w;
    }
    if p.x < screen.x {
        p.x = screen.x;
    }
    if p.y + h > bottom {
        p.y = bottom - h;
    }
    p
}

/// The screen a tip for `pos` is placed on: the one containing the point, else the primary one
/// (`QTipLabel::getTipScreen`, qtooltip.cpp:268-273).
fn tip_screen(pos: Point) -> Rect {
    let screens = qtrs_platform::integration::platform().screens();
    screens
        .iter()
        .map(|s| s.geometry())
        .find(|g| g.contains(pos))
        .unwrap_or_else(|| qtrs_platform::integration::platform().primary_screen().geometry())
}

/// `QTipLabel::instance`: the window that shows the current tip.
struct Tip {
    window: Box<Window>,
    label: WidgetRef,
    /// The widget the tip was shown for (`QTipLabel::widget`).
    widget: Option<ObjectId>,
    text: String,
}

/// `QApplicationPrivate`'s tool tip state, and `QTipLabel`'s timers.
struct Controller {
    tool_tip_widget: RefCell<Option<WidgetWeak>>,
    tool_tip_pos: Cell<Point>,
    tool_tip_global_pos: Cell<Point>,
    wake_up: RefCell<Box<Timer>>,
    fall_asleep: RefCell<Box<Timer>>,
    tip: RefCell<Option<Tip>>,
    expire: RefCell<Box<Timer>>,
    hide: RefCell<Box<Timer>>,
}

fn single_shot_timer(on_timeout: fn()) -> RefCell<Box<Timer>> {
    let mut timer = Box::new(Timer::new());
    timer.set_single_shot(true);
    timer.timeout.connect(move |()| on_timeout());
    RefCell::new(timer)
}

fn start_timer(timer: &RefCell<Box<Timer>>, ms: u64) {
    let mut timer = timer.borrow_mut();
    // SAFETY: the timer is boxed inside the thread-local controller, so its address is stable
    // until the thread ends, when `Timer::drop` unregisters it.
    unsafe { timer.start_with_interval(ms) };
}

fn stop_timer(timer: &RefCell<Box<Timer>>) {
    if let Ok(mut timer) = timer.try_borrow_mut() {
        timer.stop();
    }
}

fn timer_active(timer: &RefCell<Box<Timer>>) -> bool {
    timer.try_borrow().map(|t| t.is_active()).unwrap_or(false)
}

impl Controller {
    fn new() -> Self {
        Self {
            tool_tip_widget: RefCell::new(None),
            tool_tip_pos: Cell::new(Point::new(0, 0)),
            tool_tip_global_pos: Cell::new(Point::new(0, 0)),
            wake_up: single_shot_timer(wake_up_fired),
            fall_asleep: single_shot_timer(|| {}),
            tip: RefCell::new(None),
            expire: single_shot_timer(hide_tip_immediately),
            hide: single_shot_timer(hide_tip_immediately),
        }
    }
}

thread_local! {
    static CONTROLLER: Controller = Controller::new();
}

/// What `QApplication::notify` does with a *spontaneous* mouse move (qapplication.cpp:2712-2737):
/// remember the widget under the cursor and (re)start the wake-up timer. Only a move with no
/// button down starts it.
pub(crate) fn mouse_moved(target: &WidgetRef, pos: Point, global_pos: Point) {
    CONTROLLER.with(|c| {
        *c.tool_tip_widget.borrow_mut() = Some(std::rc::Rc::downgrade(target));
        c.tool_tip_pos.set(pos);
        c.tool_tip_global_pos.set(global_pos);
        let delay = if timer_active(&c.fall_asleep) {
            QUICK_WAKE_UP_DELAY_MS
        } else {
            WAKE_UP_DELAY_MS
        };
        start_timer(&c.wake_up, delay);
    });
}

/// `QEvent::Leave` (qapplication.cpp:2635 and `QTipLabel::eventFilter`, qtooltip.cpp:233-237): the
/// pending wake-up is cancelled and a visible tip is hidden after [`HIDE_DELAY_MS`].
pub(crate) fn leave() {
    CONTROLLER.with(|c| stop_timer(&c.wake_up));
    hide_tip();
}

/// Events after which a tip goes away at once and tool tips fall asleep (qapplication.cpp:
/// 2622-2637, `QTipLabel::eventFilter`, qtooltip.cpp:238-282): wheel, focus changes and mouse
/// buttons; key events too, which end the fall-asleep state but hide a tip only on macOS.
pub(crate) fn user_input(kind: &EventKind) {
    let hides_tip = match kind {
        EventKind::Wheel { .. }
        | EventKind::FocusIn { .. }
        | EventKind::FocusOut { .. }
        | EventKind::MouseButtonPress { .. }
        | EventKind::MouseButtonRelease { .. }
        | EventKind::MouseButtonDblClick { .. } => true,
        EventKind::KeyPress { .. } | EventKind::KeyRelease { .. } => cfg!(target_os = "macos"),
        _ => return,
    };
    let falls_asleep = matches!(
        kind,
        EventKind::Wheel { .. }
            | EventKind::FocusIn { .. }
            | EventKind::FocusOut { .. }
            | EventKind::KeyPress { .. }
            | EventKind::KeyRelease { .. }
            | EventKind::MouseButtonPress { .. }
            | EventKind::MouseButtonRelease { .. }
            | EventKind::MouseButtonDblClick { .. }
    );
    CONTROLLER.with(|c| {
        if falls_asleep {
            stop_timer(&c.fall_asleep);
        }
        stop_timer(&c.wake_up);
    });
    if hides_tip {
        hide_tip_immediately();
    }
}

/// `QApplication::event(QEvent::Timer)` for the wake-up timer (qapplication.cpp:1710-1738).
fn wake_up_fired() {
    let (widget, pos, global_pos) = CONTROLLER.with(|c| {
        (
            c.tool_tip_widget.borrow().as_ref().and_then(|w| w.upgrade()),
            c.tool_tip_pos.get(),
            c.tool_tip_global_pos.get(),
        )
    });
    let Some(widget) = widget else { return };
    if !window_shows_tool_tips(&top_level(&widget)) {
        return;
    }
    if send_tool_tip(&widget, pos, global_pos) {
        CONTROLLER.with(|c| start_timer(&c.fall_asleep, FALL_ASLEEP_DELAY_MS));
    }
}

/// `QWidget::window()`: the widget with no parent.
fn top_level(widget: &WidgetRef) -> WidgetRef {
    let mut current = widget.clone();
    loop {
        let parent = current.borrow().parent_widget().and_then(|p| p.upgrade());
        match parent {
            Some(parent) => current = parent,
            None => return current,
        }
    }
}

/// `QApplication::notify` for `ToolTip` (qapplication.cpp:2940-2962): the help event goes to the
/// widget first and, while it is not handled and accepted, to each parent in the parent's
/// coordinates, up to the window widget. Returns whether the last receiver accepted it.
///
/// A widget whose `event` does not handle the event gets `QWidget::event`'s answer
/// ([`default_tool_tip_event`]).
pub(crate) fn send_tool_tip(target: &WidgetRef, pos: Point, global_pos: Point) -> bool {
    let mut widget = target.clone();
    let mut rel = pos;
    loop {
        let mut event = Event::new_spontaneous(EventKind::ToolTip {
            x: rel.x,
            y: rel.y,
            global_x: global_pos.x,
            global_y: global_pos.y,
        });
        let handled = widget
            .try_borrow_mut()
            .map(|mut w| w.event(&mut event))
            .unwrap_or(false);
        let accepted = if handled {
            event.is_accepted()
        } else {
            default_tool_tip_event(&widget, global_pos, &mut event)
        };
        if accepted {
            return true;
        }
        let parent = widget.borrow().parent_widget().and_then(|p| p.upgrade());
        let Some(parent) = parent else { return false };
        let g = widget.borrow().geometry();
        rel = Point::new(rel.x + g.x, rel.y + g.y);
        widget = parent;
    }
}

/// `QWidget::event(ToolTip)` (qwidget.cpp:9381-9386): show the widget's own tool tip, or ignore
/// the event when it has none. Returns whether the event ends up accepted.
fn default_tool_tip_event(widget: &WidgetRef, global_pos: Point, event: &mut Event) -> bool {
    let (text, duration, id) = {
        let w = widget.borrow();
        (w.tool_tip(), w.tool_tip_duration(), w.id())
    };
    if text.is_empty() {
        event.ignore();
        return false;
    }
    ToolTip::show_text(global_pos, &text, Some(id), duration);
    event.accept();
    true
}

fn hide_tip() {
    CONTROLLER.with(|c| {
        if tip_is_visible(c) && !timer_active(&c.hide) {
            start_timer(&c.hide, HIDE_DELAY_MS);
        }
    });
}

/// `QTipLabel::hideTipImmediately`: the tip goes away now.
fn hide_tip_immediately() {
    CONTROLLER.with(|c| {
        stop_timer(&c.hide);
        stop_timer(&c.expire);
        if let Ok(mut tip) = c.tip.try_borrow_mut() {
            if let Some(tip) = tip.as_mut() {
                if tip.window.is_visible() {
                    tip.window.hide();
                }
                tip.text.clear();
                tip.widget = None;
            }
        }
    });
}

fn tip_is_visible(c: &Controller) -> bool {
    c.tip
        .try_borrow()
        .map(|t| t.as_ref().is_some_and(|t| t.window.is_visible()))
        .unwrap_or(false)
}

/// The tip's label: `QPalette::ToolTipBase` and `ToolTipText` of the inactive group, since tool
/// tips are not active windows (qtooltip.cpp:64-65). Appearance beyond that is the style's.
fn new_tip_label(text: &str) -> Label {
    let palette = crate::application::Application::palette();
    let mut label = Label::new(text);
    label.set_background_color(Some(palette.color(ColorGroup::Inactive, ColorRole::ToolTipBase)));
    label.set_color(palette.color(ColorGroup::Inactive, ColorRole::ToolTipText));
    label.set_font(qtrs_gui::text::Font::new(crate::label::APP_DEFAULT_FAMILY, 12.0));
    label.base.set_style_sheet("QLabel { padding: 2px 4px; }");
    label
}

/// `QToolTip`.
pub struct ToolTip;

impl ToolTip {
    /// `QToolTip::showText(pos, text, w, QRect(), msecDisplayTime)` (qtooltip.cpp:404-460): shows
    /// `text` next to the global position `pos`. An empty `text` hides the tip. While a tip is
    /// showing, a different text or widget replaces it in place; the same text for the same widget
    /// leaves it where it is.
    ///
    /// `widget` is the widget the tip is for (it only has to identify it); `msec_display_time <= 0`
    /// derives the display time from the text length ([`expire_time_ms`]).
    pub fn show_text(pos: Point, text: &str, widget: Option<ObjectId>, msec_display_time: i32) {
        let visible = CONTROLLER.with(tip_is_visible);
        if visible {
            if text.is_empty() {
                hide_tip();
                return;
            }
            let changed = CONTROLLER.with(|c| {
                c.tip
                    .borrow()
                    .as_ref()
                    .is_some_and(|t| t.text != text || t.widget != widget)
            });
            if changed {
                Self::place(pos, text, widget, msec_display_time);
            }
            return;
        }
        if !text.is_empty() {
            Self::place(pos, text, widget, msec_display_time);
        }
    }

    /// `QToolTip::hideText`: the same as `showText` with an empty text.
    pub fn hide_text() {
        Self::show_text(Point::new(0, 0), "", None, -1);
    }

    /// `QToolTip::isVisible`.
    pub fn is_visible() -> bool {
        CONTROLLER.with(tip_is_visible)
    }

    /// `QToolTip::text`: the text of the visible tip, or an empty string.
    pub fn text() -> String {
        CONTROLLER.with(|c| {
            c.tip
                .borrow()
                .as_ref()
                .filter(|t| t.window.is_visible())
                .map(|t| t.text.clone())
                .unwrap_or_default()
        })
    }

    /// Where the visible tip is, in global logical coordinates (`QTipLabel::geometry`).
    pub fn geometry() -> Option<Rect> {
        CONTROLLER.with(|c| {
            c.tip
                .borrow()
                .as_ref()
                .filter(|t| t.window.is_visible())
                .map(|t| t.window.geometry())
        })
    }

    /// Shows or moves the tip: `reuseTip` + `placeTip` (qtooltip.cpp:128-142, 296-370) for a tip
    /// that is up, `QTipLabel`'s constructor and `showNormal` for a new one.
    fn place(pos: Point, text: &str, widget: Option<ObjectId>, msec_display_time: i32) {
        CONTROLLER.with(|c| {
            let mut slot = c.tip.borrow_mut();
            if slot.is_none() {
                let mut label = new_tip_label(text);
                label.set_text(text);
                let label: WidgetRef = std::rc::Rc::new(RefCell::new(Box::new(label)));
                let Ok(mut window) = Window::new(
                    "tooltip",
                    Rect::new(0, 0, 1, 1),
                    WindowFlags::TOOLTIP,
                ) else {
                    return;
                };
                window.set_root_widget(label.clone());
                let mut window = Box::new(window);
                // SAFETY: boxed (stable address) and owned by this thread-local; `Drop` unregisters.
                unsafe { window.register() };
                *slot = Some(Tip { window, label, widget, text: String::new() });
            }
            let tip = slot.as_mut().expect("created above");
            tip.text = text.to_string();
            tip.widget = widget;
            let size = {
                let mut label = tip.label.borrow_mut();
                let label = label
                    .as_any_mut()
                    .downcast_mut::<Label>()
                    .expect("the tip's root widget is a Label");
                label.set_text(text);
                label.size_hint()
            };
            // `QTipLabel::updateSize`: the size hint plus one pixel of width.
            let size = Size::new(size.width + 1, size.height);
            let origin = place_tip(pos, size, CURSOR_SIZE, tip_screen(pos));
            tip.window.set_geometry(Rect::new(origin.x, origin.y, size.width, size.height));
            // `restartExpireTimer`: a new expiry cancels a pending hide.
            start_timer(&c.expire, expire_time_ms(text, msec_display_time));
            stop_timer(&c.hide);
            if !tip.window.is_visible() {
                tip.window.show();
            }
        });
    }
}
