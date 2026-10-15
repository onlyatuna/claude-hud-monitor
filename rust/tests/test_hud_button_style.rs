//! The HUD's `LayoutToggleBtn` rules through `Button`: the size hint Qt reports for the 18px toggle button,
//! and the hover background the style sheet paints.

#[test]
fn test_button_layout_toggle_btn_size_hint_matches_qt() {
    use ferrokit_core::object::qobject::QObject;
    use ferrokit_widgets::{Button, Widget};
    let qss = r#"
    QPushButton#LayoutToggleBtn {
        background-color: transparent;
        border: 1px solid rgba(255, 255, 255, 0.12);
        border-radius: 4px;
        color: #94a3b8;
        font-size: 11px;
        padding: 1px 4px;
        min-width: 18px;
        max-height: 18px;
    }
    "#;
    let mut btn = Button::new("⇄");
    btn.set_object_name("LayoutToggleBtn");
    btn.set_style_sheet(qss);
    let hint = btn.size_hint();
    // In Qt: content width is max(advance(7), min_width(18)) = 18.
    // Total width = 18 + padding (4+4) + border (1+1) = 28 px.
    // Height = clamp(15 + padding 2 + border 2, max_height 18) = 18 px; 15 is Segoe UI's line
    // height at 11px, and Segoe UI is installed only on Windows.
    assert_eq!(hint.width, 28, "button width should match Qt (28 px)");
    #[cfg(windows)]
    assert_eq!(hint.height, 18, "button height should match Qt (18 px)");
}

#[test]
fn test_button_hover_style_paints_stylesheet_hover_bg() {
    use ferrokit_core::event::{Event, EventKind};
    use ferrokit_core::object::qobject::QObject;
    use ferrokit_gui::paint::Painter;
    use ferrokit_gui::Pixmap;
    use ferrokit_widgets::{Button, Widget};

    let qss = r#"
    QPushButton#LayoutToggleBtn {
        background-color: transparent;
        border: 1px solid rgba(255, 255, 255, 0.12);
        color: #94a3b8;
    }
    QPushButton#LayoutToggleBtn:hover {
        background-color: rgba(255, 255, 255, 0.12);
        color: #38bdf8;
        border-color: #38bdf8;
    }
    "#;
    let mut btn = Button::new("⇄");
    btn.set_object_name("LayoutToggleBtn");
    btn.set_style_sheet(qss);
    btn.set_geometry(ferrokit_gui::geometry::primitives::Rect::new(0, 0, 28, 18));

    // Enter event triggers hover state
    let mut enter_ev = Event::new_spontaneous(EventKind::Enter { x: 5, y: 5 });
    btn.event(&mut enter_ev);
    assert_eq!(btn.state(), ferrokit_widgets::button::ButtonState::Hovered);

    // Paint onto a transparent pixmap
    let mut pixmap = Pixmap::new(28, 18).unwrap();
    pixmap.fill(ferrokit_gui::tiny_skia::Color::TRANSPARENT);
    let mut painter = Painter::begin(&mut pixmap);
    btn.paint_event(&mut painter);

    // Sample an interior pixel (e.g. at (2, 2))
    // rgba(255, 255, 255, 0.12) -> alpha 30: Qt truncates 0.12 * 255 = 30.6 (RC-209).
    // NOT the hardcoded hover_bg which has alpha 255 and color (225, 235, 245)!
    let pixel = pixmap.pixel(2, 2).unwrap();
    assert_eq!(
        pixel.alpha(),
        30,
        "hover background alpha should match rgba(255, 255, 255, 0.12) = 30, was {}",
        pixel.alpha()
    );
}
