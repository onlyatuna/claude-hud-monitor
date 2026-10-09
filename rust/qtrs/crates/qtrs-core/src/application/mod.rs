use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

use crate::event::Event;
use crate::event_loop::{
    install_application_event_filter, remove_application_event_filter,
    DispatcherKind, EventLoop,
};
use crate::meta::MetaObject;
use crate::object::{
    query_object_thread, unregister_qobject, ObjectData, ObjectId, QObject, ThreadContext, ThreadId,
};
use crate::signal::Signal;

/// Qt application attributes matching `Qt::ApplicationAttribute`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApplicationAttribute {
    AaDontShowIconsInMenus,
    AaNativeWindows,
    AaDontUseNativeMenuBar,
    AaMacDontSwapCtrlAndMeta,
    AaUse96Dpi,
    AaDisableNativeVirtualKeyboard,
    AaSynthesizeTouchForUnhandledMouseEvents,
    AaSynthesizeMouseForUnhandledTouchEvents,
    AaUseHighDpiPixmaps,
    AaForceRasterWidgets,
    AaUseDesktopOpenGL,
    AaUseOpenGLES,
    AaUseSoftwareOpenGL,
    AaShareOpenGLContexts,
    AaSetPalette,
    AaEnableHighDpiScaling,
    AaDisableHighDpiScaling,
    AaUseStyleSheetPropagationInWidgetStyles,
    AaDontUseNativeDialogs,
    AaSynthesizeMouseForUnhandledTabletEvents,
    AaCompressHighFrequencyEvents,
    AaDontCheckOpenGLContextThreadAffinity,
    AaDisableShaderDiskCache,
    AaPluginApplication,
}

/// Global metadata storage for the application.
#[derive(Debug, Default, Clone)]
pub struct ApplicationMetadata {
    pub application_name: String,
    pub application_version: String,
    pub organization_name: String,
    pub organization_domain: String,
    pub arguments: Vec<String>,
    pub library_paths: Vec<PathBuf>,
}

static GLOBAL_METADATA: RwLock<Option<ApplicationMetadata>> = RwLock::new(None);
static GLOBAL_ATTRIBUTES: RwLock<Option<HashSet<ApplicationAttribute>>> = RwLock::new(None);
static APP_INSTANCE_EXISTS: AtomicBool = AtomicBool::new(false);
static APP_OBJECT_ID: RwLock<Option<ObjectId>> = RwLock::new(None);

thread_local! {
    static LOCAL_EVENT_LOOP: RefCell<Option<EventLoop>> = const { RefCell::new(None) };
}

/// Core application singleton equivalent to Qt's `QCoreApplication`.
///
/// Manages the application-wide event loop, metadata, arguments, attributes,
/// and shutdown signals (`about_to_quit`).
pub struct CoreApplication {
    data: ObjectData,
    about_to_quit: Signal<()>,
}

impl CoreApplication {
    /// Initializes a new `CoreApplication` instance with optional command-line arguments.
    ///
    /// # Panics
    /// Panics if a `CoreApplication` or derivative application instance already exists.
    pub fn new(args: Vec<String>) -> Self {
        Self::with_dispatcher_kind(args, DispatcherKind::Core)
    }

    /// Like `new`, with the event loop's dispatcher from the `kind` creation path.
    /// `GuiApplication` passes `DispatcherKind::Gui`, as `QGuiApplication` takes its dispatcher
    /// from the platform plugin.
    pub fn with_dispatcher_kind(args: Vec<String>, kind: DispatcherKind) -> Self {
        APP_INSTANCE_EXISTS.store(true, Ordering::SeqCst);

        // Initialize main thread context
        ThreadContext::init_current(true, None);

        let id = ObjectId::next();
        let app = Self {
            data: ObjectData::new(id),
            about_to_quit: Signal::new(),
        };

        // Initialize metadata if not already set
        {
            let mut meta_guard = GLOBAL_METADATA.write().unwrap();
            let mut meta = meta_guard.take().unwrap_or_default();
            meta.arguments = args;
            if meta.application_name.is_empty() {
                if let Some(first_arg) = meta.arguments.first() {
                    let path = Path::new(first_arg);
                    if let Some(file_stem) = path.file_stem().and_then(|s| s.to_str()) {
                        meta.application_name = file_stem.to_string();
                    }
                }
            }
            if meta.library_paths.is_empty() {
                if let Ok(exe) = std::env::current_exe() {
                    if let Some(dir) = exe.parent() {
                        meta.library_paths.push(dir.to_path_buf());
                    }
                }
            }
            *meta_guard = Some(meta);
        }

        // Setup local thread event loop
        let _ = LOCAL_EVENT_LOOP.try_with(|el| {
            let event_loop = EventLoop::with_dispatcher_kind(kind);
            let handle = event_loop.handle();
            crate::event_loop::register_thread_event_loop(ThreadId::current(), handle);
            *el.borrow_mut() = Some(event_loop);
        });

        *APP_OBJECT_ID.write().unwrap() = Some(id);

        app
    }

    /// Returns `true` if an application instance is currently active.
    pub fn instance_exists() -> bool {
        APP_INSTANCE_EXISTS.load(Ordering::SeqCst)
    }

    /// Returns the `ObjectId` of the application object if it exists.
    pub fn instance_id() -> Option<ObjectId> {
        *APP_OBJECT_ID.read().unwrap()
    }

    /// Signal emitted when the application is about to exit the main event loop (`aboutToQuit`).
    pub fn about_to_quit(&self) -> &Signal<()> {
        &self.about_to_quit
    }

    /// Enters the main event loop and waits until `exit()` or `quit()` is called.
    /// Returns the exit code passed to `exit()`.
    pub fn exec(&mut self) -> i32 {
        let code = LOCAL_EVENT_LOOP.with(|el| {
            if let Some(loop_ref) = el.borrow_mut().as_mut() {
                loop_ref.exec()
            } else {
                0
            }
        });

        // Emit about_to_quit signal on loop exit
        self.about_to_quit.emit(&());
        code
    }

    /// Processes pending events for the calling thread (`QCoreApplication::processEvents`).
    pub fn process_events(can_wait: bool) -> bool {
        LOCAL_EVENT_LOOP.with(|el| {
            if let Ok(mut loop_ref) = el.try_borrow_mut() {
                if let Some(loop_ref) = loop_ref.as_mut() {
                    loop_ref.process_events(can_wait)
                } else {
                    false
                }
            } else {
                false
            }
        })
    }

    /// Tells the application to exit with a return code (`QCoreApplication::exit`).
    pub fn exit(return_code: i32) {
        if let Some(handle) = crate::event_loop::get_thread_event_sender(ThreadId::current()) {
            handle.exit(return_code);
        } else {
            let _ = LOCAL_EVENT_LOOP.try_with(|el| {
                if let Ok(loop_ref) = el.try_borrow() {
                    if let Some(l) = loop_ref.as_ref() {
                        l.exit(return_code);
                    }
                }
            });
        }
    }

    /// Tells the application to exit with return code 0 (`QCoreApplication::quit`).
    pub fn quit() {
        Self::exit(0);
    }

    /// Posts an event to a receiver object with priority.
    pub fn post_event_with_priority(receiver: ObjectId, event: Event, priority: i32) {
        let target_thread = query_object_thread(receiver).unwrap_or_else(ThreadId::current);
        crate::event_loop::post_event_to_thread_with_priority(
            target_thread,
            receiver,
            event,
            priority,
        );
    }

    /// Posts an event to a receiver object with normal priority.
    pub fn post_event(receiver: ObjectId, event: Event) {
        Self::post_event_with_priority(receiver, event, 0);
    }
    /// Sends an event directly to a receiver object synchronously (`QCoreApplication::sendEvent`).
    pub fn send_event(receiver: ObjectId, event: &mut Event) -> bool {
        crate::event_loop::notify_helper(receiver, event)
    }

    /// Installs an application-wide event filter (`QCoreApplication::installEventFilter`).
    pub fn install_event_filter(filter_id: ObjectId) {
        install_application_event_filter(filter_id);
    }

    /// Removes an application-wide event filter (`QCoreApplication::removeEventFilter`).
    pub fn remove_event_filter(filter_id: ObjectId) {
        remove_application_event_filter(filter_id);
    }

    /// Installs a native event filter (`QCoreApplication::installNativeEventFilter`).
    pub fn install_native_event_filter(filter: Box<dyn crate::event::NativeEventFilter>) {
        LOCAL_EVENT_LOOP.with(|el| {
            if let Some(loop_ref) = el.borrow_mut().as_mut() {
                loop_ref.install_native_event_filter(filter);
            }
        });
    }

    // --- Application Metadata ---

    pub fn application_name() -> String {
        GLOBAL_METADATA
            .read()
            .unwrap()
            .as_ref()
            .map(|m| m.application_name.clone())
            .unwrap_or_default()
    }

    pub fn set_application_name(name: impl Into<String>) {
        let mut guard = GLOBAL_METADATA.write().unwrap();
        if let Some(m) = guard.as_mut() {
            m.application_name = name.into();
        } else {
            *guard = Some(ApplicationMetadata {
                application_name: name.into(),
                ..Default::default()
            });
        }
    }

    pub fn application_version() -> String {
        GLOBAL_METADATA
            .read()
            .unwrap()
            .as_ref()
            .map(|m| m.application_version.clone())
            .unwrap_or_default()
    }

    pub fn set_application_version(version: impl Into<String>) {
        let mut guard = GLOBAL_METADATA.write().unwrap();
        if let Some(m) = guard.as_mut() {
            m.application_version = version.into();
        } else {
            *guard = Some(ApplicationMetadata {
                application_version: version.into(),
                ..Default::default()
            });
        }
    }

    pub fn organization_name() -> String {
        GLOBAL_METADATA
            .read()
            .unwrap()
            .as_ref()
            .map(|m| m.organization_name.clone())
            .unwrap_or_default()
    }

    pub fn set_organization_name(name: impl Into<String>) {
        let mut guard = GLOBAL_METADATA.write().unwrap();
        if let Some(m) = guard.as_mut() {
            m.organization_name = name.into();
        } else {
            *guard = Some(ApplicationMetadata {
                organization_name: name.into(),
                ..Default::default()
            });
        }
    }

    pub fn organization_domain() -> String {
        GLOBAL_METADATA
            .read()
            .unwrap()
            .as_ref()
            .map(|m| m.organization_domain.clone())
            .unwrap_or_default()
    }

    pub fn set_organization_domain(domain: impl Into<String>) {
        let mut guard = GLOBAL_METADATA.write().unwrap();
        if let Some(m) = guard.as_mut() {
            m.organization_domain = domain.into();
        } else {
            *guard = Some(ApplicationMetadata {
                organization_domain: domain.into(),
                ..Default::default()
            });
        }
    }

    pub fn arguments() -> Vec<String> {
        GLOBAL_METADATA
            .read()
            .unwrap()
            .as_ref()
            .map(|m| m.arguments.clone())
            .unwrap_or_else(|| std::env::args().collect())
    }

    pub fn application_dir_path() -> PathBuf {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|dir| dir.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn application_file_path() -> PathBuf {
        std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."))
    }

    pub fn application_pid() -> u32 {
        std::process::id()
    }

    // --- Library Paths ---

    pub fn library_paths() -> Vec<PathBuf> {
        GLOBAL_METADATA
            .read()
            .unwrap()
            .as_ref()
            .map(|m| m.library_paths.clone())
            .unwrap_or_default()
    }

    pub fn set_library_paths(paths: Vec<PathBuf>) {
        let mut guard = GLOBAL_METADATA.write().unwrap();
        if let Some(m) = guard.as_mut() {
            m.library_paths = paths;
        }
    }

    pub fn add_library_path(path: PathBuf) {
        let mut guard = GLOBAL_METADATA.write().unwrap();
        if let Some(m) = guard.as_mut() {
            if !m.library_paths.contains(&path) {
                m.library_paths.push(path);
            }
        }
    }

    // --- Application Attributes ---

    pub fn set_attribute(attribute: ApplicationAttribute, on: bool) {
        let mut guard = GLOBAL_ATTRIBUTES.write().unwrap();
        let set = guard.get_or_insert_with(HashSet::new);
        if on {
            set.insert(attribute);
        } else {
            set.remove(&attribute);
        }
    }

    pub fn test_attribute(attribute: ApplicationAttribute) -> bool {
        GLOBAL_ATTRIBUTES
            .read()
            .unwrap()
            .as_ref()
            .map(|set| set.contains(&attribute))
            .unwrap_or(false)
    }

    /// Resets internal singleton flags for testing isolation.
    #[doc(hidden)]
    pub fn reset_for_test() {
        APP_INSTANCE_EXISTS.store(false, Ordering::SeqCst);
        *APP_OBJECT_ID.write().unwrap() = None;
        *GLOBAL_METADATA.write().unwrap() = None;
        *GLOBAL_ATTRIBUTES.write().unwrap() = None;
        let _ = LOCAL_EVENT_LOOP.try_with(|el| {
            *el.borrow_mut() = None;
        });
        crate::event_loop::clear_application_event_filters();
    }
}

impl Drop for CoreApplication {
    fn drop(&mut self) {
        APP_INSTANCE_EXISTS.store(false, Ordering::SeqCst);
        *APP_OBJECT_ID.write().unwrap() = None;
        // SAFETY: Drop is on the application object's owning thread after dispatch ends.
        unsafe { unregister_qobject(self.data.id) };
        let _ = LOCAL_EVENT_LOOP.try_with(|el| {
            *el.borrow_mut() = None;
        });
    }
}

static CORE_APP_META_OBJECT: MetaObject = MetaObject::new(
    "QCoreApplication",
    Some(&crate::meta::QOBJECT_META_OBJECT),
    &[],
    &[],
    &[],
    &[],
);

impl QObject for CoreApplication {
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

    fn meta_object(&self) -> &'static MetaObject {
        &CORE_APP_META_OBJECT
    }
}
