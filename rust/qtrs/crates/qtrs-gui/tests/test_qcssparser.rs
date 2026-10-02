use qtrs_gui::text::qcssparser::*;
use qtrs_gui::tiny_skia::Color;

#[test]
fn test_qcss_parser_basic_selectors_and_declarations() {
    let qss = r#"
    /* Main HUD Background */
    QWidget#CentralWidget {
        background-color: rgba(14, 17, 23, 0.94);
        border: 1px solid rgba(255, 255, 255, 0.14);
        border-radius: 9px;
    }

    /* Labels */
    QLabel {
        color: #e2e8f0;
        font-family: 'Segoe UI', sans-serif;
    }

    QLabel#HeaderTitle {
        font-size: 10.5px;
        font-weight: 800;
        letter-spacing: 1.0px;
        color: #94a3b8;
    }
    "#;

    let sheet = QCssStyleSheet::parse(qss);
    assert_eq!(sheet.rules.len(), 3);

    // Rule 1: QWidget#CentralWidget
    let r1 = &sheet.rules[0];
    assert_eq!(r1.selectors.len(), 1);
    assert_eq!(r1.selectors[0].element_name, Some("QWidget".to_string()));
    assert_eq!(r1.selectors[0].id, Some("CentralWidget".to_string()));
    assert_eq!(r1.selectors[0].specificity(), 101); // 100 for ID + 1 for type
    assert_eq!(r1.declarations.len(), 3);
    assert_eq!(r1.declarations[0].property, QCssProperty::BackgroundColor);
    assert_eq!(
        r1.declarations[0].value,
        QCssValue::Color(Color::from_rgba8(14, 17, 23, 240))
    );
    assert_eq!(r1.declarations[2].property, QCssProperty::BorderRadius);
    assert_eq!(r1.declarations[2].value, QCssValue::Length(9.0));

    // Rule 2: QLabel
    let r2 = &sheet.rules[1];
    assert_eq!(r2.selectors[0].element_name, Some("QLabel".to_string()));
    assert_eq!(r2.selectors[0].id, None);
    assert_eq!(r2.selectors[0].specificity(), 1);

    // Rule 3: QLabel#HeaderTitle
    let r3 = &sheet.rules[2];
    assert_eq!(r3.selectors[0].element_name, Some("QLabel".to_string()));
    assert_eq!(r3.selectors[0].id, Some("HeaderTitle".to_string()));
    assert_eq!(r3.declarations[0].property, QCssProperty::FontSize);
    assert_eq!(r3.declarations[0].value, QCssValue::Length(10.5));
}

#[test]
fn test_qcss_parser_progress_bar_and_sub_control() {
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

    let sheet = QCssStyleSheet::parse(qss);
    assert_eq!(sheet.rules.len(), 2);

    let pb_rule = &sheet.rules[0];
    assert_eq!(pb_rule.selectors[0].element_name, Some("QProgressBar".to_string()));
    assert_eq!(pb_rule.selectors[0].sub_control, None);

    let mut min_h = None;
    let mut max_h = None;
    for decl in &pb_rule.declarations {
        if decl.property == QCssProperty::MinHeight {
            min_h = Some(&decl.value);
        } else if decl.property == QCssProperty::MaxHeight {
            max_h = Some(&decl.value);
        }
    }
    assert_eq!(min_h, Some(&QCssValue::Length(5.0)));
    assert_eq!(max_h, Some(&QCssValue::Length(5.0)));

    let chunk_rule = &sheet.rules[1];
    assert_eq!(chunk_rule.selectors[0].element_name, Some("QProgressBar".to_string()));
    assert_eq!(chunk_rule.selectors[0].sub_control, Some("chunk".to_string()));
    assert_eq!(chunk_rule.declarations.len(), 2);
    assert_eq!(chunk_rule.declarations[1].property, QCssProperty::BackgroundColor);
    assert_eq!(
        chunk_rule.declarations[1].value,
        QCssValue::Color(Color::from_rgba8(16, 185, 129, 255))
    );
}

#[test]
fn test_qcss_parser_pseudo_states_and_attributes() {
    let qss = r#"
    QPushButton#LayoutToggleBtn:hover {
        background-color: rgba(255, 255, 255, 0.12);
        color: #38bdf8;
    }

    QLabel[state="muted"] {
        color: rgba(235, 235, 245, 0.32);
    }
    "#;

    let sheet = QCssStyleSheet::parse(qss);
    assert_eq!(sheet.rules.len(), 2);

    let btn_hover = &sheet.rules[0];
    assert_eq!(btn_hover.selectors[0].element_name, Some("QPushButton".to_string()));
    assert_eq!(btn_hover.selectors[0].id, Some("LayoutToggleBtn".to_string()));
    assert_eq!(btn_hover.selectors[0].pseudo_states, vec!["hover".to_string()]);
    assert_eq!(btn_hover.selectors[0].specificity(), 111); // 100(id) + 10(pseudo) + 1(type)

    let attr_rule = &sheet.rules[1];
    assert_eq!(attr_rule.selectors[0].element_name, Some("QLabel".to_string()));
    assert_eq!(
        attr_rule.selectors[0].attributes,
        vec![("state".to_string(), "muted".to_string())]
    );
    assert_eq!(attr_rule.selectors[0].specificity(), 11); // 10(attr) + 1(type)
}

#[test]
fn test_qcss_parser_multi_selectors_and_edges() {
    let qss = r#"
    QLabel#SectionTitle, QLabel#RowLabel {
        padding: 1px 4px;
        margin: 5px 10px;
    }
    "#;

    let sheet = QCssStyleSheet::parse(qss);
    assert_eq!(sheet.rules.len(), 1);
    let rule = &sheet.rules[0];
    assert_eq!(rule.selectors.len(), 2);
    assert_eq!(rule.selectors[0].id, Some("SectionTitle".to_string()));
    assert_eq!(rule.selectors[1].id, Some("RowLabel".to_string()));

    assert_eq!(rule.declarations[0].property, QCssProperty::Padding);
    assert_eq!(rule.declarations[0].value, QCssValue::Edges([1.0, 4.0, 1.0, 4.0]));

    assert_eq!(rule.declarations[1].property, QCssProperty::Margin);
    assert_eq!(rule.declarations[1].value, QCssValue::Edges([5.0, 10.0, 5.0, 10.0]));
}
#[test]
fn test_parse_full_python_cards_stylesheet() {
    let qss = r#"
    QWidget#CentralWidget {
        background-color: rgba(14, 17, 23, 0.94);
        border: 1px solid rgba(255, 255, 255, 0.14);
        border-radius: 9px;
    }
    
    QLabel {
        color: #e2e8f0;
        font-family: 'Segoe UI', 'SF Pro Display', 'Microsoft JhengHei', sans-serif;
    }
    
    QLabel#HeaderTitle {
        font-size: 10.5px;
        font-weight: 800;
        letter-spacing: 1.0px;
        color: #94a3b8;
    }
    
    QLabel#HeaderStatus {
        font-size: 9.5px;
        color: #64748b;
        font-family: 'Consolas', monospace;
    }
    
    QLabel#MetricTitle {
        font-size: 10px;
        font-weight: 700;
        color: #94a3b8;
        letter-spacing: 0.6px;
    }
    
    QLabel#MetricValue {
        font-size: 16px;
        font-weight: 800;
        font-family: 'Consolas', 'Courier New', monospace;
    }
    
    QLabel#SubDetail {
        font-size: 9.5px;
        color: #64748b;
    }
    
    QProgressBar {
        background-color: rgba(255, 255, 255, 0.08);
        border: none;
        border-radius: 3px;
        text-align: right;
        min-height: 5px;
        max-height: 5px;
    }
    
    QProgressBar::chunk {
        border-radius: 3px;
    }
    
    QLabel#Badge {
        background-color: rgba(255, 255, 255, 0.06);
        border: 1px solid rgba(255, 255, 255, 0.08);
        border-radius: 3px;
        padding: 1px 4px;
        font-size: 9px;
        color: #cbd5e1;
        font-family: 'Consolas', monospace;
    }
    
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
    
    QPushButton#LayoutToggleBtn:hover {
        background-color: rgba(255, 255, 255, 0.12);
        color: #38bdf8;
        border-color: #38bdf8;
    }
    
    QFrame#Divider {
        border: none;
        background-color: rgba(255, 255, 255, 0.12);
        max-width: 1px;
        min-width: 1px;
    }
    
    QFrame#HorizontalDivider {
        background-color: rgba(255, 255, 255, 0.08);
        border: none;
        min-height: 1px;
        max-height: 1px;
    }

    QMenu {
        background-color: #161920;
        border: 1px solid rgba(255, 255, 255, 0.18);
        border-radius: 6px;
        padding: 4px 0px;
    }
    
    QMenu::item {
        color: #e2e8f0;
        padding: 6px 24px 6px 20px;
        font-size: 11px;
    }
    
    QMenu::item:selected {
        background-color: #272f3d;
        color: #38bdf8;
    }
    
    QMenu::separator {
        height: 1px;
        background-color: rgba(255, 255, 255, 0.12);
        margin: 4px 8px;
    }
    "#;

    let sheet = QCssStyleSheet::parse(qss);
    assert_eq!(sheet.rules.len(), 18, "All 18 rules in get_cards_stylesheet must be parsed successfully");
}
