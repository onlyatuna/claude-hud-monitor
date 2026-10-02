pub mod dispatcher;
pub mod socket_notifier;
pub use socket_notifier::*;

pub use dispatcher::*;

pub mod r#loop;
pub use r#loop::*;

#[cfg(windows)]
pub mod dispatcher_win;

#[cfg(windows)]
pub use dispatcher_win::*;
pub mod dispatcher_unix;
pub use dispatcher_unix::{EpollReactor, UnixEventDispatcher, UnixEventDispatcherHandle};

pub mod dispatcher_cocoa;
pub use dispatcher_cocoa::{CocoaEventDispatcher, CocoaEventDispatcherHandle, CocoaNativeEvent};
