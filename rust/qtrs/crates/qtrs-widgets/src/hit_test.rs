use crate::focus::{find_widget_by_id, FocusManager};
use crate::popup::PopupManager;
use crate::widget::WidgetRef;
use qtrs_core::event::FocusReason;
use qtrs_core::event::{Event, EventKind, EventPointPos};
use qtrs_core::object::ObjectId;
use qtrs_gui::geometry::primitives::{Point, Rect};

#[derive(Default)]
pub struct EventTreeDispatcher {
    last_hovered: Option<(ObjectId, WidgetRef)>,
    focus_manager: FocusManager,
    popup_manager: PopupManager,
}

impl EventTreeDispatcher {
    pub fn new() -> Self {
        Self {
            last_hovered: None,
            focus_manager: FocusManager::new(),
            popup_manager: PopupManager::new(),
        }
    }

    pub fn popup_manager(&self) -> &PopupManager {
        &self.popup_manager
    }

    pub fn popup_manager_mut(&mut self) -> &mut PopupManager {
        &mut self.popup_manager
    }

    pub fn focus_manager(&self) -> &FocusManager {
        &self.focus_manager
    }

    pub fn focus_manager_mut(&mut self) -> &mut FocusManager {
        &mut self.focus_manager
    }
    pub fn handle_mouse_leave(&mut self) {
        if let Some((_old_id, old_widget)) = self.last_hovered.take() {
            let mut leave_ev = Event::new_spontaneous(EventKind::Leave);
            let _ = old_widget.borrow_mut().event(&mut leave_ev);
        }
        crate::command::WidgetCommandQueue::flush();
    }

    pub fn dispatch_event(&mut self, root: &WidgetRef, event: &mut Event) -> bool {
        let result = self.dispatch_event_internal(root, event);
        crate::command::WidgetCommandQueue::flush();
        result
    }

    fn dispatch_event_internal(&mut self, root: &WidgetRef, event: &mut Event) -> bool {
        match &event.kind {
            EventKind::MouseMove { x, y } => {
                let win_pos = Point::new(*x, *y);
                let hit_opt = hit_test(root, win_pos);

                match hit_opt {
                    Some((target, local_pos)) => {
                        let target_id = target.borrow().id();

                        let is_different = match &self.last_hovered {
                            Some((old_id, _)) => *old_id != target_id,
                            None => true,
                        };

                        if is_different {
                            if let Some((_old_id, old_widget)) = self.last_hovered.take() {
                                let mut leave_ev = Event::new_spontaneous(EventKind::Leave);
                                let _ = old_widget.borrow_mut().event(&mut leave_ev);
                            }

                            let mut enter_ev = Event::new_spontaneous(EventKind::Enter {
                                x: local_pos.x,
                                y: local_pos.y,
                            });
                            let _ = target.borrow_mut().event(&mut enter_ev);

                            self.last_hovered = Some((target_id, target.clone()));
                        }

                        let mut local_event = Event::new_spontaneous(EventKind::MouseMove {
                            x: local_pos.x,
                            y: local_pos.y,
                        });
                        target.borrow_mut().event(&mut local_event)
                    }
                    None => {
                        self.handle_mouse_leave();
                        false
                    }
                }
            }
            EventKind::MouseButtonPress { x, y, button } => {
                let win_pos = Point::new(*x, *y);
                let _dismissed = self.popup_manager.handle_mouse_press(win_pos);

                // Check mouse grabber
                if let Some(grabber_id) = self.popup_manager.mouse_grabber() {
                    if let Some(grabber) = find_widget_by_id(root, grabber_id) {
                        let g = grabber.borrow().geometry();
                        let mut local_event = Event::new_spontaneous(EventKind::MouseButtonPress {
                            x: win_pos.x - g.x,
                            y: win_pos.y - g.y,
                            button: *button,
                        });
                        return grabber.borrow_mut().event(&mut local_event);
                    }
                }

                if let Some((target, local_pos)) = hit_test(root, win_pos) {
                    if *button == 1 {
                        self.focus_manager.handle_mouse_click(root, &target);
                    }
                    let mut local_event = Event::new_spontaneous(EventKind::MouseButtonPress {
                        x: local_pos.x,
                        y: local_pos.y,
                        button: *button,
                    });
                    return target.borrow_mut().event(&mut local_event);
                }
                false
            }
            EventKind::MouseButtonDblClick { x, y, button } => {
                let win_pos = Point::new(*x, *y);
                if let Some((target, local_pos)) = hit_test(root, win_pos) {
                    let mut local_event = Event::new_spontaneous(EventKind::MouseButtonDblClick {
                        x: local_pos.x,
                        y: local_pos.y,
                        button: *button,
                    });
                    return target.borrow_mut().event(&mut local_event);
                }
                false
            }
            EventKind::ContextMenu {
                x,
                y,
                global_x,
                global_y,
                reason,
            } => {
                let win_pos = Point::new(*x, *y);
                if let Some((target, local_pos)) = hit_test(root, win_pos) {
                    let mut local_event = Event::new_spontaneous(EventKind::ContextMenu {
                        x: local_pos.x,
                        y: local_pos.y,
                        global_x: *global_x,
                        global_y: *global_y,
                        reason: *reason,
                    });
                    return target.borrow_mut().event(&mut local_event);
                }
                false
            }
            EventKind::HoverMove {
                pos,
                old_pos,
                modifiers,
            } => {
                let win_pos = Point::new(pos.x.round() as i32, pos.y.round() as i32);
                if let Some((target, local_pos)) = hit_test(root, win_pos) {
                    let mut local_event = Event::new_spontaneous(EventKind::HoverMove {
                        pos: EventPointPos::new(local_pos.x as f32, local_pos.y as f32),
                        old_pos: *old_pos,
                        modifiers: *modifiers,
                    });
                    return target.borrow_mut().event(&mut local_event);
                }
                false
            }
            EventKind::HoverEnter {
                pos,
                old_pos,
                modifiers,
            } => {
                let win_pos = Point::new(pos.x.round() as i32, pos.y.round() as i32);
                if let Some((target, local_pos)) = hit_test(root, win_pos) {
                    let mut local_event = Event::new_spontaneous(EventKind::HoverEnter {
                        pos: EventPointPos::new(local_pos.x as f32, local_pos.y as f32),
                        old_pos: *old_pos,
                        modifiers: *modifiers,
                    });
                    return target.borrow_mut().event(&mut local_event);
                }
                false
            }
            EventKind::HoverLeave { old_pos, modifiers } => {
                if let Some((_, old_widget)) = &self.last_hovered {
                    let mut local_event = Event::new_spontaneous(EventKind::HoverLeave {
                        old_pos: *old_pos,
                        modifiers: *modifiers,
                    });
                    return old_widget.borrow_mut().event(&mut local_event);
                }
                false
            }
            EventKind::MouseButtonRelease { x, y, button } => {
                let win_pos = Point::new(*x, *y);
                if let Some(grabber_id) = self.popup_manager.mouse_grabber() {
                    if let Some(grabber) = find_widget_by_id(root, grabber_id) {
                        let g = grabber.borrow().geometry();
                        let mut local_event =
                            Event::new_spontaneous(EventKind::MouseButtonRelease {
                                x: win_pos.x - g.x,
                                y: win_pos.y - g.y,
                                button: *button,
                            });
                        return grabber.borrow_mut().event(&mut local_event);
                    }
                }
                if let Some((target, local_pos)) = hit_test(root, win_pos) {
                    let mut local_event = Event::new_spontaneous(EventKind::MouseButtonRelease {
                        x: local_pos.x,
                        y: local_pos.y,
                        button: *button,
                    });
                    return target.borrow_mut().event(&mut local_event);
                }
                false
            }
            EventKind::Wheel {
                x,
                y,
                pixel_delta_x,
                pixel_delta_y,
                angle_delta_x,
                angle_delta_y,
                modifiers,
            } => {
                let win_pos = Point::new(*x, *y);
                if let Some((target, local_pos)) = hit_test(root, win_pos) {
                    let mut local_event = Event::new_spontaneous(EventKind::Wheel {
                        x: local_pos.x,
                        y: local_pos.y,
                        pixel_delta_x: *pixel_delta_x,
                        pixel_delta_y: *pixel_delta_y,
                        angle_delta_x: *angle_delta_x,
                        angle_delta_y: *angle_delta_y,
                        modifiers: *modifiers,
                    });
                    return target.borrow_mut().event(&mut local_event);
                }
                false
            }
            EventKind::Resize {
                width,
                height,
                old_width,
                old_height,
            } => {
                root.borrow().set_geometry(Rect::new(0, 0, *width, *height));
                let mut resize_event = Event::new_spontaneous(EventKind::Resize {
                    width: *width,
                    height: *height,
                    old_width: *old_width,
                    old_height: *old_height,
                });
                root.borrow_mut().event(&mut resize_event)
            }
            EventKind::FocusIn { reason } => {
                if let Some(focused_id) = self.focus_manager.focused_widget_id() {
                    if let Some(focused_widget) = find_widget_by_id(root, focused_id) {
                        focused_widget.borrow_mut().event(event);
                    }
                } else if *reason == FocusReason::ActiveWindow {
                    self.focus_manager.focus_next(root);
                }
                root.borrow_mut().event(event)
            }
            EventKind::FocusOut { .. } => {
                if let Some(focused_id) = self.focus_manager.focused_widget_id() {
                    if let Some(focused_widget) = find_widget_by_id(root, focused_id) {
                        focused_widget.borrow_mut().event(event);
                    }
                }
                root.borrow_mut().event(event)
            }
            EventKind::KeyPress { key, modifiers, .. } => {
                // Tab / Shift+Tab keyboard focus navigation
                if *key == 0x09 || *key == 0x01000001 {
                    let is_shift = (*modifiers & 0x02000000 != 0) || (*modifiers & 1 != 0);
                    if is_shift {
                        if self.focus_manager.focus_previous(root) {
                            return true;
                        }
                    } else if self.focus_manager.focus_next(root) {
                        return true;
                    }
                }

                // Route key event to focused widget first
                if let Some(focused_id) = self.focus_manager.focused_widget_id() {
                    if let Some(focused_widget) = find_widget_by_id(root, focused_id) {
                        if focused_widget.borrow_mut().event(event) {
                            return true;
                        }
                    }
                }
                root.borrow_mut().event(event)
            }
            EventKind::KeyRelease { .. } => {
                if let Some(focused_id) = self.focus_manager.focused_widget_id() {
                    if let Some(focused_widget) = find_widget_by_id(root, focused_id) {
                        if focused_widget.borrow_mut().event(event) {
                            return true;
                        }
                    }
                }
                root.borrow_mut().event(event)
            }
            EventKind::InputMethod { .. } => {
                if let Some(focused_id) = self.focus_manager.focused_widget_id() {
                    if let Some(focused_widget) = find_widget_by_id(root, focused_id) {
                        if focused_widget.borrow_mut().event(event) {
                            return true;
                        }
                    }
                }
                root.borrow_mut().event(event)
            }
            EventKind::DpiChanged { dpi_x, .. } => {
                let new_dpr = (*dpi_x as f32) / 96.0;
                crate::window::propagate_dpi_change_recursive(root, 1.0, new_dpr);
                true
            }
            _ => false,
        }
    }
}

pub fn hit_test(root: &WidgetRef, local_pos: Point) -> Option<(WidgetRef, Point)> {
    let root_borrow = root.borrow();
    if !root_borrow.is_visible() {
        return None;
    }

    let geom = root_borrow.geometry();
    let local_rect = Rect::new(0, 0, geom.width, geom.height);
    if !local_rect.contains(local_pos) {
        return None;
    }

    let children = root_borrow.children();
    drop(root_borrow);

    for child in children.into_iter().rev() {
        let child_borrow = child.borrow();
        if !child_borrow.is_visible() {
            continue;
        }
        let child_geom = child_borrow.geometry();
        if child_geom.contains(local_pos) {
            let child_local_pos =
                Point::new(local_pos.x - child_geom.x, local_pos.y - child_geom.y);
            drop(child_borrow);

            if let Some(target) = hit_test(&child, child_local_pos) {
                return Some(target);
            }
            return Some((child, child_local_pos));
        }
    }

    Some((root.clone(), local_pos))
}

pub fn dispatch_event_to_tree(root: &WidgetRef, event: &mut Event) -> bool {
    let mut dispatcher = EventTreeDispatcher::new();
    dispatcher.dispatch_event(root, event)
}
