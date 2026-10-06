//! RC-05: a change to a widget's style sheet, font, text metrics, size policy or style
//! properties must reach the repaint and layout machinery without any manual `update_layout()`
//! or `render_and_present()`.
//!
//! Qt (`QWidget::event`, qwidget.cpp:9502-9510): `FontChange` / `StyleChange` run
//! `update(); updateGeometry(); layout->invalidate()`. `updateGeometry` (qwidget.cpp:10571-10587)
//! invalidates the parent layout, which posts a `LayoutRequest`. `QWidget::setSizePolicy` calls
//! `updateGeometry`. `style()->unpolish(w); style()->polish(w)` (what the Python HUD does after
//! `setProperty`) ends in a `StyleChange`; `setProperty` alone restyles nothing in Qt, because
//! `QWidget` and `QApplication` ignore `DynamicPropertyChange` (qwidget.cpp:9449,
//! qapplication.cpp:2594).

use qtrs_widgets::button::Button;
use qtrs_widgets::frame::Frame;
use qtrs_widgets::key_sequence_edit::KeySequenceEdit;
use qtrs_widgets::menu::Menu;
use qtrs_widgets::progress_bar::ProgressBar;
use qtrs_widgets::scroll::{ScrollArea, ScrollBar};
use qtrs_widgets::stacked::StackedWidget;
use qtrs_widgets::widget::EmptyWidget;
use qtrs_widgets::{Label, Policy, QSizePolicy, Widget};

fn every_widget_type() -> Vec<(&'static str, Box<dyn Widget>)> {
    vec![
        ("Button", Box::new(Button::new("b"))),
        ("EmptyWidget", Box::new(EmptyWidget::new())),
        ("Frame", Box::new(Frame::new())),
        ("KeySequenceEdit", Box::new(KeySequenceEdit::new())),
        ("Label", Box::new(Label::new("l"))),
        ("Menu", Box::new(Menu::new("m"))),
        ("ProgressBar", Box::new(ProgressBar::new())),
        ("ScrollArea", Box::new(ScrollArea::new())),
        (
            "ScrollBar",
            Box::new(ScrollBar::new(qtrs_widgets::scroll::Orientation::Vertical)),
        ),
        ("StackedWidget", Box::new(StackedWidget::new())),
    ]
}

#[test]
fn set_size_policy_on_every_widget_type_is_not_silently_dropped() {
    let policy = QSizePolicy::new(Policy::Fixed, Policy::Expanding);
    for (name, widget) in every_widget_type() {
        widget.set_size_policy(policy);
        assert_eq!(widget.size_policy(), policy, "{name} dropped set_size_policy");
    }
}

#[test]
fn set_style_sheet_on_every_widget_type_is_not_silently_dropped() {
    for (name, widget) in every_widget_type() {
        widget.set_style_sheet("color: red;");
        assert!(widget.style_sheet().is_some(), "{name} dropped set_style_sheet");
    }
}

#[test]
fn set_property_on_every_widget_type_can_be_read_back() {
    for (name, widget) in every_widget_type() {
        widget.set_property("state", "warn");
        assert_eq!(
            Widget::property(&*widget, "state").as_deref(),
            Some("warn"),
            "{name} dropped set_property"
        );
    }
}

#[test]
fn changing_the_size_policy_requests_a_parent_layout_and_an_unchanged_one_does_not() {
    use qtrs_widgets::command::WidgetCommandQueue;
    use std::cell::RefCell;
    use std::rc::Rc;

    let parent: qtrs_widgets::WidgetRef = Rc::new(RefCell::new(Box::new(Frame::new())));
    let label = Label::new("child");
    label.set_parent_widget(Some(Rc::downgrade(&parent)));

    WidgetCommandQueue::clear();
    label.set_size_policy(QSizePolicy::default());
    assert_eq!(
        WidgetCommandQueue::pending_count(),
        0,
        "QWidget::setSizePolicy returns early when nothing changed"
    );

    label.set_size_policy(QSizePolicy::new(Policy::Fixed, Policy::Fixed));
    assert!(
        WidgetCommandQueue::pending_count() > 0,
        "updateGeometry must ask the parent layout to run"
    );
    WidgetCommandQueue::clear();
}

#[cfg(windows)]
mod pumped {
    //! Real `Window`, real event loop, one pump per observation.
    use std::cell::RefCell;
    use std::rc::Rc;

    use qtrs_core::event_loop::EventLoop;
    use qtrs_gui::geometry::primitives::Rect;
    use qtrs_gui::text::font::Font;
    use qtrs_platform::WindowFlags;
    use qtrs_widgets::button::Button;
    use qtrs_widgets::layout::{BoxLayout, Layout};
    use qtrs_widgets::widget::{EmptyWidget, WidgetRef};
    use qtrs_widgets::window::Window;
    use qtrs_widgets::{Label, Policy, QSizePolicy, Widget};

    struct Fixture {
        el: EventLoop,
        win: Box<Window>,
        first: WidgetRef,
        second: WidgetRef,
        spacing: i32,
    }

    const WINDOW_WIDTH: i32 = 400;

    fn fixture(first: Box<dyn Widget>, second: Box<dyn Widget>) -> Fixture {
        let el = EventLoop::new();
        let mut win = Box::new(
            Window::new(
                "Widget Invalidation",
                Rect::new(0, 0, WINDOW_WIDTH, 120),
                WindowFlags::empty(),
            )
            .expect("create window"),
        );
        // SAFETY: boxed (stable address), single thread, `Drop` unregisters.
        unsafe { win.register() };

        let first: WidgetRef = Rc::new(RefCell::new(first));
        let second: WidgetRef = Rc::new(RefCell::new(second));
        let mut layout = BoxLayout::horizontal();
        layout.set_spacing(6);
        layout.add_widget(Rc::clone(&first));
        layout.add_widget(Rc::clone(&second));
        let container: WidgetRef = Rc::new(RefCell::new(Box::new(EmptyWidget::new())));
        container.borrow_mut().set_layout(Box::new(layout));
        win.set_root_widget(container);

        let mut fx = Fixture { el, win, first, second, spacing: 6 };
        // Settle the initial layout and paint.
        fx.win.render_and_present();
        pump_idle(&mut fx);
        fx
    }

    /// One posted-event pump per turn until nothing changes (bounded).
    fn pump_idle(fx: &mut Fixture) -> u32 {
        for turn in 1..=16 {
            let before = fx.win.render_stats();
            fx.el.process_events(false);
            if fx.win.render_stats() == before {
                return turn;
            }
        }
        panic!("render ping-pong: still rendering after 16 turns");
    }

    fn label_pair() -> Fixture {
        let mut a = Label::new("Alpha");
        a.set_font(Font::new("Segoe UI", 12.0));
        let mut b = Label::new("Beta");
        b.set_font(Font::new("Segoe UI", 12.0));
        fixture(Box::new(a), Box::new(b))
    }

    /// With both labels `Preferred`, the second sits right after the first's allotted width.
    fn left_of_second(fx: &Fixture) -> i32 {
        fx.second.borrow().geometry().x
    }

    #[test]
    fn style_sheet_font_size_change_relayouts_parent_after_one_pump() {
        // A Button: its size policy already works, so this isolates the style-sheet path.
        let mut fx = fixture(Box::new(Button::new("Alpha")), Box::new(Label::new("Beta")));
        let renders_before = fx.win.render_stats().render_count;
        let width_before = fx.first.borrow().geometry().width;

        // Fixed so the first label's width follows its size hint exactly.
        fx.first
            .borrow()
            .set_size_policy(QSizePolicy::new(Policy::Fixed, Policy::Preferred));
        pump_idle(&mut fx);
        fx.first.borrow().set_style_sheet("font-size: 28px;");
        let turns = pump_idle(&mut fx);

        let hint = fx.first.borrow().size_hint();
        assert!(hint.width > width_before, "a 28px font must be wider than the 12pt label was");
        assert_eq!(
            fx.first.borrow().geometry().width,
            hint.width,
            "the label did not take its new size hint (no relayout after {turns} pump turns)"
        );
        assert_eq!(left_of_second(&fx), hint.width + fx.spacing, "the sibling did not move");
        assert!(
            fx.win.render_stats().render_count > renders_before,
            "a style change must schedule a repaint"
        );
    }

    #[test]
    fn label_set_size_policy_changes_layout_result() {
        let mut fx = label_pair();
        let first_hint = fx.first.borrow().size_hint().width;

        // `Fixed` keeps the first label at its hint; the second takes everything that is left.
        fx.first
            .borrow()
            .set_size_policy(QSizePolicy::new(Policy::Fixed, Policy::Preferred));
        pump_idle(&mut fx);
        assert_eq!(fx.first.borrow().geometry().width, first_hint);
        assert_eq!(
            fx.second.borrow().geometry().width,
            WINDOW_WIDTH - first_hint - fx.spacing
        );

        // `Expanding` hands the first label the free space (equal stretch, 0): both expand.
        fx.first
            .borrow()
            .set_size_policy(QSizePolicy::new(Policy::Expanding, Policy::Preferred));
        fx.second
            .borrow()
            .set_size_policy(QSizePolicy::new(Policy::Fixed, Policy::Preferred));
        pump_idle(&mut fx);
        let second_hint = fx.second.borrow().size_hint().width;
        assert_eq!(fx.second.borrow().geometry().width, second_hint);
        assert_eq!(
            fx.first.borrow().geometry().width,
            WINDOW_WIDTH - second_hint - fx.spacing
        );
    }

    #[test]
    fn label_set_font_relayouts_parent_after_one_pump() {
        let mut fx = label_pair();
        fx.first
            .borrow()
            .set_size_policy(QSizePolicy::new(Policy::Fixed, Policy::Preferred));
        pump_idle(&mut fx);
        let before = fx.first.borrow().geometry().width;

        {
            let mut w = fx.first.borrow_mut();
            let label = w.as_any_mut().downcast_mut::<Label>().unwrap();
            label.set_font(Font::new("Segoe UI", 30.0));
        }
        pump_idle(&mut fx);

        let hint = fx.first.borrow().size_hint().width;
        assert!(hint > before);
        assert_eq!(fx.first.borrow().geometry().width, hint, "label kept its old width");
        assert_eq!(left_of_second(&fx), hint + fx.spacing);
    }

    #[test]
    fn button_set_text_relayouts_parent_after_one_pump() {
        let mut fx = fixture(Box::new(Button::new("Go")), Box::new(Label::new("tail")));
        fx.first
            .borrow()
            .set_size_policy(QSizePolicy::new(Policy::Fixed, Policy::Preferred));
        pump_idle(&mut fx);
        let before = fx.first.borrow().geometry().width;

        {
            let mut w = fx.first.borrow_mut();
            let button = w.as_any_mut().downcast_mut::<Button>().unwrap();
            button.set_text("A considerably longer button caption");
        }
        pump_idle(&mut fx);

        let hint = fx.first.borrow().size_hint().width;
        assert!(hint > before);
        assert_eq!(fx.first.borrow().geometry().width, hint, "button kept its old width");
        assert_eq!(left_of_second(&fx), hint + fx.spacing);
    }

    #[test]
    fn repolish_after_set_property_restyles_and_relayouts_after_one_pump() {
        let mut fx = label_pair();
        fx.first
            .borrow()
            .set_size_policy(QSizePolicy::new(Policy::Fixed, Policy::Preferred));
        fx.first
            .borrow()
            .set_style_sheet("QLabel { font-size: 12px; } QLabel[state=\"big\"] { font-size: 30px; }");
        pump_idle(&mut fx);
        let before = fx.first.borrow().geometry().width;

        fx.first.borrow().set_property("state", "big");
        fx.first.borrow().repolish();
        pump_idle(&mut fx);

        let hint = fx.first.borrow().size_hint().width;
        assert!(hint > before, "the property rule must enlarge the label");
        assert_eq!(fx.first.borrow().geometry().width, hint);
        assert_eq!(left_of_second(&fx), hint + fx.spacing);
    }
}
