//! Deferred command queue for safely decoupling tree modifications and layout updates
//! from synchronous event handlers and callbacks.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use qtrs_core::object::ObjectId;

use crate::widget::{WidgetRef, WidgetWeak};

/// A command that can be queued during event dispatch and executed later
/// when no widget in the hierarchy is borrowed.
pub enum WidgetCommand {
    /// Request layout recalculation on a container widget.
    RequestLayout(WidgetWeak),
    /// Deferred deletion: unlinks target from parent widget and drops it.
    DeferredDelete {
        parent: Option<WidgetWeak>,
        target_id: ObjectId,
    },
    /// Arbitrary deferred task executed once call frames return.
    Task(Box<dyn FnOnce() + 'static>),
}

thread_local! {
    static COMMAND_QUEUE: RefCell<Vec<WidgetCommand>> = const { RefCell::new(Vec::new()) };
}

/// Thread-local command queue managing deferred operations.
pub struct WidgetCommandQueue;

impl WidgetCommandQueue {
    /// Enqueues a command for deferred execution.
    pub fn post(command: WidgetCommand) {
        COMMAND_QUEUE.with(|q| q.borrow_mut().push(command));
    }

    /// Enqueues a layout update for the given widget reference.
    pub fn post_layout(widget: &WidgetRef) {
        Self::post(WidgetCommand::RequestLayout(Rc::downgrade(widget)));
    }

    /// Enqueues a layout update for the given weak reference.
    pub fn post_layout_weak(widget: &WidgetWeak) {
        Self::post(WidgetCommand::RequestLayout(widget.clone()));
    }

    /// Enqueues deferred deletion of a widget by target ObjectId.
    pub fn post_delete(parent: Option<WidgetWeak>, target_id: ObjectId) {
        Self::post(WidgetCommand::DeferredDelete { parent, target_id });
    }

    /// Enqueues an arbitrary closure to be run when the queue is flushed.
    pub fn post_task<F: FnOnce() + 'static>(task: F) {
        Self::post(WidgetCommand::Task(Box::new(task)));
    }

    /// Runs the queued layout requests and leaves every other command queued.
    ///
    /// Qt delivers the posted `LayoutRequest` events a text or size-hint change produced before
    /// the next paint; this is that step, for code that is about to paint. Unlike
    /// [`flush`](Self::flush) it never runs tasks or deletions, which may need the caller's
    /// borrows released first.
    pub fn flush_layouts() {
        let commands = COMMAND_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()));
        if commands.is_empty() {
            return;
        }

        let mut layout_targets: Vec<WidgetRef> = Vec::new();
        let mut seen_layout_ids = HashSet::new();
        let mut kept = Vec::new();
        for cmd in commands {
            match cmd {
                WidgetCommand::RequestLayout(weak) => {
                    if let Some(target) = weak.upgrade() {
                        let Ok(widget) = target.try_borrow() else {
                            kept.push(WidgetCommand::RequestLayout(weak));
                            continue;
                        };
                        let id = widget.id();
                        drop(widget);
                        if seen_layout_ids.insert(id) {
                            layout_targets.push(target);
                        }
                    }
                }
                other => kept.push(other),
            }
        }
        // Commands queued while the layouts ran go after the ones that were kept.
        COMMAND_QUEUE.with(|q| {
            let mut queue = q.borrow_mut();
            kept.append(&mut queue);
            *queue = kept;
        });

        for target in layout_targets {
            crate::layout_scheduler::LayoutScheduler::invalidate(&target);
        }
        crate::layout_scheduler::LayoutScheduler::activate_pending();
    }

    /// Safely requests deferred deletion of a `WidgetRef`.
    ///
    /// If the widget's `RefCell` is currently borrowed (e.g. inside its own event callback),
    /// the deletion is deferred to a queued task that executes as soon as the borrow is released.
    pub fn delete_later(widget: &WidgetRef) {
        if let Ok(w) = widget.try_borrow() {
            w.delete_later();
        } else {
            let weak = Rc::downgrade(widget);
            Self::post_task(move || {
                if let Some(w) = weak.upgrade() {
                    w.borrow().delete_later();
                }
            });
        }
    }

    /// Safely requests layout recalculation for a container `WidgetRef`.
    pub fn request_layout(widget: &WidgetRef) {
        Self::post_layout(widget);
    }

    /// Flushes and executes all pending commands.
    ///
    /// Must be invoked when no widget in the hierarchy is borrowed mutably.
    /// Multiple `RequestLayout` commands on the same container are deduplicated.
    pub fn flush() {
        while COMMAND_QUEUE.with(|q| !q.borrow().is_empty()) {
            let commands = COMMAND_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()));
            if commands.is_empty() {
                break;
            }

            let mut layout_targets: Vec<WidgetRef> = Vec::new();
            let mut seen_layout_ids = HashSet::new();

            for cmd in commands {
                match cmd {
                    WidgetCommand::RequestLayout(weak) => {
                        if let Some(target) = weak.upgrade() {
                            let id = target.borrow().id();
                            if seen_layout_ids.insert(id) {
                                layout_targets.push(target);
                            }
                        }
                    }
                    WidgetCommand::DeferredDelete { parent, target_id } => {
                        if let Some(parent_weak) = parent {
                            if let Some(parent_widget) = parent_weak.upgrade() {
                                parent_widget.borrow_mut().remove_child(target_id);
                                let pid = parent_widget.borrow().id();
                                if seen_layout_ids.insert(pid) {
                                    layout_targets.push(parent_widget);
                                }
                            }
                        }
                    }
                    WidgetCommand::Task(task) => {
                        task();
                    }
                }
            }

            for target in layout_targets {
                crate::layout_scheduler::LayoutScheduler::invalidate(&target);
            }
            crate::layout_scheduler::LayoutScheduler::activate_pending();
        }
    }

    /// Returns the number of currently pending commands in this thread.
    pub fn pending_count() -> usize {
        COMMAND_QUEUE.with(|q| q.borrow().len())
    }

    /// Clears any pending commands (useful for test resets).
    pub fn clear() {
        COMMAND_QUEUE.with(|q| q.borrow_mut().clear());
    }
}

/// Convenience function to request deferred deletion of a `WidgetRef`.
pub fn delete_widget_later(widget: &WidgetRef) {
    WidgetCommandQueue::delete_later(widget);
}

/// Convenience function to request deferred layout recalculation for a container `WidgetRef`.
pub fn request_widget_layout(widget: &WidgetRef) {
    WidgetCommandQueue::request_layout(widget);
}
