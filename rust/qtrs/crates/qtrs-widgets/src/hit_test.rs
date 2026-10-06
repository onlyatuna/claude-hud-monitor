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

                let path = hit_path(root, win_pos);
                if let Some(target) = path.last() {
                    if *button == 1 {
                        self.focus_manager.handle_mouse_click(root, &target.widget);
                    }
                }
                let button = *button;
                deliver_with_propagation(&path, |pos| EventKind::MouseButtonPress {
                    x: pos.x,
                    y: pos.y,
                    button,
                })
            }
            EventKind::MouseButtonDblClick { x, y, button } => {
                let win_pos = Point::new(*x, *y);
                let button = *button;
                let path = hit_path(root, win_pos);
                if let Some(target) = path.last() {
                    // The OS reports the second press of a double click as a double click only
                    // (Qt does not deliver that press either), so it must also give focus.
                    if button == 1 {
                        self.focus_manager.handle_mouse_click(root, &target.widget);
                    }
                }
                deliver_double_click(&path, button)
            }
            EventKind::ContextMenu {
                x,
                y,
                global_x,
                global_y,
                reason,
            } => {
                let win_pos = Point::new(*x, *y);
                let (global_x, global_y, reason) = (*global_x, *global_y, *reason);
                let path = hit_path(root, win_pos);
                deliver_with_propagation(&path, |pos| EventKind::ContextMenu {
                    x: pos.x,
                    y: pos.y,
                    global_x,
                    global_y,
                    reason,
                })
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
                let button = *button;
                let path = hit_path(root, win_pos);
                deliver_with_propagation(&path, |pos| EventKind::MouseButtonRelease {
                    x: pos.x,
                    y: pos.y,
                    button,
                })
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
                let (pixel_delta_x, pixel_delta_y) = (*pixel_delta_x, *pixel_delta_y);
                let (angle_delta_x, angle_delta_y, modifiers) =
                    (*angle_delta_x, *angle_delta_y, *modifiers);
                let path = hit_path(root, win_pos);
                deliver_with_propagation(&path, |pos| EventKind::Wheel {
                    x: pos.x,
                    y: pos.y,
                    pixel_delta_x,
                    pixel_delta_y,
                    angle_delta_x,
                    angle_delta_y,
                    modifiers,
                })
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

/// One widget on the path from the window's root widget down to the widget under the cursor.
pub struct HitStep {
    pub widget: WidgetRef,
    /// The cursor position in this widget's own coordinates.
    pub pos: Point,
    /// `QWidget::isEnabled`: false when this widget or any ancestor is disabled.
    pub enabled: bool,
}

/// The widgets under `pos` (window coordinates), root first, innermost widget last. Empty when
/// `pos` is outside a visible root.
pub fn hit_path(root: &WidgetRef, pos: Point) -> Vec<HitStep> {
    let mut path = Vec::new();
    {
        let r = root.borrow();
        let g = r.geometry();
        if !r.is_visible() || !Rect::new(0, 0, g.width, g.height).contains(pos) {
            return path;
        }
        path.push(HitStep {
            widget: root.clone(),
            pos,
            enabled: r.is_enabled(),
        });
    }
    loop {
        let (children, pos, enabled) = {
            let cur = path.last().expect("path starts with the root");
            (cur.widget.borrow().children(), cur.pos, cur.enabled)
        };
        let mut next = None;
        for child in children.into_iter().rev() {
            let (visible, geom, child_enabled) = {
                let b = child.borrow();
                (b.is_visible(), b.geometry(), b.is_enabled())
            };
            if visible && geom.contains(pos) {
                next = Some(HitStep {
                    pos: Point::new(pos.x - geom.x, pos.y - geom.y),
                    enabled: enabled && child_enabled,
                    widget: child,
                });
                break;
            }
        }
        match next {
            Some(step) => path.push(step),
            None => return path,
        }
    }
}

pub fn hit_test(root: &WidgetRef, local_pos: Point) -> Option<(WidgetRef, Point)> {
    hit_path(root, local_pos).pop().map(|s| (s.widget, s.pos))
}

/// `QApplication::notify` for mouse, wheel and context-menu events (qapplication.cpp:2689-2762):
/// the innermost widget gets the event first; while it is not handled and accepted, the event goes
/// to the parent (in the parent's coordinates), up to and including the window's root widget.
/// A disabled widget does not handle mouse events (`QWidget::event`, qwidget.cpp:8978-8998), so
/// they pass through it. Returns whether some widget handled and accepted the event.
fn deliver_with_propagation(path: &[HitStep], make_kind: impl Fn(Point) -> EventKind) -> bool {
    for step in path.iter().rev() {
        if !step.enabled {
            continue;
        }
        let mut ev = Event::new_spontaneous(make_kind(step.pos));
        let handled = step.widget.borrow_mut().event(&mut ev);
        if handled && ev.is_accepted() {
            return true;
        }
    }
    false
}

/// A double click propagates like `deliver_with_propagation`, except that a widget which does not
/// handle `MouseButtonDblClick` is given the press instead: the default
/// `QWidget::mouseDoubleClickEvent` calls `mousePressEvent` (qwidget.cpp:9636). The OS reports the
/// second press of a double click only as the double click, so without this a button would lose
/// the second click.
fn deliver_double_click(path: &[HitStep], button: u32) -> bool {
    for step in path.iter().rev() {
        if !step.enabled {
            continue;
        }
        let mut dbl = Event::new_spontaneous(EventKind::MouseButtonDblClick {
            x: step.pos.x,
            y: step.pos.y,
            button,
        });
        let handled = step.widget.borrow_mut().event(&mut dbl);
        if handled {
            if dbl.is_accepted() {
                return true;
            }
            continue;
        }
        let mut press = Event::new_spontaneous(EventKind::MouseButtonPress {
            x: step.pos.x,
            y: step.pos.y,
            button,
        });
        if step.widget.borrow_mut().event(&mut press) && press.is_accepted() {
            return true;
        }
    }
    false
}

pub fn dispatch_event_to_tree(root: &WidgetRef, event: &mut Event) -> bool {
    let mut dispatcher = EventTreeDispatcher::new();
    dispatcher.dispatch_event(root, event)
}
