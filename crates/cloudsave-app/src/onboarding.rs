use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;
use libadwaita as adw;

use cloudsave_core::{Config, Settings, Target};

use crate::dashboard;

pub fn build(window: &adw::ApplicationWindow, nav: &adw::NavigationView) {
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

fn welcome_page(window: &adw::ApplicationWindow, nav: &adw::NavigationView) -> adw::NavigationPage {
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

fn folders_page(window: &adw::ApplicationWindow, nav: &adw::NavigationView) -> adw::NavigationPage {
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
        if path.is_dir() {
            add_folder_row(&group, &checks, &next, path, label, active);
        }
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

fn target_page(
    window: &adw::ApplicationWindow,
    nav: &adw::NavigationView,
    folders: Vec<PathBuf>,
) -> adw::NavigationPage {
    let target: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
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

    let row_hetzner = adw::ActionRow::builder()
        .title("Hetzner Storage Box")
        .subtitle("WebDAV-Backend folgt in M2 – noch nicht verfügbar")
        .build();
    row_hetzner.set_sensitive(false);
    let radio_hetzner = gtk::CheckButton::new();
    radio_hetzner.set_group(Some(&radio_local));
    radio_hetzner.set_sensitive(false);
    radio_hetzner.set_valign(gtk::Align::Center);
    row_hetzner.add_suffix(&radio_hetzner);
    group.add(&row_hetzner);

    {
        let window = window.clone();
        let target = target.clone();
        let next = next.clone();
        let row_local = row_local.clone();
        radio_local.connect_toggled(move |c| {
            if !c.is_active() {
                return;
            }
            let dialog = gtk::FileDialog::new();
            let target = target.clone();
            let next = next.clone();
            let row_local = row_local.clone();
            dialog.select_folder(Some(&window), None::<&gtk::gio::Cancellable>, move |res| {
                if let Ok(file) = res {
                    if let Some(path) = file.path() {
                        let subtitle = path.to_string_lossy().to_string();
                        row_local.set_subtitle(&subtitle);
                        *target.borrow_mut() = Some(path);
                        next.set_sensitive(true);
                    }
                }
            });
        });
    }

    let content = page_content();
    content.append(&group);

    {
        let nav = nav.clone();
        let window = window.clone();
        let target = target.clone();
        next.connect_clicked(move |_| {
            if let Some(path) = target.borrow().clone() {
                nav.push(&done_page(&window, &nav, folders.clone(), path));
            }
        });
    }

    shell("Ziel wählen", &content, Some(next))
}

fn done_page(
    window: &adw::ApplicationWindow,
    nav: &adw::NavigationView,
    folders: Vec<PathBuf>,
    target: PathBuf,
) -> adw::NavigationPage {
    let list = folders
        .iter()
        .map(|f| format!("• {}", f.display()))
        .collect::<Vec<_>>()
        .join("\n");
    let summary = format!(
        "{} Ordner sichern nach:\n{}\n\n{}\nDer erste Sync startet sofort.",
        folders.len(),
        target.display(),
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
    {
        let nav = nav.clone();
        let window = window.clone();
        start.connect_clicked(move |_| {
            let config = Config {
                folders: folders.clone(),
                target: Target::Local {
                    path: target.clone(),
                },
                settings: Settings::default(),
            };
            if config.save(&Config::default_path()).is_err() {
                return;
            }
            let page = dashboard::build(&window, &config, true);
            nav.replace(&[adw::NavigationPage::new(&page, "Übersicht")]);
        });
    }

    content.append(&title);
    content.append(&summary_label);
    content.append(&start);
    shell("Fertig", &content, None)
}
