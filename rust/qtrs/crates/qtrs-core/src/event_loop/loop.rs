use crate::event::EventFilterChain;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use crate::object::ThreadId;

use crate::event::{
    compress_event as run_compress_event, CoreCompressor, Event, EventCompressor, EventKind,
};
use crate::object::{send_event, EventSender, ObjectData, ObjectId, QObject};
use crate::timer::TimerRegistry;

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
    create_default_dispatcher, DefaultEventDispatcher, DispatchResult, EventDispatcherHandle,
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

        let posted = PostedEvent::new(receiver, event, priority);
        let start_search = self.insertion_offset.min(self.events.len());
        let relative_idx = self.events[start_search..]
            .iter()
            .position(|e| e.priority < priority)
            .unwrap_or(self.events.len() - start_search);
        let insert_idx = start_search + relative_idx;
        self.events.insert(insert_idx, posted);
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
    pub(crate) exit_requested: bool,
    pub(crate) return_code: i32,
    pub(crate) custom_timeout: Option<Duration>,
}

impl EventLoop {
    pub fn new() -> Self {
        let dispatcher = create_default_dispatcher();
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
            exit_requested: false,
            return_code: 0,
            custom_timeout: None,
        };
        register_thread_event_loop(ThreadId::current(), el.handle());
        el
    }

    /// Creates an event loop with a shared event queue.
    pub fn with_queue(queue: Arc<Mutex<EventQueue>>) -> Self {
        let dispatcher = create_default_dispatcher();
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
            exit_requested: false,
            return_code: 0,
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
        self.exit_requested
    }

    pub fn return_code(&self) -> i32 {
        self.return_code
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
        let posted = PostedEvent::new(receiver, event, priority);
        let start_search = queue.insertion_offset.min(queue.events.len());
        let relative_idx = queue.events[start_search..]
            .iter()
            .position(|e| e.priority < priority)
            .unwrap_or(queue.events.len() - start_search);
        let insert_idx = start_search + relative_idx;
        queue.events.insert(insert_idx, posted);
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
            self.exit_requested = true;
            self.return_code = code;
        }
        delivered
    }

    pub fn process_events(&mut self, can_wait: bool) -> bool {
        let delivered = self.send_posted_events();
        let had_posted = delivered > 0;

        let next_timeout = self.next_timeout();
        let effective_wait = can_wait && !self.exit_requested;

        let res = self.dispatcher.process_events(effective_wait, next_timeout);
        if let DispatchResult::Quit(code) = res {
            self.exit_requested = true;
            self.return_code = code;
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
        while !self.exit_requested {
            self.process_events(true);
        }
        self.loop_level -= 1;
        self.return_code
    }

    pub fn exit(&mut self, return_code: i32) {
        self.exit_requested = true;
        self.return_code = return_code;
        self.dispatcher.wake_up();
    }

    pub fn quit(&mut self) {
        self.exit(0);
    }

    pub fn handle(&self) -> EventLoopHandle {
        EventLoopHandle {
            dispatcher: Arc::new(self.dispatcher.clone_handle()),
            queue: Arc::clone(&self.queue),
            compressor: Arc::clone(&self.compressor),
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

pub fn send_posted_events_for_queue(
    queue: &Arc<Mutex<EventQueue>>,
    loop_level: usize,
) -> (usize, Option<i32>) {
    let max_index = {
        let mut q = queue.lock().unwrap();
        q.insertion_offset = q.events.len();
        q.insertion_offset
    };

    if max_index == 0 {
        return (0, None);
    }

    let mut processed_count = 0;
    let mut delivered_count = 0;
    let mut quit_code = None;

    while processed_count < max_index {
        let (receiver, mut event, should_deliver) = {
            let mut q = queue.lock().unwrap();
            if q.events.is_empty() {
                break;
            }

            if let EventKind::DeferredDelete {
                loop_level: event_loop_level,
            } = q.events[0].event.kind
            {
                if event_loop_level > 0 && loop_level > event_loop_level {
                    let deferred = q.events.remove(0);
                    q.events.push(deferred);
                    processed_count += 1;
                    continue;
                }
            }

            let posted = q.events.remove(0);
            processed_count += 1;
            (posted.receiver, posted.event, true)
        };

        if should_deliver {
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
    }

    (delivered_count, quit_code)
}

#[derive(Clone)]
pub struct EventLoopHandle {
    pub dispatcher: Arc<dyn EventDispatcherHandle>,
    pub queue: Arc<Mutex<EventQueue>>,
    pub compressor: Arc<dyn EventCompressor>,
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
        let posted = PostedEvent::new(receiver, event, priority);
        let start_search = queue.insertion_offset.min(queue.events.len());
        let relative_idx = queue.events[start_search..]
            .iter()
            .position(|e| e.priority < priority)
            .unwrap_or(queue.events.len() - start_search);
        let insert_idx = start_search + relative_idx;
        queue.events.insert(insert_idx, posted);
        self.dispatcher.wake_up();
    }

    pub fn post_quit(&self, receiver: ObjectId, exit_code: i32) {
        self.post_event(receiver, Event::new(EventKind::Quit { exit_code }));
    }

    pub fn wake_up(&self) {
        self.dispatcher.wake_up();
    }
}

static THREAD_EVENT_HANDLES: RwLock<Option<HashMap<ThreadId, EventLoopHandle>>> = RwLock::new(None);

pub fn register_thread_event_loop(thread_id: ThreadId, handle: EventLoopHandle) {
    let mut reg = THREAD_EVENT_HANDLES.write().unwrap();
    if reg.is_none() {
        *reg = Some(HashMap::new());
    }
    if let Some(map) = reg.as_mut() {
        map.insert(thread_id, handle);
    }
}

pub fn unregister_thread_event_loop(thread_id: ThreadId) {
    let mut reg = THREAD_EVENT_HANDLES.write().unwrap();
    if let Some(map) = reg.as_mut() {
        map.remove(&thread_id);
    }
}

pub fn post_event_to_thread(thread_id: ThreadId, receiver: ObjectId, event: Event) -> bool {
    let handle_opt = {
        let reg = THREAD_EVENT_HANDLES.read().unwrap();
        reg.as_ref().and_then(|map| map.get(&thread_id).cloned())
    };

    if let Some(handle) = handle_opt {
        handle.post_event(receiver, event);
        return true;
    }
    false
}

pub fn get_thread_event_sender(thread_id: ThreadId) -> Option<EventLoopHandle> {
    let reg = THREAD_EVENT_HANDLES.read().unwrap();
    reg.as_ref().and_then(|map| map.get(&thread_id).cloned())
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
}
