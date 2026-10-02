use crate::event::{Event, EventKind};
use crate::object::qobject::ObjectData;
use crate::object::ObjectId;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::thread::ThreadId as StdThreadId;

use crate::event_loop::{EventQueue, PostedEvent};

/// Cross-thread event sender handle (`EventSender`).
#[derive(Clone)]
pub struct EventSender {
    pub target_thread_id: ThreadId,
    pub queue: Arc<Mutex<EventQueue>>,
    /// Wakeup callback function.
    pub wakeup: Arc<dyn Fn() + Send + Sync>,
}

impl EventSender {
    pub fn new(
        target_thread_id: ThreadId,
        queue: Arc<Mutex<EventQueue>>,
        wakeup: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            target_thread_id,
            queue,
            wakeup,
        }
    }

    /// Sends a posted event to the target thread queue and triggers wakeup.
    pub fn send(&self, event: PostedEvent) {
        {
            let mut q = self.queue.lock().unwrap();
            q.events.push(event);
        }

        (self.wakeup)();
    }

    pub fn post_event(&self, receiver: crate::object::ObjectId, event: crate::event::Event) {
        self.send(PostedEvent::new(receiver, event, 0));
    }

    /// Unique thread identifier: ThreadId. Modeled after QThread affinity.
    pub fn target_thread_id(&self) -> ThreadId {
        self.target_thread_id
    }
}

/// Unique thread identifier: ThreadId. Modeled after QThread affinity.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThreadId(pub StdThreadId);

impl ThreadId {
    /// Unique thread identifier: ThreadId. Modeled after QThread affinity.
    pub fn current() -> Self {
        Self(std::thread::current().id())
    }

    /// Unique thread identifier: ThreadId. Modeled after QThread affinity.
    pub fn as_std(&self) -> StdThreadId {
        self.0
    }
}

/// Thread local context: ThreadContext. Modeled after Qt QThreadData.
pub struct ThreadContext {
    pub id: ThreadId,
    /// Loop recursion level matching QThreadData::loopLevel.
    pub loop_level: Cell<usize>,
    /// Whether this thread is the main UI thread.
    pub is_main_thread: bool,
    /// Sends a posted event to the target thread queue and triggers wakeup.
    pub sender: Option<EventSender>,
}

thread_local! {
    static CURRENT_THREAD_CONTEXT: RefCell<Option<ThreadContext>> = const { RefCell::new(None) };
}
static GLOBAL_MAIN_THREAD_ID: OnceLock<ThreadId> = OnceLock::new();
static GLOBAL_THREAD_SENDERS: RwLock<Option<HashMap<ThreadId, EventSender>>> = RwLock::new(None);

/// Registers a thread's event sender in the global registry.
pub fn register_thread_sender(thread_id: ThreadId, sender: EventSender) {
    let mut lock = GLOBAL_THREAD_SENDERS.write().unwrap();
    if lock.is_none() {
        *lock = Some(HashMap::new());
    }
    if let Some(map) = lock.as_mut() {
        map.insert(thread_id, sender);
    }
}

/// Unregisters a thread's event sender from the global registry.
pub fn unregister_thread_sender(thread_id: ThreadId) {
    if let Ok(mut lock) = GLOBAL_THREAD_SENDERS.write() {
        if let Some(map) = lock.as_mut() {
            map.remove(&thread_id);
        }
    }
}

/// Queries an event sender handle for the given thread.
pub fn query_thread_sender(thread_id: ThreadId) -> Option<EventSender> {
    if let Ok(lock) = GLOBAL_THREAD_SENDERS.read() {
        if let Some(map) = lock.as_ref() {
            return map.get(&thread_id).cloned();
        }
    }
    None
}
impl ThreadContext {
    /// Initializes the thread context for the current thread.
    pub fn init_current(is_main_thread: bool, sender: Option<EventSender>) {
        if let Some(ref s) = sender {
            register_thread_sender(ThreadId::current(), s.clone());
        }
        CURRENT_THREAD_CONTEXT.with(|ctx| {
            *ctx.borrow_mut() = Some(ThreadContext {
                id: ThreadId::current(),
                loop_level: Cell::new(0),
                is_main_thread,
                sender,
            });
        });
    }

    /// Unique thread identifier: ThreadId. Modeled after QThread affinity.
    pub fn current_id() -> ThreadId {
        CURRENT_THREAD_CONTEXT.with(|ctx| {
            ctx.borrow()
                .as_ref()
                .map(|c| c.id)
                .unwrap_or_else(ThreadId::current)
        })
    }

    /// Loop recursion level matching QThreadData::loopLevel.
    pub fn current_loop_level() -> usize {
        CURRENT_THREAD_CONTEXT.with(|ctx| {
            ctx.borrow()
                .as_ref()
                .map(|c| c.loop_level.get())
                .unwrap_or(0)
        })
    }

    /// Loop recursion level matching QThreadData::loopLevel.
    pub fn set_current_loop_level(level: usize) {
        CURRENT_THREAD_CONTEXT.with(|ctx| {
            if let Some(c) = ctx.borrow().as_ref() {
                c.loop_level.set(level);
            }
        });
    }

    pub fn is_current_main_thread() -> bool {
        CURRENT_THREAD_CONTEXT.with(|ctx| {
            ctx.borrow()
                .as_ref()
                .map(|c| c.is_main_thread)
                .unwrap_or(false)
        })
    }

    pub fn set_main_thread_id(id: ThreadId) {
        let _ = GLOBAL_MAIN_THREAD_ID.set(id);
    }

    pub fn main_thread_id() -> ThreadId {
        *GLOBAL_MAIN_THREAD_ID.get_or_init(ThreadId::current)
    }

    pub fn is_main_thread() -> bool {
        if let Some(is_main) =
            CURRENT_THREAD_CONTEXT.with(|ctx| ctx.borrow().as_ref().map(|c| c.is_main_thread))
        {
            return is_main;
        }
        ThreadId::current() == Self::main_thread_id()
    }

    /// Whether this thread is the main UI thread.
    pub fn assert_main_thread(operation: &str) {
        if !Self::is_main_thread() {
            panic!(
                "qtrs: `{}` must be called on the main thread (Thread 0 / AppKit constraint)",
                operation
            );
        }
    }

    pub fn current_sender() -> Option<EventSender> {
        CURRENT_THREAD_CONTEXT.with(|ctx| ctx.borrow().as_ref().and_then(|c| c.sender.clone()))
    }

    pub fn clear_current() {
        unregister_thread_sender(ThreadId::current());
        CURRENT_THREAD_CONTEXT.with(|ctx| {
            *ctx.borrow_mut() = None;
        });
    }
}

/// Thread migration error matching QObject::moveToThread rules.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveError {
    HasParent,

    WrongThread,
}

fn collect_descendants(initial_children: &[ObjectId]) -> Vec<ObjectId> {
    let mut result = Vec::new();
    let mut queue: Vec<ObjectId> = initial_children.to_vec();

    while let Some(child_id) = queue.pop() {
        if !result.contains(&child_id) {
            result.push(child_id);
            crate::object::with_object(child_id, |obj| {
                queue.extend_from_slice(&obj.object_data().children);
            });
        }
    }
    result
}

/// Moves object and all its children to target thread matching Qt moveToThread semantics.
///
/// Cascading actions:
/// 1. Recursively migrates ThreadId of root and all descendant children.
/// 2. Stops and unregisters active timers in the source thread.
/// 3. Transports queued posted events from source thread to target thread.
/// 4. Dispatches `EventKind::ThreadChange` to all migrated objects.
pub fn move_to_thread(
    object_data: &mut ObjectData,
    target_thread: ThreadId,
    current_caller_thread: ThreadId,
) -> Result<(), MoveError> {
    if object_data.parent.is_some() {
        return Err(MoveError::HasParent);
    }
    if object_data.thread_id != current_caller_thread {
        return Err(MoveError::WrongThread);
    }
    if object_data.thread_id == target_thread {
        return Ok(());
    }

    let mut all_ids = vec![object_data.id];
    let descendants = collect_descendants(&object_data.children);
    all_ids.extend(descendants);

    // 1. Stop active timers in source thread for this object and all descendants
    for &id in &all_ids {
        crate::timer::stop_timers_for_object(id);
    }

    // 2. Transfer posted events from source thread event queue to target thread event queue
    let mut moved_events = Vec::new();
    if let Some(source_sender) =
        ThreadContext::current_sender().or_else(|| query_thread_sender(current_caller_thread))
    {
        if let Ok(mut q) = source_sender.queue.lock() {
            let mut i = 0;
            while i < q.events.len() {
                if all_ids.contains(&q.events[i].receiver) {
                    moved_events.push(q.events.remove(i));
                } else {
                    i += 1;
                }
            }
        }
    }

    if let Some(target_sender) = query_thread_sender(target_thread) {
        for ev in moved_events {
            target_sender.send(ev);
        }
    }

    // 3. Update thread_id for root and all descendants
    object_data.thread_id = target_thread;
    crate::object::register_object_thread(object_data.id, target_thread);
    for child in &mut object_data.owned_children {
        child.object_data_mut().thread_id = target_thread;
    }

    for &child_id in &all_ids[1..] {
        crate::object::register_object_thread(child_id, target_thread);
    }

    // 4. Send ThreadChange event to root and all descendants
    for &id in &all_ids {
        let mut ev = Event::new(EventKind::ThreadChange);
        crate::object::send_event(id, &mut ev);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Event, EventKind};
    use crate::object::ObjectId;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn test_thread_id_current() {
        let t1 = ThreadId::current();
        let t2 = ThreadId::current();
        assert_eq!(t1, t2);

        let handle = std::thread::spawn(ThreadId::current);
        let other_t = handle.join().unwrap();
        assert_ne!(t1, other_t);
    }

    #[test]
    fn test_thread_context_lifecycle() {
        ThreadContext::clear_current();
        assert_eq!(ThreadContext::current_loop_level(), 0);
        assert!(!ThreadContext::is_current_main_thread());

        ThreadContext::init_current(true, None);
        assert!(ThreadContext::is_current_main_thread());
        assert_eq!(ThreadContext::current_id(), ThreadId::current());
        assert_eq!(ThreadContext::current_loop_level(), 0);

        ThreadContext::set_current_loop_level(2);
        assert_eq!(ThreadContext::current_loop_level(), 2);

        ThreadContext::clear_current();
    }

    #[test]
    fn test_event_sender_send_and_wakeup() {
        let queue = Arc::new(Mutex::new(EventQueue::new()));
        let woken = Arc::new(AtomicBool::new(false));
        let woken_clone = Arc::clone(&woken);

        let sender = EventSender::new(
            ThreadId::current(),
            Arc::clone(&queue),
            Arc::new(move || {
                woken_clone.store(true, Ordering::Release);
            }),
        );

        let event = PostedEvent::new(ObjectId(42), Event::new(EventKind::UpdateRequest), 0);

        sender.send(event);

        assert!(woken.load(Ordering::Acquire));
        assert_eq!(queue.lock().unwrap().len(), 1);
        assert_eq!(queue.lock().unwrap().events[0].receiver, ObjectId(42));
    }

    #[test]
    fn test_move_to_thread_success() {
        let thread_a = ThreadId::current();
        let thread_b = std::thread::spawn(ThreadId::current).join().unwrap();

        let mut obj = ObjectData::with_thread(ObjectId(10), thread_a);
        assert_eq!(obj.thread_id, thread_a);

        let res = move_to_thread(&mut obj, thread_b, thread_a);
        assert!(res.is_ok());
        assert_eq!(obj.thread_id, thread_b);
    }

    #[test]
    fn test_move_to_thread_has_parent_error() {
        let thread_a = ThreadId::current();
        let thread_b = std::thread::spawn(ThreadId::current).join().unwrap();

        let mut obj = ObjectData::with_thread(ObjectId(10), thread_a);
        obj.parent = Some(ObjectId(1));

        let res = move_to_thread(&mut obj, thread_b, thread_a);
        assert_eq!(res, Err(MoveError::HasParent));
        assert_eq!(obj.thread_id, thread_a);
    }

    #[test]
    fn test_move_to_thread_wrong_thread_error() {
        let thread_a = ThreadId::current();
        let thread_b = std::thread::spawn(ThreadId::current).join().unwrap();

        let mut obj = ObjectData::with_thread(ObjectId(10), thread_a);

        let res = move_to_thread(&mut obj, thread_b, thread_b);
        assert_eq!(res, Err(MoveError::WrongThread));
        assert_eq!(obj.thread_id, thread_a);
    }
}
