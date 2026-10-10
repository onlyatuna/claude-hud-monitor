use crate::event::EventFilterChain;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::object::ThreadId;

use crate::event::{
    compress_event as run_compress_event, CoreCompressor, Event, EventCompressor, EventKind,
};
use crate::object::{send_event, EventSender, ObjectData, ObjectId, QObject};
use crate::timer::TimerRegistry;

#[allow(unused_imports)]
use crate::event_loop::dispatcher::EventDispatcher;
thread_local! {
    static APPLICATION_EVENT_FILTERS: RefCell<EventFilterChain> = RefCell::new(EventFilterChain::new());
}

pub fn install_application_event_filter(filter: ObjectId) {
    APPLICATION_EVENT_FILTERS.with(|filters| {
        filters.borrow_mut().install(filter);
    });
}

pub fn remove_application_event_filter(filter: ObjectId) {
    APPLICATION_EVENT_FILTERS.with(|filters| {
        filters.borrow_mut().remove(filter);
    });
}

pub fn clear_application_event_filters() {
    APPLICATION_EVENT_FILTERS.with(|filters| {
        *filters.borrow_mut() = EventFilterChain::new();
    });
}

pub fn notify_helper(receiver: ObjectId, event: &mut Event) -> bool {
    let app_filter_ids = APPLICATION_EVENT_FILTERS.with(|filters| filters.borrow().snapshot());
    for filter_id in app_filter_ids {
        let filtered = crate::object::with_object_mut(filter_id, |filter_obj| {
            filter_obj.event_filter(receiver, event)
        });
        if filtered == Some(true) {
            return false;
        }
    }

    let obj_filter_ids =
        crate::object::with_object(receiver, |obj| obj.object_data().event_filters.snapshot())
            .unwrap_or_default();

    for filter_id in obj_filter_ids {
        let filtered = crate::object::with_object_mut(filter_id, |filter_obj| {
            filter_obj.event_filter(receiver, event)
        });
        if filtered == Some(true) {
            return false;
        }
    }

    if matches!(&event.kind, EventKind::MetaCall(_)) {
        if let EventKind::MetaCall(task) =
            std::mem::replace(&mut event.kind, EventKind::LayoutRequest)
        {
            // Try dispatch via registered receiver first.
            let mut task_opt = Some(task);
            let handled = crate::object::with_object_mut(receiver, |obj| {
                if let Some(task) = task_opt.take() {
                    task(obj);
                }
            });
            if handled.is_some() {
                return true;
            }
            // Receiver not registered (ObjectId(0) sentinel, queued slot with unregistered
            // receiver, or receiver already dropped). Qt discards the event when the receiver
            // object is gone; here we invoke the closure via a stub so fire-and-forget
            // MetaCalls (single_shot, queued signals) still execute.
            struct NullObj(ObjectData);
            impl QObject for NullObj {
                fn object_data(&self) -> &ObjectData {
                    &self.0
                }
                fn object_data_mut(&mut self) -> &mut ObjectData {
                    &mut self.0
                }
            }
            let mut stub = NullObj(ObjectData::new(receiver));
            if let Some(task) = task_opt.take() {
                task(&mut stub);
            }
            return true;
        }
    }

    crate::object::dispatch_to_object(receiver, event)
}
use super::dispatcher::{
    create_dispatcher, DefaultEventDispatcher, DispatchResult, DispatcherKind,
    EventDispatcherHandle,
};

#[derive(Debug)]
pub struct PostedEvent {
    pub receiver: ObjectId,
    pub event: Event,
    pub priority: i32,
}

impl PostedEvent {
    pub fn new(receiver: ObjectId, event: Event, priority: i32) -> Self {
        Self {
            receiver,
            event,
            priority,
        }
    }
}

pub struct EventQueue {
    pub(crate) events: Vec<PostedEvent>,
    pub(crate) insertion_offset: usize,
    pub(crate) compressor: Arc<dyn EventCompressor>,
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl EventQueue {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            insertion_offset: 0,
            compressor: Arc::new(CoreCompressor),
        }
    }

    pub fn with_compressor(compressor: Arc<dyn EventCompressor>) -> Self {
        Self {
            events: Vec::new(),
            insertion_offset: 0,
            compressor,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn events(&self) -> &[PostedEvent] {
        &self.events
    }

    pub fn compress_event(&mut self, receiver: ObjectId, event: &mut Event) -> bool {
        run_compress_event(&mut self.events, receiver, event, &*self.compressor)
    }

    pub fn post_event(&mut self, receiver: ObjectId, event: Event) {
        self.post_event_with_priority(receiver, event, 0);
    }

    pub fn post_event_with_priority(
        &mut self,
        receiver: ObjectId,
        mut event: Event,
        priority: i32,
    ) {
        if self.compress_event(receiver, &mut event) {
            return;
        }

        self.insert_posted(PostedEvent::new(receiver, event, priority));
    }

    /// Removes the event at `index`. Removing one from the current turn shrinks
    /// `insertion_offset` with it (Qt `insertionOffset -= startOffset`, qcoreapplication.cpp:1847).
    pub(crate) fn remove_at(&mut self, index: usize) -> PostedEvent {
        if index < self.insertion_offset {
            self.insertion_offset -= 1;
        }
        self.events.remove(index)
    }

    /// Inserts in priority order after the turn boundary, so an event posted during a turn waits
    /// for the next one (qthread_p.h:65-86).
    pub(crate) fn insert_posted(&mut self, posted: PostedEvent) {
        let start_search = self.insertion_offset.min(self.events.len());
        let relative_idx = self.events[start_search..]
            .iter()
            .position(|e| e.priority < posted.priority)
            .unwrap_or(self.events.len() - start_search);
        self.events.insert(start_search + relative_idx, posted);
    }
}

pub fn compress_event_with_queue(
    queue: &mut EventQueue,
    receiver: ObjectId,
    event: &mut Event,
) -> bool {
    queue.compress_event(receiver, event)
}

pub struct EventLoop {
    pub(crate) dispatcher: DefaultEventDispatcher,
    pub(crate) queue: Arc<Mutex<EventQueue>>,
    pub(crate) timer_registry: Arc<Mutex<TimerRegistry>>,
    pub(crate) compressor: Arc<dyn EventCompressor>,
    pub(crate) loop_level: usize,
    pub(crate) exit_requested: Arc<AtomicBool>,
    pub(crate) return_code: Arc<AtomicI32>,
    pub(crate) custom_timeout: Option<Duration>,
}

impl EventLoop {
    /// A core event loop (worker threads and `CoreApplication`), whichever thread creates it.
    pub fn new() -> Self {
        Self::with_dispatcher_kind(DispatcherKind::Core)
    }

    /// An event loop whose dispatcher comes from the `kind` creation path; `GuiApplication`
    /// creates its loop with `DispatcherKind::Gui`.
    pub fn with_dispatcher_kind(kind: DispatcherKind) -> Self {
        let dispatcher = create_dispatcher(kind);
        let timer_registry = Arc::new(Mutex::new(TimerRegistry::new()));

        #[cfg(windows)]
        crate::timer::register_thread_timer_context(
            Arc::clone(&timer_registry),
            dispatcher.internal_hwnd,
        );
        #[cfg(not(windows))]
        crate::timer::register_thread_timer_context(Arc::clone(&timer_registry));

        let el = Self {
            dispatcher,
            queue: Arc::new(Mutex::new(EventQueue::new())),
            timer_registry,
            compressor: Arc::new(CoreCompressor),
            loop_level: 0,
            exit_requested: Arc::new(AtomicBool::new(false)),
            return_code: Arc::new(AtomicI32::new(0)),
            custom_timeout: None,
        };
        register_thread_event_loop(ThreadId::current(), el.handle());
        el
    }

    /// Creates an event loop with a shared event queue.
    pub fn with_queue(queue: Arc<Mutex<EventQueue>>) -> Self {
        let dispatcher = create_dispatcher(DispatcherKind::Core);
        let timer_registry = Arc::new(Mutex::new(TimerRegistry::new()));

        #[cfg(windows)]
        crate::timer::register_thread_timer_context(
            Arc::clone(&timer_registry),
            dispatcher.internal_hwnd,
        );
        #[cfg(not(windows))]
        crate::timer::register_thread_timer_context(Arc::clone(&timer_registry));

        let el = Self {
            dispatcher,
            queue,
            timer_registry,
            compressor: Arc::new(CoreCompressor),
            loop_level: 0,
            exit_requested: Arc::new(AtomicBool::new(false)),
            return_code: Arc::new(AtomicI32::new(0)),
            custom_timeout: None,
        };
        register_thread_event_loop(ThreadId::current(), el.handle());
        el
    }

    pub fn queue(&self) -> &Arc<Mutex<EventQueue>> {
        &self.queue
    }

    pub fn timer_registry(&self) -> &Arc<Mutex<TimerRegistry>> {
        &self.timer_registry
    }

    pub fn loop_level(&self) -> usize {
        self.loop_level
    }

    pub fn set_loop_level(&mut self, loop_level: usize) {
        self.loop_level = loop_level;
    }

    pub fn is_exit_requested(&self) -> bool {
        self.exit_requested.load(Ordering::SeqCst)
    }

    pub fn return_code(&self) -> i32 {
        self.return_code.load(Ordering::SeqCst)
    }

    pub fn next_timeout(&self) -> Option<Duration> {
        if let Some(custom) = self.custom_timeout {
            return Some(custom);
        }
        let deadline = self.timer_registry.lock().unwrap().next_deadline()?;
        let now = crate::timer::current_time_ms();
        if deadline > now {
            Some(Duration::from_millis(deadline - now))
        } else {
            Some(Duration::ZERO)
        }
    }

    pub fn set_next_timeout(&mut self, timeout: Option<Duration>) {
        self.custom_timeout = timeout;
    }

    pub fn post_event(&self, receiver: ObjectId, event: Event) {
        self.post_event_with_priority(receiver, event, 0);
    }

    pub fn post_event_with_priority(&self, receiver: ObjectId, event: Event, priority: i32) {
        let mut queue = self.queue.lock().unwrap();
        if run_compress_event(&mut queue.events, receiver, &event, &*self.compressor) {
            return;
        }
        queue.insert_posted(PostedEvent::new(receiver, event, priority));
        self.dispatcher.wake_up();
    }
    pub fn set_compressor(&mut self, compressor: Arc<dyn EventCompressor>) {
        self.compressor = Arc::clone(&compressor);
        self.queue.lock().unwrap().compressor = compressor;
    }

    pub fn install_native_event_filter(
        &mut self,
        filter: Box<dyn crate::event::NativeEventFilter>,
    ) {
        self.dispatcher.install_native_event_filter(filter);
    }

    pub fn send_posted_events(&mut self) -> usize {
        let (delivered, quit_code) = send_posted_events_for_queue(&self.queue, self.loop_level);
        if let Some(code) = quit_code {
            self.exit_requested.store(true, Ordering::SeqCst);
            self.return_code.store(code, Ordering::SeqCst);
        }
        delivered
    }

    pub fn process_events(&mut self, can_wait: bool) -> bool {
        let delivered = self.send_posted_events();
        let had_posted = delivered > 0;

        let next_timeout = self.next_timeout();
        let effective_wait = can_wait && !self.exit_requested.load(Ordering::SeqCst);

        let res = self.dispatcher.process_events(effective_wait, next_timeout);
        {
            let mut reg = self.timer_registry.lock().unwrap();
            self.dispatcher.send_timer_events(&mut reg);
        }
        if let DispatchResult::Quit(code) = res {
            self.exit_requested.store(true, Ordering::SeqCst);
            self.return_code.store(code, Ordering::SeqCst);
        }

        let had_system_events = matches!(
            res,
            DispatchResult::Normal | DispatchResult::Awoken | DispatchResult::Quit(_)
        );
        had_posted || had_system_events
    }

    pub fn exec(&mut self) -> i32 {
        self.loop_level += 1;
        // Note: Do not overwrite self.exit_requested to false if exit() was already invoked prior to exec()
        while !self.exit_requested.load(Ordering::SeqCst) {
            self.process_events(true);
        }
        self.loop_level -= 1;
        self.return_code.load(Ordering::SeqCst)
    }

    pub fn exit(&self, return_code: i32) {
        self.exit_requested.store(true, Ordering::SeqCst);
        self.return_code.store(return_code, Ordering::SeqCst);
        self.dispatcher.wake_up();
    }

    pub fn quit(&self) {
        self.exit(0);
    }

    pub fn handle(&self) -> EventLoopHandle {
        EventLoopHandle {
            dispatcher: Arc::new(self.dispatcher.clone_handle()),
            queue: Arc::clone(&self.queue),
            compressor: Arc::clone(&self.compressor),
            exit_requested: Arc::clone(&self.exit_requested),
            return_code: Arc::clone(&self.return_code),
        }
    }

    pub fn sender(&self) -> EventSender {
        let dispatcher_handle = self.dispatcher.clone_handle();
        EventSender::new(
            ThreadId::current(),
            Arc::clone(&self.queue),
            Arc::new(move || {
                dispatcher_handle.wake_up();
            }),
        )
    }
}

thread_local! {
    /// True while this thread is inside the posted-event pump (normal or modal).
    static POSTED_PUMP_ACTIVE: Cell<bool> = const { Cell::new(false) };
    /// Set when a modal pump left events queued (kept deferred deletes), so the outermost normal
    /// pump can re-arm the wake-up for them.
    static MODAL_PUMP_LEFT_QUEUED: Cell<bool> = const { Cell::new(false) };
}

/// RAII marker for "a posted-event pump is running on this thread".
struct PostedPumpGuard {
    was_active: bool,
}

impl PostedPumpGuard {
    fn enter() -> Self {
        Self {
            was_active: POSTED_PUMP_ACTIVE.with(|f| f.replace(true)),
        }
    }
}

impl Drop for PostedPumpGuard {
    fn drop(&mut self) {
        POSTED_PUMP_ACTIVE.with(|f| f.set(self.was_active));
    }
}

/// Normal event-loop pump. A callback may re-enter it (nested event loops) and each call runs its
/// own turn; `outermost` only decides which call re-arms the wake-up left by a modal pump.
pub fn send_posted_events_for_queue(
    queue: &Arc<Mutex<EventQueue>>,
    loop_level: usize,
) -> (usize, Option<i32>) {
    let guard = PostedPumpGuard::enter();
    let outermost = !guard.was_active;
    let result = pump_posted_events(queue, loop_level, false);
    drop(guard);

    if outermost && MODAL_PUMP_LEFT_QUEUED.with(|f| f.replace(false)) {
        // A native modal loop left events queued and consumed its wake-up. Re-arm it if this
        // loop level can deliver them, so `process_events` returns instead of blocking on them.
        let deliverable = queue
            .lock()
            .unwrap()
            .events
            .iter()
            .any(|e| deliverable_at_loop_level(&e.event, loop_level));
        if deliverable {
            if let Some(handle) = get_thread_event_sender(ThreadId::current()) {
                handle.wake_up();
            }
        }
    }
    result
}

/// Pumps the current thread's posted-event queue from a native modal loop
/// (e.g. `WM_QTRS_WAKEUP` reaching `internal_wnd_proc` during a Win32 sizing loop).
///
/// Returns `None` if no event loop is registered for this thread; otherwise the number of
/// delivered events. Nested calls are allowed: a callback of an outer pump may open a native
/// modal loop, and this pump then runs its own turn, so its posted events are delivered before
/// the callback returns (Qt re-enters `sendPostedEvents`). `DeferredDelete` events are always
/// left queued, since a modal loop runs inside an arbitrary native callback stack.
pub fn pump_posted_events_modal() -> Option<usize> {
    let handle = get_thread_event_sender(ThreadId::current())?;
    let _guard = PostedPumpGuard::enter();
    let (delivered, quit_code) = pump_posted_events(&handle.queue, usize::MAX, true);
    if let Some(code) = quit_code {
        handle.exit_requested.store(true, Ordering::SeqCst);
        handle.return_code.store(code, Ordering::SeqCst);
    }
    if !handle.queue.lock().unwrap().is_empty() {
        MODAL_PUMP_LEFT_QUEUED.with(|f| f.set(true));
    }
    Some(delivered)
}

/// Whether a normal pump at `loop_level` delivers `event` now. A `DeferredDelete` requested by a
/// deeper loop waits (see `pump_posted_events`).
fn deliverable_at_loop_level(event: &Event, loop_level: usize) -> bool {
    match event.kind {
        EventKind::DeferredDelete {
            loop_level: event_loop_level,
        } => event_loop_level == 0 || loop_level <= event_loop_level,
        _ => true,
    }
}

fn pump_posted_events(
    queue: &Arc<Mutex<EventQueue>>,
    loop_level: usize,
    modal: bool,
) -> (usize, Option<i32>) {
    {
        let mut q = queue.lock().unwrap();
        q.insertion_offset = q.events.len();
    }

    let mut delivered_count = 0;
    let mut quit_code = None;

    // `insertion_offset` counts this turn's undelivered events at the front of the queue. It
    // shrinks as they leave, so a nested pump that drains them also ends the outer turn (Qt
    // `startOffset`, qcoreapplication.cpp:1815-1858).
    loop {
        let (receiver, mut event) = {
            let mut q = queue.lock().unwrap();
            if q.insertion_offset == 0 {
                break;
            }

            if let EventKind::DeferredDelete {
                loop_level: event_loop_level,
            } = q.events[0].event.kind
            {
                if modal || (event_loop_level > 0 && loop_level > event_loop_level) {
                    // Kept for a later turn: rotate it behind the turn boundary.
                    let deferred = q.remove_at(0);
                    q.insert_posted(deferred);
                    continue;
                }
            }

            let posted = q.remove_at(0);
            (posted.receiver, posted.event)
        };

        if let EventKind::Quit { exit_code } = event.kind {
            quit_code = Some(exit_code);
        }

        send_event(receiver, &mut event);

        if let EventKind::DeferredDelete {
            loop_level: event_loop_level,
        } = event.kind
        {
            if event_loop_level == 0 || loop_level <= event_loop_level {
                // SAFETY: delivery returned, so the callback borrow has ended; deferred
                // deletion is processed on the object's registration thread.
                unsafe { crate::object::unregister_qobject(receiver) };
            }
        }

        delivered_count += 1;
    }

    (delivered_count, quit_code)
}

#[derive(Clone)]
pub struct EventLoopHandle {
    pub dispatcher: Arc<dyn EventDispatcherHandle>,
    pub queue: Arc<Mutex<EventQueue>>,
    pub compressor: Arc<dyn EventCompressor>,
    pub exit_requested: Arc<AtomicBool>,
    pub return_code: Arc<AtomicI32>,
}

impl EventLoopHandle {
    pub fn post_event(&self, receiver: ObjectId, event: Event) {
        self.post_event_with_priority(receiver, event, 0);
    }
    pub fn post_event_with_priority(&self, receiver: ObjectId, event: Event, priority: i32) {
        let mut queue = self.queue.lock().unwrap();
        if run_compress_event(&mut queue.events, receiver, &event, &*self.compressor) {
            return;
        }
        queue.insert_posted(PostedEvent::new(receiver, event, priority));
        self.dispatcher.wake_up();
    }

    pub fn post_quit(&self, receiver: ObjectId, exit_code: i32) {
        self.post_event(receiver, Event::new(EventKind::Quit { exit_code }));
    }

    pub fn wake_up(&self) {
        self.dispatcher.wake_up();
    }

    pub fn exit(&self, return_code: i32) {
        self.exit_requested.store(true, Ordering::SeqCst);
        self.return_code.store(return_code, Ordering::SeqCst);
        self.dispatcher.wake_up();
    }

    pub fn quit(&self) {
        self.exit(0);
    }
}

/// Per-thread posted-event routing: the live loop handle (if any) and events posted before one
/// exists. Qt keeps `postEventList` in `QThreadData`, independent of the event dispatcher
/// (`QCoreApplication::postEvent`, qcoreapplication.cpp:1694), so an event posted to a thread
/// that has no loop yet is queued and delivered once the thread runs events.
struct ThreadEventRegistry {
    handles: HashMap<ThreadId, EventLoopHandle>,
    /// Same ordering and compression rules as a live loop's queue (`EventQueue`), so a thread
    /// that never starts a loop does not accumulate duplicate `UpdateRequest`s.
    pending: HashMap<ThreadId, EventQueue>,
}

static THREAD_EVENT_HANDLES: Mutex<Option<ThreadEventRegistry>> = Mutex::new(None);

impl ThreadEventRegistry {
    fn new() -> Self {
        Self {
            handles: HashMap::new(),
            pending: HashMap::new(),
        }
    }
}

/// Registers the live loop for `thread_id` and delivers, in posting order, every event that was
/// posted to that thread before the loop existed. The drain happens under the registry
/// lock so a concurrent poster cannot overtake the buffered events.
pub fn register_thread_event_loop(thread_id: ThreadId, handle: EventLoopHandle) {
    let mut reg = THREAD_EVENT_HANDLES.lock().unwrap();
    let reg = reg.get_or_insert_with(ThreadEventRegistry::new);
    if let Some(buffered) = reg.pending.remove(&thread_id) {
        for posted in buffered.events {
            handle.post_event_with_priority(posted.receiver, posted.event, posted.priority);
        }
    }
    reg.handles.insert(thread_id, handle);
}

pub fn unregister_thread_event_loop(thread_id: ThreadId) {
    let mut reg = THREAD_EVENT_HANDLES.lock().unwrap();
    if let Some(reg) = reg.as_mut() {
        reg.handles.remove(&thread_id);
    }
}

/// Posts `event` to the thread's event queue. If the thread has no event loop yet the event is
/// held until one registers (see [`register_thread_event_loop`]); it is never dropped.
pub fn post_event_to_thread(thread_id: ThreadId, receiver: ObjectId, event: Event) {
    post_event_to_thread_with_priority(thread_id, receiver, event, 0);
}

pub fn post_event_to_thread_with_priority(
    thread_id: ThreadId,
    receiver: ObjectId,
    event: Event,
    priority: i32,
) {
    // One lock covers "is there a loop?" and "buffer it", so a loop registering concurrently
    // cannot slip between the two and strand the event.
    let handle = {
        let mut reg = THREAD_EVENT_HANDLES.lock().unwrap();
        let reg = reg.get_or_insert_with(ThreadEventRegistry::new);
        match reg.handles.get(&thread_id) {
            Some(handle) => handle.clone(),
            None => {
                reg.pending
                    .entry(thread_id)
                    .or_default()
                    .post_event_with_priority(receiver, event, priority);
                return;
            }
        }
    };
    handle.post_event_with_priority(receiver, event, priority);
}

pub fn get_thread_event_sender(thread_id: ThreadId) -> Option<EventLoopHandle> {
    let reg = THREAD_EVENT_HANDLES.lock().unwrap();
    reg.as_ref().and_then(|r| r.handles.get(&thread_id).cloned())
}

impl Drop for EventLoop {
    fn drop(&mut self) {
        crate::timer::unregister_thread_timer_context();
        unregister_thread_event_loop(ThreadId::current());
    }
}

impl Default for EventLoop {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{delete_later, register_qobject, unregister_qobject, ObjectData, QObject};

    #[test]
    fn test_livelock_prevention() {
        struct LivelockWidget {
            data: ObjectData,
            queue: Arc<Mutex<EventQueue>>,
            call_count: usize,
        }

        impl LivelockWidget {
            fn new(id: ObjectId, queue: Arc<Mutex<EventQueue>>) -> Self {
                Self {
                    data: ObjectData::new(id),
                    queue,
                    call_count: 0,
                }
            }
        }

        impl QObject for LivelockWidget {
            fn object_data(&self) -> &ObjectData {
                &self.data
            }
            fn object_data_mut(&mut self) -> &mut ObjectData {
                &mut self.data
            }
            fn event(&mut self, _event: &mut Event) -> bool {
                self.call_count += 1;
                let mut q = self.queue.lock().unwrap();
                q.events.push(PostedEvent::new(
                    self.data.id,
                    Event::new(EventKind::UpdateRequest),
                    0,
                ));
                true
            }
        }

        let mut event_loop = EventLoop::new();
        let q_clone = Arc::clone(event_loop.queue());

        let mut widget = LivelockWidget::new(ObjectId::next(), q_clone);
        // SAFETY: this test keeps the widget on this thread and alive through unregister.
        unsafe { register_qobject(&mut widget) };

        {
            let mut q = event_loop.queue().lock().unwrap();
            q.events.push(PostedEvent::new(
                widget.object_data().id,
                Event::new(EventKind::UpdateRequest),
                0,
            ));
        }

        event_loop.send_posted_events();

        assert_eq!(widget.call_count, 1);
        assert_eq!(event_loop.queue().lock().unwrap().len(), 1);

        event_loop.send_posted_events();
        assert_eq!(widget.call_count, 2);
        assert_eq!(event_loop.queue().lock().unwrap().len(), 1);

        // SAFETY: the event loop has finished all callbacks for this test object.
        unsafe { unregister_qobject(widget.object_data().id) };
    }

    #[test]
    fn test_lock_release_reentrancy() {
        struct ReentrantWidget {
            data: ObjectData,
            queue: Arc<Mutex<EventQueue>>,
            received: bool,
        }

        impl ReentrantWidget {
            fn new(id: ObjectId, queue: Arc<Mutex<EventQueue>>) -> Self {
                Self {
                    data: ObjectData::new(id),
                    queue,
                    received: false,
                }
            }
        }

        impl QObject for ReentrantWidget {
            fn object_data(&self) -> &ObjectData {
                &self.data
            }
            fn object_data_mut(&mut self) -> &mut ObjectData {
                &mut self.data
            }
            fn event(&mut self, _event: &mut Event) -> bool {
                self.received = true;
                let mut q = self.queue.lock().unwrap();
                q.post_event(self.data.id, Event::new(EventKind::Quit { exit_code: 0 }));
                true
            }
        }

        let mut event_loop = EventLoop::new();
        let q_clone = Arc::clone(event_loop.queue());

        let mut widget = ReentrantWidget::new(ObjectId::next(), q_clone);
        // SAFETY: this test keeps the widget on this thread and alive through unregister.
        unsafe { register_qobject(&mut widget) };

        event_loop.post_event(
            widget.object_data().id,
            Event::new(EventKind::UpdateRequest),
        );

        event_loop.send_posted_events();

        assert!(widget.received);
        assert_eq!(event_loop.queue().lock().unwrap().len(), 1);

        // SAFETY: the event loop has finished all callbacks for this test object.
        unsafe { unregister_qobject(widget.object_data().id) };
    }

    #[test]
    fn test_deferred_delete_loop_level() {
        struct DeleteWidget {
            data: ObjectData,
            deleted: bool,
        }

        impl DeleteWidget {
            fn new(id: ObjectId) -> Self {
                Self {
                    data: ObjectData::new(id),
                    deleted: false,
                }
            }
        }

        impl QObject for DeleteWidget {
            fn object_data(&self) -> &ObjectData {
                &self.data
            }
            fn object_data_mut(&mut self) -> &mut ObjectData {
                &mut self.data
            }
            fn event(&mut self, event: &mut Event) -> bool {
                if matches!(event.kind, EventKind::DeferredDelete { .. }) {
                    self.deleted = true;
                    true
                } else {
                    false
                }
            }
        }

        let mut event_loop = EventLoop::new();
        let mut widget = DeleteWidget::new(ObjectId::next());
        // SAFETY: this test keeps the widget on this thread and alive through unregister.
        unsafe { register_qobject(&mut widget) };

        event_loop.set_loop_level(1);
        let del_ev = delete_later(widget.object_data_mut(), 1).expect("should create delete event");
        event_loop.post_event(widget.object_data().id, del_ev);

        event_loop.set_loop_level(2);
        event_loop.process_events(false);

        assert!(!widget.deleted);
        assert_eq!(event_loop.queue().lock().unwrap().len(), 1);

        event_loop.set_loop_level(1);
        event_loop.process_events(false);
        assert!(widget.deleted);
        assert_eq!(event_loop.queue().lock().unwrap().len(), 0);

        // SAFETY: the deferred-delete callback has returned.
        unsafe { unregister_qobject(widget.object_data().id) };
    }

    #[test]
    fn test_process_events_pumping() {
        let mut event_loop = EventLoop::new();

        let handled = event_loop.process_events(false);
        assert!(!handled);

        let obj_id = ObjectId::next();
        event_loop.post_event(obj_id, Event::new(EventKind::LayoutRequest));
        let handled = event_loop.process_events(false);
        assert!(handled);
    }

    #[test]
    fn test_exec_and_exit() {
        let mut event_loop = EventLoop::new();
        let handle = event_loop.handle();
        let receiver_id = ObjectId::next();

        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            handle.post_quit(receiver_id, 42);
        });

        assert_eq!(event_loop.loop_level(), 0);

        let ret = event_loop.exec();

        assert_eq!(ret, 42);
        assert_eq!(event_loop.loop_level(), 0);
    }

    #[test]
    fn test_exec_exit_method() {
        struct ExitWidget {
            data: ObjectData,
            call_count: usize,
            received_quit: bool,
        }

        impl QObject for ExitWidget {
            fn object_data(&self) -> &ObjectData {
                &self.data
            }
            fn object_data_mut(&mut self) -> &mut ObjectData {
                &mut self.data
            }
            fn event(&mut self, event: &mut Event) -> bool {
                if matches!(event.kind, EventKind::Quit { .. }) {
                    self.received_quit = true;
                }
                self.call_count += 1;
                true
            }
        }

        let mut event_loop = EventLoop::new();
        let mut widget = ExitWidget {
            data: ObjectData::new(ObjectId::next()),
            call_count: 0,
            received_quit: false,
        };
        // SAFETY: this test keeps the widget on this thread and alive through unregister.
        unsafe { register_qobject(&mut widget) };

        event_loop.post_event(
            widget.object_data().id,
            Event::new(EventKind::UpdateRequest),
        );
        event_loop.post_event(
            widget.object_data().id,
            Event::new(EventKind::Quit { exit_code: 99 }),
        );

        let ret = event_loop.exec();
        assert_eq!(ret, 99);
        assert_eq!(widget.call_count, 2);
        assert!(widget.received_quit);
        assert_eq!(event_loop.loop_level(), 0);

        // SAFETY: the event loop has finished all callbacks for this test object.
        unsafe { unregister_qobject(widget.object_data().id) };
    }

    #[test]
    fn test_basic_delivery() {
        struct MockObject {
            data: ObjectData,
            counter: usize,
        }

        impl MockObject {
            fn new(id: ObjectId) -> Self {
                Self {
                    data: ObjectData::new(id),
                    counter: 0,
                }
            }
        }

        impl QObject for MockObject {
            fn object_data(&self) -> &ObjectData {
                &self.data
            }
            fn object_data_mut(&mut self) -> &mut ObjectData {
                &mut self.data
            }
            fn event(&mut self, event: &mut Event) -> bool {
                if matches!(event.kind, EventKind::User(..)) {
                    self.counter += 1;
                    true
                } else {
                    false
                }
            }
        }

        let mut event_loop = EventLoop::new();
        let mut mock = MockObject::new(ObjectId::next());
        // SAFETY: this test keeps the object on this thread and alive through unregister.
        unsafe { register_qobject(&mut mock) };

        event_loop.post_event(
            mock.object_data().id,
            Event::new(EventKind::User(Box::new(42u32))),
        );

        let processed = event_loop.process_events(false);

        assert!(processed);
        assert_eq!(mock.counter, 1);
        assert_eq!(event_loop.queue().lock().unwrap().len(), 0);

        // SAFETY: the event loop has finished all callbacks for this test object.
        unsafe { unregister_qobject(mock.object_data().id) };
    }

    #[test]
    fn test_live_lock_prevention() {
        struct RelayObject {
            data: ObjectData,
            handle: EventLoopHandle,
            call_count: usize,
        }

        impl RelayObject {
            fn new(id: ObjectId, handle: EventLoopHandle) -> Self {
                Self {
                    data: ObjectData::new(id),
                    handle,
                    call_count: 0,
                }
            }
        }

        impl QObject for RelayObject {
            fn object_data(&self) -> &ObjectData {
                &self.data
            }
            fn object_data_mut(&mut self) -> &mut ObjectData {
                &mut self.data
            }
            fn event(&mut self, _event: &mut Event) -> bool {
                self.call_count += 1;
                self.handle
                    .post_event(self.data.id, Event::new(EventKind::UpdateRequest));
                true
            }
        }

        let mut event_loop = EventLoop::new();
        let handle = event_loop.handle();

        let mut relay = RelayObject::new(ObjectId::next(), handle);
        // SAFETY: this test keeps the relay on this thread and alive through unregister.
        unsafe { register_qobject(&mut relay) };

        event_loop.post_event(relay.object_data().id, Event::new(EventKind::UpdateRequest));

        let start = std::time::Instant::now();
        let processed = event_loop.process_events(false);
        let elapsed = start.elapsed();

        assert!(processed);
        assert_eq!(relay.call_count, 1);
        assert_eq!(event_loop.queue().lock().unwrap().len(), 1);
        assert!(elapsed < Duration::from_millis(50));

        // SAFETY: the event loop has finished all callbacks for this test object.
        unsafe { unregister_qobject(relay.object_data().id) };
    }

    #[test]
    fn test_event_compression() {
        struct PaintWidget {
            data: ObjectData,
            paint_count: usize,
        }

        impl PaintWidget {
            fn new(id: ObjectId) -> Self {
                Self {
                    data: ObjectData::new(id),
                    paint_count: 0,
                }
            }
        }

        impl QObject for PaintWidget {
            fn object_data(&self) -> &ObjectData {
                &self.data
            }
            fn object_data_mut(&mut self) -> &mut ObjectData {
                &mut self.data
            }
            fn event(&mut self, event: &mut Event) -> bool {
                if matches!(event.kind, EventKind::UpdateRequest) {
                    self.paint_count += 1;
                    true
                } else {
                    false
                }
            }
        }

        let mut event_loop = EventLoop::new();
        let mut widget = PaintWidget::new(ObjectId::next());
        // SAFETY: this test keeps the widget on this thread and alive through unregister.
        unsafe { register_qobject(&mut widget) };

        let id = widget.object_data().id;

        for _ in 0..10 {
            event_loop.post_event(id, Event::new(EventKind::UpdateRequest));
        }

        assert_eq!(event_loop.queue().lock().unwrap().len(), 1);

        let processed = event_loop.process_events(false);
        assert!(processed);
        assert_eq!(widget.paint_count, 1);
        assert_eq!(event_loop.queue().lock().unwrap().len(), 0);

        // SAFETY: the event loop has finished all callbacks for this test object.
        unsafe { unregister_qobject(id) };
    }

    #[test]
    fn test_cross_thread_wakeup() {
        struct WakeupWidget {
            data: ObjectData,
            received_event: bool,
        }

        impl WakeupWidget {
            fn new(id: ObjectId) -> Self {
                Self {
                    data: ObjectData::new(id),
                    received_event: false,
                }
            }
        }

        impl QObject for WakeupWidget {
            fn object_data(&self) -> &ObjectData {
                &self.data
            }
            fn object_data_mut(&mut self) -> &mut ObjectData {
                &mut self.data
            }
            fn event(&mut self, event: &mut Event) -> bool {
                if matches!(event.kind, EventKind::User(..)) {
                    self.received_event = true;
                    true
                } else {
                    false
                }
            }
        }

        let mut event_loop = EventLoop::new();
        let handle = event_loop.handle();
        let mut widget = WakeupWidget::new(ObjectId::next());
        // SAFETY: this test keeps the widget alive but event is posted cross-thread.
        // QObject callback dispatch remains on this registration thread.
        unsafe { register_qobject(&mut widget) };

        let id = widget.object_data().id;

        let start = std::time::Instant::now();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            handle.post_event(id, Event::new(EventKind::User(Box::new("wakeup"))));
            handle.post_quit(id, 42);
        });

        let ret = event_loop.exec();
        let elapsed = start.elapsed();

        assert_eq!(ret, 42);
        assert!(widget.received_event);
        assert!(elapsed >= Duration::from_millis(15));
        assert_eq!(event_loop.loop_level(), 0);

        // SAFETY: the event loop has finished all callbacks for this test object.
        unsafe { unregister_qobject(id) };
    }

    #[test]
    fn test_notify_helper_pipeline_and_safe_removal() {
        use crate::object::{register_qobject, unregister_qobject, ObjectData};

        struct TraceFilter {
            data: ObjectData,
            trace: Arc<Mutex<Vec<&'static str>>>,
            label: &'static str,
            intercept: bool,
            remove_target: Option<ObjectId>,
        }

        impl TraceFilter {
            fn new(
                label: &'static str,
                intercept: bool,
                remove_target: Option<ObjectId>,
                trace: Arc<Mutex<Vec<&'static str>>>,
            ) -> Self {
                Self {
                    data: ObjectData::new(ObjectId::next()),
                    trace,
                    label,
                    intercept,
                    remove_target,
                }
            }
        }

        impl QObject for TraceFilter {
            fn object_data(&self) -> &ObjectData {
                &self.data
            }
            fn object_data_mut(&mut self) -> &mut ObjectData {
                &mut self.data
            }
            fn event_filter(&mut self, _watched: ObjectId, _event: &mut Event) -> bool {
                self.trace.lock().unwrap().push(self.label);
                if let Some(target) = self.remove_target {
                    crate::object::with_object_mut(target, |obj| {
                        obj.object_data_mut().remove_event_filter(self.data.id);
                    });
                    self.remove_target = None;
                }
                self.intercept
            }
        }

        struct TargetWidget {
            data: ObjectData,
            trace: Arc<Mutex<Vec<&'static str>>>,
        }

        impl TargetWidget {
            fn new(trace: Arc<Mutex<Vec<&'static str>>>) -> Self {
                Self {
                    data: ObjectData::new(ObjectId::next()),
                    trace,
                }
            }
        }

        impl QObject for TargetWidget {
            fn object_data(&self) -> &ObjectData {
                &self.data
            }
            fn object_data_mut(&mut self) -> &mut ObjectData {
                &mut self.data
            }
            fn event(&mut self, _event: &mut Event) -> bool {
                self.trace.lock().unwrap().push("target_event");
                true
            }
        }

        clear_application_event_filters();
        let trace = Arc::new(Mutex::new(Vec::new()));

        let mut app_filter = TraceFilter::new("app_filter", false, None, Arc::clone(&trace));
        // SAFETY: filters remain on this thread and alive until removed/unregistered.
        unsafe { register_qobject(&mut app_filter) };
        install_application_event_filter(app_filter.data.id);

        let mut target = TargetWidget::new(Arc::clone(&trace));
        // SAFETY: target remains on this thread and alive until unregistered.
        unsafe { register_qobject(&mut target) };

        let mut obj_filter = TraceFilter::new(
            "obj_filter",
            false,
            Some(target.data.id),
            Arc::clone(&trace),
        );
        // SAFETY: filter remains on this thread and alive until unregistered.
        unsafe { register_qobject(&mut obj_filter) };
        target
            .object_data_mut()
            .install_event_filter(obj_filter.data.id);

        let mut event1 = Event::new(EventKind::UpdateRequest);
        let handled = send_event(target.data.id, &mut event1);
        assert!(handled);
        assert_eq!(
            *trace.lock().unwrap(),
            vec!["app_filter", "obj_filter", "target_event"]
        );

        trace.lock().unwrap().clear();
        let mut event2 = Event::new(EventKind::UpdateRequest);
        let handled2 = send_event(target.data.id, &mut event2);
        assert!(handled2);
        assert_eq!(*trace.lock().unwrap(), vec!["app_filter", "target_event"]);

        app_filter.intercept = true;
        trace.lock().unwrap().clear();
        let mut event3 = Event::new(EventKind::UpdateRequest);
        let handled3 = send_event(target.data.id, &mut event3);
        assert!(!handled3);
        assert_eq!(*trace.lock().unwrap(), vec!["app_filter"]);

        clear_application_event_filters();
        // SAFETY: the event-loop callback has returned and these registrations are owner-thread.
        unsafe {
            unregister_qobject(app_filter.data.id);
            unregister_qobject(obj_filter.data.id);
            unregister_qobject(target.data.id);
        }
    }

    #[test]
    fn test_post_event_short_circuit_and_compression() {
        struct MockTarget {
            data: ObjectData,
        }
        impl MockTarget {
            fn new(id: ObjectId) -> Self {
                Self {
                    data: ObjectData::new(id),
                }
            }
        }
        impl QObject for MockTarget {
            fn object_data(&self) -> &ObjectData {
                &self.data
            }
            fn object_data_mut(&mut self) -> &mut ObjectData {
                &mut self.data
            }
            fn event(&mut self, _event: &mut Event) -> bool {
                true
            }
        }

        let event_loop = EventLoop::new();
        let mut target = MockTarget::new(ObjectId::next());
        // SAFETY: target stays on this thread and alive until the test ends.
        unsafe { register_qobject(&mut target) };

        let id = target.object_data().id;

        event_loop.post_event(id, Event::new(EventKind::UpdateRequest));
        assert_eq!(event_loop.queue().lock().unwrap().len(), 1);

        event_loop.post_event(id, Event::new(EventKind::UpdateRequest));
        assert_eq!(event_loop.queue().lock().unwrap().len(), 1);

        event_loop.post_event(id, Event::new(EventKind::Timer { timer_id: 10 }));
        assert_eq!(event_loop.queue().lock().unwrap().len(), 2);

        event_loop.post_event(id, Event::new(EventKind::Timer { timer_id: 10 }));

        assert_eq!(event_loop.queue().lock().unwrap().len(), 2);

        // SAFETY: no callback is active and this runs on the registration thread.
        unsafe { unregister_qobject(id) };
    }

    #[test]
    fn test_insertion_offset_priority_ordering() {
        let execution_order = Arc::new(Mutex::new(Vec::new()));

        struct PriorityRelay {
            data: ObjectData,
            order: Arc<Mutex<Vec<&'static str>>>,
            loop_handle: EventLoopHandle,
        }
        impl QObject for PriorityRelay {
            fn object_data(&self) -> &ObjectData {
                &self.data
            }
            fn object_data_mut(&mut self) -> &mut ObjectData {
                &mut self.data
            }
            fn event(&mut self, event: &mut Event) -> bool {
                if let EventKind::User(any_data) = &event.kind {
                    let label = *any_data.downcast_ref::<&'static str>().unwrap();
                    self.order.lock().unwrap().push(label);

                    if label == "initial_event_1" {
                        self.loop_handle.post_event_with_priority(
                            self.data.id,
                            Event::new(EventKind::User(Box::new("high_priority_event"))),
                            100,
                        );
                    }
                    true
                } else {
                    false
                }
            }
        }

        let mut event_loop = EventLoop::new();
        let mut relay = PriorityRelay {
            data: ObjectData::new(ObjectId::next()),
            order: Arc::clone(&execution_order),
            loop_handle: event_loop.handle(),
        };
        // SAFETY: relay stays on this thread and alive until the test ends.
        unsafe { register_qobject(&mut relay) };
        let id = relay.data.id;

        event_loop.post_event_with_priority(
            id,
            Event::new(EventKind::User(Box::new("initial_event_1"))),
            0,
        );
        event_loop.post_event_with_priority(
            id,
            Event::new(EventKind::User(Box::new("initial_event_2"))),
            0,
        );

        let delivered = event_loop.send_posted_events();
        assert_eq!(delivered, 2);
        assert_eq!(
            *execution_order.lock().unwrap(),
            vec!["initial_event_1", "initial_event_2"],
        );

        let delivered2 = event_loop.send_posted_events();
        assert_eq!(delivered2, 1);
        assert_eq!(
            *execution_order.lock().unwrap(),
            vec!["initial_event_1", "initial_event_2", "high_priority_event"]
        );

        // SAFETY: no callback is active and this runs on the registration thread.
        unsafe { unregister_qobject(id) };
    }
    #[test]
    fn test_generic_event_dispatcher_and_event_loop_abstraction() {
        use super::super::dispatcher::{EventDispatcher, GenericEventDispatcher};

        let mut generic_dispatcher = GenericEventDispatcher::new();
        let handle = generic_dispatcher.clone_handle();

        let handle_clone = Arc::clone(&handle);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            handle_clone.wake_up();
        });

        let res = generic_dispatcher.process_events(true, Some(Duration::from_millis(500)));
        assert_eq!(res, DispatchResult::Awoken);

        let el = EventLoop::new();
        let el_handle = el.handle();
        el_handle.wake_up();
    }

    fn meta_call<F: FnOnce() + Send + 'static>(f: F) -> Event {
        Event::new(EventKind::MetaCall(Box::new(move |_| f())))
    }

    fn post_current(event: Event) {
        post_event_to_thread(ThreadId::current(), ObjectId(900_001), event);
    }

    #[test]
    fn test_normal_pump_delivers_posted_events() {
        let mut el = EventLoop::new();
        let log = Arc::new(Mutex::new(Vec::new()));
        for name in ["a", "b"] {
            let log = Arc::clone(&log);
            post_current(meta_call(move || log.lock().unwrap().push(name)));
        }
        assert_eq!(el.send_posted_events(), 2);
        assert_eq!(*log.lock().unwrap(), vec!["a", "b"]);
        assert!(el.queue().lock().unwrap().events.is_empty());
    }

    #[test]
    fn test_modal_pump_keeps_turn_boundary() {
        let el = EventLoop::new();
        let log = Arc::new(Mutex::new(Vec::new()));
        {
            let log_a = Arc::clone(&log);
            let log_c = Arc::clone(&log);
            post_current(meta_call(move || {
                log_a.lock().unwrap().push("a");
                post_current(meta_call(move || log_c.lock().unwrap().push("c")));
            }));
        }
        {
            let log_b = Arc::clone(&log);
            post_current(meta_call(move || log_b.lock().unwrap().push("b")));
        }

        assert_eq!(pump_posted_events_modal(), Some(2));
        assert_eq!(*log.lock().unwrap(), vec!["a", "b"]);
        assert_eq!(
            el.queue().lock().unwrap().events.len(),
            1,
            "C waits for next turn"
        );

        assert_eq!(pump_posted_events_modal(), Some(1));
        assert_eq!(*log.lock().unwrap(), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_modal_pump_without_event_loop_is_noop() {
        assert_eq!(pump_posted_events_modal(), None);
    }

    /// A native modal loop opened from a posted callback (`Menu::exec_popup` from
    /// `Timer::single_shot(0)`) is a nested event loop: Qt delivers posted events from it
    /// before the callback returns (`QMenuPrivate::exec` runs a `QEventLoop`, qmenu.cpp:2691-2697;
    /// `QCoreApplication::sendPostedEvents` re-enters, qcoreapplication.cpp:1796-1817).
    #[test]
    fn test_modal_pump_inside_a_posted_callback_delivers_events_before_it_returns() {
        let mut el = EventLoop::new();
        let nested = Arc::new(Mutex::new(None));
        let log = Arc::new(Mutex::new(Vec::new()));
        {
            let nested = Arc::clone(&nested);
            let log = Arc::clone(&log);
            post_current(meta_call(move || {
                let log_late = Arc::clone(&log);
                post_current(meta_call(move || log_late.lock().unwrap().push("late")));
                *nested.lock().unwrap() = Some(pump_posted_events_modal());
                log.lock().unwrap().push("outer returns");
            }));
        }

        assert_eq!(el.send_posted_events(), 1);
        assert_eq!(*nested.lock().unwrap(), Some(Some(1)));
        assert_eq!(*log.lock().unwrap(), vec!["late", "outer returns"]);
        assert!(el.queue().lock().unwrap().events.is_empty());
    }

    #[test]
    fn test_modal_pump_inside_a_modal_pump_delivers_events_before_it_returns() {
        let el = EventLoop::new();
        let nested = Arc::new(Mutex::new(None));
        let log = Arc::new(Mutex::new(Vec::new()));
        {
            let nested = Arc::clone(&nested);
            let log = Arc::clone(&log);
            post_current(meta_call(move || {
                let log_late = Arc::clone(&log);
                post_current(meta_call(move || log_late.lock().unwrap().push("late")));
                *nested.lock().unwrap() = Some(pump_posted_events_modal());
                log.lock().unwrap().push("outer returns");
            }));
        }

        assert_eq!(pump_posted_events_modal(), Some(1));
        assert_eq!(*nested.lock().unwrap(), Some(Some(1)));
        assert_eq!(*log.lock().unwrap(), vec!["late", "outer returns"]);
        assert!(el.queue().lock().unwrap().events.is_empty());
        assert!(!POSTED_PUMP_ACTIVE.with(|f| f.get()));
    }

    /// The nested pump takes every event queued when it starts (Qt resets `insertionOffset` to
    /// the list size, qcoreapplication.cpp:1817) and delivers each once; the outer pump resumes
    /// from the shared `startOffset` (1815-1816), so it neither repeats them nor runs events
    /// posted during the nested pump, which wait for the next turn (1856-1858).
    #[test]
    fn test_a_nested_pump_delivers_each_event_once_and_leaves_its_own_posts_for_the_next_turn() {
        let mut el = EventLoop::new();
        let log = Arc::new(Mutex::new(Vec::new()));
        {
            let log = Arc::clone(&log);
            post_current(meta_call(move || {
                let _ = pump_posted_events_modal();
                log.lock().unwrap().push("outer returns");
            }));
        }
        {
            let log_b = Arc::clone(&log);
            let log_d = Arc::clone(&log);
            post_current(meta_call(move || {
                log_b.lock().unwrap().push("b");
                post_current(meta_call(move || log_d.lock().unwrap().push("d")));
            }));
        }
        {
            let log = Arc::clone(&log);
            post_current(meta_call(move || log.lock().unwrap().push("c")));
        }

        el.send_posted_events();
        assert_eq!(*log.lock().unwrap(), vec!["b", "c", "outer returns"]);
        assert_eq!(
            el.queue().lock().unwrap().events.len(),
            1,
            "d waits for the next turn"
        );

        el.send_posted_events();
        assert_eq!(*log.lock().unwrap(), vec!["b", "c", "outer returns", "d"]);
        assert!(el.queue().lock().unwrap().events.is_empty());
    }

    /// An object deleted later from the outer loop survives a nested loop and is deleted once
    /// control is back in the loop that requested it (qcoreapplication.cpp:1877-1890).
    #[test]
    fn test_a_deferred_delete_from_the_outer_loop_waits_out_a_nested_pump() {
        struct Deletable {
            data: ObjectData,
            deleted: Arc<AtomicBool>,
        }
        impl QObject for Deletable {
            fn object_data(&self) -> &ObjectData {
                &self.data
            }
            fn object_data_mut(&mut self) -> &mut ObjectData {
                &mut self.data
            }
            fn event(&mut self, event: &mut Event) -> bool {
                if matches!(event.kind, EventKind::DeferredDelete { .. }) {
                    self.deleted.store(true, Ordering::SeqCst);
                    true
                } else {
                    false
                }
            }
        }

        let mut el = EventLoop::new();
        el.set_loop_level(1);
        let deleted = Arc::new(AtomicBool::new(false));
        let mut object = Deletable {
            data: ObjectData::new(ObjectId::next()),
            deleted: Arc::clone(&deleted),
        };
        // SAFETY: the object stays on this thread and alive until the pump unregisters it after
        // delivering its deferred delete.
        unsafe { register_qobject(&mut object) };
        let id = object.data.id;
        let delete_event = delete_later(object.object_data_mut(), 1).expect("delete event");

        let deleted_during_nested = Arc::new(Mutex::new(None));
        {
            let deleted = Arc::clone(&deleted);
            let deleted_during_nested = Arc::clone(&deleted_during_nested);
            post_current(meta_call(move || {
                post_event_to_thread(ThreadId::current(), id, delete_event);
                let _ = pump_posted_events_modal();
                *deleted_during_nested.lock().unwrap() = Some(deleted.load(Ordering::SeqCst));
            }));
        }

        el.send_posted_events();
        assert_eq!(*deleted_during_nested.lock().unwrap(), Some(false));
        el.send_posted_events();
        assert!(deleted.load(Ordering::SeqCst));
        assert!(el.queue().lock().unwrap().events.is_empty());
    }

    #[test]
    fn test_modal_pump_runs_metacall_for_unregistered_receiver() {
        let _el = EventLoop::new();
        let ran = Arc::new(AtomicBool::new(false));
        let ran_clone = Arc::clone(&ran);
        post_current(meta_call(move || ran_clone.store(true, Ordering::SeqCst)));
        assert_eq!(pump_posted_events_modal(), Some(1));
        assert!(ran.load(Ordering::SeqCst));
    }

    #[test]
    fn test_posts_made_during_a_nested_pump_wait_for_the_next_turn_in_priority_order() {
        let mut el = EventLoop::new();
        let log = Arc::new(Mutex::new(Vec::<&'static str>::new()));

        let log_outer = Arc::clone(&log);
        post_current(meta_call(move || {
            let _ = pump_posted_events_modal();
            log_outer.lock().unwrap().push("outer returns");
        }));
        let log_x = Arc::clone(&log);
        post_current(meta_call(move || {
            log_x.lock().unwrap().push("x");
            for (priority, name) in [(0, "p0"), (5, "p5")] {
                let log_p = Arc::clone(&log_x);
                post_event_to_thread_with_priority(
                    ThreadId::current(),
                    ObjectId(900_001),
                    meta_call(move || log_p.lock().unwrap().push(name)),
                    priority,
                );
            }
        }));
        let log_y = Arc::clone(&log);
        post_current(meta_call(move || log_y.lock().unwrap().push("y")));

        // `x` and `y` run inside the nested pump, before the outer callback returns.
        assert_eq!(el.send_posted_events(), 1);
        assert_eq!(*log.lock().unwrap(), vec!["x", "y", "outer returns"]);

        // Posts made during the nested turn wait for the next turn, highest priority first.
        assert_eq!(el.send_posted_events(), 2);
        assert_eq!(
            *log.lock().unwrap(),
            vec!["x", "y", "outer returns", "p5", "p0"]
        );
    }

    #[test]
    fn test_a_priority_post_between_turns_overtakes_a_lower_priority_leftover() {
        let mut el = EventLoop::new();
        let log = Arc::new(Mutex::new(Vec::<&'static str>::new()));

        let log_outer = Arc::clone(&log);
        post_current(meta_call(move || {
            // Posted during the turn, so it stays queued for the next turn at priority 0.
            let log_x = Arc::clone(&log_outer);
            post_event_to_thread_with_priority(
                ThreadId::current(),
                ObjectId(900_001),
                meta_call(move || log_x.lock().unwrap().push("x")),
                0,
            );
        }));
        assert_eq!(el.send_posted_events(), 1);

        let log_y = Arc::clone(&log);
        post_event_to_thread_with_priority(
            ThreadId::current(),
            ObjectId(900_001),
            meta_call(move || log_y.lock().unwrap().push("y")),
            5,
        );
        assert_eq!(el.send_posted_events(), 2);
        assert_eq!(*log.lock().unwrap(), vec!["y", "x"]);
    }
}
