//! A persistent StatusNotifierItem for Plasma and other compatible panels.
//! GIO dispatches all callbacks on GTK's main context; no second UI/sync thread.
use crate::jobs::{self, TrayStatus};
use adw::prelude::*;
use gtk::{gio, glib};
use gtk4 as gtk;
use libadwaita as adw;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, time::Duration};

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
    refresh: Option<glib::SourceId>,
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
            refresh: None,
        };
        let interfaces = gio::DBusNodeInfo::for_xml(include_str!("tray.xml"))?;
        let pixmaps = icon_pixmaps()?;
        let menu_state = Rc::new(RefCell::new(MenuState::new()));
        let item_state = menu_state.clone();
        let app_weak = app.downgrade();
        tray.item = Some(
            tray.connection
                .register_object(
                    ITEM,
                    &interfaces
                        .lookup_interface("org.kde.StatusNotifierItem")
                        .unwrap(),
                )
                .method_call(move |connection, _, _, _, method, _, invocation| {
                    if matches!(method, "Activate" | "ContextMenu") {
                        let _ = connection.emit_signal(
                            None,
                            MENU,
                            "com.canonical.dbusmenu",
                            "ItemActivationRequested",
                            Some(&(0i32, 0u32).to_variant()),
                        );
                    }
                    if method == "SecondaryActivate" {
                        if let Some(app) = app_weak.upgrade() {
                            app.activate();
                        }
                    }
                    invocation.return_value(None);
                })
                .property(move |_, _, _, _, property| {
                    let state = item_state.borrow();
                    item_property(property, &pixmaps[&state.snapshot.status], &state.snapshot)
                })
                .build()?,
        );
        let app_weak = app.downgrade();
        let handler_state = menu_state.clone();
        tray.menu = Some(
            tray.connection
                .register_object(
                    MENU,
                    &interfaces
                        .lookup_interface("com.canonical.dbusmenu")
                        .unwrap(),
                )
                .method_call(move |_, _, _, _, method, parameters, invocation| {
                    menu_call(
                        &app_weak,
                        &handler_state.borrow(),
                        method,
                        &parameters,
                        invocation,
                    );
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
        let connection = tray.connection.clone();
        tray.refresh = Some(glib::timeout_add_local(
            Duration::from_millis(500),
            move || {
                let next = jobs::snapshot();
                let mut state = menu_state.borrow_mut();
                if state.snapshot != next {
                    let icon_changed = state.snapshot.status != next.status;
                    let menu_changed = state.snapshot.active != next.active
                        || state.snapshot.recent != next.recent;
                    state.snapshot = next;
                    if menu_changed {
                        state.revision = state.revision.wrapping_add(1);
                        let _ = connection.emit_signal(
                            None,
                            MENU,
                            "com.canonical.dbusmenu",
                            "LayoutUpdated",
                            Some(&(state.revision, 0i32).to_variant()),
                        );
                    }
                    if icon_changed {
                        let _ = connection.emit_signal(
                            None,
                            ITEM,
                            "org.kde.StatusNotifierItem",
                            "NewIcon",
                            None,
                        );
                    }
                    let _ = connection.emit_signal(
                        None,
                        ITEM,
                        "org.kde.StatusNotifierItem",
                        "NewToolTip",
                        None,
                    );
                }
                glib::ControlFlow::Continue
            },
        ));
        Ok(tray)
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        if let Some(source) = self.refresh.take() {
            source.remove();
        }
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

type Pixmaps = Vec<(i32, i32, Vec<u8>)>;

// The panel receives the actual badge pixels at each icon size.
fn icon_pixmaps() -> Result<BTreeMap<TrayStatus, Pixmaps>, glib::Error> {
    TrayStatus::ALL
        .into_iter()
        .map(|status| {
            let pixmaps = [24, 32, 48, 64]
                .into_iter()
                .map(|size| crate::brand::argb(size, status).map(|pixels| (size, size, pixels)))
                .collect::<Result<Pixmaps, _>>()?;
            Ok((status, pixmaps))
        })
        .collect()
}

fn item_property(name: &str, pixmaps: &Pixmaps, snapshot: &jobs::Snapshot) -> glib::Variant {
    match name {
        "Category" => "SystemServices".to_variant(),
        "Id" => ID.to_variant(),
        "IconName" => "".to_variant(),
        "Title" => "Cloudsave".to_variant(),
        "Status" => "Active".to_variant(),
        "WindowId" => 0u32.to_variant(),
        "IconPixmap" => pixmaps.to_variant(),
        "OverlayIconPixmap" | "AttentionIconPixmap" => Pixmaps::new().to_variant(),
        "OverlayIconName" | "AttentionIconName" | "AttentionMovieName" => "".to_variant(),
        "ToolTip" => {
            let mut text = snapshot.status.label().to_owned();
            if let Some(entry) = snapshot.active.first() {
                text.push_str(&format!("\n{}", compact(entry)));
                if snapshot.has_error {
                    text.push_str("\nVorheriger Auftrag mit Fehlern");
                }
            }
            ("", pixmaps, "Cloudsave", text).to_variant()
        }
        // Plasma opens DBusMenu on a normal click rather than activating the window.
        "ItemIsMenu" => true.to_variant(),
        "Menu" => glib::variant::ObjectPath::try_from(MENU)
            .unwrap()
            .to_variant(),
        _ => unreachable!("D-Bus validates declared properties"),
    }
}

struct MenuState {
    revision: u32,
    snapshot: jobs::Snapshot,
}
impl MenuState {
    fn new() -> Self {
        Self {
            revision: 1,
            snapshot: jobs::snapshot(),
        }
    }
    fn rows(&self) -> Vec<(i32, String, bool)> {
        let mut rows = vec![
            (1, "Cloudsave öffnen".into(), true),
            (10, "Aktive Jobs".into(), false),
        ];
        if self.snapshot.active.is_empty() {
            rows.push((11, "Keine aktiven Jobs".into(), false));
        }
        for entry in &self.snapshot.active {
            rows.push((100 + entry.id as i32, compact(entry), false));
        }
        rows.push((12, "Letzte Jobs dieser Sitzung".into(), false));
        if self.snapshot.recent.is_empty() {
            rows.push((13, "Noch keine Jobs ausgeführt".into(), false));
        }
        for entry in &self.snapshot.recent {
            rows.push((100 + entry.id as i32, compact(entry), false));
        }
        rows.push((2, "Beenden".into(), true));
        rows
    }
    fn contains(&self, id: i32) -> bool {
        id == 0 || self.rows().iter().any(|row| row.0 == id)
    }
    fn properties(&self, id: i32, names: &[String]) -> Properties {
        let mut props = Properties::new();
        if id == 0 {
            props.insert("children-display".into(), "submenu".to_variant());
        } else if let Some((_, label, enabled)) = self.rows().into_iter().find(|row| row.0 == id) {
            props.insert("label".into(), label.to_variant());
            props.insert("enabled".into(), enabled.to_variant());
            props.insert("visible".into(), true.to_variant());
        }
        props.retain(|key, _| names.is_empty() || names.contains(key));
        props
    }
    fn layout(&self, id: i32, depth: i32, names: &[String]) -> Layout {
        let children = if id == 0 && depth != 0 {
            self.rows()
                .into_iter()
                .map(|(id, _, _)| self.layout(id, 0, names).to_variant())
                .collect()
        } else {
            Vec::new()
        };
        (id, self.properties(id, names), children)
    }
}
fn compact(entry: &jobs::Entry) -> String {
    let text = format!("{} · {}", entry.label, entry.detail).replace(['\n', '\r', '\t'], " ");
    if text.chars().count() > 100 {
        format!("{}…", text.chars().take(99).collect::<String>())
    } else {
        text
    }
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
    state: &MenuState,
    method: &str,
    p: &glib::Variant,
    invocation: gio::DBusMethodInvocation,
) {
    let value = match method {
        "GetLayout" => {
            let id = p.child_get::<i32>(0);
            if !state.contains(id) {
                invocation.return_dbus_error("com.canonical.dbusmenu.Error", "Unknown menu ID");
                return;
            }
            (
                state.revision,
                state.layout(id, p.child_get(1), &p.child_get::<Vec<String>>(2)),
            )
                .to_variant()
        }
        "GetGroupProperties" => {
            let mut ids = p.child_get::<Vec<i32>>(0);
            if ids.is_empty() {
                ids = std::iter::once(0)
                    .chain(state.rows().iter().map(|row| row.0))
                    .collect();
            }
            let names = p.child_get::<Vec<String>>(1);
            let props: Vec<_> = ids
                .into_iter()
                .filter(|id| state.contains(*id))
                .map(|id| (id, state.properties(id, &names)))
                .collect();
            (props,).to_variant()
        }
        "GetProperty" => {
            let id = p.child_get::<i32>(0);
            let name = p.child_get::<String>(1);
            let props = state.properties(id, &[]);
            let Some(value) = props.get(&name).filter(|_| state.contains(id)) else {
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
                .filter(|(id, _, _, _)| !state.contains(*id))
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
