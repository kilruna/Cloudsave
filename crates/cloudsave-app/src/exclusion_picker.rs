use adw::prelude::*;
use cloudsave_core::{Config, Error, Result};
use gtk::glib;
use gtk4 as gtk;
use libadwaita as adw;
use std::{
    cell::{Cell, RefCell},
    path::{Component, Path, PathBuf},
    rc::Rc,
};

const PRESETS: &[(&str, &[&str])] = &[
    ("Cache-Ordner", &[".cache", "cache", "Cache"]),
    ("Temporäre Dateien", &["*.tmp", "*.temp"]),
    ("Temporäre Ordner", &[".tmp", "tmp"]),
    ("System-Vorschaudateien", &[".DS_Store", "Thumbs.db"]),
    ("Entwicklungs-Caches", &["node_modules", "__pycache__"]),
];

type PresetCheck = (&'static [&'static str], glib::WeakRef<gtk::CheckButton>);
struct Picker {
    config: Rc<RefCell<Config>>,
    menu: glib::WeakRef<gtk::MenuButton>,
    custom: glib::WeakRef<gtk::Box>,
    status: glib::WeakRef<gtk::Label>,
    checks: RefCell<Vec<PresetCheck>>,
    updating: Cell<bool>,
}

fn relative_path(value: &str, folders: &[PathBuf]) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.contains(['\n', '\r', '\\']) {
        return Err(Error(
            "Bitte ein einzelnes Muster mit / als Pfadtrenner eingeben.".into(),
        ));
    }
    let path = Path::new(value);
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(Error(
            "Pfade ohne .. angeben, relativ zu einem gesicherten Ordner.".into(),
        ));
    }
    let pattern = if path.is_absolute() {
        let mut matching = folders
            .iter()
            .filter_map(|folder| path.strip_prefix(folder).ok());
        let relative = matching.next()
            .ok_or_else(|| Error("Der Pfad liegt außerhalb der gesicherten Ordner. Bitte ein relatives Muster eingeben.".into()))?;
        if matching.any(|other| other != relative) {
            return Err(Error("Der Pfad gehört zu mehreren verschachtelten Quellordnern. Bitte für jede Quelle ein relatives Muster hinzufügen.".into()));
        }
        if relative.as_os_str().is_empty() {
            return Err(Error(
                "Einen ganzen Quellordner bitte unter „Gesicherte Ordner“ abwählen.".into(),
            ));
        }
        relative
            .to_str()
            .ok_or_else(|| Error("Dieser Ordnername kann nicht dargestellt werden.".into()))?
            .to_string()
    } else {
        value.strip_prefix("./").unwrap_or(value).to_string()
    };
    let pattern: PathBuf = Path::new(&pattern)
        .components()
        .filter(|part| !matches!(part, Component::CurDir))
        .collect();
    if pattern.as_os_str().is_empty() {
        return Err(Error(
            "Einen ganzen Quellordner bitte unter „Gesicherte Ordner“ abwählen.".into(),
        ));
    }
    Ok(pattern
        .to_str()
        .ok_or_else(|| Error("Dieser Ordnername kann nicht dargestellt werden.".into()))?
        .to_string())
}

fn relative_input(value: &str, folders: &[PathBuf]) -> Result<String> {
    let pattern = relative_path(value, folders)?;
    cloudsave_core::validate_exclusion_patterns(std::slice::from_ref(&pattern))?;
    Ok(pattern)
}

fn folder_pattern(path: &Path, folders: &[PathBuf]) -> Result<String> {
    let input = path
        .to_str()
        .ok_or_else(|| Error("Dieser Ordnername kann nicht dargestellt werden.".into()))?;
    let relative = relative_path(input, folders)?;
    let mut escaped = String::new();
    for ch in relative.chars() {
        match ch {
            '*' => escaped.push_str("[*]"),
            '?' => escaped.push_str("[?]"),
            '[' => escaped.push_str("[[]"),
            ']' => escaped.push_str("[]]"),
            '{' => escaped.push_str("[{]"),
            '}' => escaped.push_str("[}]"),
            _ => escaped.push(ch),
        }
    }
    // Anchor the selected directory to the source root, including its subtree.
    escaped.push_str("/**");
    cloudsave_core::validate_exclusion_patterns(std::slice::from_ref(&escaped))?;
    Ok(escaped)
}

impl Picker {
    fn report(&self, message: &str, error: bool) {
        if let Some(status) = self.status.upgrade() {
            if error {
                status.add_css_class("error");
            } else {
                status.remove_css_class("error");
            }
            status.set_text(message);
        }
    }

    fn save(self: &Rc<Self>, patterns: Vec<String>) -> Result<()> {
        cloudsave_core::validate_exclusion_patterns(&patterns)?;
        let mut next = self.config.borrow().clone();
        next.settings.exclude_patterns = patterns;
        next.save(&Config::default_path())?;
        *self.config.borrow_mut() = next;
        self.refresh();
        self.report("Auswahl gespeichert", false);
        Ok(())
    }

    fn refresh(self: &Rc<Self>) {
        let patterns = self.config.borrow().settings.exclude_patterns.clone();
        if let Some(menu) = self.menu.upgrade() {
            menu.set_label(&if patterns.is_empty() {
                "Ausschlüsse auswählen …".into()
            } else {
                format!("Ausschlüsse: {} Muster aktiv", patterns.len())
            });
        }
        self.updating.set(true);
        for (group, weak) in self.checks.borrow().iter() {
            if let Some(check) = weak.upgrade() {
                let count = group
                    .iter()
                    .filter(|rule| patterns.iter().any(|p| p == **rule))
                    .count();
                check.set_active(count == group.len());
                check.set_inconsistent(count > 0 && count < group.len());
            }
        }
        self.updating.set(false);
        if let Some(list) = self.custom.upgrade() {
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            for pattern in patterns.iter().filter(|pattern| {
                !PRESETS
                    .iter()
                    .any(|(_, rules)| rules.contains(&pattern.as_str()))
            }) {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let label = gtk::Label::new(Some(pattern));
                label.set_xalign(0.0);
                label.set_hexpand(true);
                label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
                label.set_width_chars(12);
                label.set_tooltip_text(Some(pattern));
                row.append(&label);
                let remove = gtk::Button::with_label("Entfernen");
                remove.update_property(&[gtk::accessible::Property::Label(&format!(
                    "Ausschluss {pattern} entfernen"
                ))]);
                let picker = self.clone();
                let pattern = pattern.clone();
                remove.connect_clicked(move |_| {
                    let mut patterns = picker.config.borrow().settings.exclude_patterns.clone();
                    patterns.retain(|p| p != &pattern);
                    if let Err(error) = picker.save(patterns) {
                        picker.report(&format!("Nicht gespeichert: {error}"), true);
                    }
                });
                row.append(&remove);
                list.append(&row);
            }
        }
    }

    fn add(self: &Rc<Self>, pattern: String) -> Result<()> {
        let mut patterns = self.config.borrow().settings.exclude_patterns.clone();
        if !patterns.contains(&pattern) {
            patterns.push(pattern);
        }
        self.save(patterns)
    }
}

pub fn build(window: &gtk::Window, config: Rc<RefCell<Config>>) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.add_css_class("folder-row");
    let title = gtk::Label::new(Some("Dateien und Ordner ausschließen"));
    title.add_css_class("row-label");
    title.set_xalign(0.0);
    root.append(&title);
    let menu = gtk::MenuButton::new();
    menu.add_css_class("exclusion-select");
    root.append(&menu);
    let hint = gtk::Label::new(Some("Standardregeln anklicken oder eigene hinzufügen. Regeln gelten für alle Quellordner; vorhandene Sicherungen bleiben erhalten."));
    hint.add_css_class("dim-label");
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    root.append(&hint);
    let status = gtk::Label::new(None);
    status.set_wrap(true);
    status.set_xalign(0.0);
    root.append(&status);
    let popover = gtk::Popover::new();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    content.set_halign(gtk::Align::Fill);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let options = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let custom = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let picker = Rc::new(Picker {
        config,
        menu: menu.downgrade(),
        custom: custom.downgrade(),
        status: status.downgrade(),
        checks: RefCell::new(Vec::new()),
        updating: Cell::new(false),
    });
    for &(name, rules) in PRESETS {
        let check = gtk::CheckButton::new();
        let labels = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let title = gtk::Label::new(Some(name));
        title.set_xalign(0.0);
        labels.append(&title);
        let detail = gtk::Label::new(Some(&rules.join(", ")));
        detail.add_css_class("dim-label");
        detail.set_xalign(0.0);
        detail.set_wrap(true);
        detail.set_max_width_chars(32);
        labels.append(&detail);
        check.set_child(Some(&labels));
        check.update_property(&[gtk::accessible::Property::Label(name)]);
        picker.checks.borrow_mut().push((rules, check.downgrade()));
        let picker = picker.clone();
        check.connect_toggled(move |check| {
            if picker.updating.get() {
                return;
            }
            let mut patterns = picker.config.borrow().settings.exclude_patterns.clone();
            if check.is_active() {
                for rule in rules {
                    if !patterns.iter().any(|p| p == rule) {
                        patterns.push((*rule).into());
                    }
                }
            } else {
                patterns.retain(|p| !rules.contains(&p.as_str()));
            }
            if let Err(error) = picker.save(patterns) {
                picker.report(&format!("Nicht gespeichert: {error}"), true);
                picker.refresh();
            }
        });
        options.append(&check);
    }
    options.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    options.append(&custom);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_propagate_natural_height(true);
    scroll.set_max_content_height(320);
    scroll.set_child(Some(&options));
    content.append(&scroll);
    let manual = gtk::Button::with_label("Eigene Regel hinzufügen …");
    {
        let picker = picker.clone();
        let window = window.downgrade();
        let popover = popover.downgrade();
        manual.connect_clicked(move |_| {
            let Some(window) = window.upgrade() else {
                return;
            };
            if let Some(popover) = popover.upgrade() {
                popover.popdown();
            }
            manual_dialog(&window, &picker);
        });
    }
    content.append(&manual);
    let folder = gtk::Button::with_label("Ordner auswählen …");
    {
        let picker = picker.clone();
        let window = window.downgrade();
        let popover = popover.downgrade();
        folder.connect_clicked(move |_| {
            let Some(window) = window.upgrade() else {
                return;
            };
            if let Some(popover) = popover.upgrade() {
                popover.popdown();
            }
            let picker = picker.clone();
            gtk::FileDialog::new().select_folder(
                Some(&window),
                None::<&gtk::gio::Cancellable>,
                move |result| {
                    if let Ok(file) = result {
                        let result = file
                            .path()
                            .ok_or_else(|| Error("Bitte einen lokalen Ordner wählen.".into()))
                            .and_then(|path| folder_pattern(&path, &picker.config.borrow().folders))
                            .and_then(|pattern| picker.add(pattern));
                        if let Err(error) = result {
                            picker.report(&format!("Nicht gespeichert: {error}"), true);
                        }
                    }
                },
            );
        });
    }
    content.append(&folder);
    popover.set_child(Some(&content));
    menu.set_popover(Some(&popover));
    picker.refresh();
    root
}

fn manual_dialog(window: &gtk::Window, picker: &Rc<Picker>) {
    let dialog = adw::Dialog::builder()
        .title("Eigene Ausschlussregel")
        .content_width(440)
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(24);
    content.set_margin_bottom(24);
    content.set_margin_start(24);
    content.set_margin_end(24);
    let hint = gtk::Label::new(Some(
        "Dateimuster oder Pfad relativ zum Quellordner, z. B. *.bak oder Projekte/Cache/**.",
    ));
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    content.append(&hint);
    let entry = gtk::Entry::new();
    entry.set_placeholder_text(Some("*.bak oder Projekte/Cache/**"));
    entry.update_property(&[gtk::accessible::Property::Label("Ausschlussmuster")]);
    content.append(&entry);
    let error = gtk::Label::new(None);
    error.add_css_class("error");
    error.set_wrap(true);
    error.set_xalign(0.0);
    content.append(&error);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Abbrechen");
    let close = dialog.downgrade();
    cancel.connect_clicked(move |_| {
        if let Some(dialog) = close.upgrade() {
            dialog.close();
        }
    });
    actions.append(&cancel);
    let add = gtk::Button::with_label("Regel hinzufügen");
    add.add_css_class("suggested-action");
    add.set_sensitive(false);
    actions.append(&add);
    content.append(&actions);
    {
        let add = add.clone();
        let error = error.clone();
        let picker = picker.clone();
        entry.connect_changed(move |entry| {
            let result = relative_input(&entry.text(), &picker.config.borrow().folders);
            add.set_sensitive(result.is_ok());
            error.set_text(
                &result
                    .err()
                    .map(|e| e.to_string())
                    .filter(|_| !entry.text().is_empty())
                    .unwrap_or_default(),
            );
        });
    }
    let activate = add.downgrade();
    entry.connect_activate(move |_| {
        if let Some(add) = activate.upgrade() {
            if add.is_sensitive() {
                add.emit_clicked();
            }
        }
    });
    let picker = picker.clone();
    let close = dialog.downgrade();
    add.connect_clicked(move |_| {
        let folders = picker.config.borrow().folders.clone();
        let result =
            relative_input(&entry.text(), &folders).and_then(|pattern| picker.add(pattern));
        match result {
            Ok(()) => {
                if let Some(dialog) = close.upgrade() {
                    dialog.close();
                }
            }
            Err(message) => error.set_text(&format!("Nicht gespeichert: {message}")),
        }
    });
    dialog.set_child(Some(&content));
    dialog.present(Some(window));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_paths_are_relative_and_outside_or_parent_paths_are_rejected() {
        let roots = vec![PathBuf::from("/home/test/Documents")];
        assert_eq!(
            relative_input(" /home/test/Documents/cache/** ", &roots).unwrap(),
            "cache/**"
        );
        assert_eq!(
            relative_input("./nested/*.bak", &roots).unwrap(),
            "nested/*.bak"
        );
        for input in [
            "",
            ".",
            "./",
            "[",
            "/home/test/Documents",
            "/home/test/Private",
            "../private/**",
            "x/../../private",
            "*.bak\n*.tmp",
        ] {
            assert!(relative_input(input, &roots).is_err(), "{input}");
        }
        assert_eq!(
            folder_pattern(Path::new("/home/test/Documents/Cache [old]*"), &roots).unwrap(),
            "Cache [[]old[]][*]/**"
        );
        assert!(folder_pattern(
            Path::new("/home/test/Documents/cache"),
            &[
                PathBuf::from("/home/test"),
                PathBuf::from("/home/test/Documents")
            ]
        )
        .is_err());
    }
}
