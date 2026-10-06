use crate::event::EventFilterChain;
use crate::variant::Variant;

use super::thread::ThreadId;
use crate::event::{Event, EventKind};
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

thread_local! {
    static QOBJECT_REGISTRY: std::cell::RefCell<HashMap<ObjectId, *mut dyn QObject>> =
        std::cell::RefCell::new(HashMap::new());
}

static GLOBAL_OBJECT_THREADS: RwLock<Option<HashMap<ObjectId, ThreadId>>> = RwLock::new(None);

/// Thread-safe object metadata. It never contains a pointer to the QObject.
/// Registration metadata plus physical-thread and dynamic-borrow enforcement.
pub struct ObjectRecord {
    pub id: ObjectId,
    pub thread_id: RwLock<ThreadId>,
    registration_thread: ThreadId,
    pub parent: RwLock<Option<ObjectId>>,
    pub children: RwLock<Vec<ObjectId>>,
    pub signals_blocked: Arc<AtomicBool>,
    pub delete_later_called: AtomicBool,
    pub delete_later_loop_level: AtomicUsize,
    pub liveness: Arc<AtomicBool>,
    pub generation: AtomicU32,
    borrow_flag: Arc<AtomicBool>,
}

pub struct ObjectBorrowGuard {
    ptr: *mut dyn QObject,
    borrow_flag: Arc<AtomicBool>,
    _entry: Arc<ObjectRecord>,
}

impl std::ops::Deref for ObjectBorrowGuard {
    type Target = dyn QObject;
    fn deref(&self) -> &Self::Target {
        // SAFETY: registration requires pointer stability and exclusive registry access;
        // registered_ptr checked physical thread and acquired this entry's borrow flag.
        unsafe { &*self.ptr }
    }
}

impl std::ops::DerefMut for ObjectBorrowGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: the guard owns the entry's exclusive borrow flag.
        unsafe { &mut *self.ptr }
    }
}

impl Drop for ObjectBorrowGuard {
    fn drop(&mut self) {
        self.borrow_flag.store(false, Ordering::Release);
    }
}

static GLOBAL_OBJECT_REGISTRY: std::sync::LazyLock<RwLock<HashMap<ObjectId, Arc<ObjectRecord>>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

fn registered_ptr(id: ObjectId) -> Option<(Arc<ObjectRecord>, *mut dyn QObject)> {
    let entry = GLOBAL_OBJECT_REGISTRY.read().ok()?.get(&id)?.clone();
    if entry.registration_thread != ThreadId::current() || !entry.liveness.load(Ordering::Acquire) {
        return None;
    }
    let ptr = QOBJECT_REGISTRY.with(|registry| registry.borrow().get(&id).copied())?;
    Some((entry, ptr))
}

fn borrow_registered(id: ObjectId) -> Option<ObjectBorrowGuard> {
    let (entry, ptr) = registered_ptr(id)?;
    entry
        .borrow_flag
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .ok()?;
    Some(ObjectBorrowGuard {
        ptr,
        borrow_flag: Arc::clone(&entry.borrow_flag),
        _entry: entry,
    })
}

/// Calls `f` with the registered QObject on its physical registration thread.
///
/// The callback receives exclusive mutable access; registration's unsafe contract
/// requires that no direct aliases are accessed for the duration of this call.
pub fn with_object_mut<R, F: FnOnce(&mut dyn QObject) -> R>(id: ObjectId, f: F) -> Option<R> {
    let mut guard = borrow_registered(id)?;
    Some(f(&mut *guard))
}

/// Calls `f` with the registered QObject on its physical registration thread.
///
/// Registration's unsafe contract requires that no mutable aliases are accessed
/// for the duration of this call.
pub fn with_object<R, F: FnOnce(&dyn QObject) -> R>(id: ObjectId, f: F) -> Option<R> {
    let guard = borrow_registered(id)?;
    Some(f(&*guard))
}

pub fn dispatch_to_object(receiver: ObjectId, event: &mut Event) -> bool {
    with_object_mut(receiver, |obj| obj.event(event)).unwrap_or(false)
}

/// Registers an object's thread affinity in the global registry.
pub fn register_object_thread(id: ObjectId, thread_id: ThreadId) {
    let mut lock = GLOBAL_OBJECT_THREADS.write().unwrap();
    if lock.is_none() {
        *lock = Some(HashMap::new());
    }
    if let Some(map) = lock.as_mut() {
        map.insert(id, thread_id);
    }
    if let Ok(reg) = GLOBAL_OBJECT_REGISTRY.read() {
        if let Some(entry) = reg.get(&id) {
            *entry.thread_id.write().unwrap() = thread_id;
        }
    }
}

/// Unregisters an object's thread affinity from the global registry.
pub fn unregister_object_thread(id: ObjectId) {
    if let Ok(mut lock) = GLOBAL_OBJECT_THREADS.write() {
        if let Some(map) = lock.as_mut() {
            map.remove(&id);
        }
    }
}

/// Queries the target object's thread affinity across threads.
pub fn query_object_thread(id: ObjectId) -> Option<ThreadId> {
    if let Ok(reg) = GLOBAL_OBJECT_REGISTRY.read() {
        if let Some(entry) = reg.get(&id) {
            return Some(*entry.thread_id.read().unwrap());
        }
    }
    if let Ok(lock) = GLOBAL_OBJECT_THREADS.read() {
        if let Some(map) = lock.as_ref() {
            return map.get(&id).copied();
        }
    }
    None
}

pub fn query_object_signals_blocked(id: ObjectId) -> Option<bool> {
    GLOBAL_OBJECT_REGISTRY
        .read()
        .ok()?
        .get(&id)
        .map(|record| record.signals_blocked.load(Ordering::Acquire))
}

/// # Safety
/// The object must remain alive at the same address until `unregister_qobject` completes.
/// Do not move or drop it while registered. All access from any thread must occur through
/// registry callbacks, except sequential direct access between callbacks on the registration
/// thread. The object must not be unregistered or re-registered while a callback is active.
/// Callbacks execute only on the physical registration thread; thread-affinity changes do not
/// transfer registration. The caller must ensure no overlapping mutable or mutable/shared
/// aliases exist while a callback runs.
pub unsafe fn register_qobject(obj: &mut dyn QObject) {
    let ptr = obj as *mut dyn QObject;
    let data = unsafe { &*ptr }.object_data();
    register_object_metadata(data, ptr);
}

fn register_object_metadata(data: &ObjectData, ptr: *mut dyn QObject) {
    let id = data.id;
    let thread_id = data.thread_id;
    let record = Arc::new(ObjectRecord {
        id,
        thread_id: RwLock::new(thread_id),
        registration_thread: ThreadId::current(),
        parent: RwLock::new(data.parent),
        children: RwLock::new(data.children.clone()),
        signals_blocked: Arc::clone(&data.signals_blocked),
        delete_later_called: AtomicBool::new(data.delete_later_called),
        delete_later_loop_level: AtomicUsize::new(0),
        liveness: Arc::clone(&data.liveness),
        generation: AtomicU32::new(data.generation),
        borrow_flag: Arc::new(AtomicBool::new(false)),
    });
    if let Ok(mut reg) = GLOBAL_OBJECT_REGISTRY.write() {
        if let Some(previous) = reg.insert(id, record) {
            // Re-registering the same object (e.g. `Timer::start` after `stop`, possibly at a new
            // address) shares the liveness token with the replaced record. Killing it would
            // mark the live object dead and null every `QPointer` to it. Only a different
            // object that reused the id is retired.
            if !Arc::ptr_eq(&previous.liveness, &data.liveness) {
                previous.liveness.store(false, Ordering::Release);
            }
        }
    }
    QOBJECT_REGISTRY.with(|registry| {
        registry.borrow_mut().insert(id, ptr);
    });
    register_object_thread(id, thread_id);
}

/// # Safety
/// The pinned object must remain alive and at its pinned address until unregistered.
/// All access and aliasing requirements are identical to [`register_qobject`].
pub unsafe fn register_pinned_qobject(mut obj: std::pin::Pin<&mut dyn QObject>) {
    let ptr = unsafe { obj.as_mut().get_unchecked_mut() as *mut dyn QObject };
    let data = unsafe { (*ptr).object_data() };
    register_object_metadata(data, ptr);
}

/// # Safety
/// The returned Box must remain alive and unmoved until unregistered. All access and aliasing
/// requirements are identical to [`register_qobject`].
pub unsafe fn register_boxed_qobject<T: QObject + 'static>(
    mut boxed: Box<T>,
) -> (ObjectId, Box<T>) {
    let id = boxed.object_data().id;
    let ptr = &mut *boxed as *mut dyn QObject;
    register_object_metadata(boxed.object_data(), ptr);
    (id, boxed)
}

/// Unregisters an object from all registries and cleans up active timers and connections.
///
/// # Safety
/// The caller must ensure that no callback currently holds a borrow of this object, and that
/// no concurrent registry callback can begin until this call completes. The object must remain
/// alive through this call; invoke on the physical registration thread so the thread-local
/// pointer entry is removed.
pub unsafe fn unregister_qobject(id: ObjectId) {
    let _ = QOBJECT_REGISTRY.try_with(|registry| {
        registry.borrow_mut().remove(&id);
    });
    let removed = GLOBAL_OBJECT_REGISTRY
        .write()
        .ok()
        .and_then(|mut reg| reg.remove(&id));
    if let Some(entry) = removed {
        entry.liveness.store(false, Ordering::Release);
        if let Ok(registry) = GLOBAL_OBJECT_REGISTRY.read() {
            if let Some(parent_id) = *entry.parent.read().unwrap() {
                if let Some(parent) = registry.get(&parent_id) {
                    parent
                        .children
                        .write()
                        .unwrap()
                        .retain(|&child_id| child_id != id);
                }
            }
            for child_id in entry.children.read().unwrap().iter() {
                if let Some(child) = registry.get(child_id) {
                    *child.parent.write().unwrap() = None;
                }
            }
        }
    }
    unregister_object_thread(id);
    crate::signal::disconnect_all_for_object(id);
    crate::timer::stop_timers_for_object(id);
}
/// Unique object identifier: ObjectId (`QObject*` equivalent).

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectId(pub u64);

impl ObjectId {
    /// Unique object identifier: ObjectId (`QObject*` equivalent).
    pub fn next() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        ObjectId(NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

/// Core object metadata: ObjectData (`QObjectData` equivalent).
/// Core object metadata: ObjectData (`QObjectData` equivalent).
/// Unique generational object identifier for stale-reference detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GenerationalId {
    pub id: ObjectId,
    pub generation: u32,
}

impl GenerationalId {
    pub fn new(id: ObjectId, generation: u32) -> Self {
        Self { id, generation }
    }
}

/// Core object metadata: ObjectData (`QObjectData` equivalent).
pub struct ObjectData {
    pub id: ObjectId,
    pub parent: Option<ObjectId>,
    pub children: Vec<ObjectId>,

    /// Thread affinity.
    pub thread_id: ThreadId,

    pub delete_later_called: bool,

    pub signals_blocked: Arc<AtomicBool>,

    /// Installed event filters chain.
    pub event_filters: EventFilterChain,

    /// Dynamic properties map (`QObject::dynamicPropertyNames`).
    pub dynamic_properties: HashMap<String, Variant>,

    /// Object name for hierarchy queries (`QObject::objectName`).
    pub object_name: Option<String>,

    /// Shared liveness token for guarded weak references (`QPointer`).
    pub liveness: Arc<AtomicBool>,

    /// Object lifecycle generation counter.
    pub generation: u32,

    /// Directly owned child objects for cascading destruction.
    pub owned_children: Vec<Box<dyn QObject>>,
    pub timers: Vec<crate::timer::TimerId>,

    /// True while this object's `Box` lives in its parent's `owned_children`. Such an object's
    /// lifetime belongs to the parent, so `set_parent` (which only sees `&mut ObjectData`, a
    /// borrow *into* that `Box`) must not detach it; [`reparent_owned`] moves the `Box` safely.
    owned_by_parent: bool,
}

impl fmt::Debug for ObjectData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ObjectData")
            .field("id", &self.id)
            .field("object_name", &self.object_name)
            .field("parent", &self.parent)
            .field("children", &self.children)
            .field("thread_id", &self.thread_id)
            .field("delete_later_called", &self.delete_later_called)
            .field(
                "signals_blocked",
                &self.signals_blocked.load(Ordering::Relaxed),
            )
            .field("generation", &self.generation)
            .field("is_alive", &self.liveness.load(Ordering::Relaxed))
            .field("owned_children_count", &self.owned_children.len())
            .finish()
    }
}
impl Default for ObjectData {
    fn default() -> Self {
        Self::with_auto_id()
    }
}

impl ObjectData {
    /// Core object metadata: ObjectData (`QObjectData` equivalent).
    pub fn new(id: ObjectId) -> Self {
        let thread_id = ThreadId::current();
        register_object_thread(id, thread_id);
        Self {
            id,
            parent: None,
            children: Vec::new(),
            thread_id,
            delete_later_called: false,
            signals_blocked: Arc::new(AtomicBool::new(false)),
            event_filters: EventFilterChain::new(),
            dynamic_properties: HashMap::new(),
            object_name: None,
            liveness: Arc::new(AtomicBool::new(true)),
            generation: 1,
            owned_children: Vec::new(),
            timers: Vec::new(),
            owned_by_parent: false,
        }
    }
    /// Creates a new ObjectData with a unique auto-generated ObjectId.
    #[allow(clippy::new_without_default)]
    pub fn with_auto_id() -> Self {
        Self::new(ObjectId::next())
    }

    /// Core object metadata: ObjectData (`QObjectData` equivalent).
    pub fn with_thread(id: ObjectId, thread_id: ThreadId) -> Self {
        register_object_thread(id, thread_id);
        Self {
            id,
            parent: None,
            children: Vec::new(),
            thread_id,
            delete_later_called: false,
            signals_blocked: Arc::new(AtomicBool::new(false)),
            event_filters: EventFilterChain::new(),
            dynamic_properties: HashMap::new(),
            object_name: None,
            liveness: Arc::new(AtomicBool::new(true)),
            generation: 1,
            owned_children: Vec::new(),
            timers: Vec::new(),
            owned_by_parent: false,
        }
    }

    pub fn delete_later(&mut self, loop_level: usize) -> Option<Event> {
        delete_later(self, loop_level)
    }

    pub fn install_event_filter(&mut self, filter: ObjectId) {
        install_event_filter(self, filter);
    }

    pub fn remove_event_filter(&mut self, filter: ObjectId) {
        remove_event_filter(self, filter);
    }

    /// See [`set_parent`].
    pub fn set_parent(&mut self, new_parent: Option<ObjectId>) -> Result<(), ReparentError> {
        set_parent(self, new_parent)
    }

    /// True while a parent owns this object's `Box` (see [`reparent_owned`]).
    pub fn is_owned_by_parent(&self) -> bool {
        self.owned_by_parent
    }

    pub fn add_child(&mut self, child_id: ObjectId) {
        if !self.children.contains(&child_id) {
            self.children.push(child_id);
        }
    }

    pub fn remove_child(&mut self, child_id: ObjectId) {
        self.children.retain(|&id| id != child_id);
    }
    /// Sets object name for reflection and hierarchy queries (`QObject::setObjectName`).
    pub fn set_object_name(&mut self, name: impl Into<String>) {
        self.object_name = Some(name.into());
    }

    /// Returns object name (`QObject::objectName`).
    pub fn object_name(&self) -> Option<&str> {
        self.object_name.as_deref()
    }

    /// Shared liveness token.
    pub fn liveness(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.liveness)
    }

    /// Returns true if the object has not been dropped.
    pub fn is_alive(&self) -> bool {
        self.liveness.load(Ordering::Acquire)
    }

    /// Current generation index.
    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// Generational identifier for guarded validation.
    pub fn generational_id(&self) -> GenerationalId {
        GenerationalId::new(self.id, self.generation)
    }

    /// Adds an owned child and registers it for owner-thread dispatch.
    ///
    /// # Safety
    /// The child must remain at its allocated address while owned and registered. Do not
    /// access it through any alias during registry callbacks; all registration lifetime and
    /// physical-thread requirements of [`register_qobject`] apply.
    pub unsafe fn add_owned_child<T: QObject + 'static>(&mut self, mut child: Box<T>) -> ObjectId {
        let child_id = child.object_data().id;
        {
            let data = child.object_data_mut();
            data.parent = Some(self.id);
            data.owned_by_parent = true;
        }
        self.add_child(child_id);

        // SAFETY: delegated to this method's caller contract.
        unsafe { register_qobject(&mut *child) };
        self.owned_children.push(child);
        child_id
    }

    /// Recursively cascades deletion to all children matching Qt `QObjectPrivate::deleteChildren`.
    pub fn delete_children(&mut self) {
        for child in &mut self.owned_children {
            child.object_data_mut().parent = None;
        }
        for child_id in std::mem::take(&mut self.children) {
            // SAFETY: child destruction is synchronous on the owning thread; no callback is
            // active for this child while the parent cascades deletion.
            unsafe { unregister_qobject(child_id) };
        }
        self.owned_children.clear();
    }

    /// Unregisters this object from dispatch and global metadata.
    ///
    /// # Safety
    /// Follows [`unregister_qobject`] requirements; no callback may be active for this object.
    pub unsafe fn unregister(&mut self) {
        unsafe { unregister_qobject(self.id) };
    }

    /// Sets a dynamic property value. Returns true if the value was added or changed.
    pub fn set_property(&mut self, name: &str, value: Variant) -> bool {
        let changed = match self.dynamic_properties.get(name) {
            Some(existing) => existing != &value,
            None => true,
        };
        if changed {
            self.dynamic_properties.insert(name.to_string(), value);
        }
        changed
    }

    /// Retrieves a dynamic property by name.
    pub fn property(&self, name: &str) -> Option<&Variant> {
        self.dynamic_properties.get(name)
    }

    /// Returns all registered dynamic property names.
    pub fn dynamic_property_names(&self) -> Vec<String> {
        self.dynamic_properties.keys().cloned().collect()
    }
}

impl Drop for ObjectData {
    fn drop(&mut self) {
        // 1. If we have a parent, unlink from parent and notify it
        if let Some(parent_id) = self.parent {
            with_object_mut(parent_id, |parent_obj| {
                parent_obj
                    .object_data_mut()
                    .children
                    .retain(|&id| id != self.id);
                parent_obj
                    .object_data_mut()
                    .owned_children
                    .retain(|c| c.object_data().id != self.id);
                let mut ev = Event::new(EventKind::ChildRemoved { child_id: self.id });
                parent_obj.event(&mut ev);
            });
        }

        // 2. Cascade deletion to all children (deleteChildren)
        self.delete_children();

        // 3. Mark liveness dead & advance generation for QPointer / GenerationalId safety
        self.liveness.store(false, Ordering::Release);
        self.generation = self.generation.wrapping_add(1);

        // SAFETY: an object registered through the unsafe registration API must be
        // unregistered only after dispatch callbacks have completed; Drop follows that contract.
        unsafe { unregister_qobject(self.id) };
    }
}

/// Marks object for deferred deletion (`QObject::deleteLater`).
/// Marks object for deferred deletion (`QObject::deleteLater`).
pub fn delete_later(obj: &mut ObjectData, loop_level: usize) -> Option<Event> {
    if obj.delete_later_called {
        return None;
    }
    obj.delete_later_called = true;
    Some(Event::new(EventKind::DeferredDelete { loop_level }))
}

/// Event filter callback (`QObject::eventFilter`).
pub fn install_event_filter(target: &mut ObjectData, filter: ObjectId) -> bool {
    // 1. Cannot filter self
    if target.id == filter {
        return false;
    }
    // 2. Cross-thread check: filter and target must be in the same thread (Qt invariant)
    if let Some(filter_thread) = query_object_thread(filter) {
        if filter_thread != target.thread_id {
            return false;
        }
    }
    // 3. Direct cycle prevention: if filter is already watched by target, reject cycle
    let is_watched = with_object(filter, |f| {
        f.object_data().event_filters.contains(target.id)
    })
    .unwrap_or(false);
    if is_watched {
        return false;
    }

    target.event_filters.install(filter);
    true
}

pub fn remove_event_filter(target: &mut ObjectData, filter: ObjectId) {
    target.event_filters.remove(filter);
}

/// Why a parent change was refused. Nothing is changed when an error is returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReparentError {
    /// [`set_parent`]: the object's `Box` is held by its parent's `owned_children`. Detaching it
    /// there would destroy it (or leave the caller's `&mut` dangling); use [`reparent_owned`].
    OwnedByParent,
    /// [`reparent_owned`]: the object has no parent that owns it.
    NotOwned,
    /// The object is not registered, is dead, or is registered on another thread.
    UnknownObject,
    /// The new parent is the object itself, one of its descendants, or not registered.
    InvalidParent,
    /// The object or a parent is borrowed by an active callback.
    Busy,
}

/// Sets a non-owned child's parent and updates registered relationship metadata, sending
/// `ChildRemoved`/`ChildAdded` to the registered parents.
///
/// Refuses with [`ReparentError::OwnedByParent`] when a parent owns the child's `Box`; use
/// [`reparent_owned`] for those. Qt's `setParent_helper` (qobject.cpp:2287-2345) never destroys
/// the child; here the borrow `child` points into the owning `Box`, so ownership has to move
/// through an id-based call.
pub fn set_parent(child: &mut ObjectData, new_parent: Option<ObjectId>) -> Result<(), ReparentError> {
    if child.parent == new_parent {
        return Ok(());
    }
    if child.owned_by_parent {
        return Err(ReparentError::OwnedByParent);
    }
    let child_id = child.id;

    // Update global registry parent/children records.
    if let Ok(registry) = GLOBAL_OBJECT_REGISTRY.read() {
        if let Some(old_record) = child.parent.and_then(|id| registry.get(&id)) {
            old_record
                .children
                .write()
                .unwrap()
                .retain(|&id| id != child_id);
        }
        if let Some(child_record) = registry.get(&child_id) {
            *child_record.parent.write().unwrap() = new_parent;
        }
        if let Some(new_record) = new_parent.and_then(|id| registry.get(&id)) {
            let mut children = new_record.children.write().unwrap();
            if !children.contains(&child_id) {
                children.push(child_id);
            }
        }
    }

    // Update local children lists and send ChildRemoved/ChildAdded via registered callbacks.
    if let Some(old_parent_id) = child.parent {
        with_object_mut(old_parent_id, |obj| {
            obj.object_data_mut().children.retain(|&id| id != child_id);
        });
        let mut ev = Event::new(EventKind::ChildRemoved { child_id });
        dispatch_to_object(old_parent_id, &mut ev);
    }
    child.parent = new_parent;
    if let Some(new_parent_id) = new_parent {
        with_object_mut(new_parent_id, |obj| {
            let data = obj.object_data_mut();
            if !data.children.contains(&child_id) {
                data.children.push(child_id);
            }
        });
        let mut ev = Event::new(EventKind::ChildAdded { child_id });
        dispatch_to_object(new_parent_id, &mut ev);
    }
    Ok(())
}

/// Moves an owned child to `new_parent`, or releases it to the caller when `new_parent` is
/// `None` (`Ok(Some(box))`; the child stays alive and registered, with no parent).
///
/// Unlike Qt's `setParent`, ownership is explicit: the `Box` is the owner, so detaching returns
/// it instead of dropping it. `ChildRemoved`/`ChildAdded` go through `notify_helper`, so object
/// and application event filters see them. Validation and borrows are taken before anything is
/// changed, so an `Err` leaves the hierarchy exactly as it was.
pub fn reparent_owned(
    child_id: ObjectId,
    new_parent: Option<ObjectId>,
) -> Result<Option<Box<dyn QObject>>, ReparentError> {
    let (child_record, _) = registered_ptr(child_id).ok_or(ReparentError::UnknownObject)?;
    // Only the borrow flag is used: the guard is never dereferenced, so no `&mut` aliases the
    // `Box` that is about to move. It rejects a call made from inside the child's own callback.
    let child_guard = borrow_registered(child_id).ok_or(ReparentError::Busy)?;
    let old_parent_id = (*child_record.parent.read().unwrap()).ok_or(ReparentError::NotOwned)?;
    registered_ptr(old_parent_id).ok_or(ReparentError::NotOwned)?;

    if new_parent == Some(old_parent_id) {
        let owned = with_object(old_parent_id, |p| {
            p.object_data()
                .owned_children
                .iter()
                .any(|c| c.object_data().id == child_id)
        })
        .ok_or(ReparentError::Busy)?;
        return if owned { Ok(None) } else { Err(ReparentError::NotOwned) };
    }

    if let Some(np) = new_parent {
        if np == child_id {
            return Err(ReparentError::InvalidParent);
        }
        registered_ptr(np).ok_or(ReparentError::InvalidParent)?;
        // The new parent must not be a descendant of the child (would orphan a cycle).
        let registry = GLOBAL_OBJECT_REGISTRY.read().unwrap();
        let mut cursor = Some(np);
        for _ in 0..=registry.len() {
            let Some(id) = cursor else { break };
            if id == child_id {
                return Err(ReparentError::InvalidParent);
            }
            cursor = registry.get(&id).and_then(|r| *r.parent.read().unwrap());
        }
    }

    let mut old_guard = borrow_registered(old_parent_id).ok_or(ReparentError::Busy)?;
    let mut new_guard = match new_parent {
        Some(np) => Some(borrow_registered(np).ok_or(ReparentError::Busy)?),
        None => None,
    };

    let mut boxed = {
        let data = old_guard.object_data_mut();
        let pos = data
            .owned_children
            .iter()
            .position(|c| c.object_data().id == child_id)
            .ok_or(ReparentError::NotOwned)?;
        data.children.retain(|&id| id != child_id);
        data.owned_children.remove(pos)
    };
    {
        let data = boxed.object_data_mut();
        data.parent = new_parent;
        data.owned_by_parent = new_parent.is_some();
    }

    if let Ok(registry) = GLOBAL_OBJECT_REGISTRY.read() {
        if let Some(old_record) = registry.get(&old_parent_id) {
            old_record
                .children
                .write()
                .unwrap()
                .retain(|&id| id != child_id);
        }
        *child_record.parent.write().unwrap() = new_parent;
        if let Some(new_record) = new_parent.and_then(|id| registry.get(&id)) {
            let mut children = new_record.children.write().unwrap();
            if !children.contains(&child_id) {
                children.push(child_id);
            }
        }
    }

    let released = match new_guard.as_mut() {
        Some(guard) => {
            let data = guard.object_data_mut();
            if !data.children.contains(&child_id) {
                data.children.push(child_id);
            }
            data.owned_children.push(boxed);
            None
        }
        None => Some(boxed),
    };

    drop(new_guard);
    drop(old_guard);
    drop(child_guard);

    let mut removed = Event::new(EventKind::ChildRemoved { child_id });
    crate::event_loop::notify_helper(old_parent_id, &mut removed);
    if let Some(np) = new_parent {
        let mut added = Event::new(EventKind::ChildAdded { child_id });
        crate::event_loop::notify_helper(np, &mut added);
    }
    Ok(released)
}

pub fn reparent(
    child: &mut ObjectData,
    old_parent: Option<&mut ObjectData>,
    new_parent: Option<&mut ObjectData>,
) {
    if let Some(old) = old_parent {
        old.children.retain(|&id| id != child.id);
    }
    if let Some(new_p) = new_parent {
        child.parent = Some(new_p.id);
        if !new_p.children.contains(&child.id) {
            new_p.children.push(child.id);
        }
    } else {
        child.parent = None;
    }
}

/// Core polymorphic object trait: `QObject`.
pub trait QObject: std::any::Any {
    /// Core object metadata: ObjectData (`QObjectData` equivalent).
    fn object_data(&self) -> &ObjectData;
    fn object_data_mut(&mut self) -> &mut ObjectData;
    /// Returns the static MetaObject reflection structure for this class (`QObject::metaObject`).
    fn meta_object(&self) -> &'static crate::meta::MetaObject {
        &crate::meta::QOBJECT_META_OBJECT
    }

    /// Returns true if this object inherits the given class name (`QObject::inherits`).
    fn inherits(&self, class_name: &str) -> bool {
        self.meta_object().inherits_name(class_name)
    }
    /// Optional downcast to Any for hierarchy reflection.
    fn as_qobject_any(&self) -> Option<&dyn std::any::Any> {
        None
    }

    /// Optional mutable downcast to Any for hierarchy reflection.
    fn as_qobject_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        None
    }

    /// Returns object name (`QObject::objectName`).
    fn object_name(&self) -> Option<&str> {
        self.object_data().object_name()
    }

    /// Sets object name (`QObject::setObjectName`).
    fn set_object_name(&mut self, name: &str) {
        self.object_data_mut().set_object_name(name);
    }

    /// Blocks or unblocks signals emitted by this object (`QObject::blockSignals`).
    /// Returns previous blocked state.
    fn block_signals(&mut self, block: bool) -> bool {
        self.object_data()
            .signals_blocked
            .swap(block, Ordering::AcqRel)
    }

    fn signals_blocked(&self) -> bool {
        self.object_data().signals_blocked.load(Ordering::Acquire)
    }

    /// Queries a child object by type ID and optional name (`QObject::findChild`).
    fn find_child_any(&self, name: &str, type_id: std::any::TypeId) -> Option<&dyn std::any::Any> {
        // 1. Search owned children first
        for child in &self.object_data().owned_children {
            let name_matches = name.is_empty() || child.object_name() == Some(name);
            if name_matches {
                if let Some(any) = child.as_qobject_any() {
                    if any.type_id() == type_id {
                        return Some(any);
                    }
                }
            }
            if let Some(found) = child.find_child_any(name, type_id) {
                return Some(found);
            }
        }

        None
    }

    /// Queries the ObjectId of a child object matching type ID and optional name.
    fn find_child_id(&self, name: &str, type_id: std::any::TypeId) -> Option<ObjectId> {
        // 1. Search owned children first
        for child in &self.object_data().owned_children {
            let name_matches = name.is_empty() || child.object_name() == Some(name);
            if name_matches {
                if let Some(any) = child.as_qobject_any() {
                    if any.type_id() == type_id {
                        return Some(child.object_data().id);
                    }
                }
            }
            if let Some(found_id) = child.find_child_id(name, type_id) {
                return Some(found_id);
            }
        }

        None
    }

    fn find_child_any_mut(
        &mut self,
        name: &str,
        type_id: std::any::TypeId,
    ) -> Option<&mut dyn std::any::Any> {
        for child in &mut self.object_data_mut().owned_children {
            let name_matches = name.is_empty() || child.object_name() == Some(name);
            if name_matches
                && child
                    .as_qobject_any()
                    .is_some_and(|a| a.type_id() == type_id)
            {
                return child.as_qobject_any_mut();
            }
            if let Some(found) = child.find_child_any_mut(name, type_id) {
                return Some(found);
            }
        }
        None
    }

    /// Collects all descendant objects matching type ID and optional name (`QObject::findChildren`).
    fn find_children_any(
        &self,
        name: Option<&str>,
        type_id: std::any::TypeId,
    ) -> Vec<&dyn std::any::Any> {
        let mut results = Vec::new();
        let target_name = name.unwrap_or("");

        // 1. Check owned children
        for child in &self.object_data().owned_children {
            let name_matches = target_name.is_empty() || child.object_name() == Some(target_name);
            if name_matches {
                if let Some(any) = child.as_qobject_any() {
                    if any.type_id() == type_id {
                        results.push(any);
                    }
                }
            }
            results.extend(child.find_children_any(name, type_id));
        }

        results
    }

    /// Main event entry point (`QObject::event`).
    fn event(&mut self, event: &mut Event) -> bool {
        match &event.kind {
            EventKind::Timer { timer_id } | EventKind::ZeroTimer { timer_id } => {
                self.timer_event(*timer_id);
                true
            }
            EventKind::DynamicPropertyChange { property_name } => {
                let name = property_name.clone();
                self.dynamic_property_change(&name);
                true
            }
            EventKind::ChildAdded { .. } | EventKind::ChildRemoved { .. } => {
                self.child_event(event);
                true
            }
            EventKind::User(_) => {
                self.custom_event(event);
                true
            }
            _ => false,
        }
    }

    /// Timer event handler (`QObject::timerEvent`).
    fn timer_event(&mut self, _timer_id: u64) {}

    /// Child added / removed event handler (`QObject::childEvent`).
    fn child_event(&mut self, _event: &mut Event) {}

    /// Custom event handler (`QObject::customEvent`).
    fn custom_event(&mut self, _event: &mut Event) {}

    /// Dynamic property change handler (`QObject::event` handling `QDynamicPropertyChangeEvent`).
    fn dynamic_property_change(&mut self, _property_name: &str) {}

    /// Retrieves property value, prioritizing static MetaProperty then dynamic property (`QObject::property`).
    fn property(&self, name: &str) -> Option<Variant> {
        let mo = self.meta_object();
        if let Some(idx) = mo.index_of_property(name) {
            if let Some(prop) = mo.property(idx) {
                if let Some(any) = self.as_qobject_any() {
                    if let Some(val) = prop.read(any) {
                        return Some(val);
                    }
                }
            }
        }
        self.object_data().property(name).cloned()
    }

    /// Sets property value, prioritizing static MetaProperty then fallback to dynamic property (`QObject::setProperty`).
    /// Returns true if property value changed or set successfully.
    fn set_property(&mut self, name: &str, value: Variant) -> bool {
        let mo = self.meta_object();
        if let Some(idx) = mo.index_of_property(name) {
            if let Some(prop) = mo.property(idx).cloned() {
                if !prop.is_writable() {
                    return false;
                }
                if let Some(any_mut) = self.as_qobject_any_mut() {
                    return prop.write(any_mut, value).is_ok();
                }
                return false;
            }
        }

        let changed = self.object_data_mut().set_property(name, value);
        if changed {
            let mut ev = Event::new(EventKind::DynamicPropertyChange {
                property_name: name.to_string(),
            });
            self.event(&mut ev);
        }
        changed
    }

    /// Dynamically invokes a method by signature or name (`QMetaObject::invokeMethod`).
    fn invoke_method(
        &mut self,
        member: &str,
        args: &[Variant],
    ) -> Result<Variant, crate::meta::InvokeError> {
        let mo = self.meta_object();
        if let Some(any_mut) = self.as_qobject_any_mut() {
            mo.invoke_method(any_mut, member, args)
        } else {
            Err(crate::meta::InvokeError::TargetBorrowFailed)
        }
    }

    /// Returns all registered dynamic property names (`QObject::dynamicPropertyNames`).
    fn dynamic_property_names(&self) -> Vec<String> {
        self.object_data().dynamic_property_names()
    }

    /// Event filter handler (`QObject::eventFilter`).
    fn event_filter(&mut self, _watched: ObjectId, _event: &mut Event) -> bool {
        false
    }

    /// Starts a timer and returns a TimerId (`QObject::startTimer`).
    fn start_timer(
        &mut self,
        interval_ms: u64,
        timer_type: crate::timer::TimerType,
    ) -> crate::timer::TimerId {
        let receiver = self.object_data().id;
        let id = crate::timer::start_object_timer(receiver, interval_ms, timer_type);
        self.object_data_mut().timers.push(id);
        id
    }

    /// Kills the timer with the specified timer ID (`QObject::killTimer`).
    fn kill_timer(&mut self, id: crate::timer::TimerId) -> bool {
        self.object_data_mut().timers.retain(|&t| t != id);
        crate::timer::kill_object_timer(id)
    }

    /// Schedules this object for deferred deletion (`QObject::deleteLater`).
    fn delete_later(&mut self, loop_level: usize) -> Option<Event> {
        self.object_data_mut().delete_later(loop_level)
    }
}

/// Generic extension trait for QObject reflection and hierarchy queries (`QObject::findChild<T>`).
pub trait QObjectExt {
    /// Recursively queries a child object by type `T` and optional `name` (`QObject::findChild<T>`).
    fn find_child<T: 'static>(&self, name: &str) -> Option<&T>;

    /// Recursively queries a mutable child object by type `T` and optional `name` (`QObject::findChild<T>`).
    fn find_child_mut<T: 'static>(&mut self, name: &str) -> Option<&mut T>;

    /// Recursively collects all matching descendant objects by type `T` (`QObject::findChildren<T>`).
    fn find_children<T: 'static>(&self, name: Option<&str>) -> Vec<&T>;
}

impl<O: QObject + ?Sized> QObjectExt for O {
    fn find_child<T: 'static>(&self, name: &str) -> Option<&T> {
        self.find_child_any(name, std::any::TypeId::of::<T>())
            .and_then(|any| any.downcast_ref::<T>())
    }

    fn find_child_mut<T: 'static>(&mut self, name: &str) -> Option<&mut T> {
        self.find_child_any_mut(name, std::any::TypeId::of::<T>())
            .and_then(|any| any.downcast_mut::<T>())
    }

    fn find_children<T: 'static>(&self, name: Option<&str>) -> Vec<&T> {
        self.find_children_any(name, std::any::TypeId::of::<T>())
            .into_iter()
            .filter_map(|any| any.downcast_ref::<T>())
            .collect()
    }
}

/// RAII guard for temporarily blocking signals emitted by a QObject (`QSignalBlocker`).
pub struct SignalBlocker {
    signals_blocked: Arc<AtomicBool>,
    previous_state: bool,
    active: bool,
}

impl SignalBlocker {
    pub fn new(obj: &mut dyn QObject) -> Self {
        Self::from_data(obj.object_data_mut())
    }

    pub fn from_data(data: &mut ObjectData) -> Self {
        let signals_blocked = Arc::clone(&data.signals_blocked);
        let previous_state = signals_blocked.swap(true, Ordering::AcqRel);
        Self {
            signals_blocked,
            previous_state,
            active: true,
        }
    }

    pub fn reblock(&mut self) {
        self.signals_blocked.store(true, Ordering::Release);
        self.active = true;
    }

    pub fn unblock(&mut self) {
        self.signals_blocked
            .store(self.previous_state, Ordering::Release);
        self.active = false;
    }
}

impl Drop for SignalBlocker {
    fn drop(&mut self) {
        if self.active {
            self.signals_blocked
                .store(self.previous_state, Ordering::Release);
        }
    }
}

/// Liveness token for a QObject identity. It does not expose references to the QObject.
/// Becomes invalid when the underlying `ObjectData` is dropped.
pub struct QPointer<T: ?Sized> {
    id: ObjectId,
    generation: u32,
    liveness: Arc<AtomicBool>,
    _marker: std::marker::PhantomData<T>,
}

impl<T: ?Sized> QPointer<T> {
    pub fn new<O: QObject>(obj: &O) -> Self {
        let data = obj.object_data();
        Self {
            id: data.id,
            generation: data.generation,
            liveness: Arc::clone(&data.liveness),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn from_data(data: &ObjectData) -> Self {
        Self {
            id: data.id,
            generation: data.generation,
            liveness: Arc::clone(&data.liveness),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn null() -> Self {
        Self {
            id: ObjectId(0),
            generation: 0,
            liveness: Arc::new(AtomicBool::new(false)),
            _marker: std::marker::PhantomData,
        }
    }

    #[inline]
    pub fn is_null(&self) -> bool {
        !self.liveness.load(Ordering::Acquire)
    }

    #[inline]
    pub fn is_valid(&self) -> bool {
        !self.is_null()
    }

    #[inline]
    pub fn id(&self) -> Option<ObjectId> {
        if self.is_null() {
            None
        } else {
            Some(self.id)
        }
    }

    pub fn clear(&mut self) {
        self.liveness = Arc::new(AtomicBool::new(false));
    }
}

impl<T: ?Sized> Clone for QPointer<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            generation: self.generation,
            liveness: Arc::clone(&self.liveness),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T: ?Sized> Default for QPointer<T> {
    fn default() -> Self {
        Self::null()
    }
}

impl<T: ?Sized> fmt::Debug for QPointer<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QPointer")
            .field("id", &self.id)
            .field("generation", &self.generation)
            .field("is_null", &self.is_null())
            .finish()
    }
}

impl<T: ?Sized> PartialEq for QPointer<T> {
    fn eq(&self, other: &Self) -> bool {
        if self.is_null() && other.is_null() {
            true
        } else if self.is_null() || other.is_null() {
            false
        } else {
            self.id == other.id && self.generation == other.generation
        }
    }
}

impl<T: ?Sized> Eq for QPointer<T> {}

/// Sends an event to a receiver object synchronously (`QCoreApplication::sendEvent`).
pub fn send_event(receiver: ObjectId, event: &mut Event) -> bool {
    crate::event_loop::notify_helper(receiver, event)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Timer event handler (`QObject::timerEvent`).
    #[test]
    fn test_timer_event_dispatch() {
        struct MockWidget {
            data: ObjectData,
            received_timer_id: Option<u64>,
        }

        impl MockWidget {
            fn new(id: ObjectId) -> Self {
                Self {
                    data: ObjectData::new(id),
                    received_timer_id: None,
                }
            }
        }

        impl QObject for MockWidget {
            fn object_data(&self) -> &ObjectData {
                &self.data
            }
            fn object_data_mut(&mut self) -> &mut ObjectData {
                &mut self.data
            }
            fn timer_event(&mut self, timer_id: u64) {
                self.received_timer_id = Some(timer_id);
            }
        }

        let mut mock = MockWidget::new(ObjectId::next());
        let mut ev = Event::new(EventKind::Timer { timer_id: 99 });
        let handled = mock.event(&mut ev);

        assert!(handled);
        assert_eq!(mock.received_timer_id, Some(99));
    }

    /// Event filter callback (`QObject::eventFilter`).

    #[test]
    fn test_event_filter_interception() {
        struct EventSnooper {
            data: ObjectData,
            intercept: bool,
        }

        impl EventSnooper {
            fn new(id: ObjectId, intercept: bool) -> Self {
                Self {
                    data: ObjectData::new(id),
                    intercept,
                }
            }
        }

        impl QObject for EventSnooper {
            fn object_data(&self) -> &ObjectData {
                &self.data
            }
            fn object_data_mut(&mut self) -> &mut ObjectData {
                &mut self.data
            }
            fn event_filter(&mut self, _watched: ObjectId, _event: &mut Event) -> bool {
                self.intercept
            }
        }

        struct TargetWidget {
            data: ObjectData,
            event_count: usize,
        }

        impl TargetWidget {
            fn new(id: ObjectId) -> Self {
                Self {
                    data: ObjectData::new(id),
                    event_count: 0,
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
                self.event_count += 1;
                true
            }
        }

        let mut snooper = EventSnooper::new(ObjectId::next(), true);
        let mut target = TargetWidget::new(ObjectId::next());

        install_event_filter(target.object_data_mut(), snooper.object_data().id);

        let mut ev = Event::new(EventKind::UpdateRequest);

        let mut filtered = false;
        for filter_id in target.object_data().event_filters.snapshot() {
            if filter_id == snooper.object_data().id
                && snooper.event_filter(target.object_data().id, &mut ev)
            {
                filtered = true;
                break;
            }
        }

        if !filtered {
            target.event(&mut ev);
        }

        assert_eq!(target.event_count, 0);
    }

    /// Sets the parent object, updating parent-child relationships.

    #[test]
    fn test_borrowed_parent_links_only_update_child_metadata() {
        let parent = ObjectData::new(ObjectId::next());
        let mut child = ObjectData::new(ObjectId::next());

        set_parent(&mut child, Some(parent.id)).unwrap();
        assert_eq!(child.parent, Some(parent.id));
        assert!(parent.children.is_empty());

        let parent2 = ObjectData::new(ObjectId::next());
        set_parent(&mut child, Some(parent2.id)).unwrap();
        assert_eq!(child.parent, Some(parent2.id));
        assert!(parent.children.is_empty());
        assert!(parent2.children.is_empty());

        set_parent(&mut child, None).unwrap();
        assert_eq!(child.parent, None);
        assert!(parent2.children.is_empty());
    }

    /// Marks object for deferred deletion (`QObject::deleteLater`).
    /// Marks object for deferred deletion (`QObject::deleteLater`).
    #[test]
    fn test_delete_later() {
        let mut obj = ObjectData::new(ObjectId::next());

        let event_opt = delete_later(&mut obj, 2);

        assert!(obj.delete_later_called);

        assert!(event_opt.is_some());
        let event = event_opt.unwrap();
        assert!(matches!(
            event.kind,
            EventKind::DeferredDelete { loop_level: 2 }
        ));

        assert!(delete_later(&mut obj, 2).is_none());
    }

    /// Core object metadata: ObjectData (`QObjectData` equivalent).
    #[test]
    fn test_unsafe_registration_dispatches_on_owner_thread() {
        struct EphemeralWidget {
            data: ObjectData,
            events: Arc<AtomicUsize>,
        }
        impl QObject for EphemeralWidget {
            fn object_data(&self) -> &ObjectData {
                &self.data
            }
            fn object_data_mut(&mut self) -> &mut ObjectData {
                &mut self.data
            }
            fn custom_event(&mut self, _event: &mut Event) {
                self.events.fetch_add(1, Ordering::SeqCst);
            }
        }

        let id = ObjectId::next();
        let events = Arc::new(AtomicUsize::new(0));
        let mut widget = EphemeralWidget {
            data: ObjectData::new(id),
            events: Arc::clone(&events),
        };
        // SAFETY: widget remains at this stack address and alive until unregistered; this
        // thread performs all accesses and no direct alias is used during registry callbacks.
        unsafe { register_qobject(&mut widget) };

        assert!(with_object(id, |_| ()).is_some());
        assert!(with_object_mut(id, |_| ()).is_some());
        let mut ev = Event::new(EventKind::User(Box::new(())));
        assert!(dispatch_to_object(id, &mut ev));
        assert_eq!(events.load(Ordering::SeqCst), 1);
        // SAFETY: the last registry callback has completed on this thread.
        unsafe { unregister_qobject(id) };

        let mut ev = Event::new(EventKind::UpdateRequest);
        assert!(!send_event(id, &mut ev));
    }
}
