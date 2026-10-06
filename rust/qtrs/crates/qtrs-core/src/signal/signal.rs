#![allow(clippy::unnecessary_map_or, clippy::bool_assert_comparison)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use crate::object::query_object_thread;
use crate::object::{ObjectId, QObject, ThreadId};

use crate::event::{Event, EventKind};
use crate::event_loop::post_event_to_thread;

thread_local! {
    static CURRENT_SENDER_STACK: RefCell<Vec<ObjectId>> = const { RefCell::new(Vec::new()) };
}

/// Returns the ObjectId of the object that sent the signal currently being handled.
/// Returns None if no signal is actively being emitted.
/// Modeled after Qt's QObject::sender().
pub fn sender() -> Option<ObjectId> {
    CURRENT_SENDER_STACK.with(|stack| stack.borrow().last().copied())
}

/// RAII guard to track current signal emitter in thread-local sender stack.
pub struct SenderGuard(bool);

impl SenderGuard {
    pub fn new(sender_id: Option<ObjectId>) -> Self {
        if let Some(id) = sender_id {
            CURRENT_SENDER_STACK.with(|stack| stack.borrow_mut().push(id));
            SenderGuard(true)
        } else {
            SenderGuard(false)
        }
    }
}

impl Drop for SenderGuard {
    fn drop(&mut self) {
        if self.0 {
            let _ = CURRENT_SENDER_STACK.try_with(|stack| {
                if let Ok(mut borrow) = stack.try_borrow_mut() {
                    borrow.pop();
                }
            });
        }
    }
}

/// Connection type matching Qt::ConnectionType.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ConnectionType {
    /// Direct connection: invoked synchronously in emitter's thread.
    #[default]
    Auto,
    /// Direct connection: invoked synchronously in caller's thread.
    Direct,
    /// Queued connection: posted into receiver's thread event loop.
    Queued,
    /// Blocking queued connection: caller thread blocks until receiver thread finishes execution.
    BlockingQueued,
}

impl ConnectionType {
    pub fn is_auto(&self) -> bool {
        matches!(self, ConnectionType::Auto)
    }

    pub fn is_direct(&self) -> bool {
        matches!(self, ConnectionType::Direct)
    }

    pub fn is_queued(&self) -> bool {
        matches!(self, ConnectionType::Queued)
    }

    pub fn is_blocking_queued(&self) -> bool {
        matches!(self, ConnectionType::BlockingQueued)
    }
}
/// Unique connection identifier: ConnectionId.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConnectionId(pub u64);

impl ConnectionId {
    /// Unique connection identifier: ConnectionId.
    pub fn next() -> Self {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        ConnectionId(NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

/// RAII handle that disconnects the connection when dropped (`QMetaObject::Connection`).
pub struct ScopedConnection {
    id: ConnectionId,
    disconnect_fn: Option<Box<dyn FnOnce(ConnectionId) + Send>>,
}

impl ScopedConnection {
    pub fn new<F>(id: ConnectionId, disconnect_fn: F) -> Self
    where
        F: FnOnce(ConnectionId) + Send + 'static,
    {
        Self {
            id,
            disconnect_fn: Some(Box::new(disconnect_fn)),
        }
    }

    pub fn id(&self) -> ConnectionId {
        self.id
    }

    pub fn release(mut self) -> ConnectionId {
        self.disconnect_fn = None;
        self.id
    }

    pub fn disconnect(mut self) {
        if let Some(f) = self.disconnect_fn.take() {
            f(self.id);
        }
    }
}

impl Drop for ScopedConnection {
    fn drop(&mut self) {
        if let Some(f) = self.disconnect_fn.take() {
            f(self.id);
        }
    }
}

struct ConnectionRecord {
    #[allow(dead_code)]
    id: ConnectionId,
    sender_id: Option<ObjectId>,
    receiver_id: Option<ObjectId>,
    disconnect_fn: Arc<dyn Fn() + Send + Sync>,
}

static GLOBAL_CONNECTIONS: std::sync::LazyLock<RwLock<HashMap<ConnectionId, ConnectionRecord>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

pub fn register_connection(
    id: ConnectionId,
    sender_id: Option<ObjectId>,
    receiver_id: Option<ObjectId>,
    disconnect_fn: Arc<dyn Fn() + Send + Sync>,
) {
    if let Ok(mut conns) = GLOBAL_CONNECTIONS.write() {
        conns.insert(
            id,
            ConnectionRecord {
                id,
                sender_id,
                receiver_id,
                disconnect_fn,
            },
        );
    }
}

pub fn unregister_connection(id: ConnectionId) {
    if let Ok(mut conns) = GLOBAL_CONNECTIONS.write() {
        conns.remove(&id);
    }
}

pub fn disconnect_all_for_object(object_id: ObjectId) {
    let to_disconnect: Vec<Arc<dyn Fn() + Send + Sync>> = {
        if let Ok(mut conns) = GLOBAL_CONNECTIONS.write() {
            let mut matched = Vec::new();
            conns.retain(|_, record| {
                if record.sender_id == Some(object_id) || record.receiver_id == Some(object_id) {
                    matched.push(Arc::clone(&record.disconnect_fn));
                    false
                } else {
                    true
                }
            });
            matched
        } else {
            Vec::new()
        }
    };

    for disc in to_disconnect {
        disc();
    }
}

type QueuedSlotFn<T> =
    Arc<dyn Fn(&T, Option<ObjectId>, Option<ThreadId>, Option<ObjectId>) + Send + Sync + 'static>;

/// Internal slot invocation dispatcher for Direct vs Queued connections.
enum SlotDispatcher<T> {
    /// Direct slot callable synchronously. T does not need Send, Sync, Clone, or 'static.
    Direct(Arc<dyn Fn(&T) + Send + Sync + 'static>),
    /// Queued slot capable of cross-thread posting via cloned payload, while retaining direct invocation for same thread.
    Queued {
        direct_slot: Arc<dyn Fn(&T) + Send + Sync + 'static>,
        queued_fn: QueuedSlotFn<T>,
    },
}

impl<T> Clone for SlotDispatcher<T> {
    fn clone(&self) -> Self {
        match self {
            SlotDispatcher::Direct(s) => SlotDispatcher::Direct(Arc::clone(s)),
            SlotDispatcher::Queued {
                direct_slot,
                queued_fn,
            } => SlotDispatcher::Queued {
                direct_slot: Arc::clone(direct_slot),
                queued_fn: Arc::clone(queued_fn),
            },
        }
    }
}

/// Internal subscriber representation.
struct Subscriber<T> {
    id: ConnectionId,
    receiver_id: Option<ObjectId>,
    receiver_thread: Option<ThreadId>,
    conn_type: ConnectionType,
    dispatcher: SlotDispatcher<T>,
}

impl<T> Clone for Subscriber<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            receiver_id: self.receiver_id,
            receiver_thread: self.receiver_thread,
            conn_type: self.conn_type,
            dispatcher: self.dispatcher.clone(),
        }
    }
}

/// Strongly typed signal (`Signal<T>`). Modeled after Qt signals.
///
/// Unlike standard Rust channels, direct connections in Qt operate synchronously within
/// the caller/emitter's execution context and do **not** require payload types `T`
/// to implement `Send`, `Sync`, `Clone`, or have `'static` lifetime bounds.
///
/// For asynchronous cross-thread or queued connections (`ConnectionType::Queued` / `BlockingQueued`),
/// methods require `T: Clone + Send + 'static`.
pub struct Signal<T> {
    inner: Arc<Mutex<SignalInner<T>>>,
    emitter_id: Option<ObjectId>,
}

/// Type alias for signals explicitly dedicated to queued or cross-thread messaging.
pub type QueuedSignal<T> = Signal<T>;

struct SignalInner<T> {
    subscribers: Vec<Subscriber<T>>,
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            emitter_id: self.emitter_id,
        }
    }
}

impl<T> Default for Signal<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Signal<T> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(SignalInner {
                subscribers: Vec::new(),
            })),
            emitter_id: None,
        }
    }

    /// Creates a signal bound to an emitter QObject.
    /// Emitted calls will be suppressed when emitter object's signals are blocked.
    pub fn with_emitter(emitter_id: ObjectId) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SignalInner {
                subscribers: Vec::new(),
            })),
            emitter_id: Some(emitter_id),
        }
    }

    /// Internal subscriber representation.
    pub fn subscriber_count(&self) -> usize {
        self.inner.lock().unwrap().subscribers.len()
    }

    /// Internal subscriber representation.
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().subscribers.is_empty()
    }

    /// Connects a direct slot executed synchronously when the signal is emitted.
    ///
    /// This connection does **not** require `T` to be `Send`, `Sync`, `Clone`, or `'static`.
    pub fn connect<F>(&self, slot: F) -> ConnectionId
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        self.connect_with_type(ConnectionType::Direct, slot)
    }

    /// Connects a slot with an explicit direct connection type.
    pub fn connect_with_type<F>(&self, conn_type: ConnectionType, slot: F) -> ConnectionId
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        let mut inner = self.inner.lock().unwrap();
        let id = ConnectionId::next();

        inner.subscribers.push(Subscriber {
            id,
            receiver_id: None,
            receiver_thread: None,
            conn_type,
            dispatcher: SlotDispatcher::Direct(Arc::new(slot)),
        });

        id
    }

    /// Disconnects a connection by its ID.
    pub fn disconnect(&self, id: ConnectionId) -> bool {
        let removed = {
            let mut inner = self.inner.lock().unwrap();
            let initial_len = inner.subscribers.len();
            inner.subscribers.retain(|sub| sub.id != id);
            inner.subscribers.len() < initial_len
        };
        // Only the owner of a connection may retire its registry entry.
        if removed {
            unregister_connection(id);
        }
        removed
    }

    /// Disconnects all connections bound to a given receiver object ID.
    pub fn disconnect_receiver(&self, receiver_id: ObjectId) -> usize {
        let mut inner = self.inner.lock().unwrap();
        let initial_len = inner.subscribers.len();
        inner
            .subscribers
            .retain(|sub| sub.receiver_id != Some(receiver_id));
        initial_len - inner.subscribers.len()
    }

    /// Disconnects all connections from this signal.
    pub fn disconnect_all(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.subscribers.clear();
    }

    /// Emits the signal to all connected subscribers.
    /// Modeled after Qt `qobject.cpp:doActivate`.
    pub fn emit(&self, value: &T) {
        if let Some(emitter) = self.emitter_id {
            if let Some(blocked) = crate::object::query_object_signals_blocked(emitter) {
                if blocked {
                    return;
                }
            }
        }

        let _sender_guard = SenderGuard::new(self.emitter_id);

        let snapshot = self.inner.lock().unwrap().subscribers.clone();
        let current_thread = ThreadId::current();

        for sub in snapshot {
            let target_thread = sub
                .receiver_thread
                .or_else(|| sub.receiver_id.and_then(query_object_thread));

            let is_same_thread = target_thread.map_or(true, |thread| thread == current_thread);

            match sub.conn_type {
                ConnectionType::Direct => match sub.dispatcher {
                    SlotDispatcher::Direct(ref slot) => slot(value),
                    SlotDispatcher::Queued {
                        ref direct_slot, ..
                    } => {
                        direct_slot(value);
                    }
                },
                ConnectionType::Auto if is_same_thread => match sub.dispatcher {
                    SlotDispatcher::Direct(ref slot) => slot(value),
                    SlotDispatcher::Queued {
                        ref direct_slot, ..
                    } => {
                        direct_slot(value);
                    }
                },
                _ => match sub.dispatcher {
                    SlotDispatcher::Direct(ref slot) => {
                        slot(value);
                    }
                    SlotDispatcher::Queued { ref queued_fn, .. } => {
                        queued_fn(value, sub.receiver_id, target_thread, self.emitter_id);
                    }
                },
            }
        }
    }
}

impl<T: 'static> Signal<T> {
    /// RAII connection guard: automatically disconnects when dropped.
    pub fn connect_scoped<F>(&self, slot: F) -> ScopedConnection
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        let id = self.connect(slot);
        let signal_weak = Arc::downgrade(&self.inner);
        ScopedConnection::new(id, move |conn_id| {
            if let Some(inner_arc) = signal_weak.upgrade() {
                let mut inner = inner_arc.lock().unwrap();
                inner.subscribers.retain(|sub| sub.id != conn_id);
            }
        })
    }
    /// Connects directly to a receiver object with automatic disconnect on receiver drop,
    /// without requiring `T` to be `Send`, `Sync`, or `Clone`.
    pub fn connect_direct_object<F>(&self, receiver_id: ObjectId, slot: F) -> ConnectionId
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        let mut inner = self.inner.lock().unwrap();
        let id = ConnectionId::next();

        inner.subscribers.push(Subscriber {
            id,
            receiver_id: Some(receiver_id),
            receiver_thread: None,
            conn_type: ConnectionType::Direct,
            dispatcher: SlotDispatcher::Direct(Arc::new(slot)),
        });

        let inner_weak = Arc::downgrade(&self.inner);
        let disconnect_fn = Arc::new(move || {
            if let Some(inner_arc) = inner_weak.upgrade() {
                if let Ok(mut inner) = inner_arc.lock() {
                    inner.subscribers.retain(|sub| sub.id != id);
                }
            }
        });
        register_connection(id, self.emitter_id, Some(receiver_id), disconnect_fn);

        id
    }

    /// Connects directly to a receiver QObject with automatic disconnect on receiver drop,
    /// without requiring `T` to be `Send`, `Sync`, or `Clone`.
    pub fn connect_direct_to<R: QObject + ?Sized, F>(&self, receiver: &R, slot: F) -> ConnectionId
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        let id = receiver.object_data().id;
        self.connect_direct_object(id, slot)
    }
}

impl<T: Clone + Send + 'static> Signal<T> {
    /// Connects to a receiver object with explicit thread context and connection type.
    ///
    /// Requires `T: Clone + Send + 'static` to permit queued cross-thread dispatch.
    pub fn connect_object<F>(
        &self,
        receiver_id: ObjectId,
        receiver_thread: ThreadId,
        conn_type: ConnectionType,
        slot: F,
    ) -> ConnectionId
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        let mut inner = self.inner.lock().unwrap();
        let id = ConnectionId::next();

        let slot_arc = Arc::new(slot);
        let slot_for_queued = Arc::clone(&slot_arc);

        let direct_slot = Arc::clone(&slot_arc);
        let queued_fn = Arc::new(
            move |value: &T,
                  target_receiver: Option<ObjectId>,
                  dest_thread: Option<ThreadId>,
                  emitter: Option<ObjectId>| {
                let value_clone = value.clone();
                let slot = Arc::clone(&slot_for_queued);
                let receiver_id = target_receiver.unwrap_or(ObjectId(0));

                let meta_event = Event::new(EventKind::MetaCall(Box::new(move |_| {
                    let _inner_guard = SenderGuard::new(emitter);
                    slot(&value_clone);
                })));

                if let Some(dest) = dest_thread {
                    post_event_to_thread(dest, receiver_id, meta_event);
                } else {
                    post_event_to_thread(ThreadId::current(), receiver_id, meta_event);
                }
            },
        );
        let dispatcher = SlotDispatcher::Queued {
            direct_slot,
            queued_fn,
        };

        inner.subscribers.push(Subscriber {
            id,
            receiver_id: Some(receiver_id),
            receiver_thread: Some(receiver_thread),
            conn_type,
            dispatcher,
        });

        let inner_weak = Arc::downgrade(&self.inner);
        let disconnect_fn = Arc::new(move || {
            if let Some(inner_arc) = inner_weak.upgrade() {
                if let Ok(mut inner) = inner_arc.lock() {
                    inner.subscribers.retain(|sub| sub.id != id);
                }
            }
        });
        register_connection(id, self.emitter_id, Some(receiver_id), disconnect_fn);

        id
    }

    /// Connects to a receiver QObject with automatic thread affinity tracking.
    ///
    /// When emitted from the same thread, slot is invoked directly.
    /// When emitted from a different thread, automatically queued into receiver's thread event loop.
    pub fn connect_to<R: QObject + ?Sized, F>(&self, receiver: &R, slot: F) -> ConnectionId
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        let id = receiver.object_data().id;
        let thread = receiver.object_data().thread_id;
        self.connect_object(id, thread, ConnectionType::Auto, slot)
    }

    /// Connects to a receiver with automatic dispatch: direct on same thread, queued across threads.
    pub fn connect_auto<F>(
        &self,
        receiver_id: ObjectId,
        receiver_thread: ThreadId,
        slot: F,
    ) -> ConnectionId
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        self.connect_object(receiver_id, receiver_thread, ConnectionType::Auto, slot)
    }

    /// Queued connection: posted as MetaCall event to receiver thread's event loop.
    pub fn connect_queued<F>(&self, receiver_thread: ThreadId, slot: F) -> ConnectionId
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        self.connect_object(ObjectId(0), receiver_thread, ConnectionType::Queued, slot)
    }

    /// Blocking queued connection: emitter thread will block until the receiver's event loop finishes executing slot.
    pub fn connect_blocking_queued<F>(
        &self,
        receiver_id: ObjectId,
        receiver_thread: ThreadId,
        slot: F,
    ) -> ConnectionId
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        let mut inner = self.inner.lock().unwrap();
        let id = ConnectionId::next();

        let slot_arc = Arc::new(slot);
        let slot_for_blocking = Arc::clone(&slot_arc);

        let direct_slot = Arc::clone(&slot_arc);
        let queued_fn = Arc::new(
            move |value: &T,
                  target_receiver: Option<ObjectId>,
                  dest_thread: Option<ThreadId>,
                  emitter: Option<ObjectId>| {
                let current_thread = ThreadId::current();
                let is_same_thread = dest_thread.map_or(true, |d| d == current_thread);
                if is_same_thread {
                    let _inner_guard = SenderGuard::new(emitter);
                    slot_for_blocking(value);
                } else {
                    let value_clone = value.clone();
                    let slot = Arc::clone(&slot_for_blocking);
                    let receiver_id = target_receiver.unwrap_or(ObjectId(0));
                    let (tx, rx) = std::sync::mpsc::channel();

                    let meta_event = Event::new(EventKind::MetaCall(Box::new(move |_| {
                        let _inner_guard = SenderGuard::new(emitter);
                        slot(&value_clone);
                        let _ = tx.send(());
                    })));

                    if let Some(dest) = dest_thread {
                        post_event_to_thread(dest, receiver_id, meta_event);
                    } else {
                        post_event_to_thread(ThreadId::current(), receiver_id, meta_event);
                    }

                    let _ = rx.recv();
                }
            },
        );
        let dispatcher = SlotDispatcher::Queued {
            direct_slot,
            queued_fn,
        };

        inner.subscribers.push(Subscriber {
            id,
            receiver_id: Some(receiver_id),
            receiver_thread: Some(receiver_thread),
            conn_type: ConnectionType::BlockingQueued,
            dispatcher,
        });

        let inner_weak = Arc::downgrade(&self.inner);
        let disconnect_fn = Arc::new(move || {
            if let Some(inner_arc) = inner_weak.upgrade() {
                if let Ok(mut inner) = inner_arc.lock() {
                    inner.subscribers.retain(|sub| sub.id != id);
                }
            }
        });
        register_connection(id, self.emitter_id, Some(receiver_id), disconnect_fn);

        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_connection_type_defaults_and_predicates() {
        let default_conn = ConnectionType::default();
        assert_eq!(default_conn, ConnectionType::Auto);
        assert!(default_conn.is_auto());
        assert!(!default_conn.is_direct());
        assert!(!default_conn.is_queued());

        let direct = ConnectionType::Direct;
        assert!(direct.is_direct());
        assert!(!direct.is_auto());

        let queued = ConnectionType::Queued;
        assert!(queued.is_queued());
        assert!(!queued.is_auto());
    }

    #[test]
    fn test_connection_id_unique() {
        let id1 = ConnectionId::next();
        let id2 = ConnectionId::next();
        assert_ne!(id1, id2);
        assert!(id2 > id1);
    }

    #[test]
    fn test_scoped_connection_raii_drop() {
        let disconnected = Arc::new(AtomicBool::new(false));
        let disc_clone = Arc::clone(&disconnected);
        let id = ConnectionId::next();

        {
            let _conn = ScopedConnection::new(id, move |conn_id| {
                assert_eq!(conn_id, id);
                disc_clone.store(true, Ordering::Release);
            });

            assert!(!disconnected.load(Ordering::Acquire));
        }

        assert!(disconnected.load(Ordering::Acquire));
    }

    #[test]
    fn test_scoped_connection_release() {
        let disconnected = Arc::new(AtomicBool::new(false));
        let disc_clone = Arc::clone(&disconnected);
        let id = ConnectionId::next();

        {
            let conn = ScopedConnection::new(id, move |_| {
                disc_clone.store(true, Ordering::Release);
            });
            let released_id = conn.release();
            assert_eq!(released_id, id);
        }

        assert!(!disconnected.load(Ordering::Acquire));
    }

    #[test]
    fn test_scoped_connection_manual_disconnect() {
        let disconnected = Arc::new(AtomicBool::new(false));
        let disc_clone = Arc::clone(&disconnected);
        let id = ConnectionId::next();

        let conn = ScopedConnection::new(id, move |_| {
            disc_clone.store(true, Ordering::Release);
        });
        assert!(!disconnected.load(Ordering::Acquire));

        conn.disconnect();
        assert!(disconnected.load(Ordering::Acquire));
    }

    #[test]
    fn test_signal_connect_and_disconnect() {
        let signal: Signal<i32> = Signal::new();
        assert_eq!(signal.subscriber_count(), 0);
        assert!(signal.is_empty());

        let id1 = signal.connect(|val| {
            let _ = val;
        });
        assert_eq!(signal.subscriber_count(), 1);
        assert!(!signal.is_empty());

        let id2 = signal.connect_with_type(ConnectionType::Direct, |_| {});
        assert_eq!(signal.subscriber_count(), 2);

        let disc = signal.disconnect(id1);
        assert!(disc);
        assert_eq!(signal.subscriber_count(), 1);

        assert!(!signal.disconnect(id1));

        assert!(signal.disconnect(id2));
        assert_eq!(signal.subscriber_count(), 0);
    }

    #[test]
    fn test_signal_scoped_connection() {
        let signal: Signal<String> = Signal::new();
        assert_eq!(signal.subscriber_count(), 0);

        {
            let _scoped = signal.connect_scoped(|_| {});
            assert_eq!(signal.subscriber_count(), 1);
        }

        assert_eq!(signal.subscriber_count(), 0);
    }

    #[test]
    fn test_signal_disconnect_receiver() {
        let signal: Signal<u64> = Signal::new();
        let receiver1 = ObjectId::next();
        let receiver2 = ObjectId::next();
        let thread_id = ThreadId::current();

        signal.connect_object(receiver1, thread_id, ConnectionType::Auto, |_| {});
        signal.connect_object(receiver1, thread_id, ConnectionType::Direct, |_| {});
        signal.connect_object(receiver2, thread_id, ConnectionType::Queued, |_| {});
        assert_eq!(signal.subscriber_count(), 3);

        let count = signal.disconnect_receiver(receiver1);
        assert_eq!(count, 2);
        assert_eq!(signal.subscriber_count(), 1);

        signal.disconnect_all();
        assert_eq!(signal.subscriber_count(), 0);
    }

    #[test]
    fn test_signal_emit_direct_and_auto_same_thread() {
        let signal: Signal<i32> = Signal::new();
        let received_auto = Arc::new(Mutex::new(Vec::new()));
        let received_direct = Arc::new(Mutex::new(Vec::new()));

        let a_clone = Arc::clone(&received_auto);
        signal.connect(move |val| {
            a_clone.lock().unwrap().push(*val);
        });

        let d_clone = Arc::clone(&received_direct);
        signal.connect_with_type(ConnectionType::Direct, move |val| {
            d_clone.lock().unwrap().push(*val);
        });

        signal.emit(&42);
        signal.emit(&100);

        assert_eq!(*received_auto.lock().unwrap(), vec![42, 100]);
        assert_eq!(*received_direct.lock().unwrap(), vec![42, 100]);
    }

    #[test]
    fn test_signal_emit_highest_id_protection() {
        let signal: Signal<i32> = Signal::new();
        let signal_clone = signal.clone();

        let dynamic_called = Arc::new(AtomicBool::new(false));
        let dyn_clone = Arc::clone(&dynamic_called);

        let round1_count = Arc::new(Mutex::new(0));
        let r1_clone = Arc::clone(&round1_count);

        signal.connect(move |_val| {
            *r1_clone.lock().unwrap() += 1;

            let dyn_target = Arc::clone(&dyn_clone);
            signal_clone.connect(move |_| {
                dyn_target.store(true, Ordering::Release);
            });
        });

        signal.emit(&1);
        assert_eq!(*round1_count.lock().unwrap(), 1);

        assert!(!dynamic_called.load(Ordering::Acquire));

        signal.emit(&2);
        assert_eq!(*round1_count.lock().unwrap(), 2);
        assert!(dynamic_called.load(Ordering::Acquire));
    }

    #[test]
    fn test_signal_emit_queued_dispatch() {
        use crate::event_loop::EventLoop;

        let signal: Signal<String> = Signal::new();
        let receiver_id = ObjectId::next();
        let target_thread = ThreadId::current();

        let received_msg = Arc::new(Mutex::new(String::new()));
        let msg_clone = Arc::clone(&received_msg);

        let mut event_loop = EventLoop::new();

        signal.connect_object(
            receiver_id,
            target_thread,
            ConnectionType::Queued,
            move |msg| {
                *msg_clone.lock().unwrap() = msg.clone();
            },
        );

        signal.emit(&"hello queued".to_string());

        assert_eq!(*received_msg.lock().unwrap(), "");

        let processed = event_loop.process_events(false);
        assert!(processed);

        assert_eq!(*received_msg.lock().unwrap(), "hello queued");
    }

    #[test]
    fn test_basic_emission() {
        let signal = Signal::<i32>::new();
        let received = Arc::new(Mutex::new(Vec::new()));

        let r1 = received.clone();
        signal.connect(move |val| {
            r1.lock().unwrap().push(*val * 2);
        });

        let r2 = received.clone();
        signal.connect(move |val| {
            r2.lock().unwrap().push(*val + 1);
        });

        signal.emit(&10);

        assert_eq!(*received.lock().unwrap(), vec![20, 11]);
    }

    #[test]
    fn test_scoped_connection_auto_disconnect() {
        let signal = Signal::<&str>::new();
        let count = Arc::new(AtomicUsize::new(0));

        {
            let c = count.clone();

            let _guard = signal.connect_scoped(move |_| {
                c.fetch_add(1, Ordering::SeqCst);
            });

            signal.emit(&"first");
            assert_eq!(count.load(Ordering::SeqCst), 1);
        }

        signal.emit(&"second");
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_reentrancy_and_highest_id_guard() {
        let signal = Arc::new(Signal::<i32>::new());
        let dynamic_called = Arc::new(AtomicBool::new(false));

        let sig_clone = signal.clone();
        let dyn_flag = dynamic_called.clone();

        signal.connect(move |_| {
            let flag = dyn_flag.clone();
            sig_clone.connect(move |_| {
                flag.store(true, Ordering::SeqCst);
            });
        });

        signal.emit(&1);
        assert_eq!(dynamic_called.load(Ordering::SeqCst), false);

        signal.emit(&2);
        assert_eq!(dynamic_called.load(Ordering::SeqCst), true);
    }

    #[test]
    fn test_concurrent_emission() {
        let signal = Arc::new(Signal::<usize>::new());
        let sum = Arc::new(AtomicUsize::new(0));

        let s = sum.clone();
        signal.connect(move |val| {
            s.fetch_add(*val, Ordering::SeqCst);
        });

        let mut handles = vec![];

        for i in 1..=10 {
            let sig = signal.clone();
            handles.push(std::thread::spawn(move || {
                sig.emit(&i);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // 1 + 2 + ... + 10 = 55
        assert_eq!(sum.load(Ordering::SeqCst), 55);
    }
}
