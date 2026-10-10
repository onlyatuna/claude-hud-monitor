//! Contract C8.1 / Qt layout metric caching:
//! [QT-SRC widgets/kernel/qboxlayout.cpp:219-361, 590-625, 735-775]
//! [QT-SRC widgets/kernel/qgridlayout.cpp:719-745, 880-928, 1181-1205, 1320-1328]
//!
//! When a layout is not dirty:
//! 1. `size_hint()`, `minimum_size()`, and `maximum_size()` must return cached layout metrics
//!    without re-querying child widget size hints / minimum sizes.
//! 2. `set_geometry()` during a resize (rect != geometry) must distribute space using the cached
//!    layout items / geomArray without re-querying child widget metrics.
//! 3. Only when `invalidate()` is explicitly called (or items added/removed/properties changed)
//!    should the child metrics be re-queried on the next layout pass.

use qtrs_core::event::Event;
use qtrs_core::object::{ObjectData, ObjectId, QObject};
use qtrs_gui::geometry::primitives::{Rect, Size};
use qtrs_widgets::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct MetricCountingProbe {
    base: WidgetBase,
    hint: Size,
    hint_queries: Rc<AtomicUsize>,
    min_queries: Rc<AtomicUsize>,
    max_queries: Rc<AtomicUsize>,
}

impl MetricCountingProbe {
    fn new(w: i32, h: i32) -> (WidgetRef, Rc<AtomicUsize>, Rc<AtomicUsize>, Rc<AtomicUsize>) {
        let hint_queries = Rc::new(AtomicUsize::new(0));
        let min_queries = Rc::new(AtomicUsize::new(0));
        let max_queries = Rc::new(AtomicUsize::new(0));
        let probe = Self {
            base: WidgetBase::new(),
            hint: Size::new(w, h),
            hint_queries: Rc::clone(&hint_queries),
            min_queries: Rc::clone(&min_queries),
            max_queries: Rc::clone(&max_queries),
        };
        (
            Rc::new(RefCell::new(Box::new(probe))),
            hint_queries,
            min_queries,
            max_queries,
        )
    }
}

impl QObject for MetricCountingProbe {
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

impl Widget for MetricCountingProbe {
    fn widget_base(&self) -> &WidgetBase {
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
        self.hint_queries.fetch_add(1, Ordering::SeqCst);
        self.hint
    }
    fn minimum_size_hint(&self) -> Size {
        self.min_queries.fetch_add(1, Ordering::SeqCst);
        Size::new(10, 10)
    }
    fn maximum_size(&self) -> Size {
        self.max_queries.fetch_add(1, Ordering::SeqCst);
        Size::new(1000, 1000)
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

#[test]
fn test_box_layout_caches_child_metrics_and_does_not_requery_during_resize() {
    let mut layout = BoxLayout::vertical();
    layout.set_spacing(0);

    let (p1, h1, m1, _x1) = MetricCountingProbe::new(50, 30);
    let (p2, h2, m2, _x2) = MetricCountingProbe::new(60, 40);

    layout.add_widget(p1);
    layout.add_widget(p2);

    // Invalidate and reset counters
    layout.invalidate();
    h1.store(0, Ordering::SeqCst);
    h2.store(0, Ordering::SeqCst);
    m1.store(0, Ordering::SeqCst);
    m2.store(0, Ordering::SeqCst);

    // Initial query: computes setupGeom once (each child queried during setupGeom)
    let hint = layout.size_hint();
    assert_eq!(hint, Size::new(60, 70));
    let initial_h1 = h1.load(Ordering::SeqCst);
    let initial_h2 = h2.load(Ordering::SeqCst);
    assert!(initial_h1 > 0, "p1 queried on initial setupGeom");
    assert!(initial_h2 > 0, "p2 queried on initial setupGeom");

    // Second query without changes: must reuse cached metric, 0 additional queries
    let hint2 = layout.size_hint();
    assert_eq!(hint2, hint);
    assert_eq!(
        h1.load(Ordering::SeqCst),
        initial_h1,
        "p1 size_hint must not be requeried on second query"
    );
    assert_eq!(
        h2.load(Ordering::SeqCst),
        initial_h2,
        "p2 size_hint must not be requeried on second query"
    );

    // Querying minimum_size must also use cached metrics from setupGeom
    let min = layout.minimum_size();
    assert_eq!(min, Size::new(10, 20));
    assert_eq!(
        h1.load(Ordering::SeqCst),
        initial_h1,
        "p1 size_hint must not be requeried on minimum_size"
    );
    assert_eq!(
        h2.load(Ordering::SeqCst),
        initial_h2,
        "p2 size_hint must not be requeried on minimum_size"
    );

    // Resizing layout (set_geometry) must distribute space without re-querying child size metrics
    layout.set_geometry(Rect::new(0, 0, 200, 300));
    assert_eq!(
        h1.load(Ordering::SeqCst),
        initial_h1,
        "p1 size_hint must not be requeried during set_geometry"
    );
    assert_eq!(
        h2.load(Ordering::SeqCst),
        initial_h2,
        "p2 size_hint must not be requeried during set_geometry"
    );

    // Second resize (e.g. interactive window drag): still no re-queries
    layout.set_geometry(Rect::new(0, 0, 220, 310));
    assert_eq!(
        h1.load(Ordering::SeqCst),
        initial_h1,
        "p1 size_hint must not be requeried on second resize"
    );
    assert_eq!(
        h2.load(Ordering::SeqCst),
        initial_h2,
        "p2 size_hint must not be requeried on second resize"
    );

    // Invalidation: now and only now must child metrics be requeried on next layout/query
    layout.invalidate();
    let hint3 = layout.size_hint();
    assert_eq!(hint3, hint);
    assert_eq!(
        h1.load(Ordering::SeqCst),
        initial_h1 * 2,
        "p1 size_hint requeried after invalidate"
    );
    assert_eq!(
        h2.load(Ordering::SeqCst),
        initial_h2 * 2,
        "p2 size_hint requeried after invalidate"
    );
}

#[test]
fn test_grid_layout_caches_child_metrics_and_does_not_requery_during_resize() {
    let mut layout = GridLayout::new();
    layout.set_horizontal_spacing(0);
    layout.set_vertical_spacing(0);

    let (p1, h1, _m1, _x1) = MetricCountingProbe::new(50, 30);
    let (p2, h2, _m2, _x2) = MetricCountingProbe::new(60, 40);

    layout.add_widget(p1, 0, 0);
    layout.add_widget(p2, 1, 0);

    // Invalidate and reset counters
    layout.invalidate();
    h1.store(0, Ordering::SeqCst);
    h2.store(0, Ordering::SeqCst);

    // Initial query
    let hint = layout.size_hint();
    assert_eq!(hint, Size::new(60, 70));
    let initial_h1 = h1.load(Ordering::SeqCst);
    let initial_h2 = h2.load(Ordering::SeqCst);
    assert!(initial_h1 > 0, "p1 queried on initial setup_layout_data");
    assert!(initial_h2 > 0, "p2 queried on initial setup_layout_data");

    // Second query: must reuse cached metric
    let hint2 = layout.size_hint();
    assert_eq!(hint2, hint);
    assert_eq!(
        h1.load(Ordering::SeqCst),
        initial_h1,
        "p1 size_hint must not be requeried on second query"
    );
    assert_eq!(
        h2.load(Ordering::SeqCst),
        initial_h2,
        "p2 size_hint must not be requeried on second query"
    );

    // Resizing grid layout must distribute space without re-querying child size metrics
    layout.set_geometry(Rect::new(0, 0, 200, 300));
    assert_eq!(
        h1.load(Ordering::SeqCst),
        initial_h1,
        "p1 size_hint must not be requeried during set_geometry"
    );
    assert_eq!(
        h2.load(Ordering::SeqCst),
        initial_h2,
        "p2 size_hint must not be requeried during set_geometry"
    );

    // Second resize: still no re-queries
    layout.set_geometry(Rect::new(0, 0, 220, 310));
    assert_eq!(
        h1.load(Ordering::SeqCst),
        initial_h1,
        "p1 size_hint must not be requeried on second resize"
    );
    assert_eq!(
        h2.load(Ordering::SeqCst),
        initial_h2,
        "p2 size_hint must not be requeried on second resize"
    );

    // Invalidation: now requeried
    layout.invalidate();
    let hint3 = layout.size_hint();
    assert_eq!(hint3, hint);
    assert_eq!(
        h1.load(Ordering::SeqCst),
        initial_h1 * 2,
        "p1 size_hint requeried after invalidate"
    );
    assert_eq!(
        h2.load(Ordering::SeqCst),
        initial_h2 * 2,
        "p2 size_hint requeried after invalidate"
    );
}
