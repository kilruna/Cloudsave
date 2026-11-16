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
    // The application titlebar owns window controls; retain page title/back navigation.
    header.set_show_start_title_buttons(false);
    header.set_show_end_title_buttons(false);
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
    let existing = checks
        .borrow()
        .iter()
        .find(|(existing, _)| existing == &path)
        .map(|(_, check)| check.clone());
    if let Some(check) = existing {
        check.set_active(true);
        next.set_sensitive(true);
        return;
    }
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

fn home_folders() -> std::io::Result<Vec<PathBuf>> {
    let home = std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| std::io::Error::other("Home-Verzeichnis nicht gefunden"))?;
    let mut folders = Vec::new();
    // Only inspect the immediate children; never recursively scan their contents here.
    for entry in std::fs::read_dir(home)? {
        let entry = entry?;
        if !entry.file_name().to_string_lossy().starts_with('.') && entry.path().is_dir() {
            folders.push(entry.path());
        }
    }
    folders.sort_by_cached_key(|path| {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
    });
    Ok(folders)
}

fn folders_page(window: &gtk::Window, nav: &adw::NavigationView) -> adw::NavigationPage {
    let checks: Rc<RefCell<Vec<(PathBuf, gtk::CheckButton)>>> = Rc::new(RefCell::new(Vec::new()));
    let next = gtk::Button::with_label("Weiter");
    next.add_css_class("suggested-action");
    next.add_css_class("pill");

    let group = adw::PreferencesGroup::builder()
        .title("Welche Ordner sollen gesichert werden?")
        .description(
            "Vorhandene Ordner in deinem Home-Verzeichnis. Wähle aus, was gesichert werden soll.",
        )
        .build();

    match home_folders() {
        Ok(folders) if folders.is_empty() => group.set_description(Some(
            "Keine sichtbaren Ordner im Home-Verzeichnis gefunden. Du kannst einen Ordner hinzufügen.",
        )),
        Ok(folders) => {
            for path in folders {
                let label = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                add_folder_row(&group, &checks, &next, path, &label, false);
            }
        }
        Err(error) => group.set_description(Some(&format!(
            "Home-Verzeichnis konnte nicht gelesen werden: {error}. Du kannst einen Ordner hinzufügen.",
        ))),
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

fn target_picker(
    window: &gtk::Window,
    button_label: &str,
) -> (gtk::Box, Rc<RefCell<Option<TargetChoice>>>, gtk::Button) {
    let choice: Rc<RefCell<Option<TargetChoice>>> = Rc::new(RefCell::new(None));
    let next = gtk::Button::with_label(button_label);
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
    let select_local = gtk::Button::with_label("Ordner auswählen …");
    select_local.set_valign(gtk::Align::Center);
    row_local.add_suffix(&select_local);
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
    let url_error = gtk::Label::new(None);
    url_error.set_wrap(true);
    url_error.set_xalign(0.0);
    url_error.add_css_class("error");
    form.add(&url_error);

    {
        let form = form.clone();
        radio_webdav.connect_toggled(move |c| {
            form.set_visible(c.is_active());
        });
    }

    let local_path: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
    {
        let local_path = local_path.clone();
        let choice = choice.clone();
        let next = next.clone();
        radio_local.connect_toggled(move |c| {
            if c.is_active() {
                *choice.borrow_mut() = local_path.borrow().clone().map(TargetChoice::Local);
                next.set_sensitive(choice.borrow().is_some());
            }
        });
    }
    {
        let window = window.clone();
        let choice = choice.clone();
        let next = next.clone();
        let row_local = row_local.clone();
        let radio_local = radio_local.clone();
        select_local.connect_clicked(move |button| {
            radio_local.set_active(true);
            button.set_sensitive(false);
            let dialog = gtk::FileDialog::new();
            let choice = choice.clone();
            let next = next.clone();
            let row_local = row_local.clone();
            let local_path = local_path.clone();
            let radio_local = radio_local.clone();
            let button = button.clone();
            dialog.select_folder(Some(&window), None::<&gtk::gio::Cancellable>, move |res| {
                button.set_sensitive(true);
                if let Ok(file) = res {
                    if let Some(path) = file.path() {
                        let subtitle = path.to_string_lossy().to_string();
                        row_local.set_subtitle(&subtitle);
                        *local_path.borrow_mut() = Some(path.clone());
                        if radio_local.is_active() {
                            *choice.borrow_mut() = Some(TargetChoice::Local(path));
                            next.set_sensitive(true);
                        }
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
            let url_error = url_error.clone();
            let test_btn = test_btn.clone();
            Rc::new(move || {
                if !radio_webdav.is_active() {
                    return;
                }
                let user = user_entry.text().trim().to_string();
                let url = url_entry.text().trim().to_string();
                let password = pw_entry.text().to_string();
                let validation = cloudsave_core::validate_webdav_url(&url);
                url_error.set_text(
                    validation
                        .as_ref()
                        .err()
                        .map(|e| e.0.as_str())
                        .unwrap_or(""),
                );
                let valid = !user.is_empty() && validation.is_ok() && !password.is_empty();
                test_btn.set_sensitive(valid);
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
        radio_webdav.connect_toggled(move |c| {
            if c.is_active() {
                u();
            }
        });
    }

    {
        let url_entry = url_entry.clone();
        let user_entry = user_entry.clone();
        let pw_entry = pw_entry.clone();
        let status_label = status_label.clone();
        test_btn.connect_clicked(move |_| {
            let url = url_entry.text().trim().to_string();
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

    (content, choice, next)
}

fn target_page(
    window: &gtk::Window,
    nav: &adw::NavigationView,
    folders: Vec<PathBuf>,
) -> adw::NavigationPage {
    let (content, choice, next) = target_picker(window, "Weiter");
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

pub fn add_target_page(
    window: &gtk::Window,
    nav: &adw::NavigationView,
    config: Rc<RefCell<Config>>,
) -> adw::NavigationPage {
    let (content, choice, add) = target_picker(window, "Hinzufügen");
    {
        let nav = nav.clone();
        let window = window.clone();
        let choice = choice.clone();
        let config = config.clone();
        add.connect_clicked(move |_| {
            if let Some(choice) = choice.borrow().clone() {
                let target = match &choice {
                    TargetChoice::Local(path) => Target::Local { path: path.clone() },
                    TargetChoice::Webdav {
                        url,
                        user,
                        password,
                    } => {
                        let _ = cloudsave_core::save_password(url, user, password);
                        Target::Webdav {
                            url: url.clone(),
                            user: user.clone(),
                        }
                    }
                };
                {
                    let mut cfg = config.borrow_mut();
                    if !cfg.targets.contains(&target) {
                        cfg.targets.push(target);
                    }
                    let _ = cfg.save(&Config::default_path());
                }
                let cfg = config.borrow().clone();
                let page = dashboard::build(&window, &nav, &cfg, false);
                nav.replace(&[adw::NavigationPage::new(&page, "Übersicht")]);
            }
        });
    }
    shell("Endpunkt hinzufügen", &content, Some(add))
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
            let autostart = autostart_switch.is_active();
            let settings = Settings {
                autostart,
                ..Settings::default()
            };
            let config = Config {
                folders: folders.clone(),
                targets: vec![target.clone()],
                settings,
            };
            if config.save(&Config::default_path()).is_err() {
                return;
            }
            if autostart {
                let _ = cloudsave_core::set_autostart(true);
            }
            let page = dashboard::build(&window, &nav, &config, true);
            nav.replace(&[adw::NavigationPage::new(&page, "Übersicht")]);
        });
    }

    content.append(&title);
    content.append(&summary_label);
    content.append(&autostart_row);
    content.append(&start);
    shell("Fertig", &content, None)
}
