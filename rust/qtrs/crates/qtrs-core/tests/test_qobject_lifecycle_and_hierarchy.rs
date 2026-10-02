use qtrs_core::event::{Event, EventKind};
use qtrs_core::event_loop::EventQueue;
use qtrs_core::object::{
    move_to_thread, register_boxed_qobject, register_qobject, unregister_qobject, ObjectData,
    ObjectId, QObject, QObjectExt, QPointer, SignalBlocker, ThreadContext, ThreadId,
};
use qtrs_core::signal::Signal;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

// --- Test Structures ---

struct MockContainer {
    data: ObjectData,
    child_removed_count: Arc<AtomicUsize>,
    child_added_count: Arc<AtomicUsize>,
    thread_changed: Arc<AtomicBool>,
}

impl MockContainer {
    fn new(name: &str) -> Self {
        let mut data = ObjectData::new(ObjectId::next());
        data.set_object_name(name);
        Self {
            data,
            child_removed_count: Arc::new(AtomicUsize::new(0)),
            child_added_count: Arc::new(AtomicUsize::new(0)),
            thread_changed: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl QObject for MockContainer {
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
        match &event.kind {
            EventKind::ChildAdded { .. } => {
                self.child_added_count.fetch_add(1, Ordering::SeqCst);
                true
            }
            EventKind::ChildRemoved { .. } => {
                self.child_removed_count.fetch_add(1, Ordering::SeqCst);
                true
            }
            EventKind::ThreadChange => {
                self.thread_changed.store(true, Ordering::SeqCst);
                true
            }
            _ => false,
        }
    }
}

struct MockButton {
    data: ObjectData,
    pub clicked: Signal<()>,
    pub text: String,
}

impl MockButton {
    fn new(name: &str, text: &str) -> Self {
        let id = ObjectId::next();
        let mut data = ObjectData::new(id);
        data.set_object_name(name);
        Self {
            data,
            clicked: Signal::with_emitter(id),
            text: text.to_string(),
        }
    }
}

impl QObject for MockButton {
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
}

struct MockLabel {
    data: ObjectData,
    pub text: String,
}

impl MockLabel {
    fn new(name: &str, text: &str) -> Self {
        let mut data = ObjectData::new(ObjectId::next());
        data.set_object_name(name);
        Self {
            data,
            text: text.to_string(),
        }
    }
}

impl QObject for MockLabel {
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
}

// --- 1. Ownership & Deletion Cascade Tests ---

#[test]
fn test_ownership_and_deletion_cascade() {
    let mut parent = MockContainer::new("root_parent");
    let parent_id = parent.data.id;
    // SAFETY: parent stays on this thread and alive until the explicit drop.
    unsafe { register_qobject(&mut parent) };

    let child_liveness: Arc<AtomicBool>;
    let grandchild_liveness: Arc<AtomicBool>;

    {
        let mut child = Box::new(MockContainer::new("child_1"));
        child_liveness = child.data.liveness();
        assert!(child_liveness.load(Ordering::SeqCst));

        let grandchild = Box::new(MockContainer::new("grandchild_1"));
        grandchild_liveness = grandchild.data.liveness();
        assert!(grandchild_liveness.load(Ordering::SeqCst));

        // SAFETY: both boxed children remain owned at stable addresses until cascade drop.
        unsafe { child.data.add_owned_child(grandchild) };
        // SAFETY: child remains boxed and owned by parent through its destruction.
        unsafe { parent.data.add_owned_child(child) };
    }

    assert_eq!(parent.data.owned_children.len(), 1);
    assert_eq!(parent.data.children.len(), 1);
    assert!(child_liveness.load(Ordering::SeqCst));
    assert!(grandchild_liveness.load(Ordering::SeqCst));

    // Dropping parent must recursively cascade-destroy child and grandchild
    drop(parent);

    // Both child and grandchild should be marked dead
    assert!(!child_liveness.load(Ordering::SeqCst));
    assert!(!grandchild_liveness.load(Ordering::SeqCst));

    // SAFETY: parent destruction and all callbacks are complete on this thread.
    unsafe { unregister_qobject(parent_id) };
}

#[test]
fn test_child_unlink_notifies_parent() {
    let mut parent = MockContainer::new("parent");
    let parent_id = parent.data.id;
    // SAFETY: parent stays on this thread and alive until unregistered.
    unsafe { register_qobject(&mut parent) };

    {
        let mut child = MockContainer::new("child");
        let child_id = child.data.id;
        // SAFETY: child stays on this thread and alive until unregistered.
        unsafe { register_qobject(&mut child) };

        child.data.set_parent(Some(parent_id));
        assert_eq!(parent.data.children.len(), 1);
        assert_eq!(parent.child_added_count.load(Ordering::SeqCst), 1);

        // Explicitly unparenting
        child.data.set_parent(None);
        assert_eq!(parent.data.children.len(), 0);
        assert_eq!(parent.child_removed_count.load(Ordering::SeqCst), 1);

        // SAFETY: child callback is no longer active and this is its registration thread.
        unsafe { unregister_qobject(child_id) };
    }

    // SAFETY: parent has no active callbacks and remains on this thread.
    unsafe { unregister_qobject(parent_id) };
}

// --- 2. Object Retrieval & Hierarchy Query Tests ---

#[test]
fn test_object_hierarchy_search_find_child() {
    let mut root = MockContainer::new("window");
    // SAFETY: root stays on this thread and alive through all hierarchy queries.
    unsafe { register_qobject(&mut root) };

    let mut panel = Box::new(MockContainer::new("panel"));
    let btn1 = Box::new(MockButton::new("ok_button", "OK"));
    let btn2 = Box::new(MockButton::new("cancel_button", "Cancel"));
    let label = Box::new(MockLabel::new("status_label", "Ready"));

    // SAFETY: all children remain boxed at stable addresses while parent owns them.
    unsafe { panel.data.add_owned_child(btn1) };
    // SAFETY: all children remain boxed at stable addresses while parent owns them.
    unsafe { panel.data.add_owned_child(btn2) };
    // SAFETY: all children remain boxed at stable addresses while parent owns them.
    unsafe { panel.data.add_owned_child(label) };
    // SAFETY: panel remains boxed at a stable address while root owns it.
    unsafe { root.data.add_owned_child(panel) };

    // Query by specific name and type
    let ok_btn = root.find_child::<MockButton>("ok_button");
    assert!(ok_btn.is_some());
    assert_eq!(ok_btn.unwrap().text, "OK");

    let cancel_btn = root.find_child::<MockButton>("cancel_button");
    assert!(cancel_btn.is_some());
    assert_eq!(cancel_btn.unwrap().text, "Cancel");

    let status_lbl = root.find_child::<MockLabel>("status_label");
    assert!(status_lbl.is_some());
    assert_eq!(status_lbl.unwrap().text, "Ready");

    // Query non-existent name
    let missing = root.find_child::<MockButton>("non_existent");
    assert!(missing.is_none());

    // Query with empty name matches first child of type
    let first_btn = root.find_child::<MockButton>("");
    assert!(first_btn.is_some());

    // Query all children of type MockButton
    let all_buttons = root.find_children::<MockButton>(None);
    assert_eq!(all_buttons.len(), 2);

    // Mutable find_child_mut
    if let Some(btn) = root.find_child_mut::<MockButton>("ok_button") {
        btn.text = "Confirmed".to_string();
    }
    let updated_ok = root.find_child::<MockButton>("ok_button").unwrap();
    assert_eq!(updated_ok.text, "Confirmed");
}

// --- 3. Thread Migration Cascading Tests ---

#[test]
fn test_move_to_thread_cascades_children_and_events() {
    let current_thread = ThreadId::current();
    let target_thread = std::thread::spawn(ThreadId::current).join().unwrap();

    // Setup source thread context and event sender
    let source_queue = Arc::new(Mutex::new(EventQueue::new()));
    let source_sender =
        qtrs_core::object::EventSender::new(current_thread, source_queue.clone(), Arc::new(|| {}));
    ThreadContext::init_current(false, Some(source_sender));

    // Setup target thread sender
    let target_queue = Arc::new(Mutex::new(EventQueue::new()));
    let target_sender =
        qtrs_core::object::EventSender::new(target_thread, target_queue.clone(), Arc::new(|| {}));
    qtrs_core::object::register_thread_sender(target_thread, target_sender);

    let mut parent = MockContainer::new("worker_root");
    // SAFETY: parent stays on this thread and alive until cleanup below.
    unsafe { register_qobject(&mut parent) };
    let child = Box::new(MockContainer::new("worker_child"));
    let child_id = child.data.id;
    // SAFETY: child remains boxed and owned by parent at a stable address.
    unsafe { parent.data.add_owned_child(child) };
    // Post an event to child in source thread queue
    {
        let mut q = source_queue.lock().unwrap();
        q.post_event(child_id, Event::new(EventKind::UpdateRequest));
    }
    assert_eq!(source_queue.lock().unwrap().len(), 1);
    assert_eq!(target_queue.lock().unwrap().len(), 0);

    // Execute cascading moveToThread
    let result = move_to_thread(&mut parent.data, target_thread, current_thread);
    assert_eq!(result, Ok(()));

    // Invariants:
    // 1. Parent thread_id updated
    assert_eq!(parent.data.thread_id, target_thread);
    // 2. Child thread_id updated
    assert_eq!(
        parent.data.owned_children[0].object_data().thread_id,
        target_thread
    );
    assert_eq!(
        qtrs_core::object::query_object_thread(child_id),
        Some(target_thread)
    );

    // 3. Queued event transferred from source to target
    assert_eq!(source_queue.lock().unwrap().len(), 0);
    assert_eq!(target_queue.lock().unwrap().len(), 1);
    assert_eq!(target_queue.lock().unwrap().events()[0].receiver, child_id);

    // 4. ThreadChange event received
    assert!(parent.thread_changed.load(Ordering::SeqCst));

    // Clean up
    // SAFETY: parent dispatch has ended on this registration thread.
    unsafe { unregister_qobject(parent.data.id) };
    ThreadContext::clear_current();
    qtrs_core::object::unregister_thread_sender(target_thread);
}
// --- 4. RAII Signal Blocker Tests ---

#[test]
fn test_raii_signal_blocker() {
    let mut btn = MockButton::new("my_btn", "Click Me");
    // SAFETY: button stays on this thread and alive until unregistered.
    unsafe { register_qobject(&mut btn) };

    let click_count = Arc::new(AtomicUsize::new(0));
    let counter_clone = click_count.clone();

    btn.clicked.connect(move |_| {
        counter_clone.fetch_add(1, Ordering::SeqCst);
    });

    // Initial emit works
    btn.clicked.emit(&());
    assert_eq!(click_count.load(Ordering::SeqCst), 1);

    // Block with RAII SignalBlocker
    {
        let mut blocker = SignalBlocker::new(&mut btn);
        assert!(btn.signals_blocked());

        // Emit while blocked should be suppressed
        btn.clicked.emit(&());
        assert_eq!(click_count.load(Ordering::SeqCst), 1);

        // Temporarily unblock and reblock
        blocker.unblock();
        assert!(!btn.signals_blocked());
        btn.clicked.emit(&());
        assert_eq!(click_count.load(Ordering::SeqCst), 2);

        blocker.reblock();
        assert!(btn.signals_blocked());
        btn.clicked.emit(&());
        assert_eq!(click_count.load(Ordering::SeqCst), 2);
    }

    // After blocker dropped, signals automatically restored
    assert!(!btn.signals_blocked());
    btn.clicked.emit(&());
    assert_eq!(click_count.load(Ordering::SeqCst), 3);

    // QObject::block_signals convenience method
    let prev = btn.block_signals(true);
    assert!(!prev);
    assert!(btn.signals_blocked());
    btn.clicked.emit(&());
    assert_eq!(click_count.load(Ordering::SeqCst), 3);

    let prev2 = btn.block_signals(false);
    assert!(prev2);
    assert!(!btn.signals_blocked());
    btn.clicked.emit(&());
    assert_eq!(click_count.load(Ordering::SeqCst), 4);

    // SAFETY: button callbacks have returned on this registration thread.
    unsafe { unregister_qobject(btn.data.id) };
}

// --- 5. Guarded Pointer (QPointer) & Generational Liveness Tests ---

#[test]
fn test_qpointer_and_generational_liveness() {
    let widget = Box::new(MockButton::new("guarded_btn", "Submit"));
    let (id, registered) = unsafe { register_boxed_qobject(widget) };

    let qptr: QPointer<MockButton> = QPointer::new(&*registered);
    assert!(!qptr.is_null());
    assert!(qptr.is_valid());
    assert_eq!(qptr.id(), Some(id));

    // Generational identity before drop
    let gen_id_before = registered.data.generational_id();
    assert_eq!(gen_id_before.id, id);
    assert_eq!(gen_id_before.generation, 1);

    // Drop the target object
    drop(registered);

    // Guarded pointer automatically invalidated without dangling pointer
    assert!(qptr.is_null());
    assert!(!qptr.is_valid());
    assert_eq!(qptr.id(), None);

    // SAFETY: dropping the returned Box ended its callback lifetime.
    unsafe { unregister_qobject(id) };
}
