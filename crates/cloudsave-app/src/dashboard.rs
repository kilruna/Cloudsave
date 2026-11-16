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

use crate::{icon, Icon};

#[derive(Clone)]
struct SyncUi {
    title: gtk::Label,
    sub: gtk::Label,
    progress: gtk::ProgressBar,
    btn: gtk::Button,
}

pub fn build(window: &gtk::Window, config: &Config, autostart: bool) -> gtk::Widget {
    let config = Rc::new(RefCell::new(config.clone()));
    let state_path = Config::default_state_path();

    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.set_margin_top(26);
    page.set_margin_bottom(32);
    page.set_margin_start(36);
    page.set_margin_end(36);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    header.set_margin_bottom(30);
    header.append(&icon(Icon::Cloud, 26, 0.616, 0.486, 1.0));
    let title = gtk::Label::new(Some("Cloudsave"));
    title.add_css_class("app-title");
    title.set_valign(gtk::Align::Center);
    header.append(&title);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    header.append(&spacer);
    let sync_btn = gtk::Button::with_label("Jetzt synchronisieren");
    sync_btn.add_css_class("sync-btn");
    sync_btn.set_valign(gtk::Align::Center);
    header.append(&sync_btn);
    page.append(&header);

    let status_title = gtk::Label::new(Some("Bereit"));
    status_title.add_css_class("sync-title");
    status_title.set_halign(gtk::Align::Start);
    page.append(&status_title);

    let status_sub = gtk::Label::new(Some("Noch kein Sync durchgeführt"));
    status_sub.add_css_class("sync-sub");
    status_sub.set_halign(gtk::Align::Start);
    status_sub.set_margin_top(6);
    page.append(&status_sub);

    let progress = gtk::ProgressBar::new();
    progress.add_css_class("sync-progress");
    progress.set_fraction(0.0);
    progress.set_halign(gtk::Align::Start);
    progress.set_width_request(560);
    progress.set_margin_top(14);
    page.append(&progress);

    let cols = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    cols.set_hexpand(true);
    cols.set_margin_top(30);
    page.append(&cols);

    let left = gtk::Box::new(gtk::Orientation::Vertical, 0);
    left.set_hexpand(true);
    cols.append(&left);

    let section = gtk::Label::new(Some("Gesicherte Ordner"));
    section.add_css_class("section-title");
    section.set_halign(gtk::Align::Start);
    section.set_margin_start(4);
    section.set_margin_top(4);
    section.set_margin_bottom(12);
    left.append(&section);

    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list.add_css_class("card");
    for folder in config.borrow().folders.clone() {
        append_folder_row(&list, &config, &folder);
    }
    left.append(&list);

    let add_btn = gtk::Button::new();
    add_btn.add_css_class("add-btn");
    add_btn.set_halign(gtk::Align::Start);
    add_btn.set_margin_top(14);
    let add_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    add_box.append(&icon(Icon::Folder, 18, 1.0, 1.0, 1.0));
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
    settings_card.set_margin_top(22);
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
    let body_sep = gtk::Separator::new(gtk::Orientation::Horizontal);
    body_sep.add_css_class("row-sep");
    settings_body.append(&body_sep);

    let rhythm_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    rhythm_row.add_css_class("folder-row");
    let rhythm_label = gtk::Label::new(Some("Sync-Rhythmus"));
    rhythm_label.add_css_class("row-label");
    rhythm_label.set_halign(gtk::Align::Start);
    rhythm_label.set_hexpand(true);
    let rhythm = gtk::DropDown::from_strings(&["Bei jeder Änderung", "Stündlich", "Täglich"]);
    rhythm.set_selected(0);
    rhythm.set_valign(gtk::Align::Center);
    rhythm_row.append(&rhythm_label);
    rhythm_row.append(&rhythm);
    settings_body.append(&rhythm_row);

    for (name, active) in [
        ("Benachrichtigung bei Fehlern", true),
        ("Pausieren bei niedrigem Akku", false),
    ] {
        let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
        sep.add_css_class("row-sep");
        settings_body.append(&sep);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.add_css_class("folder-row");
        let label = gtk::Label::new(Some(name));
        label.add_css_class("row-label");
        label.set_halign(gtk::Align::Start);
        label.set_hexpand(true);
        let sw = gtk::Switch::new();
        sw.set_active(active);
        row.append(&label);
        row.append(&sw);
        settings_body.append(&row);
    }

    let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
    sep.add_css_class("row-sep");
    settings_body.append(&sep);
    let test_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    test_row.add_css_class("folder-row");
    let test_label = gtk::Label::new(Some("Ziel-Verbindung"));
    test_label.add_css_class("row-label");
    test_label.set_halign(gtk::Align::Start);
    test_label.set_hexpand(true);
    let test_btn = gtk::Button::with_label("Verbindung testen");
    test_btn.set_valign(gtk::Align::Center);
    {
        let config = config.clone();
        let sub = status_sub.clone();
        test_btn.connect_clicked(move |_| {
            let cfg = config.borrow().clone();
            match &cfg.target {
                Target::Local { path } => {
                    let r = std::fs::create_dir_all(path).and_then(|_| {
                        let tmp = path.join(".cloudsave-test");
                        std::fs::write(&tmp, b"ok").and_then(|_| std::fs::remove_file(&tmp))
                    });
                    match r {
                        Ok(()) => sub.set_text(&format!("Ziel erreichbar: {}", path.display())),
                        Err(e) => sub.set_text(&format!("Ziel nicht erreichbar: {}", e)),
                    }
                }
            }
        });
    }
    test_row.append(&test_label);
    test_row.append(&test_btn);
    settings_body.append(&test_row);

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
    left.append(&settings_card);

    let right = gtk::Box::new(gtk::Orientation::Vertical, 0);
    right.set_width_request(360);
    cols.append(&right);

    let target = gtk::Box::new(gtk::Orientation::Vertical, 0);
    target.add_css_class("card");
    target.add_css_class("target-card");
    target.set_valign(gtk::Align::Start);
    let (t_name, t_sub_text) = match &config.borrow().target {
        Target::Local { path } => ("Lokales Ziel", path.to_string_lossy().to_string()),
    };
    let t_title = gtk::Label::new(Some(t_name));
    t_title.add_css_class("target-title");
    t_title.set_halign(gtk::Align::Start);
    target.append(&t_title);
    let t_sub = gtk::Label::new(Some(&t_sub_text));
    t_sub.add_css_class("target-sub");
    t_sub.set_halign(gtk::Align::Start);
    t_sub.set_margin_top(4);
    target.append(&t_sub);
    let t_sep = gtk::Separator::new(gtk::Orientation::Horizontal);
    t_sep.add_css_class("row-sep");
    t_sep.set_margin_top(16);
    t_sep.set_margin_bottom(4);
    target.append(&t_sep);
    let auto_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    auto_row.add_css_class("folder-row");
    auto_row.set_margin_start(-16);
    auto_row.set_margin_end(-16);
    auto_row.set_margin_bottom(-13);
    let auto_label = gtk::Label::new(Some("Automatisch synchronisieren"));
    auto_label.add_css_class("target-row-label");
    auto_label.set_halign(gtk::Align::Start);
    auto_label.set_hexpand(true);
    let auto_switch = gtk::Switch::new();
    auto_switch.set_active(config.borrow().settings.auto_sync);
    {
        let config = config.clone();
        auto_switch.connect_active_notify(move |s| {
            config.borrow_mut().settings.auto_sync = s.is_active();
            let _ = config.borrow().save(&Config::default_path());
        });
    }
    auto_row.append(&auto_label);
    auto_row.append(&auto_switch);
    target.append(&auto_row);
    right.append(&target);

    let ui = SyncUi {
        title: status_title.clone(),
        sub: status_sub.clone(),
        progress: progress.clone(),
        btn: sync_btn.clone(),
    };
    if let Ok(state) = State::open(&state_path) {
        if state.count().unwrap_or(0) > 0 {
            ui.title.set_text("Alles synchronisiert");
            ui.progress.set_fraction(1.0);
        }
        ui.sub.set_text(&status_sub_text(&state_path));
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
    scrolled.set_child(Some(&page));
    scrolled.set_vexpand(true);

    if autostart {
        trigger_sync(&config, &ui, &state_path);
    }

    scrolled.upcast()
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
    row.append(&icon(Icon::Folder, 20, 0.50, 0.70, 0.96));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    let label = gtk::Label::new(Some(&name));
    label.add_css_class("row-label");
    label.set_halign(gtk::Align::Start);
    label.set_hexpand(true);
    row.append(&label);
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
    ui.btn.set_sensitive(false);
    ui.title.set_text("Sync läuft");
    ui.sub.set_text("Dateien werden abgeglichen …");
    ui.progress.set_fraction(0.0);

    let cfg = config.borrow().clone();
    let state_path_owned = state_path.to_path_buf();
    let (tx, rx) = mpsc::channel::<Result<SyncReport, String>>();
    std::thread::spawn(move || {
        let res = cloudsave_core::sync(&cfg, &state_path_owned).map_err(|e| e.to_string());
        let _ = tx.send(res);
    });

    let ui = ui.clone();
    let state_path = state_path.to_path_buf();
    glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
        Ok(Ok(report)) => {
            if report.errors.is_empty() {
                ui.title.set_text("Alles synchronisiert");
            } else {
                ui.title
                    .set_text(&format!("Sync mit {} Fehler(n)", report.errors.len()));
            }
            ui.sub.set_text(&status_sub_text(&state_path));
            ui.progress.set_fraction(1.0);
            ui.btn.set_sensitive(true);
            glib::ControlFlow::Break
        }
        Ok(Err(e)) => {
            ui.title.set_text("Sync fehlgeschlagen");
            ui.sub.set_text(&e);
            ui.btn.set_sensitive(true);
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => {
            ui.progress.pulse();
            glib::ControlFlow::Continue
        }
        Err(mpsc::TryRecvError::Disconnected) => {
            ui.title.set_text("Sync abgebrochen");
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
