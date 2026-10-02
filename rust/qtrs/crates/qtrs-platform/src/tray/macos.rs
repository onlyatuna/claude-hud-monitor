use qtrs_core::object::ThreadContext;
use qtrs_gui::paint::Pixmap;

use crate::menu::PlatformMenu;
use crate::objc_runtime::{Class, Id, ObjcMsg, Sel, NS_VARIABLE_STATUS_ITEM_LENGTH};
use crate::platform_tray::PlatformTrayIcon;

pub struct CocoaStatusItem {
    item_id: usize,
    status_item: Id,
    tooltip: String,
    is_visible: bool,
    icon_width: u32,
    icon_height: u32,
    icon_bytes: Vec<u8>,
    is_template: bool,
    menu: Option<Box<dyn PlatformMenu>>,
    last_message: Option<(String, String, crate::platform_tray::TrayMessageIcon, u32)>,
}

unsafe impl Send for CocoaStatusItem {}
unsafe impl Sync for CocoaStatusItem {}

impl CocoaStatusItem {
    pub fn new(item_id: usize) -> Self {
        ThreadContext::assert_main_thread("CocoaStatusItem::new");

        let status_bar_class = Class::get("NSStatusBar").unwrap_or(Class::NIL);
        let status_bar = ObjcMsg::send_class_0(status_bar_class, Sel::register("systemStatusBar"));

        let status_item = ObjcMsg::send_length(
            status_bar,
            Sel::register("statusItemWithLength:"),
            NS_VARIABLE_STATUS_ITEM_LENGTH,
        );

        Self {
            item_id,
            status_item,
            tooltip: String::new(),
            is_visible: false,
            icon_width: 0,
            icon_height: 0,
            icon_bytes: Vec::new(),
            is_template: true,
            menu: None,
            last_message: None,
        }
    }

    pub fn item_id(&self) -> usize {
        self.item_id
    }

    #[inline]
    pub fn native_status_item(&self) -> Id {
        self.status_item
    }

    pub fn is_template(&self) -> bool {
        self.is_template
    }

    pub fn set_template(&mut self, is_template: bool) {
        self.is_template = is_template;
    }

    pub fn tooltip(&self) -> &str {
        &self.tooltip
    }

    pub fn icon_size(&self) -> (u32, u32) {
        (self.icon_width, self.icon_height)
    }

    pub fn is_visible(&self) -> bool {
        self.is_visible
    }

    pub fn last_message(&self) -> Option<(&str, &str, crate::platform_tray::TrayMessageIcon, u32)> {
        self.last_message
            .as_ref()
            .map(|(t, m, i, d)| (t.as_str(), m.as_str(), *i, *d))
    }
}

impl PlatformTrayIcon for CocoaStatusItem {
    fn set_icon(&mut self, pixmap: &Pixmap) -> Result<(), &'static str> {
        self.icon_width = pixmap.physical_width();
        self.icon_height = pixmap.physical_height();
        self.icon_bytes = pixmap.data().to_vec();

        // Configure icon on status item button
        let button = ObjcMsg::send_0(self.status_item, Sel::register("button"));
        if !button.is_nil() {
            // Update size/data in mock environment
            let _ = button;
        }
        Ok(())
    }

    fn set_tooltip(&mut self, tooltip: &str) -> Result<(), &'static str> {
        self.tooltip = tooltip.to_string();

        // Set title and tooltip
        let button = ObjcMsg::send_0(self.status_item, Sel::register("button"));
        if !button.is_nil() {
            ObjcMsg::send_str(button, Sel::register("setTitle:"), tooltip);
        }
        Ok(())
    }

    fn set_menu(&mut self, menu: Box<dyn PlatformMenu>) {
        // [status_item setMenu:ns_menu]
        let menu_id = Id(menu.native_handle() as *mut std::ffi::c_void);
        ObjcMsg::send_id(self.status_item, Sel::register("setMenu:"), menu_id);
        self.menu = Some(menu);
    }

    fn show(&mut self) -> Result<(), &'static str> {
        self.is_visible = true;
        // Set status item visible
        ObjcMsg::send_bool(self.status_item, Sel::register("setVisible:"), true);
        Ok(())
    }

    fn hide(&mut self) -> Result<(), &'static str> {
        self.is_visible = false;
        // Set status item hidden
        ObjcMsg::send_bool(self.status_item, Sel::register("setVisible:"), false);
        Ok(())
    }

    fn show_message(
        &mut self,
        title: &str,
        msg: &str,
        icon: crate::platform_tray::TrayMessageIcon,
        duration_ms: u32,
    ) -> Result<(), &'static str> {
        let dur = if duration_ms == 0 { 10000 } else { duration_ms };
        self.last_message = Some((title.to_string(), msg.to_string(), icon, dur));

        let center_class = Class::get("NSUserNotificationCenter").unwrap_or(Class::NIL);
        let center =
            ObjcMsg::send_class_0(center_class, Sel::register("defaultUserNotificationCenter"));
        let notif_class = Class::get("NSUserNotification").unwrap_or(Class::NIL);
        let notif_alloc = ObjcMsg::send_class_0(notif_class, Sel::register("alloc"));
        let notif = ObjcMsg::send_0(notif_alloc, Sel::register("init"));
        if !notif.is_nil() {
            ObjcMsg::send_str(notif, Sel::register("setTitle:"), title);
            ObjcMsg::send_str(notif, Sel::register("setInformativeText:"), msg);
            ObjcMsg::send_id(center, Sel::register("deliverNotification:"), notif);
        }
        Ok(())
    }

    fn supports_messages(&self) -> bool {
        true
    }
}

impl Drop for CocoaStatusItem {
    fn drop(&mut self) {
        // [[NSStatusBar systemStatusBar] removeStatusItem:status_item]
        if !self.status_item.is_nil() {
            let status_bar_class = Class::get("NSStatusBar").unwrap_or(Class::NIL);
            let status_bar =
                ObjcMsg::send_class_0(status_bar_class, Sel::register("systemStatusBar"));
            ObjcMsg::send_id(
                status_bar,
                Sel::register("removeStatusItem:"),
                self.status_item,
            );
        }
    }
}
