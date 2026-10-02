use crate::event::Event;
use crate::object::ObjectId;

/// Event filter result matching Qt event filter return semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterResult {
    /// Pass the event to the next filter or target event handler.
    Pass,
    /// Intercept and filter the event, stopping further propagation.
    Filtered,
}

impl FilterResult {
    /// Returns true if the event was intercepted.
    pub fn is_filtered(self) -> bool {
        matches!(self, FilterResult::Filtered)
    }
}

/// Event filter trait. Modeled after `QObject::eventFilter`.
pub trait EventFilter {
    /// Filters events sent to watched object.
    fn event_filter(&mut self, watched: ObjectId, event: &mut Event) -> FilterResult;
}

/// Event filter chain manager (`QObjectPrivate::extraData->eventFilters`).
#[derive(Default, Debug, Clone)]
pub struct EventFilterChain {
    filters: Vec<Option<ObjectId>>,
}

impl EventFilterChain {
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
        }
    }

    /// Installs a filter (LIFO ordering with deduplication).
    pub fn install(&mut self, filter: ObjectId) {
        self.filters.retain(|item| match item {
            Some(id) => *id != filter,
            None => false,
        });
        self.filters.insert(0, Some(filter));
    }

    /// Removes a filter using tombstone marking to avoid invalidating active iterations.
    pub fn remove(&mut self, filter: ObjectId) {
        for slot in &mut self.filters {
            if *slot == Some(filter) {
                *slot = None;
                break;
            }
        }
    }

    /// Returns true if the filter is currently installed in this chain.
    pub fn contains(&self, filter: ObjectId) -> bool {
        self.filters.contains(&Some(filter))
    }

    /// Returns a snapshot of active filter IDs.
    pub fn snapshot(&self) -> Vec<ObjectId> {
        self.filters.iter().filter_map(|&f| f).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.filters.iter().all(|f| f.is_none())
    }
}

/// Native OS message wrapper matching Qt `QAbstractNativeEventFilter`.
pub enum NativeMessage<'a> {
    #[cfg(windows)]
    Windows(&'a windows_sys::Win32::UI::WindowsAndMessaging::MSG),
    #[cfg(target_os = "linux")]
    Xcb(*mut std::ffi::c_void),
    #[cfg(target_os = "macos")]
    Mac(*mut std::ffi::c_void),
    Custom(&'a str, *mut std::ffi::c_void),
}

/// Native OS event filter matching Qt `QAbstractNativeEventFilter`.
pub trait NativeEventFilter: Send + Sync + 'static {
    fn native_event_filter(
        &mut self,
        event_type: &str,
        msg: &NativeMessage,
        result: &mut isize,
    ) -> bool;
}

#[derive(Default)]
pub struct NativeEventFilterChain {
    filters: Vec<Option<Box<dyn NativeEventFilter>>>,
}

impl std::fmt::Debug for NativeEventFilterChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeEventFilterChain")
            .field("filters_count", &self.filters.len())
            .finish()
    }
}

impl NativeEventFilterChain {
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
        }
    }

    pub fn install(&mut self, filter: Box<dyn NativeEventFilter>) {
        self.filters.insert(0, Some(filter));
    }

    /// Filters native messages (`QAbstractEventDispatcher::filterNativeEvent`).
    pub fn filter_native(
        &mut self,
        event_type: &str,
        msg: &NativeMessage,
        result: &mut isize,
    ) -> bool {
        for filter in self.filters.iter_mut().flatten() {
            if filter.native_event_filter(event_type, msg, result) {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventKind;
    use std::cell::RefCell;
    use std::rc::Rc;

    type CallLog = Rc<RefCell<Vec<String>>>;

    #[allow(dead_code)]
    struct MockReceiver {
        id: ObjectId,
        filter_chain: EventFilterChain,
        log: CallLog,
    }

    struct MockFilter {
        name: String,
        should_intercept: bool,
        remove_self_on_event: bool,
        log: CallLog,
    }

    impl MockFilter {
        fn filter(
            &mut self,
            chain: &mut EventFilterChain,
            self_id: ObjectId,
            _event: &mut Event,
        ) -> FilterResult {
            self.log
                .borrow_mut()
                .push(format!("{}: filtered", self.name));

            if self.remove_self_on_event {
                chain.remove(self_id);
            }

            if self.should_intercept {
                FilterResult::Filtered
            } else {
                FilterResult::Pass
            }
        }
    }

    #[test]
    fn test_filter_result_methods() {
        assert!(!FilterResult::Pass.is_filtered());
        assert!(FilterResult::Filtered.is_filtered());
    }

    #[test]
    fn test_filter_pass_and_intercept() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut chain = EventFilterChain::new();

        let filter1_id = ObjectId(1);
        chain.install(filter1_id);

        let mut event = Event::new(EventKind::User(Box::new(1)));
        let mut filter1 = MockFilter {
            name: "Filter1".into(),
            should_intercept: false,
            remove_self_on_event: false,
            log: log.clone(),
        };

        let res = filter1.filter(&mut chain, filter1_id, &mut event);
        assert_eq!(res, FilterResult::Pass);

        let mut filter_intercept = MockFilter {
            name: "FilterIntercept".into(),
            should_intercept: true,
            remove_self_on_event: false,
            log: log.clone(),
        };

        let res = filter_intercept.filter(&mut chain, filter1_id, &mut event);
        assert_eq!(res, FilterResult::Filtered);
    }

    #[test]
    fn test_lifo_order() {
        let mut chain = EventFilterChain::new();

        let id_a = ObjectId(10);
        let id_b = ObjectId(20);
        let id_c = ObjectId(30);

        chain.install(id_a);
        chain.install(id_b);
        chain.install(id_c);

        let snapshot = chain.snapshot();
        assert_eq!(snapshot, vec![id_c, id_b, id_a]);
    }

    #[test]
    fn test_reinstall_deduplication() {
        let mut chain = EventFilterChain::new();

        let id_a = ObjectId(1);
        let id_b = ObjectId(2);

        chain.install(id_a);
        chain.install(id_b);
        chain.install(id_a);

        assert_eq!(chain.snapshot(), vec![id_a, id_b]);
    }

    #[test]
    fn test_tombstone_safe_removal_during_iteration() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut chain = EventFilterChain::new();

        let id_a = ObjectId(1);
        let id_b = ObjectId(2);

        chain.install(id_b);
        chain.install(id_a);

        let mut filter_a = MockFilter {
            name: "FilterA".into(),
            should_intercept: false,
            remove_self_on_event: true,
            log: log.clone(),
        };

        let mut filter_b = MockFilter {
            name: "FilterB".into(),
            should_intercept: false,
            remove_self_on_event: false,
            log: log.clone(),
        };

        let snapshot = chain.snapshot();
        let mut event = Event::new(EventKind::User(Box::new(42)));

        for &fid in &snapshot {
            if fid == id_a {
                let res = filter_a.filter(&mut chain, fid, &mut event);
                if res.is_filtered() {
                    break;
                }
            } else if fid == id_b {
                let res = filter_b.filter(&mut chain, fid, &mut event);
                if res.is_filtered() {
                    break;
                }
            }
        }

        assert_eq!(
            *log.borrow(),
            vec!["FilterA: filtered", "FilterB: filtered"]
        );
        assert_eq!(chain.snapshot(), vec![id_b]);
    }

    #[cfg(windows)]
    #[test]
    fn test_native_event_filter_chain() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MSG, WM_HOTKEY};

        struct HotkeyFilter;
        impl NativeEventFilter for HotkeyFilter {
            fn native_event_filter(
                &mut self,
                event_type: &str,
                msg: &NativeMessage,
                result: &mut isize,
            ) -> bool {
                if event_type == "windows_generic_MSG" {
                    if let NativeMessage::Windows(m) = msg {
                        if m.message == WM_HOTKEY {
                            *result = 42;
                            return true;
                        }
                    }
                }
                false
            }
        }

        let mut chain = NativeEventFilterChain::default();
        chain.install(Box::new(HotkeyFilter));

        let hotkey_msg = MSG {
            hwnd: std::ptr::null_mut(),
            message: WM_HOTKEY,
            wParam: 0,
            lParam: 0,
            time: 0,
            pt: windows_sys::Win32::Foundation::POINT { x: 0, y: 0 },
        };

        let timer_msg = MSG {
            hwnd: std::ptr::null_mut(),
            message: windows_sys::Win32::UI::WindowsAndMessaging::WM_TIMER,
            wParam: 0,
            lParam: 0,
            time: 0,
            pt: windows_sys::Win32::Foundation::POINT { x: 0, y: 0 },
        };

        let mut result = 0;
        assert!(chain.filter_native(
            "windows_generic_MSG",
            &NativeMessage::Windows(&hotkey_msg),
            &mut result
        ));
        assert_eq!(result, 42);

        result = 0;
        assert!(!chain.filter_native(
            "windows_generic_MSG",
            &NativeMessage::Windows(&timer_msg),
            &mut result
        ));
        assert!(!chain.filter_native(
            "other_type",
            &NativeMessage::Windows(&hotkey_msg),
            &mut result
        ));
    }
}
