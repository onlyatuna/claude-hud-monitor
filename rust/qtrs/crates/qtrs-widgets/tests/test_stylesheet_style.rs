use qtrs_gui::tiny_skia::Color;
use qtrs_widgets::style::stylesheet::{QStyleSheetStyle, WidgetStyleContext};

#[test]
fn test_stylesheet_style_resolution_and_specificity() {
    let qss = r#"
    QLabel {
        color: #e2e8f0;
        font-size: 12px;
    }

    QLabel#HeaderTitle {
        color: #94a3b8;
        font-size: 10.5px;
        font-weight: 800;
    }
    "#;

    let style = QStyleSheetStyle::parse(qss);

    // Context 1: Generic QLabel
    let ctx_generic = WidgetStyleContext {
        type_name: "QLabel",
        object_name: "",
        pseudo_states: &[],
        sub_control: None,
        attributes: &[],
    };
    let res_generic = style.resolve(&ctx_generic);
    assert_eq!(
        res_generic.color,
        Some(Color::from_rgba8(226, 232, 240, 255))
    );
    assert_eq!(res_generic.font_size, Some(12.0));
    assert_eq!(res_generic.font_weight, None);

    // Context 2: QLabel#HeaderTitle (ID specificity should override generic)
    let ctx_id = WidgetStyleContext {
        type_name: "QLabel",
        object_name: "HeaderTitle",
        pseudo_states: &[],
        sub_control: None,
        attributes: &[],
    };
    let res_id = style.resolve(&ctx_id);
    assert_eq!(res_id.color, Some(Color::from_rgba8(148, 163, 184, 255)));
    assert_eq!(res_id.font_size, Some(10.5));
    assert_eq!(res_id.font_weight, Some(800));
}

#[test]
fn test_stylesheet_style_progress_bar_and_chunk() {
    let qss = r#"
    QProgressBar {
        background-color: rgba(255, 255, 255, 0.08);
        border: none;
        border-radius: 3px;
        min-height: 5px;
        max-height: 5px;
    }

    QProgressBar::chunk {
        border-radius: 3px;
        background-color: #10b981;
    }
    "#;

    let style = QStyleSheetStyle::parse(qss);

    // 1. Groove (no sub-control)
    let ctx_groove = WidgetStyleContext {
        type_name: "QProgressBar",
        object_name: "",
        pseudo_states: &[],
        sub_control: None,
        attributes: &[],
    };
    let res_groove = style.resolve(&ctx_groove);
    assert_eq!(res_groove.min_height, Some(5));
    assert_eq!(res_groove.max_height, Some(5));
    assert_eq!(res_groove.border_radius, Some(3.0));
    assert_eq!(
        res_groove.background_color,
        Some(Color::from_rgba8(255, 255, 255, 20))
    );

    // 2. Chunk (sub-control = "chunk")
    let ctx_chunk = WidgetStyleContext {
        type_name: "QProgressBar",
        object_name: "",
        pseudo_states: &[],
        sub_control: Some("chunk"),
        attributes: &[],
    };
    let res_chunk = style.resolve(&ctx_chunk);
    assert_eq!(res_chunk.border_radius, Some(3.0));
    assert_eq!(
        res_chunk.background_color,
        Some(Color::from_rgba8(16, 185, 129, 255))
    );
}

#[test]
fn test_stylesheet_style_cascaded_resolution() {
    // Global window stylesheet has the 5px height and groove background
    let global_qss = r#"
    QProgressBar {
        background-color: rgba(255, 255, 255, 0.08);
        min-height: 5px;
        max-height: 5px;
    }
    "#;
    let global_style = QStyleSheetStyle::parse(global_qss);

    // Local widget stylesheet ONLY changes chunk color dynamically
    let local_qss = r#"
    QProgressBar::chunk {
        background-color: #ef4444;
    }
    "#;
    let local_style = QStyleSheetStyle::parse(local_qss);

    // Context for chunk: local chunk color overrides, but global 5px stays intact!
    let ctx_groove = WidgetStyleContext {
        type_name: "QProgressBar",
        object_name: "",
        pseudo_states: &[],
        sub_control: None,
        attributes: &[],
    };
    let res = QStyleSheetStyle::resolve_cascaded(
        Some(&local_style),
        Some(&global_style),
        &ctx_groove,
    );
    assert_eq!(res.min_height, Some(5));
    assert_eq!(res.max_height, Some(5));
    assert_eq!(
        res.background_color,
        Some(Color::from_rgba8(255, 255, 255, 20))
    );

    let ctx_chunk = WidgetStyleContext {
        type_name: "QProgressBar",
        object_name: "",
        pseudo_states: &[],
        sub_control: Some("chunk"),
        attributes: &[],
    };
    let res_chunk = QStyleSheetStyle::resolve_cascaded(
        Some(&local_style),
        Some(&global_style),
        &ctx_chunk,
    );
    assert_eq!(
        res_chunk.background_color,
        Some(Color::from_rgba8(239, 68, 68, 255))
    );
}

#[test]
fn test_stylesheet_style_pseudo_states_hover() {
    let qss = r#"
    QPushButton#LayoutToggleBtn {
        background-color: transparent;
        color: #94a3b8;
        min-width: 18px;
        max-height: 18px;
    }

    QPushButton#LayoutToggleBtn:hover {
        background-color: rgba(255, 255, 255, 0.12);
        color: #38bdf8;
    }
    "#;

    let style = QStyleSheetStyle::parse(qss);

    // Normal state
    let ctx_normal = WidgetStyleContext {
        type_name: "QPushButton",
        object_name: "LayoutToggleBtn",
        pseudo_states: &[],
        sub_control: None,
        attributes: &[],
    };
    let res_normal = style.resolve(&ctx_normal);
    assert_eq!(res_normal.min_width, Some(18));
    assert_eq!(res_normal.max_height, Some(18));
    assert_eq!(res_normal.color, Some(Color::from_rgba8(148, 163, 184, 255)));

    // Hover state
    let ctx_hover = WidgetStyleContext {
        type_name: "QPushButton",
        object_name: "LayoutToggleBtn",
        pseudo_states: &["hover"],
        sub_control: None,
        attributes: &[],
    };
    let res_hover = style.resolve(&ctx_hover);
    assert_eq!(res_hover.color, Some(Color::from_rgba8(56, 189, 248, 255)));
    assert_eq!(
        res_hover.background_color,
        Some(Color::from_rgba8(255, 255, 255, 31))
    );
}

#[test]
fn test_button_layout_toggle_btn_size_hint_matches_qt() {
    use qtrs_core::object::qobject::QObject;
    use qtrs_widgets::{Button, Widget};
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
    use qtrs_core::event::{Event, EventKind};
    use qtrs_core::object::qobject::QObject;
    use qtrs_gui::paint::Painter;
    use qtrs_gui::Pixmap;
    use qtrs_widgets::{Button, Widget};

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
    btn.set_geometry(qtrs_gui::geometry::primitives::Rect::new(0, 0, 28, 18));

    // Enter event triggers hover state
    let mut enter_ev = Event::new_spontaneous(EventKind::Enter { x: 5, y: 5 });
    btn.event(&mut enter_ev);
    assert_eq!(btn.state(), qtrs_widgets::button::ButtonState::Hovered);

    // Paint onto a transparent pixmap
    let mut pixmap = Pixmap::new(28, 18).unwrap();
    pixmap.fill(qtrs_gui::tiny_skia::Color::TRANSPARENT);
    let mut painter = Painter::begin(&mut pixmap);
    btn.paint_event(&mut painter);

    // Sample an interior pixel (e.g. at (2, 2))
    // rgba(255, 255, 255, 0.12) -> alpha is ~31 (0.12 * 255 = 30.6)
    // NOT the hardcoded hover_bg which has alpha 255 and color (225, 235, 245)!
    let pixel = pixmap.pixel(2, 2).unwrap();
    assert_eq!(pixel.alpha(), 31, "hover background alpha should match rgba(255, 255, 255, 0.12) ~ 31, was {}", pixel.alpha());
}
