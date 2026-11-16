use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use adw::prelude::*;
use gtk::glib;
use gtk4 as gtk;
use libadwaita as adw;

use cloudsave_core::{Config, State, SyncReport, Target};

pub fn build(window: &adw::ApplicationWindow, config: &Config, autostart: bool) -> gtk::Widget {
    let config = Rc::new(RefCell::new(config.clone()));
    let state_path = Config::default_state_path();

    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let title = gtk::Label::new(Some("Cloudsave"));
    title.add_css_class("title-2");
    header.set_title_widget(Some(&title));
    let sync_btn = gtk::Button::with_label("Jetzt synchronisieren");
    sync_btn.add_css_class("suggested-action");
    sync_btn.add_css_class("pill");
    header.pack_end(&sync_btn);
    toolbar.add_top_bar(&header);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
    content.set_margin_start(24);
    content.set_margin_end(24);
    content.set_margin_top(16);
    content.set_margin_bottom(24);
    content.set_halign(gtk::Align::Center);
    content.set_width_request(720);
    content.set_vexpand(true);

    let banner = adw::Banner::new("Bereit");
    banner.set_revealed(true);
    content.append(&banner);

    let stats = gtk::Grid::new();
    stats.set_column_spacing(16);
    stats.set_hexpand(true);
    let (files_card, files_val) = stat_card("–", "Dateien gesichert");
    let (bytes_card, bytes_val) = stat_card("–", "auf dem Ziel belegt");
    let (last_card, last_val) = stat_card("–", "letzter Sync");
    stats.attach(&files_card, 0, 0, 1, 1);
    stats.attach(&bytes_card, 1, 0, 1, 1);
    stats.attach(&last_card, 2, 0, 1, 1);
    content.append(&stats);

    let folders_group = adw::PreferencesGroup::builder()
        .title("Gesicherte Ordner")
        .build();
    for folder in config.borrow().folders.clone() {
        folders_group.add(&folder_row(&config, &folder));
    }
    let add_row = adw::ActionRow::builder()
        .title("Anderer Ordner hinzufügen …")
        .build();
    {
        let window = window.clone();
        let config = config.clone();
        let folders_group = folders_group.clone();
        add_row.connect_activated(move |_| {
            let dialog = gtk::FileDialog::new();
            let window_arg = window.clone();
            let config = config.clone();
            let folders_group = folders_group.clone();
            dialog.select_folder(
                Some(&window_arg),
                None::<&gtk::gio::Cancellable>,
                move |res| {
                    if let Ok(file) = res {
                        if let Some(path) = file.path() {
                            if !config.borrow().folders.contains(&path) {
                                config.borrow_mut().folders.push(path.clone());
                                let _ = config.borrow().save(&Config::default_path());
                                folders_group.add(&folder_row(&config, &path));
                            }
                        }
                    }
                },
            );
        });
    }
    folders_group.add(&add_row);
    content.append(&folders_group);

    let target_group = adw::PreferencesGroup::builder().title("Ziel").build();
    let target_sub = match &config.borrow().target {
        Target::Local { path } => path.to_string_lossy().to_string(),
    };
    let target_row = adw::ActionRow::builder()
        .title("Lokales Ziel")
        .subtitle(&target_sub)
        .build();
    let test_btn = gtk::Button::with_label("Verbindung testen");
    test_btn.set_valign(gtk::Align::Center);
    {
        let config = config.clone();
        let banner = banner.clone();
        test_btn.connect_clicked(move |_| {
            let cfg = config.borrow().clone();
            match &cfg.target {
                Target::Local { path } => {
                    let r = std::fs::create_dir_all(path).and_then(|_| {
                        let tmp = path.join(".cloudsave-test");
                        std::fs::write(&tmp, b"ok").and_then(|_| std::fs::remove_file(&tmp))
                    });
                    match r {
                        Ok(()) => banner.set_title(&format!("Ziel erreichbar: {}", path.display())),
                        Err(e) => banner.set_title(&format!("Ziel nicht erreichbar: {}", e)),
                    }
                }
            }
        });
    }
    target_row.add_suffix(&test_btn);
    target_group.add(&target_row);
    let auto = adw::SwitchRow::builder()
        .title("Automatisch synchronisieren")
        .active(config.borrow().settings.auto_sync)
        .build();
    {
        let config = config.clone();
        auto.connect_active_notify(move |s| {
            config.borrow_mut().settings.auto_sync = s.is_active();
            let _ = config.borrow().save(&Config::default_path());
        });
    }
    target_group.add(&auto);
    content.append(&target_group);

    let settings = adw::PreferencesGroup::builder()
        .title("Einstellungen")
        .build();
    let rhythm = adw::ComboRow::builder().title("Sync-Rhythmus").build();
    rhythm.set_model(Some(&gtk::StringList::new(&[
        "Bei jeder Änderung",
        "Stündlich",
        "Täglich",
    ])));
    rhythm.set_selected(0);
    settings.add(&rhythm);
    settings.add(
        &adw::SwitchRow::builder()
            .title("Benachrichtigung bei Fehlern")
            .active(true)
            .build(),
    );
    settings.add(
        &adw::SwitchRow::builder()
            .title("Pausieren bei niedrigem Akku")
            .build(),
    );
    content.append(&settings);

    refresh_stats(&state_path, &files_val, &bytes_val, &last_val);
    {
        let config = config.clone();
        let banner = banner.clone();
        let btn_cb = sync_btn.clone();
        let files_val = files_val.clone();
        let bytes_val = bytes_val.clone();
        let last_val = last_val.clone();
        let state_path = state_path.clone();
        sync_btn.connect_clicked(move |_| {
            trigger_sync(
                &config,
                &banner,
                &btn_cb,
                &files_val,
                &bytes_val,
                &last_val,
                &state_path,
            );
        });
    }

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scrolled.set_child(Some(&content));
    scrolled.set_vexpand(true);
    toolbar.set_content(Some(&scrolled));

    if autostart {
        trigger_sync(
            &config,
            &banner,
            &sync_btn,
            &files_val,
            &bytes_val,
            &last_val,
            &state_path,
        );
    }

    toolbar.upcast()
}

fn folder_row(config: &Rc<RefCell<Config>>, path: &Path) -> adw::ActionRow {
    let path = path.to_path_buf();
    let subtitle = path.to_string_lossy().to_string();
    let row = adw::ActionRow::builder().title(&subtitle).build();
    let check = gtk::CheckButton::new();
    check.set_active(true);
    check.set_valign(gtk::Align::Center);
    row.set_activatable_widget(Some(&check));
    row.add_suffix(&check);
    let config = config.clone();
    let path = path.clone();
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
    row
}

fn trigger_sync(
    config: &Rc<RefCell<Config>>,
    banner: &adw::Banner,
    btn: &gtk::Button,
    files_val: &gtk::Label,
    bytes_val: &gtk::Label,
    last_val: &gtk::Label,
    state_path: &Path,
) {
    btn.set_sensitive(false);
    banner.set_title("Sync läuft …");

    let cfg = config.borrow().clone();
    let state_path_owned = state_path.to_path_buf();
    let (tx, rx) = mpsc::channel::<Result<SyncReport, String>>();
    std::thread::spawn(move || {
        let res = cloudsave_core::sync(&cfg, &state_path_owned).map_err(|e| e.to_string());
        let _ = tx.send(res);
    });

    let banner = banner.clone();
    let btn = btn.clone();
    let files_val = files_val.clone();
    let bytes_val = bytes_val.clone();
    let last_val = last_val.clone();
    let state_path = state_path.to_path_buf();
    glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
        Ok(Ok(report)) => {
            if report.errors.is_empty() {
                banner.set_title(&format!(
                    "Alles synchronisiert · {} hochgeladen, {} gelöscht, {} unverändert",
                    report.uploaded, report.deleted, report.skipped
                ));
            } else {
                banner.set_title(&format!(
                    "Sync mit {} Fehler(n) · {} hochgeladen",
                    report.errors.len(),
                    report.uploaded
                ));
            }
            refresh_stats(&state_path, &files_val, &bytes_val, &last_val);
            btn.set_sensitive(true);
            glib::ControlFlow::Break
        }
        Ok(Err(e)) => {
            banner.set_title(&format!("Sync fehlgeschlagen: {}", e));
            btn.set_sensitive(true);
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(mpsc::TryRecvError::Disconnected) => {
            banner.set_title("Sync abgebrochen");
            btn.set_sensitive(true);
            glib::ControlFlow::Break
        }
    });
}

fn refresh_stats(
    state_path: &Path,
    files_val: &gtk::Label,
    bytes_val: &gtk::Label,
    last_val: &gtk::Label,
) {
    match State::open(state_path) {
        Ok(state) => {
            files_val.set_text(&state.count().unwrap_or(0).to_string());
            bytes_val.set_text(&fmt_bytes(state.total_size().unwrap_or(0)));
            last_val.set_text(&fmt_rel(state.last_synced().unwrap_or(None)));
        }
        Err(_) => {
            files_val.set_text("0");
            bytes_val.set_text("0 B");
            last_val.set_text("noch nie");
        }
    }
}

fn stat_card(value: &str, label: &str) -> (gtk::Box, gtk::Label) {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
    card.add_css_class("stat-card");
    card.set_hexpand(true);
    let v = gtk::Label::new(Some(value));
    v.add_css_class("stat-value");
    v.set_xalign(0.0);
    let l = gtk::Label::new(Some(label));
    l.add_css_class("stat-label");
    l.set_xalign(0.0);
    card.append(&v);
    card.append(&l);
    (card, v)
}

fn fmt_bytes(n: i64) -> String {
    if n < 1024 {
        format!("{} B", n)
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else if n < 1024 * 1024 * 1024 {
        format!("{:.1} MB", n as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", n as f64 / (1024.0 * 1024.0 * 1024.0))
    }
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
