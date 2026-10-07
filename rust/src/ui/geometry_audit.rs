//! Geometry differential audit (RC-14/15/16): dumps the per-widget geometry of the real HUD in
//! cards mode as JSON, in the same shape as `$TEMP/rc14/py_geom.py` produces for the PySide6 HUD.
//! `#[ignore]`: it is an audit tool, not a regression test. Run with
//! `GEOM_OUT=<dir> cargo test -j 1 geometry_audit -- --ignored --nocapture`.
use std::sync::Arc;

use parking_lot::Mutex;
use qtrs_widgets::widget::WidgetRef;
use qtrs_widgets::{Label, Policy, Widget};
use serde_json::{json, Value};

use super::hud_window::HUDWindow;
use crate::config::Config;
use crate::refresh_controller::RefreshController;
use qtrs_gui::text::{FontMetrics, FontWeight};

fn policy(p: Policy) -> i32 {
    match p {
        Policy::Fixed => 0,
        Policy::Minimum => 1,
        Policy::Maximum => 4,
        Policy::Preferred => 5,
        Policy::MinimumExpanding => 3,
        Policy::Expanding => 7,
        Policy::Ignored => 13,
    }
}

/// Position of `w` in the coordinates of the top-level root widget.
fn origin(w: &WidgetRef) -> (i32, i32) {
    let (mut x, mut y) = (0, 0);
    let mut cur = Some(w.clone());
    while let Some(c) = cur {
        let g = c.borrow().geometry();
        x += g.x;
        y += g.y;
        cur = c.borrow().parent_widget().and_then(|p| p.upgrade());
    }
    (x, y)
}

fn layout_info(w: &dyn Widget) -> Value {
    match w.layout() {
        Some(l) => {
            let m = l.margins();
            json!({"spacing": l.spacing(), "margins": [m.left, m.top, m.right, m.bottom],
                   "sizeHint": [l.size_hint().width, l.size_hint().height],
                   "minimumSize": [l.minimum_size().width, l.minimum_size().height]})
        }
        None => Value::Null,
    }
}

fn info(w: &WidgetRef) -> Value {
    let b = w.borrow();
    if !b.is_visible() {
        return json!({"visible": false});
    }
    let g = b.geometry();
    let (ox, oy) = origin(w);
    let (ox, oy) = (ox - g.x, oy - g.y); // origin() already includes the widget's own x/y
    let sp = b.size_policy();
    let mut d = json!({
        "rect": [ox + g.x, oy + g.y, g.width, g.height],
        "sizeHint": [b.size_hint().width, b.size_hint().height],
        "minimumSizeHint": [b.minimum_size_hint().width, b.minimum_size_hint().height],
        "minimumSize": [b.minimum_size().width, b.minimum_size().height],
        "maximumSize": [b.maximum_size().width, b.maximum_size().height],
        "sizePolicy": [policy(sp.horizontal), policy(sp.vertical)],
        "layout": layout_info(&**b),
    });
    if let Some(l) = b.as_any().downcast_ref::<Label>() {
        let style = l.resolved_style();
        // the same four overrides as `Label::styled_font` (private)
        let mut f = l.font().clone();
        if let Some(s) = style.font_size {
            f.size = s.round();
        }
        if let Some(fam) = &style.font_family() {
            f.family = fam.clone();
        }
        if let Some(wt) = style.font_weight {
            f.weight = match wt {
                800..=900 => FontWeight::Black,
                700..=799 => FontWeight::Bold,
                600..=699 => FontWeight::SemiBold,
                _ => FontWeight::Normal,
            };
        }
        if let Some(ls) = style.letter_spacing {
            f.letter_spacing = ls;
        }
        let m = FontMetrics::from_font(&f);
        d["font"] = json!({"family": f.family, "size": f.size, "weight": format!("{:?}", f.weight),
            "layout_height": FontMetrics::layout_height(&f), "ascent": m.ascent, "descent": m.descent,
            "height": m.height, "letter_spacing": f.letter_spacing});
        d["text"] = json!(l.text());
        d["alignment"] = json!(format!("{:?}", l.alignment()));
        d["textWidth"] = json!(m.horizontal_advance_exact(l.text(), &f));
        d["style"] = json!({"padding": style.padding, "border_width": style.border_width,
            "min_height": style.min_height, "max_height": style.max_height, "min_width": style.min_width,
            "font_size": style.font_size});
    }
    d
}

/// What-if variants, applied from the outside through public widget APIs only (no production
/// code is touched): V1 adds Python's widget-local `font-size: 14px` on the metric values, V2 also
/// uses the card root spacing 5, V3 also drops the Badge `max-height`.
fn apply_variant(hud: &mut HUDWindow, variant: u32, horizontal: bool) {
    if variant >= 1 {
        for c in hud.cards.values() {
            for v in [&c.m1_val, &c.m2_val] {
                v.borrow().set_style_sheet("font-size: 14px;");
            }
        }
    }
    if variant >= 2 {
        for c in hud.cards.values() {
            if let Some(mut l) = c.widget().borrow().layout_ref_mut() {
                l.set_spacing(5);
            }
        }
    }
    if variant == 3 || variant == 4 || variant == 7 {
        let sheet = super::styles::get_cards_stylesheet(true).replace("max-height: 15px;", "");
        hud.window.set_style_sheet(&sheet);
        qtrs_widgets::application::Application::set_style_sheet(Box::leak(sheet.into_boxed_str()));
    }
    if variant >= 5 {
        apply_python_structure(hud, horizontal);
    }
    if variant == 6 || variant == 7 {
        // Python's cards container and (absent) stack carry the default Preferred policy
        let p = qtrs_widgets::QSizePolicy::new(Policy::Preferred, Policy::Preferred);
        hud.cards_container.borrow().set_size_policy(p);
        hud.stack.borrow().set_size_policy(p);
    }
    hud.window.root_widget().borrow().update_layout();
}

/// V5 = V2 plus the container structure of the Python HUD (`_init_ui_skeleton`,
/// `_apply_cards_layout`): the header takes no fixed policy, the body has no stretch, the
/// horizontal body has spacing 8 and the vertical one has no card stretch.
fn apply_python_structure(hud: &mut HUDWindow, horizontal: bool) {
    use qtrs_widgets::{BoxLayout, Layout, QSizePolicy};
    let root = hud.window.root_widget();
    let kids = root.borrow().children();
    kids[0]
        .borrow()
        .set_size_policy(QSizePolicy::new(Policy::Preferred, Policy::Preferred));
    let mut l = BoxLayout::vertical();
    l.set_margins(qtrs_gui::geometry::primitives::Margins::new(12, 8, 12, 10));
    l.set_spacing(6);
    l.add_widget(kids[0].clone());
    l.add_widget(hud.stack.clone());
    root.borrow_mut().set_layout(Box::new(l));

    eprintln!("V5 horizontal={horizontal} root kids={}", kids.len());
    let items = hud
        .cards_container
        .borrow()
        .layout_ref_mut()
        .map(|l| l.widgets())
        .unwrap_or_default();
    let mut nl = BoxLayout::new(if horizontal {
        qtrs_widgets::layout::Direction::LeftToRight
    } else {
        qtrs_widgets::layout::Direction::TopToBottom
    });
    nl.set_spacing(if horizontal { 8 } else { 6 });
    for w in items {
        let is_card = hud
            .cards
            .values()
            .any(|c| std::rc::Rc::ptr_eq(&c.widget(), &w));
        if is_card && horizontal {
            nl.add_widget_with_stretch(w, 1);
        } else {
            nl.add_widget(w);
        }
    }
    eprintln!("V5 items={}", nl.widgets().len());
    hud.cards_container.borrow_mut().set_layout(Box::new(nl));
}

fn dump(mode: &str, w: i32, h: i32, variant: u32) -> Value {
    // `Application::new` does this from the primary screen (1.25 on this machine, as in the Python
    // oracle); without it the GDI text engine would be modelled instead of DirectWrite.
    qtrs_gui::text::font_database::set_application_device_pixel_ratio(1.25);
    let mut cfg = Config::default();
    cfg.ui_mode = "cards".into();
    cfg.layout_mode = mode.into();
    cfg.appearance = "dark".into();
    cfg.window_x = Some(0);
    cfg.window_y = Some(0);
    cfg.horizontal_width = w as u32;
    cfg.horizontal_height = h as u32;
    cfg.vertical_width = w as u32;
    cfg.vertical_height = h as u32;
    let cfg = Arc::new(Mutex::new(cfg));
    let ctrl = Arc::new(Mutex::new(RefreshController::new(60)));
    let mut hud = HUDWindow::with_providers(cfg, ctrl, crate::providers::stub::stub_providers())
        .expect("HUDWindow::new");
    hud.show();
    hud.window.root_widget().borrow().update_layout();
    apply_variant(&mut hud, variant, mode == "horizontal");

    let mut out = serde_json::Map::new();
    let g = hud.window.geometry();
    let meta = json!({"mode": mode, "dpr": hud.window.device_pixel_ratio(), "hud": [g.x, g.y, g.width, g.height]});
    for pid in ["claude", "agy", "codex"] {
        let c = &hud.cards[pid];
        let card = c.widget();
        out.insert(format!("{pid}.card"), info(&card));
        let kids = card.borrow().children();
        for (n, k) in ["header", "m1_box", "m2_box"].iter().zip(kids.iter()) {
            let mut d = info(k);
            // nested header layout of the m*_box widgets: the layout of the first child
            if *n != "header" {
                let first = k.borrow().children();
                if let Some(h0) = first.first() {
                    d["hdr"] = info(h0);
                }
            }
            out.insert(format!("{pid}.{n}"), d);
        }
        for (n, wd) in [
            ("dot", &c.dot),
            ("title", &c.title),
            ("badge", &c.badge),
            ("badge2", &c.badge2),
            ("m1_label", &c.m1_label),
            ("m1_val", &c.m1_val),
            ("m1_bar", &c.m1_bar),
            ("m1_sub", &c.m1_sub),
            ("m2_label", &c.m2_label),
            ("m2_val", &c.m2_val),
            ("m2_bar", &c.m2_bar),
            ("m2_sub", &c.m2_sub),
        ] {
            out.insert(format!("{pid}.{n}"), info(wd));
        }
    }
    for (n, wd) in [
        ("title_label", &hud.title_label),
        ("time_label", &hud.time_label),
        ("layout_toggle_btn", &hud.layout_toggle_btn),
        ("status_dot", &hud.status_dot),
        ("ghost_label", &hud.ghost_label),
    ] {
        out.insert(format!("hud.{n}"), info(wd));
    }
    let root = hud.window.root_widget();
    let rk = root.borrow().children();
    out.insert("hud.header".into(), info(&rk[0]));
    out.insert("hud.stack".into(), info(&rk[1]));
    out.insert("hud.cards_container".into(), info(&hud.cards_container));
    out.insert("hud.root_layout".into(), layout_info(&**root.borrow()));
    json!({"meta": meta, "w": out})
}

#[test]
#[ignore]
fn probe_badge_font() {
    qtrs_gui::text::font_database::set_application_device_pixel_ratio(1.25);
    use qtrs_gui::text::Font;
    for (fam, px, w) in [
        ("Consolas", 9.0, FontWeight::Normal),
        ("Consolas", 9.0, FontWeight::Bold),
        ("Segoe UI", 10.0, FontWeight::Normal),
        ("Consolas", 14.0, FontWeight::Black),
    ] {
        let f = Font::new(fam, px).with_weight(w);
        let m = FontMetrics::from_font(&f);
        let b = m.bounding_rect_exact("--", &f);
        eprintln!("PROBE {fam} {px} {w:?}: asc={} desc={} height={} leading={} layout_height={} bbox_exact_h={} bbox_h={}",
            m.ascent, m.descent, m.height, m.leading(), FontMetrics::layout_height(&f), b.height, m.bounding_rect("--", &f).height);
    }
}

#[test]
#[ignore]
fn dump_hud_geometry() {
    let dir = std::env::var("GEOM_OUT").expect("GEOM_OUT");
    // 0 = as is; 1 = + local font-size; 2 = + spacing 5; 3 = + no badge max-height (on top of 2);
    // 4 = no badge max-height only
    for variant in 0..=7 {
        for (mode, w, h) in [("horizontal", 690, 145), ("vertical", 280, 410)] {
            let v = dump(mode, w, h, variant);
            std::fs::write(
                format!("{dir}/rs{variant}_{mode}.json"),
                serde_json::to_string_pretty(&v).unwrap(),
            )
            .unwrap();
        }
    }
}
