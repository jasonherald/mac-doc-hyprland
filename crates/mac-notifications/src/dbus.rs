use crate::notification::{Notification, Urgency, parse_actions};
use crate::state::NotificationState;
use gtk4::gio;
use gtk4::glib;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::SystemTime;

/// D-Bus introspection XML for org.freedesktop.Notifications.
const INTROSPECT_XML: &str = r#"
<node>
  <interface name="org.freedesktop.Notifications">
    <method name="Notify">
      <arg name="app_name" type="s" direction="in"/>
      <arg name="replaces_id" type="u" direction="in"/>
      <arg name="app_icon" type="s" direction="in"/>
      <arg name="summary" type="s" direction="in"/>
      <arg name="body" type="s" direction="in"/>
      <arg name="actions" type="as" direction="in"/>
      <arg name="hints" type="a{sv}" direction="in"/>
      <arg name="expire_timeout" type="i" direction="in"/>
      <arg name="id" type="u" direction="out"/>
    </method>
    <method name="CloseNotification">
      <arg name="id" type="u" direction="in"/>
    </method>
    <method name="GetCapabilities">
      <arg name="capabilities" type="as" direction="out"/>
    </method>
    <method name="GetServerInformation">
      <arg name="name" type="s" direction="out"/>
      <arg name="vendor" type="s" direction="out"/>
      <arg name="version" type="s" direction="out"/>
      <arg name="spec_version" type="s" direction="out"/>
    </method>
    <signal name="NotificationClosed">
      <arg name="id" type="u"/>
      <arg name="reason" type="u"/>
    </signal>
    <signal name="ActionInvoked">
      <arg name="id" type="u"/>
      <arg name="action_key" type="s"/>
    </signal>
  </interface>
</node>
"#;

/// Callback invoked when a new notification arrives via D-Bus.
/// Implement this to show popups, update waybar, etc.
pub type OnNotify = Rc<dyn Fn(&Notification)>;

/// Callback invoked when a notification is closed via D-Bus.
pub type OnClose = Rc<dyn Fn(u32)>;

/// Registers the notification D-Bus server on the session bus.
///
/// Runs entirely on the glib main loop — no threads or async needed.
pub fn register_server(
    state: &Rc<RefCell<NotificationState>>,
    on_notify: OnNotify,
    on_close: OnClose,
) {
    let state = Rc::clone(state);
    let on_notify = Rc::clone(&on_notify);
    let on_close = Rc::clone(&on_close);

    gio::bus_own_name(
        gio::BusType::Session,
        "org.freedesktop.Notifications",
        gio::BusNameOwnerFlags::REPLACE,
        move |connection, _name| {
            log::info!("Acquired D-Bus name: org.freedesktop.Notifications");
            register_object(&connection, &state, &on_notify, &on_close);
        },
        |_connection, _name| {
            log::debug!("D-Bus name acquired callback");
        },
        |_connection, _name| {
            log::error!(
                "Lost D-Bus name org.freedesktop.Notifications — is another daemon running?"
            );
        },
    );
}

fn register_object(
    connection: &gio::DBusConnection,
    state: &Rc<RefCell<NotificationState>>,
    on_notify: &OnNotify,
    on_close: &OnClose,
) {
    let node_info = gio::DBusNodeInfo::for_xml(INTROSPECT_XML)
        .expect("Failed to parse notification introspection XML");

    let interface_info = node_info
        .lookup_interface("org.freedesktop.Notifications")
        .expect("Interface not found in XML");

    let state = Rc::clone(state);
    let on_notify = Rc::clone(on_notify);
    let on_close = Rc::clone(on_close);

    connection
        .register_object("/org/freedesktop/Notifications", &interface_info)
        .method_call(
            move |_conn, _sender, _path, _iface, method, params, invocation| {
                handle_method(method, params, invocation, &state, &on_notify, &on_close);
            },
        )
        .build()
        .expect("Failed to register D-Bus object");
}

fn handle_method(
    method: &str,
    params: glib::Variant,
    invocation: gio::DBusMethodInvocation,
    state: &Rc<RefCell<NotificationState>>,
    on_notify: &OnNotify,
    on_close: &OnClose,
) {
    match method {
        "Notify" => handle_notify(&params, invocation, state, on_notify),
        "CloseNotification" => handle_close(&params, invocation, state, on_close),
        "GetCapabilities" => handle_capabilities(invocation),
        "GetServerInformation" => handle_server_info(invocation),
        _ => {
            log::warn!("Unknown D-Bus method: {}", method);
        }
    }
}

fn handle_notify(
    params: &glib::Variant,
    invocation: gio::DBusMethodInvocation,
    state: &Rc<RefCell<NotificationState>>,
    on_notify: &OnNotify,
) {
    // Parse the Notify parameters: (susssasa{sv}i)
    let app_name: String = params.child_value(0).get().unwrap_or_default();
    let replaces_id: u32 = params.child_value(1).get().unwrap_or(0);
    let app_icon: String = params.child_value(2).get().unwrap_or_default();
    let summary: String = params.child_value(3).get().unwrap_or_default();
    let body: String = params.child_value(4).get().unwrap_or_default();
    let timeout: i32 = params.child_value(7).get().unwrap_or(-1);

    // Parse actions array
    let actions_variant = params.child_value(5);
    let actions: Vec<String> = (0..actions_variant.n_children())
        .filter_map(|i| actions_variant.child_value(i).get::<String>())
        .collect();

    // Parse hints dict for urgency and desktop-entry
    let hints_variant = params.child_value(6);
    let urgency = extract_urgency(&hints_variant);
    let desktop_entry = extract_string_hint(&hints_variant, "desktop-entry");

    let notif = Notification {
        id: 0, // assigned by state.add/replace
        app_name,
        app_icon,
        summary,
        body,
        actions: parse_actions(&actions),
        urgency,
        timeout_ms: timeout,
        timestamp: SystemTime::now(),
        read: false,
        desktop_entry,
    };

    log::debug!(
        "Notify: app={}, summary={}, urgency={:?}",
        notif.app_name,
        notif.summary,
        notif.urgency
    );

    let id = state.borrow_mut().replace(replaces_id, notif.clone());

    // Update the notification with the assigned ID for the callback
    let mut notif_with_id = notif;
    notif_with_id.id = id;
    on_notify(&notif_with_id);

    // Return the assigned ID
    let result = glib::Variant::from((id,));
    invocation.return_value(Some(&result));
}

fn handle_close(
    params: &glib::Variant,
    invocation: gio::DBusMethodInvocation,
    state: &Rc<RefCell<NotificationState>>,
    on_close: &OnClose,
) {
    let id: u32 = params.child_value(0).get().unwrap_or(0);
    state.borrow_mut().remove(id);
    on_close(id);
    invocation.return_value(None);
}

fn handle_capabilities(invocation: gio::DBusMethodInvocation) {
    let caps = vec!["body", "body-markup", "actions", "icon-static"];
    let variant = glib::Variant::from((caps,));
    invocation.return_value(Some(&variant));
}

fn handle_server_info(invocation: gio::DBusMethodInvocation) {
    let info = ("mac-notifications", "mac-dock-hyprland", "0.1.0", "1.2");
    let variant = glib::Variant::from(info);
    invocation.return_value(Some(&variant));
}

fn extract_urgency(hints: &glib::Variant) -> Urgency {
    // Look for "urgency" key in the a{sv} dict
    for i in 0..hints.n_children() {
        let entry = hints.child_value(i);
        let key: Option<String> = entry.child_value(0).get();
        if key.as_deref() == Some("urgency") {
            let val = entry.child_value(1).child_value(0);
            if let Some(u) = val.get::<u8>() {
                return Urgency::from(u);
            }
        }
    }
    Urgency::Normal
}

fn extract_string_hint(hints: &glib::Variant, key_name: &str) -> Option<String> {
    for i in 0..hints.n_children() {
        let entry = hints.child_value(i);
        let key: Option<String> = entry.child_value(0).get();
        if key.as_deref() == Some(key_name) {
            let val = entry.child_value(1).child_value(0);
            return val.get::<String>();
        }
    }
    None
}
