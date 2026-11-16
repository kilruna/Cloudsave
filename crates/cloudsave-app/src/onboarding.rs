use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;
use gtk4 as gtk;
use libadwaita as adw;

use cloudsave_core::{Config, Settings, Target};

use crate::dashboard;

pub fn build(window: &gtk::Window, nav: &adw::NavigationView) {
    nav.push(&welcome_page(window, nav));
}

fn shell(
    title: &str,
    content: &impl IsA<gtk::Widget>,
    action: Option<gtk::Button>,
) -> adw::NavigationPage {
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let label = gtk::Label::new(Some(title));
    label.add_css_class("title-4");
    header.set_title_widget(Some(&label));
    toolbar.add_top_bar(&header);

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scrolled.set_child(Some(content));
    scrolled.set_vexpand(true);
    toolbar.set_content(Some(&scrolled));

    if let Some(btn) = action {
        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        bar.set_margin_top(12);
        bar.set_margin_bottom(12);
        bar.set_margin_start(24);
        bar.set_margin_end(24);
        bar.set_halign(gtk::Align::End);
        bar.append(&btn);
        toolbar.add_bottom_bar(&bar);
    }

    adw::NavigationPage::new(&toolbar, title)
}

fn page_content() -> gtk::Box {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
    content.set_margin_start(24);
    content.set_margin_end(24);
    content.set_margin_top(16);
    content.set_margin_bottom(24);
    content.set_halign(gtk::Align::Center);
    content.set_width_request(640);
    content
}

fn welcome_page(window: &gtk::Window, nav: &adw::NavigationView) -> adw::NavigationPage {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
    content.set_vexpand(true);
    content.set_valign(gtk::Align::Center);
    content.set_halign(gtk::Align::Center);
    content.set_width_request(440);
    content.set_margin_top(48);
    content.set_margin_bottom(48);

    let title = gtk::Label::new(Some("Cloudsave"));
    title.add_css_class("title-1");
    title.set_halign(gtk::Align::Center);

    let sub = gtk::Label::new(Some(
        "Sichere deine Ordner automatisch – lokal oder in der Cloud, z. B. auf einer Hetzner Storage Box.",
    ));
    sub.add_css_class("dim-label");
    sub.set_wrap(true);
    sub.set_justify(gtk::Justification::Center);

    let start = gtk::Button::with_label("Einrichtung starten");
    start.add_css_class("suggested-action");
    start.add_css_class("pill");
    start.set_halign(gtk::Align::Center);
    {
        let nav = nav.clone();
        let window = window.clone();
        start.connect_clicked(move |_| {
            nav.push(&folders_page(&window, &nav));
        });
    }

    content.append(&title);
    content.append(&sub);
    content.append(&start);
    shell("Willkommen", &content, None)
}

fn add_folder_row(
    group: &adw::PreferencesGroup,
    checks: &Rc<RefCell<Vec<(PathBuf, gtk::CheckButton)>>>,
    next: &gtk::Button,
    path: PathBuf,
    label: &str,
    active: bool,
) {
    let subtitle = path.to_string_lossy().to_string();
    let row = adw::ActionRow::builder()
        .title(label)
        .subtitle(&subtitle)
        .build();
    let check = gtk::CheckButton::new();
    check.set_active(active);
    check.set_valign(gtk::Align::Center);
    row.set_activatable_widget(Some(&check));
    row.add_suffix(&check);
    group.add(&row);
    checks.borrow_mut().push((path, check.clone()));
    let checks = checks.clone();
    let next = next.clone();
    check.connect_toggled(move |_| {
        let any = checks.borrow().iter().any(|(_, c)| c.is_active());
        next.set_sensitive(any);
    });
}

fn folders_page(window: &gtk::Window, nav: &adw::NavigationView) -> adw::NavigationPage {
    let checks: Rc<RefCell<Vec<(PathBuf, gtk::CheckButton)>>> = Rc::new(RefCell::new(Vec::new()));
    let next = gtk::Button::with_label("Weiter");
    next.add_css_class("suggested-action");
    next.add_css_class("pill");

    let group = adw::PreferencesGroup::builder()
        .title("Welche Ordner sollen gesichert werden?")
        .build();

    let home = std::env::var("HOME").map(PathBuf::from).unwrap_or_default();
    for (label, dir, active) in [
        ("Dokumente", "Dokumente", true),
        ("Bilder", "Bilder", true),
        ("Musik", "Musik", false),
        ("Videos", "Videos", false),
        ("Schreibtisch", "Schreibtisch", false),
    ] {
        let path = home.join(dir);
        add_folder_row(&group, &checks, &next, path, label, active);
    }
    next.set_sensitive(checks.borrow().iter().any(|(_, c)| c.is_active()));

    let add_row = adw::ActionRow::builder()
        .title("Anderer Ordner hinzufügen …")
        .build();
    {
        let group = group.clone();
        let checks = checks.clone();
        let next = next.clone();
        let window = window.clone();
        add_row.connect_activated(move |_| {
            let dialog = gtk::FileDialog::new();
            let group = group.clone();
            let checks = checks.clone();
            let next = next.clone();
            dialog.select_folder(Some(&window), None::<&gtk::gio::Cancellable>, move |res| {
                if let Ok(file) = res {
                    if let Some(path) = file.path() {
                        let label = path.to_string_lossy().to_string();
                        add_folder_row(&group, &checks, &next, path, &label, true);
                        next.set_sensitive(true);
                    }
                }
            });
        });
    }
    group.add(&add_row);

    let content = page_content();
    content.append(&group);

    {
        let nav = nav.clone();
        let window = window.clone();
        let checks = checks.clone();
        next.connect_clicked(move |_| {
            let folders: Vec<PathBuf> = checks
                .borrow()
                .iter()
                .filter(|(_, c)| c.is_active())
                .map(|(p, _)| p.clone())
                .collect();
            if folders.is_empty() {
                return;
            }
            nav.push(&target_page(&window, &nav, folders));
        });
    }

    shell("Ordner wählen", &content, Some(next))
}

#[derive(Clone)]
enum TargetChoice {
    Local(PathBuf),
    Webdav {
        url: String,
        user: String,
        password: String,
    },
}

fn target_page(
    window: &gtk::Window,
    nav: &adw::NavigationView,
    folders: Vec<PathBuf>,
) -> adw::NavigationPage {
    let choice: Rc<RefCell<Option<TargetChoice>>> = Rc::new(RefCell::new(None));
    let next = gtk::Button::with_label("Weiter");
    next.add_css_class("suggested-action");
    next.add_css_class("pill");
    next.set_sensitive(false);

    let group = adw::PreferencesGroup::builder()
        .title("Wohin sollen die Dateien gesichert werden?")
        .build();

    let row_local = adw::ActionRow::builder()
        .title("Lokaler Ordner / USB-Laufwerk")
        .subtitle("z. B. externe Platte oder lokales Backup-Verzeichnis")
        .build();
    let radio_local = gtk::CheckButton::new();
    radio_local.set_active(true);
    radio_local.set_valign(gtk::Align::Center);
    row_local.set_activatable_widget(Some(&radio_local));
    row_local.add_suffix(&radio_local);
    group.add(&row_local);

    let row_webdav = adw::ActionRow::builder()
        .title("WebDAV (HTTPS)")
        .subtitle("z. B. Hetzner Storage Box, Nextcloud, ownCloud – Port 443")
        .build();
    let radio_webdav = gtk::CheckButton::new();
    radio_webdav.set_group(Some(&radio_local));
    radio_webdav.set_valign(gtk::Align::Center);
    row_webdav.set_activatable_widget(Some(&radio_webdav));
    row_webdav.add_suffix(&radio_webdav);
    group.add(&row_webdav);

    for (title, sub) in [
        ("SFTP", "noch nicht verfügbar"),
        ("SSH (rsync)", "noch nicht verfügbar"),
        ("FTP / FTPS", "noch nicht verfügbar"),
    ] {
        let row = adw::ActionRow::builder().title(title).subtitle(sub).build();
        row.set_sensitive(false);
        let radio = gtk::CheckButton::new();
        radio.set_sensitive(false);
        radio.set_valign(gtk::Align::Center);
        row.add_suffix(&radio);
        group.add(&row);
    }

    let form = adw::PreferencesGroup::builder()
        .title("Zugangsdaten (WebDAV)")
        .description("Beispiel Hetzner Storage Box: https://u123456.your-storagebox.de")
        .visible(false)
        .build();
    let user_entry = adw::EntryRow::builder().title("Benutzername").build();
    let url_entry = adw::EntryRow::builder().title("Server-URL").build();
    let pw_entry = adw::PasswordEntryRow::builder().title("Passwort").build();
    let test_row = adw::ActionRow::builder().title("Verbindung").build();
    let test_btn = gtk::Button::with_label("Testen");
    test_btn.set_valign(gtk::Align::Center);
    test_row.add_suffix(&test_btn);
    let status_label = gtk::Label::new(Some(""));
    status_label.add_css_class("dim-label");
    status_label.set_xalign(0.0);
    let status_row = adw::ActionRow::builder().title("Status").build();
    status_row.add_suffix(&status_label);
    form.add(&user_entry);
    form.add(&url_entry);
    form.add(&pw_entry);
    form.add(&test_row);
    form.add(&status_row);

    {
        let form = form.clone();
        radio_webdav.connect_toggled(move |c| {
            form.set_visible(c.is_active());
        });
    }

    {
        let window = window.clone();
        let choice = choice.clone();
        let next = next.clone();
        let row_local = row_local.clone();
        radio_local.connect_toggled(move |c| {
            if !c.is_active() {
                return;
            }
            let dialog = gtk::FileDialog::new();
            let choice = choice.clone();
            let next = next.clone();
            let row_local = row_local.clone();
            dialog.select_folder(Some(&window), None::<&gtk::gio::Cancellable>, move |res| {
                if let Ok(file) = res {
                    if let Some(path) = file.path() {
                        let subtitle = path.to_string_lossy().to_string();
                        row_local.set_subtitle(&subtitle);
                        *choice.borrow_mut() = Some(TargetChoice::Local(path));
                        next.set_sensitive(true);
                    }
                }
            });
        });
    }

    {
        let update = {
            let choice = choice.clone();
            let next = next.clone();
            let radio_webdav = radio_webdav.clone();
            let user_entry = user_entry.clone();
            let url_entry = url_entry.clone();
            let pw_entry = pw_entry.clone();
            Rc::new(move || {
                if !radio_webdav.is_active() {
                    return;
                }
                let user = user_entry.text().trim().to_string();
                let url = url_entry.text().trim().to_string();
                let password = pw_entry.text().to_string();
                let valid = !user.is_empty() && !url.is_empty() && !password.is_empty();
                *choice.borrow_mut() = if valid {
                    Some(TargetChoice::Webdav {
                        url,
                        user,
                        password,
                    })
                } else {
                    None
                };
                next.set_sensitive(valid);
            })
        };
        let u = update.clone();
        user_entry.connect_changed(move |_| u());
        let u = update.clone();
        url_entry.connect_changed(move |_| u());
        let u = update.clone();
        pw_entry.connect_changed(move |_| u());
        let u = update.clone();
        let choice = choice.clone();
        let next = next.clone();
        radio_webdav.connect_toggled(move |c| {
            if c.is_active() {
                u();
            } else {
                *choice.borrow_mut() = None;
                next.set_sensitive(false);
            }
        });
    }

    {
        let url_entry = url_entry.clone();
        let user_entry = user_entry.clone();
        let pw_entry = pw_entry.clone();
        let status_label = status_label.clone();
        test_btn.connect_clicked(move |_| {
            let url = url_entry.text().to_string();
            let user = user_entry.text().to_string();
            let password = pw_entry.text().to_string();
            status_label.set_text("Teste …");
            let (tx, rx) = mpsc::channel::<String>();
            std::thread::spawn(move || {
                let msg = match cloudsave_core::WebdavBackend::new(&url, &user, &password) {
                    Ok(b) => match b.test() {
                        Ok(()) => "Verbunden ✓".to_string(),
                        Err(e) => format!("Fehler: {}", e),
                    },
                    Err(e) => format!("Fehler: {}", e),
                };
                let _ = tx.send(msg);
            });
            let status_label = status_label.clone();
            glib::timeout_add_local(Duration::from_millis(150), move || match rx.try_recv() {
                Ok(msg) => {
                    status_label.set_text(&msg);
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
            });
        });
    }

    let content = page_content();
    content.append(&group);
    content.append(&form);

    {
        let nav = nav.clone();
        let window = window.clone();
        let choice = choice.clone();
        next.connect_clicked(move |_| {
            if let Some(choice) = choice.borrow().clone() {
                nav.push(&done_page(&window, &nav, folders.clone(), choice));
            }
        });
    }

    shell("Ziel wählen", &content, Some(next))
}

fn done_page(
    window: &gtk::Window,
    nav: &adw::NavigationView,
    folders: Vec<PathBuf>,
    choice: TargetChoice,
) -> adw::NavigationPage {
    let (target_desc, target) = match &choice {
        TargetChoice::Local(path) => (
            path.display().to_string(),
            Target::Local { path: path.clone() },
        ),
        TargetChoice::Webdav { url, user, .. } => (
            format!("{} (Benutzer {})", url, user),
            Target::Webdav {
                url: url.clone(),
                user: user.clone(),
            },
        ),
    };
    let list = folders
        .iter()
        .map(|f| format!("• {}", f.display()))
        .collect::<Vec<_>>()
        .join("\n");
    let summary = format!(
        "{} Ordner sichern nach:\n{}\n\n{}\nDer erste Sync startet sofort.",
        folders.len(),
        target_desc,
        list
    );

    let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
    content.set_vexpand(true);
    content.set_valign(gtk::Align::Center);
    content.set_halign(gtk::Align::Center);
    content.set_width_request(480);
    content.set_margin_top(48);
    content.set_margin_bottom(48);

    let title = gtk::Label::new(Some("Alles bereit"));
    title.add_css_class("title-2");
    title.set_halign(gtk::Align::Center);

    let summary_label = gtk::Label::new(Some(&summary));
    summary_label.add_css_class("dim-label");
    summary_label.set_wrap(true);
    summary_label.set_xalign(0.0);

    let start = gtk::Button::with_label("Sync einrichten & starten");
    start.add_css_class("suggested-action");
    start.add_css_class("pill");
    start.set_halign(gtk::Align::Center);

    let autostart_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    autostart_row.add_css_class("folder-row");
    let autostart_label = gtk::Label::new(Some("Beim Systemstart automatisch starten"));
    autostart_label.add_css_class("row-label");
    autostart_label.set_halign(gtk::Align::Start);
    autostart_label.set_hexpand(true);
    let autostart_switch = gtk::Switch::new();
    autostart_switch.set_active(true);
    autostart_switch.set_valign(gtk::Align::Center);
    autostart_row.append(&autostart_label);
    autostart_row.append(&autostart_switch);

    {
        let nav = nav.clone();
        let window = window.clone();
        let autostart_switch = autostart_switch.clone();
        start.connect_clicked(move |_| {
            if let TargetChoice::Webdav {
                url,
                user,
                password,
            } = &choice
            {
                let _ = cloudsave_core::save_password(url, user, password);
            }
            let mut settings = Settings::default();
            settings.autostart = autostart_switch.is_active();
            let autostart = settings.autostart;
            let config = Config {
                folders: folders.clone(),
                target: target.clone(),
                settings,
            };
            if config.save(&Config::default_path()).is_err() {
                return;
            }
            if autostart {
                let _ = cloudsave_core::set_autostart(true);
            }
            let page = dashboard::build(&window, &config, true);
            nav.replace(&[adw::NavigationPage::new(&page, "Übersicht")]);
        });
    }

    content.append(&title);
    content.append(&summary_label);
    content.append(&autostart_row);
    content.append(&start);
    shell("Fertig", &content, None)
}
