use qtrs_gui::geometry::primitives::{Point, Size};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    None,
    Left,
    Right,
    Middle,
    Other(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyboardModifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub meta: bool,
}

impl KeyboardModifiers {
    pub fn bits(&self) -> u32 {
        let mut bits = 0u32;
        if self.shift {
            bits |= 0x0200_0000;
        }
        if self.control {
            bits |= 0x0400_0000;
        }
        if self.alt {
            bits |= 0x0800_0000;
        }
        if self.meta {
            bits |= 0x1000_0000;
        }
        bits
    }

    pub fn from_bits(bits: u32) -> Self {
        Self {
            shift: (bits & (0x0200_0000 | 1)) != 0,
            control: (bits & (0x0400_0000 | 4)) != 0,
            alt: (bits & (0x0800_0000 | 8)) != 0,
            meta: (bits & (0x1000_0000 | 64)) != 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WheelDelta {
    pub y: i32,
    pub x: i32,
}

impl WheelDelta {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn vertical(y: i32) -> Self {
        Self { x: 0, y }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WindowSystemEvent {
    MouseMove {
        pos: Point,
        global_pos: Point,
    },
    MousePress {
        pos: Point,
        global_pos: Point,
        button: MouseButton,
        modifiers: KeyboardModifiers,
    },
    MouseRelease {
        pos: Point,
        global_pos: Point,
        button: MouseButton,
        modifiers: KeyboardModifiers,
    },
    Wheel {
        pos: Point,
        global_pos: Point,
        delta: WheelDelta,
        modifiers: KeyboardModifiers,
    },
    MouseLeave,
    KeyPress {
        key: u32,
        modifiers: KeyboardModifiers,
        is_repeat: bool,
    },
    KeyRelease {
        key: u32,
        modifiers: KeyboardModifiers,
    },
    Resize {
        size: Size,
    },
    CloseRequest,
    FocusIn,
    FocusOut,
    DpiChanged {
        dpi_x: u32,
        dpi_y: u32,
    },
    InputMethod {
        commit_string: String,
        preedit_string: String,
        cursor_position: i32,
    },
    DragEnter {
        pos: Point,
        formats: Vec<String>,
        drop_action: u32,
    },
    DragMove {
        pos: Point,
        drop_action: u32,
    },
    DragLeave,
    Drop {
        pos: Point,
        formats: Vec<String>,
        data: Vec<(String, Vec<u8>)>,
        drop_action: u32,
    },
    /// Power management event (aligned with QWindowsContext / WM_POWERBROADCAST)
    Power {
        event: PowerEvent,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerEvent {
    /// System is entering sleep or standby
    Suspend,
    /// System has resumed from sleep or standby
    Resume,
}

pub trait WindowSystemEventHandler: Send + Sync {
    fn handle_window_event(&mut self, event: WindowSystemEvent);
}

pub struct ClosureWindowEventHandler<F>
where
    F: FnMut(WindowSystemEvent) + Send + Sync,
{
    handler: F,
}

impl<F> ClosureWindowEventHandler<F>
where
    F: FnMut(WindowSystemEvent) + Send + Sync,
{
    pub fn new(handler: F) -> Self {
        Self { handler }
    }
}

impl<F> WindowSystemEventHandler for ClosureWindowEventHandler<F>
where
    F: FnMut(WindowSystemEvent) + Send + Sync,
{
    fn handle_window_event(&mut self, event: WindowSystemEvent) {
        (self.handler)(event);
    }
}
