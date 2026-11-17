use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use adw::prelude::*;
use gtk::glib;
use gtk4 as gtk;
use libadwaita as adw;

use cloudsave_core::{
    ChangeWatcher, Config, State, SyncMode, SyncReport, Target, WatchConfig, WatchEvent,
};

use crate::{icon, Icon};

#[derive(Clone)]
struct SyncUi {
    title: gtk::Label,
    sub: gtk::Label,
    progress: gtk::ProgressBar,
    btn: gtk::Button,
    banner: gtk::Box,
    symbol: gtk::DrawingArea,
    files: gtk::Label,
    size: gtk::Label,
    last_sync: gtk::Label,
}

fn separator() -> gtk::Separator {
    let line = gtk::Separator::new(gtk::Orientation::Horizontal);
    line.add_css_class("row-sep");
    line
}

fn stat_card(symbol: Icon, caption: &str) -> (gtk::Box, gtk::Label) {
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 17);
    card.add_css_class("card");
    card.add_css_class("stat-card");
    let glyph = icon(symbol, 34, 0.96, 0.74, 0.51);
    glyph.add_css_class("stat-icon");
    card.append(&glyph);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    let value = gtk::Label::new(Some("—"));
    value.add_css_class("stat-value");
    value.set_xalign(0.0);
    value.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let name = gtk::Label::new(Some(caption));
    name.add_css_class("stat-caption");
    name.set_xalign(0.0);
    name.set_wrap(true);
    text.append(&value);
    text.append(&name);
    card.append(&text);
    (card, value)
}

fn update_stats(ui: &SyncUi, state_path: &Path) {
    let values = State::open(state_path)
        .and_then(|state| Ok((state.count()?, state.total_size()?, state.last_synced()?)));
    match values {
        Ok((count, size, last)) => {
            ui.files.set_text(&count.to_string());
            let mut bytes = size.max(0) as f64;
            let mut unit = "B";
            for next in ["KiB", "MiB", "GiB", "TiB"] {
                if bytes < 1024.0 {
                    break;
                }
                bytes /= 1024.0;
                unit = next;
            }
            ui.size.set_text(&if unit == "B" {
                format!("{size} B")
            } else {
                format!("{bytes:.1} {unit}").replace('.', ",")
            });
            ui.last_sync.set_text(&if last.is_some() {
                fmt_rel(last)
            } else {
                "Noch nie".into()
            });
        }
        Err(_) => {
            ui.files.set_text("—");
            ui.size.set_text("—");
            ui.last_sync.set_text("—");
        }
    }
}

fn set_status(ui: &SyncUi, status: &str) {
    for class in ["success", "busy", "error"] {
        ui.banner.remove_css_class(class);
    }
    ui.banner.add_css_class(status);
    let (glyph, color) = match status {
        "success" => (Icon::Check, (0.64, 0.87, 0.54)),
        "error" => (Icon::Warning, (1.0, 0.68, 0.60)),
        _ => (Icon::Refresh, (1.0, 0.72, 0.43)),
    };
    crate::set_icon(&ui.symbol, glyph, color.0, color.1, color.2);
    ui.progress.set_visible(status == "busy");
}

fn endpoint_card(
    window: &gtk::Window,
    nav: &adw::NavigationView,
    target: &Target,
    index: usize,
    count: usize,
) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("card");
    card.add_css_class("panel");
    card.add_css_class("endpoint-card");
    let heading = gtk::Label::new(Some(&if count == 1 {
        "Ziel".into()
    } else {
        format!("Ziel {}", index + 1)
    }));
    heading.add_css_class("section-title");
    heading.set_xalign(0.0);
    heading.set_margin_bottom(16);
    card.append(&heading);
    card.append(&separator());

    let (name, description) = match target {
        Target::Local { path } => ("Lokaler Ordner", path.to_string_lossy().to_string()),
        Target::Webdav { url, user } => (
            if glib::Uri::parse(url, glib::UriFlags::NONE)
                .ok()
                .and_then(|uri| uri.host())
                .is_some_and(|host| host.ends_with(".your-storagebox.de"))
            {
                "Hetzner Storage Box"
            } else {
                "WebDAV"
            },
            format!("{} · Benutzer {}", url, user),
        ),
    };
    let details = gtk::Box::new(gtk::Orientation::Horizontal, 15);
    details.add_css_class("target-details");
    details.append(&icon(
        if matches!(target, Target::Local { .. }) {
            Icon::Storage
        } else {
            Icon::Server
        },
        36,
        0.96,
        0.74,
        0.51,
    ));
    let text = gtk::Box::new(gtk::Orientation::Vertical, 4);
    text.set_hexpand(true);
    let title = gtk::Label::new(Some(name));
    title.add_css_class("target-title");
    title.set_xalign(0.0);
    title.set_wrap(true);
    text.append(&title);
    let subtitle = gtk::Label::new(Some(&description));
    subtitle.add_css_class("target-sub");
    subtitle.set_xalign(0.0);
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    subtitle.set_width_chars(20);
    subtitle.set_max_width_chars(30);
    subtitle.set_tooltip_text(Some(&description));
    text.append(&subtitle);
    details.append(&text);
    card.append(&details);

    let test = gtk::Button::with_label("Verbindung testen");
    test.add_css_class("secondary-btn");
    test.set_halign(gtk::Align::Start);
    card.append(&test);
    let status = gtk::Label::new(None);
    status.set_wrap(true);
    status.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    status.set_selectable(true);
    status.set_xalign(0.0);
    status.set_margin_top(10);
    status.set_visible(false);
    card.append(&status);
    {
        let target = target.clone();
        test.connect_clicked(move |button| {
            button.set_sensitive(false);
            status.set_visible(true);
            status.remove_css_class("error");
            status.set_text("Verbindung wird geprüft …");
            let target = target.clone();
            let label = cloudsave_core::target_label(&target);
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let result = cloudsave_core::backend_for(&target)
                    .and_then(|backend| backend.test_connection().map_err(Into::into));
                let _ = tx.send(result);
            });
            let status = status.clone();
            let button = button.clone();
            glib::timeout_add_local(Duration::from_millis(150), move || {
                let result = match rx.try_recv() {
                    Ok(result) => result,
                    Err(mpsc::TryRecvError::Empty) => return glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => Err(cloudsave_core::Error(
                        "Verbindungsprüfung abgebrochen. Bitte erneut versuchen.".into(),
                    )),
                };
                button.set_sensitive(true);
                match result {
                    Ok(()) => status.set_text(&format!("{label} ✓")),
                    Err(error) => {
                        status.add_css_class("error");
                        status.set_text(&format!("{label}: {error}"));
                    }
                }
                glib::ControlFlow::Break
            });
        });
    }

    let restore = gtk::Button::with_label("Sicherungen wiederherstellen");
    restore.add_css_class("add-btn");
    restore.set_margin_top(12);
    card.append(&restore);
    {
        let window = window.clone();
        let nav = nav.clone();
        let target = target.clone();
        restore.connect_clicked(move |_| {
            nav.push(&crate::restore::page(
                &window,
                std::slice::from_ref(&target),
            ));
        });
    }
    card
}

pub fn build(
    window: &gtk::Window,
    nav: &adw::NavigationView,
    config: &Config,
    autostart: bool,
) -> gtk::Widget {
    crate::jobs::set_auto_sync(config.settings.auto_sync);
    let config = Rc::new(RefCell::new(config.clone()));
    let state_path = Config::default_state_path();

    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.add_css_class("dashboard");
    page.set_spacing(16);

    let banner = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    banner.add_css_class("status-banner");
    let status_symbol = icon(Icon::Refresh, 46, 1.0, 0.72, 0.43);
    status_symbol.set_halign(gtk::Align::Start);
    status_symbol.add_css_class("status-symbol");
    banner.append(&status_symbol);
    let status_text = gtk::Box::new(gtk::Orientation::Vertical, 5);
    status_text.set_hexpand(true);
    banner.append(&status_text);
    let sync_btn = gtk::Button::with_label("Jetzt synchronisieren");
    sync_btn.add_css_class("sync-btn");
    sync_btn.set_valign(gtk::Align::Center);
    let action = gtk::Box::new(gtk::Orientation::Horizontal, 9);
    action.append(&gtk::Image::from_icon_name("view-refresh-symbolic"));
    action.append(&gtk::Label::new(Some("Jetzt synchronisieren")));
    sync_btn.set_child(Some(&action));
    banner.append(&sync_btn);
    page.append(&banner);

    let status_title = gtk::Label::new(Some("Bereit"));
    status_title.add_css_class("sync-title");
    status_title.set_halign(gtk::Align::Start);
    status_title.set_xalign(0.0);
    status_title.set_wrap(true);
    status_text.append(&status_title);

    let status_sub = gtk::Label::new(Some("Noch kein Sync durchgeführt"));
    status_sub.add_css_class("sync-sub");
    status_sub.set_halign(gtk::Align::Start);
    status_sub.set_xalign(0.0);
    status_sub.set_wrap(true);
    status_sub.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    status_sub.set_selectable(true);
    status_text.append(&status_sub);

    let progress = gtk::ProgressBar::new();
    progress.add_css_class("sync-progress");
    progress.set_fraction(0.0);
    progress.set_hexpand(true);
    progress.set_visible(false);
    status_text.append(&progress);

    let auto_status = gtk::Label::new(None);
    auto_status.set_wrap(true);
    auto_status.set_xalign(0.0);
    auto_status.add_css_class("watch-status");

    let cols = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    cols.set_hexpand(true);

    page.append(&cols);

    let left = gtk::Box::new(gtk::Orientation::Vertical, 0);
    left.add_css_class("card");
    left.add_css_class("panel");
    left.set_valign(gtk::Align::Fill);
    left.set_hexpand(true);
    cols.append(&left);

    let section = gtk::Label::new(Some("Gesicherte Ordner"));
    section.add_css_class("section-title");
    section.set_halign(gtk::Align::Start);
    section.set_margin_bottom(16);
    left.append(&section);

    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for folder in config.borrow().folders.clone() {
        append_folder_row(&list, &config, &folder);
    }
    left.append(&list);

    let add_btn = gtk::Button::new();
    add_btn.add_css_class("add-btn");
    add_btn.set_halign(gtk::Align::Start);
    add_btn.set_margin_top(14);
    let add_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    add_box.append(&gtk::Image::from_icon_name("list-add-symbolic"));
    add_box.append(&gtk::Label::new(Some("Ordner hinzufügen")));
    add_btn.set_child(Some(&add_box));
    {
        let window = window.clone();
        let config = config.clone();
        let list = list.clone();
        add_btn.connect_clicked(move |_| {
            let dialog = gtk::FileDialog::new();
            let window_arg = window.clone();
            let config = config.clone();
            let list = list.clone();
            dialog.select_folder(
                Some(&window_arg),
                None::<&gtk::gio::Cancellable>,
                move |res| {
                    if let Ok(file) = res {
                        if let Some(path) = file.path() {
                            if !config.borrow().folders.contains(&path) {
                                config.borrow_mut().folders.push(path.clone());
                                let _ = config.borrow().save(&Config::default_path());
                                append_folder_row(&list, &config, &path);
                            }
                        }
                    }
                },
            );
        });
    }
    left.append(&add_btn);

    let settings_card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    settings_card.add_css_class("card");
    let expander_btn = gtk::Button::new();
    expander_btn.add_css_class("flat");
    expander_btn.add_css_class("expander-btn");
    let exp_hbox = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let exp_label = gtk::Label::new(Some("Einstellungen"));
    exp_label.set_halign(gtk::Align::Start);
    exp_label.set_hexpand(true);
    let arrow = gtk::Image::from_icon_name("pan-down-symbolic");
    arrow.set_pixel_size(14);
    arrow.set_valign(gtk::Align::Center);
    exp_hbox.append(&exp_label);
    exp_hbox.append(&arrow);
    expander_btn.set_child(Some(&exp_hbox));
    settings_card.append(&expander_btn);

    let revealer = gtk::Revealer::new();
    revealer.set_transition_type(gtk::RevealerTransitionType::SlideDown);
    let settings_body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    settings_body.add_css_class("settings-body");
    let body_sep = gtk::Separator::new(gtk::Orientation::Horizontal);
    body_sep.add_css_class("row-sep");
    settings_body.append(&body_sep);

    let interval_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    interval_row.add_css_class("folder-row");
    let interval_label = gtk::Label::new(Some("Zusätzlicher Kontrollscan (Minuten)"));
    interval_label.add_css_class("row-label");
    interval_label.set_halign(gtk::Align::Start);
    interval_label.set_hexpand(true);
    let interval_adj = gtk::Adjustment::new(
        config.borrow().settings.sync_interval_minutes as f64,
        1.0,
        120.0,
        1.0,
        10.0,
        0.0,
    );
    let interval_spin = gtk::SpinButton::new(Some(&interval_adj), 1.0, 0);
    interval_spin.set_valign(gtk::Align::Center);
    {
        let config = config.clone();
        interval_spin.connect_value_changed(move |s| {
            let mut cfg = config.borrow_mut();
            cfg.settings.sync_interval_minutes = s.value().max(1.0) as u64;
            let _ = cfg.save(&Config::default_path());
        });
    }
    interval_row.append(&interval_label);
    interval_spin.update_property(&[gtk::accessible::Property::Label(
        "Kontrollscan-Intervall in Minuten",
    )]);
    interval_row.append(&interval_spin);
    settings_body.append(&interval_row);
    let live_hint = gtk::Label::new(Some("Live-Sync erkennt Dateiänderungen und überträgt sie nach etwa 3 Sekunden Ruhezeit. Der zusätzliche Kontrollscan fängt verpasste Ereignisse auf; er bestimmt nicht die Live-Sync-Geschwindigkeit."));
    live_hint.set_wrap(true);
    live_hint.set_xalign(0.0);
    live_hint.add_css_class("dim-label");
    settings_body.append(&live_hint);

    settings_body.append(&crate::exclusion_picker::build(window, config.clone()));

    let mode_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    mode_box.add_css_class("folder-row");
    let mode_label = gtk::Label::new(Some("Sicherungsmodus"));
    mode_label.set_xalign(0.0);
    mode_label.add_css_class("row-label");
    mode_box.append(&mode_label);
    let mode =
        gtk::DropDown::from_strings(&["Backup (keine Löschungen)", "Spiegelung (mit Löschungen)"]);
    mode.set_selected(if config.borrow().settings.sync_mode == SyncMode::Backup {
        0
    } else {
        1
    });
    mode_box.append(&mode);
    let mode_hint = gtk::Label::new(Some(
        "Bei Spiegelung werden lokal gelöschte Dateien auch aus den Sicherungen entfernt.",
    ));
    mode_hint.set_wrap(true);
    mode_hint.set_xalign(0.0);
    mode_hint.add_css_class("dim-label");
    mode_box.append(&mode_hint);
    {
        let config = config.clone();
        let reverting = Rc::new(std::cell::Cell::new(false));
        mode.connect_selected_notify(move |dropdown| {
            if reverting.get() { return; }
            let mut next = config.borrow().clone();
            next.settings.sync_mode = if dropdown.selected() == 0 { SyncMode::Backup } else { SyncMode::Mirror };
            match next.save(&Config::default_path()) {
                Ok(()) => { *config.borrow_mut() = next; mode_hint.set_text("Bei Spiegelung werden lokal gelöschte Dateien auch aus den Sicherungen entfernt."); }
                Err(error) => {
                    mode_hint.set_text(&format!("Modus nicht gespeichert: {error}"));
                    reverting.set(true);
                    dropdown.set_selected(if config.borrow().settings.sync_mode == SyncMode::Backup { 0 } else { 1 });
                    reverting.set(false);
                }
            }
        });
    }
    settings_body.append(&mode_box);

    {
        let autostart = config.borrow().settings.autostart;
        let config = config.clone();
        switch_row(
            &settings_body,
            "Beim Systemstart automatisch starten",
            autostart,
            move |on| {
                {
                    let mut cfg = config.borrow_mut();
                    cfg.settings.autostart = on;
                    let _ = cfg.save(&Config::default_path());
                }
                let _ = cloudsave_core::set_autostart(on);
            },
        );
    }
    {
        let run_bg = config.borrow().settings.run_in_background;
        let config = config.clone();
        switch_row(
            &settings_body,
            "Beim Schließen im Hintergrund weiterlaufen",
            run_bg,
            move |on| {
                let mut cfg = config.borrow_mut();
                cfg.settings.run_in_background = on;
                let _ = cfg.save(&Config::default_path());
            },
        );
    }

    let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
    sep.add_css_class("row-sep");
    settings_body.append(&sep);
    let quit_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    quit_row.add_css_class("folder-row");
    let quit_label = gtk::Label::new(Some("Beenden"));
    quit_label.add_css_class("row-label");
    quit_label.set_halign(gtk::Align::Start);
    quit_label.set_hexpand(true);
    let quit_btn = gtk::Button::with_label("App beenden");
    quit_btn.set_valign(gtk::Align::Center);
    {
        let window = window.clone();
        quit_btn.connect_clicked(move |_| {
            if let Some(app) = window.application() {
                app.quit();
            }
        });
    }
    quit_row.append(&quit_label);
    quit_row.append(&quit_btn);
    settings_body.append(&quit_row);

    revealer.set_child(Some(&settings_body));
    settings_card.append(&revealer);
    let rev = revealer.clone();
    let arr = arrow.clone();
    expander_btn.connect_clicked(move |_| {
        let revealed = rev.reveals_child();
        rev.set_reveal_child(!revealed);
        arr.set_icon_name(Some(if revealed {
            "pan-down-symbolic"
        } else {
            "pan-up-symbolic"
        }));
    });

    let right = gtk::Box::new(gtk::Orientation::Vertical, 16);
    right.set_width_request(365);
    cols.append(&right);
    let targets = config.borrow().targets.clone();
    if targets.is_empty() {
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 8);
        empty.add_css_class("card");
        empty.add_css_class("panel");
        let title = gtk::Label::new(Some("Kein Endpunkt konfiguriert"));
        title.add_css_class("target-title");
        title.set_xalign(0.0);
        title.set_wrap(true);
        empty.append(&title);
        right.append(&empty);
    }
    for (index, target) in targets.iter().enumerate() {
        right.append(&endpoint_card(window, nav, target, index, targets.len()));
    }

    let add_target_btn = gtk::Button::new();
    add_target_btn.add_css_class("add-btn");
    add_target_btn.set_halign(gtk::Align::Start);
    add_target_btn.set_margin_top(10);
    let add_target_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    add_target_box.append(&icon(Icon::Cloud, 18, 1.0, 1.0, 1.0));
    add_target_box.append(&gtk::Label::new(Some("Weiteren Endpunkt hinzufügen")));
    add_target_btn.set_child(Some(&add_target_box));
    {
        let window = window.clone();
        let nav = nav.clone();
        let config = config.clone();
        add_target_btn.connect_clicked(move |_| {
            nav.push(&crate::onboarding::add_target_page(
                &window,
                &nav,
                config.clone(),
            ));
        });
    }
    right.append(&add_target_btn);

    let automatic = gtk::Box::new(gtk::Orientation::Vertical, 6);
    automatic.add_css_class("sync-controls");
    let auto_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let auto_text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    auto_text.set_hexpand(true);
    let auto_label = gtk::Label::new(Some("Automatisch synchronisieren"));
    auto_label.add_css_class("target-row-label");
    auto_label.set_xalign(0.0);
    auto_label.set_wrap(true);
    auto_text.append(&auto_label);
    let scope = gtk::Label::new(Some("Für alle Endpunkte"));
    scope.add_css_class("watch-status");
    scope.set_xalign(0.0);
    auto_text.append(&scope);
    let auto_switch = gtk::Switch::new();
    auto_switch.set_active(config.borrow().settings.auto_sync);
    auto_switch.set_valign(gtk::Align::Center);
    auto_switch.update_property(&[gtk::accessible::Property::Label(
        "Automatisch synchronisieren – für alle Endpunkte",
    )]);
    {
        let config = config.clone();
        auto_switch.connect_active_notify(move |switch| {
            crate::jobs::set_auto_sync(switch.is_active());
            config.borrow_mut().settings.auto_sync = switch.is_active();
            let _ = config.borrow().save(&Config::default_path());
        });
    }
    auto_row.append(&auto_text);
    auto_row.append(&auto_switch);
    automatic.append(&auto_row);
    automatic.append(&auto_status);
    right.append(&automatic);

    let stats = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    stats.set_homogeneous(true);
    let (files_card, files_value) = stat_card(Icon::File, "Dateien gesichert");
    let (size_card, size_value) = stat_card(Icon::Storage, "Erfasste Sicherungsgröße");
    let (time_card, time_value) = stat_card(Icon::Clock, "Letzte Übertragung");
    files_card.set_tooltip_text(Some("Erfasste Dateikopien über alle Sicherungsziele."));
    size_card.set_tooltip_text(Some("Größe der erfassten Sicherungskopien über alle Ziele. Dies ist nicht der gesamte belegte oder freie Speicherplatz des Servers."));
    time_card.set_tooltip_text(Some("Zeitpunkt der letzten erfassten Dateiübertragung."));
    stats.append(&files_card);
    stats.append(&size_card);
    stats.append(&time_card);
    page.insert_child_after(&stats, Some(&banner));
    page.append(&settings_card);

    let ui = SyncUi {
        title: status_title.clone(),
        sub: status_sub.clone(),
        progress: progress.clone(),
        btn: sync_btn.clone(),
        banner: banner.clone(),
        symbol: status_symbol.clone(),
        files: files_value,
        size: size_value,
        last_sync: time_value,
    };
    ui.sub.set_text(&status_sub_text(&state_path));
    update_stats(&ui, &state_path);
    if config.borrow().targets.is_empty() {
        set_status(&ui, "error");
        ui.title.set_text("Endpunkt einrichten");
        ui.sub
            .set_text("Kein Endpunkt konfiguriert. Bitte einen Endpunkt hinzufügen.");
        ui.btn.set_sensitive(false);
    }
    {
        let config = config.clone();
        let ui = ui.clone();
        let state_path = state_path.clone();
        sync_btn.connect_clicked(move |_| {
            trigger_sync(&config, &ui, &state_path);
        });
    }

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    let responsive = adw::BreakpointBin::new();
    responsive.set_size_request(360, 360);
    scrolled.set_child(Some(&page));
    responsive.set_child(Some(&scrolled));
    let narrow = adw::Breakpoint::new(
        adw::BreakpointCondition::parse("max-width: 860px").expect("valid layout breakpoint"),
    );
    narrow.add_setter(
        &cols,
        "orientation",
        Some(&gtk::Orientation::Vertical.to_value()),
    );
    narrow.add_setter(&right, "width-request", Some(&(-1_i32).to_value()));
    narrow.add_setter(
        &banner,
        "orientation",
        Some(&gtk::Orientation::Vertical.to_value()),
    );
    narrow.add_setter(
        &stats,
        "orientation",
        Some(&gtk::Orientation::Vertical.to_value()),
    );
    responsive.add_breakpoint(narrow);
    scrolled.set_vexpand(true);

    if autostart {
        trigger_sync(&config, &ui, &state_path);
    }

    {
        let config = config.clone();
        let ui = ui.clone();
        let state_path = state_path.clone();
        let mut watch_config = WatchConfig::for_config(&config.borrow(), &state_path);
        let watcher = ChangeWatcher::new(watch_config.clone());
        let mut pending: Option<Instant> = None;
        let mut watch_ready = false;
        let mut watch_error: Option<String> = None;
        let mut last = Instant::now();
        let mut stats_updated = Instant::now();
        let owner = page.downgrade();
        glib::timeout_add_local(Duration::from_millis(500), move || {
            if owner.upgrade().is_none() {
                return glib::ControlFlow::Break;
            }
            if stats_updated.elapsed() >= Duration::from_secs(30) {
                update_stats(&ui, &state_path);
                stats_updated = Instant::now();
            }
            let cfg = config.borrow();
            let next_watch_config = WatchConfig::for_config(&cfg, &state_path);
            if next_watch_config != watch_config {
                watch_config = next_watch_config;
                watcher.update(watch_config.clone());
                pending = None;
                watch_ready = false;
                watch_error = None;
            }
            while let Ok(event) = watcher.try_recv() {
                match event {
                    WatchEvent::Changed if cfg.settings.auto_sync => pending = Some(Instant::now()),
                    WatchEvent::Ready => {
                        watch_ready = true;
                        watch_error = None;
                    }
                    WatchEvent::Unavailable(error) => {
                        watch_ready = false;
                        watch_error = Some(error);
                    }
                    _ => {}
                }
            }
            let message = if !cfg.settings.auto_sync {
                "Live-Sync pausiert. Manuelles Synchronisieren ist weiterhin möglich.".to_string()
            } else if let Some(error) = &watch_error {
                format!("Live-Sync eingeschränkt; der Kontrollscan bleibt aktiv: {error}")
            } else if watch_ready {
                "Live-Sync aktiv · Übertragung nach etwa 3 Sekunden Ruhezeit.".to_string()
            } else {
                "Live-Sync wird vorbereitet …".to_string()
            };
            if auto_status.text().as_str() != message {
                auto_status.set_text(&message);
            }
            if !cfg.settings.auto_sync {
                pending = None;
                return glib::ControlFlow::Continue;
            }
            let due = pending.is_some_and(|time| time.elapsed() >= Duration::from_secs(3));
            let fallback_due = last.elapsed()
                >= Duration::from_secs(
                    cfg.settings.sync_interval_minutes.max(1).saturating_mul(60),
                );
            drop(cfg);
            if ui.btn.is_sensitive() && (due || fallback_due) {
                if fallback_due {
                    watcher.update(watch_config.clone());
                }
                pending = None;
                last = Instant::now();
                trigger_sync(&config, &ui, &state_path);
            }
            glib::ControlFlow::Continue
        });
    }

    responsive.upcast()
}

fn switch_row(body: &gtk::Box, name: &str, active: bool, on_toggle: impl Fn(bool) + 'static) {
    let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
    sep.add_css_class("row-sep");
    body.append(&sep);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("folder-row");
    let label = gtk::Label::new(Some(name));
    label.add_css_class("row-label");
    label.set_halign(gtk::Align::Start);
    label.set_hexpand(true);
    let sw = gtk::Switch::new();
    sw.set_active(active);
    sw.connect_active_notify(move |s| on_toggle(s.is_active()));
    row.append(&label);
    row.append(&sw);
    body.append(&row);
}

fn append_folder_row(list: &gtk::Box, config: &Rc<RefCell<Config>>, path: &Path) {
    if list.first_child().is_some() {
        let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
        sep.add_css_class("row-sep");
        list.append(&sep);
    }
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("folder-row");
    let check = gtk::CheckButton::new();
    check.set_active(true);
    row.append(&check);
    row.append(&icon(Icon::Folder, 30, 0.83, 0.80, 0.77));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    let label = gtk::Label::new(Some(&name));
    label.add_css_class("folder-name");
    label.set_xalign(0.0);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_width_chars(15);
    label.set_max_width_chars(40);
    label.set_tooltip_text(Some(&name));
    check.update_property(&[gtk::accessible::Property::Label(&format!("{name} sichern"))]);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    text.append(&label);
    let path_text = path.to_string_lossy();
    let location = gtk::Label::new(Some(&path_text));
    location.add_css_class("folder-path");
    location.set_xalign(0.0);
    location.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    location.set_width_chars(15);
    location.set_max_width_chars(45);
    location.set_tooltip_text(Some(&path_text));
    text.append(&location);
    row.append(&text);
    let config = config.clone();
    let path = path.to_path_buf();
    check.connect_toggled(move |c| {
        let mut cfg = config.borrow_mut();
        if c.is_active() {
            if !cfg.folders.contains(&path) {
                cfg.folders.push(path.clone());
            }
        } else {
            cfg.folders.retain(|p| p != &path);
        }
        let _ = cfg.save(&Config::default_path());
    });
    list.append(&row);
}

fn status_sub_text(state_path: &Path) -> String {
    match State::open(state_path) {
        Ok(state) => format!(
            "{} Dateien gesichert · letzter Sync {}",
            state.count().unwrap_or(0),
            fmt_rel(state.last_synced().unwrap_or(None))
        ),
        Err(_) => "Noch kein Sync durchgeführt".to_string(),
    }
}

fn trigger_sync(config: &Rc<RefCell<Config>>, ui: &SyncUi, state_path: &Path) {
    if config.borrow().targets.is_empty() {
        set_status(ui, "error");
        ui.title.set_text("Endpunkt einrichten");
        ui.sub
            .set_text("Kein Endpunkt konfiguriert. Bitte einen Endpunkt hinzufügen.");
        ui.progress.set_fraction(0.0);
        ui.btn.set_sensitive(false);
        return;
    }
    ui.btn.set_sensitive(false);
    set_status(ui, "busy");
    ui.title.set_text("Sync läuft");
    ui.sub.set_text("Dateien werden abgeglichen …");
    ui.progress.set_fraction(0.0);

    let cfg = config.borrow().clone();
    let state_path_owned = state_path.to_path_buf();
    let (tx, rx) = mpsc::channel::<Result<SyncReport, String>>();
    std::thread::spawn(move || {
        let res = crate::jobs::sync(&cfg, &state_path_owned).map_err(|e| e.to_string());
        let _ = tx.send(res);
    });

    let ui = ui.clone();
    let state_path = state_path.to_path_buf();
    glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
        Ok(Ok(report)) => {
            update_stats(&ui, &state_path);
            if report.errors.is_empty() {
                set_status(&ui, "success");
                ui.title.set_text("Alles synchronisiert");
                ui.sub.set_text(&status_sub_text(&state_path));
                ui.progress.set_fraction(1.0);
            } else {
                set_status(&ui, "error");
                ui.title
                    .set_text(&format!("Sync mit {} Fehler(n)", report.errors.len()));
                ui.sub.set_text(&format!(
                    "{}\n{}\nMit „Jetzt synchronisieren“ erneut versuchen.",
                    status_sub_text(&state_path),
                    report.errors.join("\n")
                ));
                ui.progress.set_fraction(0.0);
            }
            ui.btn.set_sensitive(true);
            glib::ControlFlow::Break
        }
        Ok(Err(e)) => {
            set_status(&ui, "error");
            update_stats(&ui, &state_path);
            ui.title.set_text("Sync fehlgeschlagen");
            ui.sub.set_text(&e);
            ui.progress.set_fraction(0.0);
            ui.btn.set_sensitive(true);
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => {
            ui.progress.pulse();
            glib::ControlFlow::Continue
        }
        Err(mpsc::TryRecvError::Disconnected) => {
            set_status(&ui, "error");
            ui.title.set_text("Sync abgebrochen");
            ui.sub
                .set_text("Mit „Jetzt synchronisieren“ erneut versuchen.");
            ui.progress.set_fraction(0.0);
            ui.btn.set_sensitive(true);
            glib::ControlFlow::Break
        }
    });
}

fn fmt_rel(secs: Option<i64>) -> String {
    match secs {
        None => "noch nie".to_string(),
        Some(s) => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let diff = (now - s).max(0);
            if diff < 60 {
                "gerade eben".to_string()
            } else if diff < 3600 {
                format!("vor {} Min", diff / 60)
            } else if diff < 86400 {
                format!("vor {} Std", diff / 3600)
            } else {
                format!("vor {} Tagen", diff / 86400)
            }
        }
    }
}
