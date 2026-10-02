use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{Builder as StdThreadBuilder, JoinHandle};
use std::time::Duration;

use crate::event_loop::EventLoop;
use crate::object::{unregister_thread_sender, EventSender, ThreadContext, ThreadId};

/// Thread priority levels, modeled after Qt's `QThread::Priority`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThreadPriority {
    /// Scheduled only when no other threads are running.
    Idle,
    /// Lowest scheduling priority.
    Lowest,
    /// Lower scheduling priority than normal.
    Low,
    /// Default operating system priority.
    #[default]
    Normal,
    /// Higher scheduling priority than normal.
    High,
    /// Highest priority before real-time/time-critical.
    Highest,
    /// Real-time or time-critical priority.
    TimeCritical,
}

thread_local! {
    static CURRENT_INTERRUPTION_FLAG: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

/// Thread management utilities, modeled after Qt's `QThread`.
pub struct Thread;

impl Thread {
    /// Spawns a new OS thread initialized with `ThreadContext`.
    pub fn spawn<F, T>(f: F) -> ThreadHandle<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        ThreadBuilder::new()
            .spawn(f)
            .expect("failed to spawn thread")
    }

    /// Spawns a dedicated thread with an integrated Qt-style event loop.
    pub fn spawn_with_event_loop<F>(setup: F) -> EventLoopThreadHandle
    where
        F: FnOnce(&EventSender) + Send + 'static,
    {
        let (tx, rx) = std::sync::mpsc::channel();
        let interruption_flag = Arc::new(AtomicBool::new(false));
        let flag_clone = Arc::clone(&interruption_flag);

        let handle = StdThreadBuilder::new()
            .name("qtrs-event-thread".to_string())
            .spawn(move || {
                let mut event_loop = EventLoop::new();
                let sender = event_loop.sender();
                let thread_id = ThreadId::current();

                ThreadContext::init_current(false, Some(sender.clone()));
                CURRENT_INTERRUPTION_FLAG.with(|flag| {
                    *flag.borrow_mut() = Some(flag_clone);
                });

                setup(&sender);
                let _ = tx.send((thread_id, sender.clone()));

                let code = event_loop.exec();
                ThreadContext::clear_current();
                unregister_thread_sender(thread_id);
                code
            })
            .expect("failed to spawn event loop thread");

        let (thread_id, sender) = rx.recv().expect("event loop thread failed to initialize");

        EventLoopThreadHandle {
            thread_id,
            sender,
            interruption_flag,
            join_handle: Some(handle),
        }
    }

    /// Creates a builder to configure and spawn a thread.
    pub fn builder() -> ThreadBuilder {
        ThreadBuilder::new()
    }

    /// Returns the current thread's unique [`ThreadId`].
    pub fn current_id() -> ThreadId {
        ThreadContext::current_id()
    }

    /// Returns whether the current thread is the main UI thread.
    pub fn is_main_thread() -> bool {
        ThreadContext::is_main_thread()
    }

    /// Returns whether interruption has been requested on the current thread.
    pub fn is_interruption_requested() -> bool {
        CURRENT_INTERRUPTION_FLAG.with(|flag| {
            flag.borrow()
                .as_ref()
                .map(|f| f.load(Ordering::SeqCst))
                .unwrap_or(false)
        })
    }

    /// Suspends execution for the specified milliseconds, matching `QThread::msleep`.
    pub fn msleep(ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }

    /// Suspends execution for the specified duration, matching `QThread::sleep`.
    pub fn sleep(duration: Duration) {
        std::thread::sleep(duration);
    }

    /// Yields CPU execution to another ready thread, matching `QThread::yieldCurrentThread`.
    pub fn yield_now() {
        std::thread::yield_now();
    }

    /// Returns the ideal number of threads for the system (CPU cores), matching `QThread::idealThreadCount`.
    pub fn ideal_thread_count() -> usize {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    }
}

/// A builder for configuring and spawning OS threads.
pub struct ThreadBuilder {
    name: Option<String>,
    stack_size: Option<usize>,
    priority: ThreadPriority,
}

impl Default for ThreadBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadBuilder {
    /// Creates a default thread builder.
    pub fn new() -> Self {
        Self {
            name: None,
            stack_size: None,
            priority: ThreadPriority::Normal,
        }
    }

    /// Sets the thread's name.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets the thread stack size in bytes.
    pub fn stack_size(mut self, size: usize) -> Self {
        self.stack_size = Some(size);
        self
    }

    /// Sets the thread scheduling priority.
    pub fn priority(mut self, priority: ThreadPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Spawns the thread executing `f`.
    pub fn spawn<F, T>(self, f: F) -> std::io::Result<ThreadHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let mut builder = StdThreadBuilder::new();
        if let Some(name) = self.name {
            builder = builder.name(name);
        }
        if let Some(stack) = self.stack_size {
            builder = builder.stack_size(stack);
        }

        let interruption_flag = Arc::new(AtomicBool::new(false));
        let flag_clone = Arc::clone(&interruption_flag);

        let (id_tx, id_rx) = std::sync::mpsc::channel();

        let handle = builder.spawn(move || {
            let current_id = ThreadId::current();
            let _ = id_tx.send(current_id);

            ThreadContext::init_current(false, None);
            CURRENT_INTERRUPTION_FLAG.with(|flag| {
                *flag.borrow_mut() = Some(flag_clone);
            });

            let res = f();
            ThreadContext::clear_current();
            res
        })?;

        let thread_id = id_rx.recv().unwrap_or_else(|_| ThreadId::current());

        Ok(ThreadHandle {
            thread_id,
            interruption_flag,
            join_handle: Some(handle),
            is_finished: Arc::new(AtomicBool::new(false)),
        })
    }
}

/// A handle to a spawned OS thread.
pub struct ThreadHandle<T> {
    thread_id: ThreadId,
    interruption_flag: Arc<AtomicBool>,
    join_handle: Option<JoinHandle<T>>,
    is_finished: Arc<AtomicBool>,
}

impl<T> ThreadHandle<T> {
    /// Returns the thread's unique identifier.
    pub fn id(&self) -> ThreadId {
        self.thread_id
    }

    /// Requests cooperative interruption of the thread, matching `QThread::requestInterruption`.
    pub fn request_interruption(&self) {
        self.interruption_flag.store(true, Ordering::SeqCst);
    }

    /// Returns whether interruption has been requested.
    pub fn is_interruption_requested(&self) -> bool {
        self.interruption_flag.load(Ordering::SeqCst)
    }

    /// Whether the thread has finished executing.
    pub fn is_finished(&self) -> bool {
        self.is_finished.load(Ordering::SeqCst)
    }

    /// Blocks until the thread finishes and returns its result, matching `QThread::wait`.
    pub fn join(mut self) -> std::thread::Result<T> {
        if let Some(h) = self.join_handle.take() {
            let res = h.join();
            self.is_finished.store(true, Ordering::SeqCst);
            res
        } else {
            panic!("thread already joined");
        }
    }
}

/// A handle to a running thread with an active [`EventLoop`].
pub struct EventLoopThreadHandle {
    thread_id: ThreadId,
    sender: EventSender,
    interruption_flag: Arc<AtomicBool>,
    join_handle: Option<JoinHandle<i32>>,
}

impl EventLoopThreadHandle {
    /// Returns the thread's unique identifier.
    pub fn id(&self) -> ThreadId {
        self.thread_id
    }

    /// Returns an [`EventSender`] for dispatching events to this thread.
    pub fn sender(&self) -> &EventSender {
        &self.sender
    }

    /// Tells the thread's event loop to quit, matching `QThread::quit`.
    pub fn quit(&self) {
        self.interruption_flag.store(true, Ordering::SeqCst);
        let quit_event = crate::event::Event::new(crate::event::EventKind::Quit { exit_code: 0 });
        self.sender
            .post_event(crate::object::ObjectId(0), quit_event);
    }

    /// Requests interruption of the thread.
    pub fn request_interruption(&self) {
        self.interruption_flag.store(true, Ordering::SeqCst);
    }

    /// Returns whether interruption has been requested.
    pub fn is_interruption_requested(&self) -> bool {
        self.interruption_flag.load(Ordering::SeqCst)
    }

    /// Blocks until the thread's event loop exits and thread terminates.
    pub fn join(mut self) -> std::thread::Result<i32> {
        if let Some(h) = self.join_handle.take() {
            h.join()
        } else {
            panic!("event loop thread already joined");
        }
    }
}
