//! A persistent StatusNotifierItem for Plasma and other compatible panels.
//! GIO dispatches all callbacks on GTK's main context; no second UI/sync thread.
use adw::prelude::*;
use gtk::{gio, glib};
use gtk4 as gtk;
use libadwaita as adw;
use std::collections::BTreeMap;

const ITEM: &str = "/StatusNotifierItem";
const MENU: &str = "/MenuBar";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const ID: &str = "org.cloudsave.Cloudsave";
type Properties = BTreeMap<String, glib::Variant>;
type Layout = (i32, Properties, Vec<glib::Variant>);

pub struct Tray {
    connection: gio::DBusConnection,
    item: Option<gio::RegistrationId>,
    menu: Option<gio::RegistrationId>,
    // gio 0.21's bus-name WatcherId is shadowed by its signal WatcherId export.
    // Capture the inferred ID in its cleanup rather than naming the wrong type.
    unwatch: Option<Box<dyn FnOnce()>>,
}

impl Tray {
    pub fn new(app: &adw::Application) -> Result<Self, glib::Error> {
        let connection = app.dbus_connection().ok_or_else(|| {
            glib::Error::new(gio::IOErrorEnum::NotConnected, "Kein Sitzungs-D-Bus")
        })?;
        let mut tray = Self {
            connection,
            item: None,
            menu: None,
            unwatch: None,
        };
        let interfaces = gio::DBusNodeInfo::for_xml(include_str!("tray.xml"))?;
        let app_weak = app.downgrade();
        tray.item = Some(
            tray.connection
                .register_object(
                    ITEM,
                    &interfaces
                        .lookup_interface("org.kde.StatusNotifierItem")
                        .unwrap(),
                )
                .method_call(move |_, _, _, _, method, _, invocation| {
                    if matches!(method, "Activate" | "SecondaryActivate") {
                        if let Some(app) = app_weak.upgrade() {
                            app.activate();
                        }
                    }
                    invocation.return_value(None);
                })
                .property(|_, _, _, _, property| item_property(property))
                .build()?,
        );
        let app_weak = app.downgrade();
        tray.menu = Some(
            tray.connection
                .register_object(
                    MENU,
                    &interfaces
                        .lookup_interface("com.canonical.dbusmenu")
                        .unwrap(),
                )
                .method_call(move |_, _, _, _, method, parameters, invocation| {
                    menu_call(&app_weak, method, &parameters, invocation);
                })
                .property(|_, _, _, _, property| match property {
                    "Version" => 3u32.to_variant(),
                    "TextDirection" => "ltr".to_variant(),
                    "Status" => "normal".to_variant(),
                    "IconThemePath" => Vec::<String>::new().to_variant(),
                    _ => unreachable!("D-Bus validates declared properties"),
                })
                .build()?,
        );
        // Register again if Plasma's shell/watcher restarts. Watching also handles
        // autostart before the panel is ready. Never block GTK waiting for D-Bus.
        let watcher = gio::bus_watch_name_on_connection(
            &tray.connection,
            WATCHER,
            gio::BusNameWatcherFlags::NONE,
            |connection, _, _| {
                connection.call(
                    Some(WATCHER),
                    "/StatusNotifierWatcher",
                    WATCHER,
                    "RegisterStatusNotifierItem",
                    Some(&(ITEM,).to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    5000,
                    gio::Cancellable::NONE,
                    |result| {
                        if let Err(error) = result {
                            eprintln!("Systemabschnitt-Registrierung fehlgeschlagen: {error}");
                        }
                    },
                );
            },
            |_, _| {},
        );
        tray.unwatch = Some(Box::new(move || gio::bus_unwatch_name(watcher)));
        Ok(tray)
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        if let Some(unwatch) = self.unwatch.take() {
            unwatch();
        }
        if let Some(item) = self.item.take() {
            let _ = self.connection.unregister_object(item);
        }
        if let Some(menu) = self.menu.take() {
            let _ = self.connection.unregister_object(menu);
        }
    }
}

fn item_property(name: &str) -> glib::Variant {
    let pixmaps = Vec::<(i32, i32, Vec<u8>)>::new();
    match name {
        "Category" => "SystemServices".to_variant(),
        "Id" | "IconName" => ID.to_variant(),
        "Title" => "Cloudsave".to_variant(),
        // Keep the icon visible while the window is open AND hidden.
        "Status" => "Active".to_variant(),
        "WindowId" => 0u32.to_variant(),
        "IconPixmap" | "OverlayIconPixmap" | "AttentionIconPixmap" => pixmaps.to_variant(),
        "OverlayIconName" | "AttentionIconName" | "AttentionMovieName" => "".to_variant(),
        "ToolTip" => (ID, pixmaps, "Cloudsave", "Ordner automatisch sichern").to_variant(),
        "ItemIsMenu" => false.to_variant(),
        "Menu" => glib::variant::ObjectPath::try_from(MENU)
            .unwrap()
            .to_variant(),
        _ => unreachable!("D-Bus validates declared properties"),
    }
}

fn properties(id: i32, names: &[String]) -> Properties {
    let mut props = Properties::new();
    if id == 0 {
        props.insert("children-display".into(), "submenu".to_variant());
    } else {
        props.insert(
            "label".into(),
            if id == 1 {
                "Cloudsave öffnen"
            } else {
                "Beenden"
            }
            .to_variant(),
        );
        props.insert("enabled".into(), true.to_variant());
        props.insert("visible".into(), true.to_variant());
    }
    props.retain(|key, _| names.is_empty() || names.contains(key));
    props
}

fn layout(id: i32, depth: i32, names: &[String]) -> Layout {
    let children = if id == 0 && depth != 0 {
        vec![
            layout(1, 0, names).to_variant(),
            layout(2, 0, names).to_variant(),
        ]
    } else {
        Vec::new()
    };
    (id, properties(id, names), children)
}

fn event(app: &glib::WeakRef<adw::Application>, id: i32, event: &str) {
    if event == "clicked" {
        if let Some(app) = app.upgrade() {
            match id {
                1 => app.activate(),
                2 => app.quit(),
                _ => {}
            }
        }
    }
}

fn menu_call(
    app: &glib::WeakRef<adw::Application>,
    method: &str,
    p: &glib::Variant,
    invocation: gio::DBusMethodInvocation,
) {
    let value = match method {
        "GetLayout" => {
            let id = p.child_get::<i32>(0);
            if !(0..=2).contains(&id) {
                invocation.return_dbus_error("com.canonical.dbusmenu.Error", "Unknown menu ID");
                return;
            }
            (
                1u32,
                layout(id, p.child_get(1), &p.child_get::<Vec<String>>(2)),
            )
                .to_variant()
        }
        "GetGroupProperties" => {
            let mut ids = p.child_get::<Vec<i32>>(0);
            if ids.is_empty() {
                ids = vec![0, 1, 2];
            }
            let names = p.child_get::<Vec<String>>(1);
            let props: Vec<_> = ids
                .into_iter()
                .filter(|id| (0..=2).contains(id))
                .map(|id| (id, properties(id, &names)))
                .collect();
            (props,).to_variant()
        }
        "GetProperty" => {
            let id = p.child_get::<i32>(0);
            let name = p.child_get::<String>(1);
            let props = properties(id, &[]);
            let Some(value) = props.get(&name).filter(|_| (0..=2).contains(&id)) else {
                invocation
                    .return_dbus_error("com.canonical.dbusmenu.Error", "Unknown menu property");
                return;
            };
            (value,).to_variant()
        }
        "Event" => {
            // Reply before an explicit quit tears down the connection.
            invocation.return_value(None);
            event(app, p.child_get(0), &p.child_get::<String>(1));
            return;
        }
        "EventGroup" => {
            let events = p.child_get::<Vec<(i32, String, glib::Variant, u32)>>(0);
            let errors: Vec<i32> = events
                .iter()
                .filter(|(id, _, _, _)| !(0..=2).contains(id))
                .map(|(id, _, _, _)| *id)
                .collect();
            invocation.return_value(Some(&(errors,).to_variant()));
            for (id, name, _, _) in events {
                event(app, id, &name);
            }
            return;
        }
        "AboutToShow" => (false,).to_variant(),
        "AboutToShowGroup" => (Vec::<i32>::new(), Vec::<i32>::new()).to_variant(),
        _ => {
            invocation.return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod", method);
            return;
        }
    };
    invocation.return_value(Some(&value));
}
