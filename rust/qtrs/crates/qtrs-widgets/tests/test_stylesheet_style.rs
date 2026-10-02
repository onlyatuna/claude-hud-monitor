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
