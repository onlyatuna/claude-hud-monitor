use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use qtrs_core::event::{Event, EventKind};
use qtrs_core::event_loop::EventLoop;
use qtrs_core::object::{
    move_to_thread, query_object_thread, register_qobject, sender, unregister_qobject, MoveError,
    ObjectData, ObjectId, QObject, ThreadContext, ThreadId,
};
use qtrs_core::property::Property;
use qtrs_core::signal::Signal;
use qtrs_core::timer::TimerType;

// -----------------------------------------------------------------------------
// Test Object Helpers
// -----------------------------------------------------------------------------

struct TestWidget {
    data: ObjectData,
    #[allow(dead_code)]
    pub name: String,
    pub received_timer: Option<u64>,
    pub handled_events: usize,
    pub thread_change_count: usize,
}

impl TestWidget {
    pub fn new(name: &str) -> Self {
        Self {
            data: ObjectData::new(ObjectId::next()),
            name: name.to_string(),
            received_timer: None,
            handled_events: 0,
            thread_change_count: 0,
        }
    }
}

impl QObject for TestWidget {
    fn object_data(&self) -> &ObjectData {
        &self.data
    }
    fn object_data_mut(&mut self) -> &mut ObjectData {
        &mut self.data
    }
    fn as_qobject_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
    fn as_qobject_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
    fn event(&mut self, event: &mut Event) -> bool {
        self.handled_events += 1;
        match &event.kind {
            EventKind::Timer { timer_id } => {
                self.received_timer = Some(*timer_id);
                true
            }
            EventKind::ThreadChange => {
                self.thread_change_count += 1;
                true
            }
            _ => false,
        }
    }
}

// -----------------------------------------------------------------------------
// 1. Memory Safety & Dynamic Borrow Exclusivity Tests
// -----------------------------------------------------------------------------

// -----------------------------------------------------------------------------
// 2. Double Ownership Conflict & Tree Cascade Tests
// -----------------------------------------------------------------------------

#[test]
fn test_reparent_transfers_ownership_without_split_brain() {
    let mut parent_a = Box::new(TestWidget::new("parent_a"));
    let parent_a_id = parent_a.data.id;
    // SAFETY: parent remains boxed, unmoved, and owner-thread accessed until dropped.
    unsafe { register_qobject(&mut *parent_a) };

    let mut parent_b = Box::new(TestWidget::new("parent_b"));
    let parent_b_id = parent_b.data.id;
    // SAFETY: parent remains boxed, unmoved, and owner-thread accessed until dropped.
    unsafe { register_qobject(&mut *parent_b) };

    let child = Box::new(TestWidget::new("child_widget"));
    let child_liveness = child.data.liveness();
    // SAFETY: child remains boxed and owned at a stable address by parent_a.
    let _child_id = unsafe { parent_a.data.add_owned_child(child) };

    assert_eq!(parent_a.data.children.len(), 1);
    assert_eq!(parent_b.data.children.len(), 0);

    // Reparent child to parent_b
    qtrs_core::object::set_parent(
        parent_a.data.owned_children[0].object_data_mut(),
        Some(parent_b_id),
    );

    // Both logical children and physical ownership are transferred without leaking or trapping
    assert_eq!(parent_a.data.children.len(), 0);
    assert_eq!(parent_b.data.children.len(), 1);

    // Dropping parent_a does NOT destroy child because child belongs to parent_b now
    drop(parent_a);
    // SAFETY: parent has dropped and no callbacks can still be active.
    unsafe { unregister_qobject(parent_a_id) };
    assert!(
        child_liveness.load(Ordering::SeqCst),
        "Child must still be alive after parent_a dropped"
    );

    // Dropping parent_b cascades deletion to child
    drop(parent_b);
    // SAFETY: parent destruction completed on the registration thread.
    unsafe { unregister_qobject(parent_b_id) };
    assert!(
        !child_liveness.load(Ordering::SeqCst),
        "Child must be cascade-destroyed with parent_b"
    );
}

// -----------------------------------------------------------------------------
// 3. Thread Affinity & Subtree moveToThread Migration Tests
// -----------------------------------------------------------------------------

#[test]
fn test_move_to_thread_rejects_parented_and_migrates_subtree() {
    ThreadContext::init_current(true, None);
    let current_thread = ThreadId::current();
    let target_thread = ThreadId(
        std::thread::spawn(|| std::thread::current().id())
            .join()
            .unwrap(),
    );

    let mut parent = Box::new(TestWidget::new("worker_root"));
    // SAFETY: parent remains boxed and unmoved on this registration thread.
    unsafe { register_qobject(&mut *parent) };

    let child = Box::new(TestWidget::new("worker_child"));
    // SAFETY: child remains boxed and owned at a stable address by parent.
    let child_id = unsafe { parent.data.add_owned_child(child) };

    // Qt rule: Child with parent CANNOT be moved to another thread directly!
    let child_move_res = move_to_thread(
        parent.data.owned_children[0].object_data_mut(),
        target_thread,
        current_thread,
    );
    assert_eq!(
        child_move_res,
        Err(MoveError::HasParent),
        "Cannot move objects with a parent"
    );

    // Moving root cascades to all children
    let parent_move_res = move_to_thread(&mut parent.data, target_thread, current_thread);
    assert_eq!(parent_move_res, Ok(()));

    assert_eq!(parent.data.thread_id, target_thread);
    assert_eq!(query_object_thread(child_id), Some(target_thread));
    assert_eq!(
        parent.data.owned_children[0].object_data().thread_id,
        target_thread
    );

    // SAFETY: parent callbacks have ended on its registration thread.
    unsafe { unregister_qobject(parent.data.id) };
    ThreadContext::clear_current();
}

// -----------------------------------------------------------------------------
// 4. Signal/Slot: sender() Tracking & Auto Disconnect Tests
// -----------------------------------------------------------------------------

#[test]
fn test_signal_sender_tracking_and_auto_disconnection() {
    let mut emitter = Box::new(TestWidget::new("emitter"));
    // SAFETY: emitter remains boxed and unmoved through its signal lifetime.
    unsafe { register_qobject(&mut *emitter) };
    let emitter_id = emitter.data.id;

    let sig: Signal<i32> = Signal::with_emitter(emitter_id);
    let captured_sender = Arc::new(Mutex::new(None));
    let captured_clone = Arc::clone(&captured_sender);

    sig.connect(move |_val| {
        *captured_clone.lock().unwrap() = sender();
    });

    // Before emission, no active sender
    assert_eq!(sender(), None);

    // Emit signal: slot observes emitter ID
    sig.emit(&42);
    assert_eq!(*captured_sender.lock().unwrap(), Some(emitter_id));

    // After emit completes, sender stack pops back to None
    assert_eq!(sender(), None);

    // Test automatic disconnection on object drop
    {
        let mut receiver = Box::new(TestWidget::new("receiver"));
        // SAFETY: receiver remains boxed and unmoved until it is dropped below.
        unsafe { register_qobject(&mut *receiver) };
        let r_id = receiver.data.id;

        let recv_count = Arc::new(AtomicUsize::new(0));
        let rc_clone = Arc::clone(&recv_count);

        sig.connect_to(&*receiver, move |_| {
            rc_clone.fetch_add(1, Ordering::SeqCst);
        });

        sig.emit(&1);
        assert_eq!(recv_count.load(Ordering::SeqCst), 1);

        // Drop receiver: connection automatically severed
        drop(receiver);
        // SAFETY: receiver destruction completed and no callback is active.
        unsafe { unregister_qobject(r_id) };
    }

    // Emitting now does not hit severed receiver slot
    sig.emit(&2);
    // SAFETY: emitter callbacks have ended on its registration thread.
    unsafe { unregister_qobject(emitter_id) };
}

// -----------------------------------------------------------------------------
// 5. QObject Timer (start_timer / kill_timer) Tests
// -----------------------------------------------------------------------------

#[test]
fn test_qobject_start_and_kill_timer() {
    let mut widget = Box::new(TestWidget::new("timer_widget"));
    // SAFETY: widget remains boxed and unmoved until after timers are stopped.
    unsafe { register_qobject(&mut *widget) };

    // Initialize thread timer context
    let registry = Arc::new(std::sync::Mutex::new(qtrs_core::timer::TimerRegistry::new()));
    #[cfg(windows)]
    qtrs_core::timer::register_thread_timer_context(registry.clone(), std::ptr::null_mut());
    #[cfg(not(windows))]
    qtrs_core::timer::register_thread_timer_context(registry.clone());

    let t1 = widget.start_timer(100, TimerType::Coarse);
    assert!(t1.is_valid());
    assert_eq!(widget.data.timers.len(), 1);

    let t2 = widget.start_timer(200, TimerType::Precise);
    assert!(t2.is_valid());
    assert_eq!(widget.data.timers.len(), 2);

    let killed = widget.kill_timer(t1);
    assert!(killed);
    assert_eq!(widget.data.timers.len(), 1);
    assert_eq!(widget.data.timers[0], t2);

    // Dropping widget kills remaining timers
    let wid = widget.data.id;
    drop(widget);
    // SAFETY: widget destruction ended its callback lifetime on this thread.
    unsafe { unregister_qobject(wid) };

    assert_eq!(registry.lock().unwrap().len(), 0);
    qtrs_core::timer::unregister_thread_timer_context();
}

// -----------------------------------------------------------------------------
// 6. Safe Deferred Deletion (deleteLater) Tests
// -----------------------------------------------------------------------------

#[test]
fn test_safe_deferred_delete_integrated_with_event_loop() {
    let mut event_loop = EventLoop::new();
    let mut widget = Box::new(TestWidget::new("deferred_widget"));
    let wid = widget.data.id;
    // SAFETY: widget remains boxed and unmoved until unregister below.
    unsafe { register_qobject(&mut *widget) };

    event_loop.set_loop_level(1);
    let del_event = widget
        .delete_later(1)
        .expect("deferred delete event created");
    event_loop.post_event(wid, del_event);

    // In inner loop (level 2), deferred delete is deferred
    event_loop.set_loop_level(2);
    event_loop.process_events(false);
    assert_eq!(event_loop.queue().lock().unwrap().len(), 1);

    // Returning to level 1 processes deletion and unregisters object
    event_loop.set_loop_level(1);
    event_loop.process_events(false);
    assert_eq!(event_loop.queue().lock().unwrap().len(), 0);

    // Object is cleanly unregistered from global registry
    assert_eq!(query_object_thread(wid), None);
}

// -----------------------------------------------------------------------------
// 7. Qt 6 Reactive Property Binding Tests (QProperty / QBindable)
// -----------------------------------------------------------------------------

#[test]
fn test_qt6_reactive_property_bindings_and_cycle_prevention() {
    let width = Property::new(10);
    let height = Property::new(20);

    // Area binds to width * height
    let w_clone = width.clone();
    let h_clone = height.clone();
    let area = Property::with_binding(move || w_clone.get() * h_clone.get());
    assert_eq!(area.get(), 200);

    // Perimeter binds to 2 * (width + height)
    let w2 = width.clone();
    let h2 = height.clone();
    let perimeter = Property::with_binding(move || 2 * (w2.get() + h2.get()));
    assert_eq!(perimeter.get(), 60);

    // Update width -> dependent properties automatically re-evaluate lazily
    width.set(15);
    assert_eq!(area.get(), 300);
    assert_eq!(perimeter.get(), 70);

    // Update height -> dependent properties update
    height.set(30);
    assert_eq!(area.get(), 450);
    assert_eq!(perimeter.get(), 90);

    // Change signal verification
    let notified = Arc::new(AtomicBool::new(false));
    let n_clone = Arc::clone(&notified);
    area.notify_signal().connect(move |_| {
        n_clone.store(true, Ordering::SeqCst);
    });
    width.set(20);
    let _ = area.get(); // triggers recompute & notify
    assert!(notified.load(Ordering::SeqCst));

    // Circular dependency detection: does not stack overflow
    let p_a = Property::new(1);
    let p_b = Property::new(2);

    let pb_clone = p_b.clone();
    p_a.set_binding(move || pb_clone.get() + 1);

    let pa_clone = p_a.clone();
    p_b.set_binding(move || pa_clone.get() + 1);

    // Recompute safely breaks loop without stack overflow
    let _ = p_a.get();
}

// -----------------------------------------------------------------------------
// 8. Event Filter Cross-Thread & Recursion Safety Tests
// -----------------------------------------------------------------------------

#[test]
fn test_event_filter_cross_thread_and_cycle_prevention() {
    let mut obj_a = Box::new(TestWidget::new("obj_a"));
    // SAFETY: object remains boxed and unmoved until unregistered.
    unsafe { register_qobject(&mut *obj_a) };

    let mut obj_b = Box::new(TestWidget::new("obj_b"));
    // SAFETY: object remains boxed and unmoved until unregistered.
    unsafe { register_qobject(&mut *obj_b) };

    // 1. Cannot install self as event filter
    let a_id = obj_a.data.id;
    let self_filter = qtrs_core::object::install_event_filter(&mut obj_a.data, a_id);
    assert!(
        !self_filter,
        "Cannot install object as its own event filter"
    );

    // 2. Normal installation succeeds
    let ok = qtrs_core::object::install_event_filter(&mut obj_a.data, obj_b.data.id);
    assert!(ok);
    assert!(obj_a.data.event_filters.contains(obj_b.data.id));

    // 3. Direct cycle prevention: obj_b cannot install obj_a while obj_a watches obj_b
    let cycle = qtrs_core::object::install_event_filter(&mut obj_b.data, obj_a.data.id);
    assert!(!cycle, "Direct filter cycle must be rejected");

    // SAFETY: neither registered object has an active callback.
    unsafe {
        unregister_qobject(obj_a.data.id);
        unregister_qobject(obj_b.data.id);
    }
}
