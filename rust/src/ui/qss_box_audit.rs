//! QSS min/max box-model audit (RC-19): dumps `minimumSize`/`maximumSize`/`sizeHint`/`minimumSizeHint`
//! of Label/Button/Frame/ProgressBar under `rust/tools/qss_box_audit/cases.json`, the same cases
//! `py_box.py` runs under PySide6. `#[ignore]`: audit tool. Run with
//! `BOX_OUT=<file> cargo test -j 1 qss_box_audit -- --ignored --nocapture`.
use qtrs_widgets::frame::Frame;
use qtrs_widgets::progress_bar::ProgressBar;
use qtrs_widgets::widget::WidgetRef;
use qtrs_widgets::{Button, Label, Widget};
use serde_json::{json, Map, Value};

fn make_widget<W: Widget + 'static>(w: W) -> WidgetRef {
    std::rc::Rc::new(std::cell::RefCell::new(Box::new(w)))
}

pub(crate) fn dump(cases: &Value) -> Value {
    let sheet: &'static str = Box::leak(
        cases["sheet"]
            .as_str()
            .unwrap()
            .to_string()
            .into_boxed_str(),
    );
    qtrs_widgets::application::Application::set_style_sheet(sheet);
    let mut out = Map::new();
    for c in cases["cases"].as_array().unwrap() {
        let (name, typ, text) = (
            c[0].as_str().unwrap(),
            c[1].as_str().unwrap(),
            c[2].as_str().unwrap(),
        );
        let vertical = c
            .as_array()
            .unwrap()
            .iter()
            .skip(3)
            .any(|o| o == "vertical");
        let w = match typ {
            "QPushButton" => make_widget(Button::new(text)),
            "QLabel" => make_widget(Label::new(text)),
            "QFrame" => make_widget(Frame::new()),
            "QProgressBar" => {
                let mut p = ProgressBar::new();
                if vertical {
                    p.set_orientation(qtrs_widgets::scroll::Orientation::Vertical);
                }
                make_widget(p)
            }
            t => panic!("unknown type {t}"),
        };
        w.borrow_mut().set_object_name(name);
        let b = w.borrow();
        let sz = |s: qtrs_gui::geometry::primitives::Size| json!([s.width, s.height]);
        out.insert(
            name.into(),
            json!({"min": sz(b.minimum_size()), "max": sz(b.maximum_size()), "hint": sz(b.size_hint()),
                   "minHint": sz(b.minimum_size_hint())}),
        );
    }
    Value::Object(out)
}

#[test]
#[ignore]
fn dump_qss_box() {
    let path = std::env::var("BOX_OUT").expect("BOX_OUT");
    let dpr: f32 = std::env::var("BOX_DPR")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.25);
    qtrs_gui::text::font_database::set_application_device_pixel_ratio(dpr);
    let cases: Value = serde_json::from_str(include_str!("../../tools/qss_box_audit/cases.json"))
        .expect("cases.json");
    let mut v = dump(&cases);
    v["meta"] = json!({"dpr": dpr});
    std::fs::write(path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
}
