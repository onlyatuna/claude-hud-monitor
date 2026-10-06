//! Differential probe for the layout port: reads one layout per line from stdin, lays it out and
//! prints the rectangle of every item, to be compared with PySide6 by
//! `tools/second_layer_harness/qt_layout_compare.py`.
//!
//! Line format: `kind spacing margin width height | item ; item ; ...` where `kind` is `H`, `V`
//! or `G<columns>` and an item is `hint_w,hint_h,min_w,min_h,max_w,max_h,policy_h,policy_v,
//! stretch,hidden`. A grid fills row by row; its stretch field is the column stretch of the
//! item's column when it is in the first row. Policies are `Fixed`, `Minimum`, `Maximum`,
//! `Preferred`, `Expanding`, `MinimumExpanding` or `Ignored`.

use qtrs_core::event::Event;
use qtrs_core::object::{ObjectData, ObjectId, QObject};
use qtrs_gui::geometry::primitives::{Margins, Rect, Size};
use qtrs_widgets::*;
use std::cell::RefCell;
use std::io::BufRead;
use std::rc::Rc;

struct Probe {
    base: WidgetBase,
    hint: Size,
    min: Size,
    max: Size,
}

impl QObject for Probe {
    fn object_data(&self) -> &ObjectData {
        &self.base.object_data
    }
    fn object_data_mut(&mut self) -> &mut ObjectData {
        &mut self.base.object_data
    }
    fn event(&mut self, _event: &mut Event) -> bool {
        false
    }
}

impl Widget for Probe {
    fn widget_base(&self) -> &qtrs_widgets::widget::WidgetBase {
        &self.base
    }

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
        self.hint
    }
    fn minimum_size(&self) -> Size {
        self.min
    }
    fn minimum_size_hint(&self) -> Size {
        Size::new(0, 0)
    }
    fn maximum_size(&self) -> Size {
        self.max
    }
    fn is_visible(&self) -> bool {
        self.base.is_visible()
    }
    fn set_visible(&self, visible: bool) {
        self.base.set_visible(visible);
    }
    fn is_enabled(&self) -> bool {
        true
    }
    fn set_enabled(&self, _enabled: bool) {}
    fn update(&self) {}
    fn dirty_rect(&self) -> Option<Rect> {
        None
    }
    fn clear_dirty(&self) {}
    fn parent_widget(&self) -> Option<WidgetWeak> {
        self.base.parent_widget()
    }
    fn set_parent_widget(&self, parent: Option<WidgetWeak>) {
        self.base.set_parent_widget(parent);
    }
    fn window_id(&self) -> Option<ObjectId> {
        None
    }
    fn set_window_id(&self, _window_id: Option<ObjectId>) {}
    fn children(&self) -> Vec<WidgetRef> {
        Vec::new()
    }
    fn add_child(&mut self, _child: WidgetRef) {}
    fn remove_child(&mut self, _child_id: ObjectId) {}
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn policy(name: &str) -> Policy {
    match name {
        "Fixed" => Policy::Fixed,
        "Minimum" => Policy::Minimum,
        "Maximum" => Policy::Maximum,
        "Preferred" => Policy::Preferred,
        "Expanding" => Policy::Expanding,
        "MinimumExpanding" => Policy::MinimumExpanding,
        "Ignored" => Policy::Ignored,
        other => panic!("unknown policy {other}"),
    }
}

fn main() {
    for line in std::io::stdin().lock().lines() {
        let line = line.expect("stdin");
        let (head, items) = line.split_once('|').expect("head | items");
        let mut head = head.split_whitespace();
        let kind = head.next().unwrap();
        let spacing: i32 = head.next().unwrap().parse().unwrap();
        let margin: i32 = head.next().unwrap().parse().unwrap();
        let width: i32 = head.next().unwrap().parse().unwrap();
        let height: i32 = head.next().unwrap().parse().unwrap();

        let mut widgets: Vec<WidgetRef> = Vec::new();
        let mut stretches = Vec::new();
        let mut cells = Vec::new();
        for item in items.split(';') {
            let f: Vec<&str> = item.trim().split(',').collect();
            let n = |i: usize| f[i].parse::<i32>().unwrap();
            let hidden = f[9] == "1";
            let probe = Probe {
                base: WidgetBase::new(),
                hint: Size::new(n(0), n(1)),
                min: Size::new(n(2), n(3)),
                max: Size::new(n(4), n(5)),
            };
            probe.base.set_size_policy(QSizePolicy::new(policy(f[6]), policy(f[7])));
            probe.base.set_visible(!hidden);
            widgets.push(Rc::new(RefCell::new(Box::new(probe))));
            stretches.push(n(8) as u32);
            cells.push((n(10) as usize, n(11) as usize, n(12) as usize, n(13) as usize, n(14) as u32));
        }

        let container: WidgetRef = Rc::new(RefCell::new(Box::new(EmptyWidget::with_geometry(
            Rect::new(0, 0, width, height),
        ))));
        let margins = Margins::new(margin, margin, margin, margin);
        if kind.starts_with('G') {
            let mut grid = GridLayout::new();
            grid.set_margins(margins);
            grid.set_horizontal_spacing(spacing);
            grid.set_vertical_spacing(spacing);
            for (i, w) in widgets.iter().enumerate() {
                let (row, col, row_span, col_span, row_stretch) = cells[i];
                grid.add_widget_with_span(w.clone(), row, col, row_span, col_span);
                if row == 0 && stretches[i] > 0 {
                    grid.set_column_stretch(col, stretches[i]);
                }
                if row_stretch > 0 {
                    grid.set_row_stretch(row, row_stretch);
                }
            }
            container.borrow_mut().set_layout(Box::new(grid));
        } else {
            let mut layout = if kind == "H" { BoxLayout::horizontal() } else { BoxLayout::vertical() };
            layout.set_margins(margins);
            layout.set_spacing(spacing);
            for (w, stretch) in widgets.iter().zip(&stretches) {
                layout.add_widget_with_stretch(w.clone(), *stretch);
            }
            container.borrow_mut().set_layout(Box::new(layout));
        }
        container.borrow_mut().set_geometry(Rect::new(0, 0, width, height));
        container.borrow().update_layout();

        let out: Vec<String> = widgets
            .iter()
            .map(|w| {
                let g = w.borrow().geometry();
                format!("{},{},{},{}", g.x, g.y, g.width, g.height)
            })
            .collect();
        println!("{}", out.join(";"));
    }
}
