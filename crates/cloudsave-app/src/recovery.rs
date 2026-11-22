//! Guided whole-folder recovery before the first sync (also after a restart).
use adw::prelude::*;
use cloudsave_core::{BackupFolder, BackupInventory, Config, RestoreLocation, SyncMode, Target};
use gtk::{gio, glib};
use gtk4 as gtk;
use libadwaita as adw;
use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::mpsc, time::Duration};

type ChosenPath = (PathBuf, PathBuf); // approved root and a validated relative path
struct Folder {
    backup: BackupFolder,
    check: glib::WeakRef<gtk::CheckButton>,
    label: glib::WeakRef<gtk::Label>,
    original: RefCell<Option<ChosenPath>>,
}
struct Selection {
    folders: Vec<Folder>,
    original_mode: glib::WeakRef<gtk::CheckButton>,
    separate: RefCell<Option<PathBuf>>,
    start: glib::WeakRef<gtk::Button>,
    status: glib::WeakRef<gtk::Label>,
}
impl Selection {
    fn locations(&self) -> Option<Vec<(String, ChosenPath)>> {
        let originals = self.original_mode.upgrade()?.is_active();
        let mut locations = Vec::new();
        for folder in &self.folders {
            if !folder.check.upgrade()?.is_active() {
                continue;
            }
            let chosen = if originals {
                folder.original.borrow().clone()
            } else {
                self.separate
                    .borrow()
                    .as_ref()
                    .map(|root| (root.clone(), PathBuf::from(&folder.backup.namespace)))
            }?;
            locations.push((folder.backup.namespace.clone(), chosen));
        }
        (!locations.is_empty()).then_some(locations)
    }
    fn refresh(&self) {
        let originals = self
            .original_mode
            .upgrade()
            .is_some_and(|mode| mode.is_active());
        for folder in &self.folders {
            let chosen = if originals {
                folder.original.borrow().clone()
            } else {
                self.separate
                    .borrow()
                    .as_ref()
                    .map(|root| (root.clone(), PathBuf::from(&folder.backup.namespace)))
            };
            if let Some(label) = folder.label.upgrade() {
                label.set_text(
                    &chosen
                        .map(|(base, rel)| base.join(rel).display().to_string())
                        .unwrap_or_else(|| "Bitte einen Zielordner wählen".into()),
                );
            }
        }
        if let Some(start) = self.start.upgrade() {
            start.set_sensitive(self.locations().is_some());
        }
    }
    fn error(&self, message: &str) {
        if let Some(status) = self.status.upgrade() {
            status.set_text(message);
        }
    }
}
fn task<T: Send + 'static>(
    work: impl FnOnce() -> cloudsave_core::Result<T> + Send + 'static,
    done: impl Fn(cloudsave_core::Result<T>) + 'static,
) {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    glib::timeout_add_local(Duration::from_millis(100), move || match rx.try_recv() {
        Ok(result) => {
            done(result);
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(mpsc::TryRecvError::Disconnected) => {
            done(Err(cloudsave_core::Error(
                "Auftrag abgebrochen. Bitte erneut versuchen.".into(),
            )));
            glib::ControlFlow::Break
        }
    });
}

pub fn resume(window: &gtk::Window, nav: &adw::NavigationView, config: &Config) {
    let Some(target) = config.settings.pending_recovery.clone() else {
        return;
    };
    let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
    content.set_margin_top(24);
    content.set_margin_start(24);
    content.set_margin_end(24);
    let status = gtk::Label::new(Some(
        "Vorhandene Sicherung wird geprüft. Synchronisierung bleibt pausiert …",
    ));
    status.set_wrap(true);
    content.append(&status);
    let retry = gtk::Button::with_label("Sicherung erneut prüfen");
    content.append(&retry);
    nav.push(&crate::onboarding::shell(
        "Wiederherstellung fortsetzen",
        &content,
        None,
    ));
    let window = window.clone();
    let nav = nav.clone();
    let config = config.clone();
    retry.connect_clicked(move |button| {
        button.set_sensitive(false);
        let target_work = target.clone();
        let target = target.clone();
        let window = window.clone();
        let nav = nav.clone();
        let config = config.clone();
        let status = status.clone();
        let button = button.downgrade();
        task(
            move || cloudsave_core::discover_backup(&target_work),
            move |result| match result {
                Ok(inventory) => nav.replace(&[page(&window, &nav, &config, &target, inventory)]),
                Err(error) => {
                    status.set_text(&format!(
                        "Sicherung nicht erreichbar: {error}. Automatik bleibt ausgeschaltet."
                    ));
                    if let Some(button) = button.upgrade() {
                        button.set_sensitive(true);
                    }
                }
            },
        );
    });
    retry.emit_clicked();
}

pub fn page(
    window: &gtk::Window,
    nav: &adw::NavigationView,
    config: &Config,
    target: &Target,
    inventory: BackupInventory,
) -> adw::NavigationPage {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 14);
    content.set_margin_top(20);
    content.set_margin_bottom(24);
    content.set_margin_start(24);
    content.set_margin_end(24);
    let title = gtk::Label::new(Some(if inventory.folders.is_empty() {
        "Keine Sicherung gefunden"
    } else {
        "Bestehende Sicherung gefunden"
    }));
    title.add_css_class("title-1");
    title.set_xalign(0.0);
    content.append(&title);
    let hint = gtk::Label::new(Some(&format!("{} Dateien · {} Ordner. Möchtest du deinen bisherigen PC wiederherstellen oder die Sicherung in einen separaten Ordner laden? Vorhandene Dateien bleiben erhalten. Bis zum Abschluss wird nichts hochgeladen oder gespiegelt.", inventory.files.len(), inventory.folders.len())));
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    content.append(&hint);
    let original = gtk::CheckButton::with_label("An die bisherigen Orte wiederherstellen");
    original.set_active(true);
    content.append(&original);
    let separate = gtk::CheckButton::with_label("In einen separaten Ordner wiederherstellen");
    separate.set_group(Some(&original));
    content.append(&separate);
    let mode_hint = gtk::Label::new(Some(if inventory.has_catalog {
        "Die gespeicherte Ordnerzuordnung wird vorgeschlagen. Bitte die Zielpfade prüfen."
    } else {
        "Diese ältere Sicherung enthält keine gespeicherten Zielpfade. Gleichnamige ausgewählte Ordner oder Ordner unter deinem Home werden vorgeschlagen. Bitte die Zuordnung prüfen."
    }));
    mode_hint.set_wrap(true);
    mode_hint.set_xalign(0.0);
    mode_hint.add_css_class("supporting-text");
    content.append(&mode_hint);
    let choose = gtk::Button::with_label("Separaten Ordner wählen …");
    choose.set_visible(false);
    content.append(&choose);
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let needs_grant = std::env::var_os("FLATPAK_ID").is_some();
    let grant = gtk::Button::with_label("Home-Ordner für Schreibzugriff bestätigen …");
    grant.set_visible(needs_grant);
    content.append(&grant);
    {
        let choose = choose.downgrade();
        let grant = grant.downgrade();
        original.connect_toggled(move |mode| {
            if let Some(choose) = choose.upgrade() {
                choose.set_visible(!mode.is_active());
            }
            if let Some(grant) = grant.upgrade() {
                grant.set_visible(needs_grant && mode.is_active());
            }
        });
    }
    let mut rows = Vec::new();
    let mut pickers = Vec::new();
    for folder in &inventory.folders {
        let row = gtk::Box::new(gtk::Orientation::Vertical, 5);
        row.add_css_class("card");
        row.add_css_class("recovery-folder");
        row.set_margin_top(4);
        let check = gtk::CheckButton::with_label(if folder.namespace.is_empty() {
            "Dateien im Sicherungs-Stammverzeichnis"
        } else {
            &folder.namespace
        });
        check.set_active(true);
        row.append(&check);
        let label = gtk::Label::new(None);
        label.set_wrap(true);
        label.set_xalign(0.0);
        label.set_selectable(true);
        row.append(&label);
        let pick = gtk::Button::with_label("Anderen Zielordner wählen …");
        row.append(&pick);
        let visible_pick = pick.downgrade();
        original.connect_toggled(move |mode| {
            if let Some(pick) = visible_pick.upgrade() {
                pick.set_visible(mode.is_active());
            }
        });
        let candidate = if needs_grant {
            None
        } else {
            suggestion(folder, &inventory, config, home.as_ref())
        };
        rows.push(Folder {
            backup: folder.clone(),
            check: check.downgrade(),
            label: label.downgrade(),
            original: RefCell::new(candidate),
        });
        pickers.push((check, pick));
        content.append(&row);
    }
    let status = gtk::Label::new(None);
    status.set_wrap(true);
    status.set_selectable(true);
    status.set_xalign(0.0);
    content.append(&status);
    let start = gtk::Button::with_label("Ausgewählte Ordner wiederherstellen");
    start.add_css_class("suggested-action");
    start.add_css_class("primary-action");
    content.append(&start);
    let state = Rc::new(Selection {
        folders: rows,
        original_mode: original.downgrade(),
        separate: RefCell::new(None),
        start: start.downgrade(),
        status: status.downgrade(),
    });
    for mode in [&original, &separate] {
        let state = state.clone();
        mode.connect_toggled(move |_| state.refresh());
    }
    for (index, (check, pick)) in pickers.into_iter().enumerate() {
        let refresh = state.clone();
        check.connect_toggled(move |_| refresh.refresh());
        let state = state.clone();
        let window = window.downgrade();
        let original = original.downgrade();
        pick.connect_clicked(move |_| {
            let Some(window) = window.upgrade() else {
                return;
            };
            let state = state.clone();
            let original = original.clone();
            gtk::FileDialog::new().select_folder(
                Some(&window),
                None::<&gio::Cancellable>,
                move |result| {
                    if let Ok(file) = result {
                        if let Some(path) = file.path() {
                            *state.folders[index].original.borrow_mut() =
                                Some((path, PathBuf::new()));
                            if let Some(mode) = original.upgrade() {
                                mode.set_active(true);
                            }
                            state.refresh();
                        }
                    }
                },
            );
        });
    }
    {
        let window = window.downgrade();
        let state = state.clone();
        let separate = separate.downgrade();
        choose.connect_clicked(move |_| {
            let Some(window) = window.upgrade() else {
                return;
            };
            let state = state.clone();
            let separate = separate.clone();
            gtk::FileDialog::new().select_folder(
                Some(&window),
                None::<&gio::Cancellable>,
                move |result| {
                    if let Ok(file) = result {
                        if let Some(path) = file.path() {
                            *state.separate.borrow_mut() = Some(path);
                            if let Some(mode) = separate.upgrade() {
                                mode.set_active(true);
                            }
                            state.refresh();
                        }
                    }
                },
            );
        });
    }
    {
        let window = window.downgrade();
        let state = state.clone();
        let inventory = inventory.clone();
        let config = config.clone();
        let home = home.clone();
        grant.connect_clicked(move |_| {
            let Some(window) = window.upgrade() else {
                return;
            };
            let state = state.clone();
            let inventory = inventory.clone();
            let config = config.clone();
            let dialog = gtk::FileDialog::builder()
                .title("Dein Home-Verzeichnis für die Wiederherstellung wählen")
                .build();
            if let Some(home) = &home {
                dialog.set_initial_folder(Some(&gio::File::for_path(home)));
            }
            gtk::FileDialog::select_folder(
                &dialog,
                Some(&window),
                None::<&gio::Cancellable>,
                move |result| {
                    if let Ok(file) = result {
                        if let Some(path) = file.path() {
                            for folder in &state.folders {
                                *folder.original.borrow_mut() =
                                    suggestion(&folder.backup, &inventory, &config, Some(&path));
                            }
                            state.refresh();
                        }
                    }
                },
            );
        });
    }
    {
        let state = state.clone();
        let window = window.clone();
        let nav = nav.clone();
        let config = config.clone();
        let target = target.clone();
        let content = content.downgrade();
        start.connect_clicked(move |_| {
            let Some(chosen) = state.locations() else { return; };
            let originals = state.original_mode.upgrade().is_some_and(|mode| mode.is_active());
            let Some(content) = content.upgrade() else { return; }; content.set_sensitive(false);
            state.error("Wiederherstellung läuft … Automatischer Sync bleibt bis zum Abschluss pausiert.");
            let files = inventory.files.clone(); let target = target.clone(); let config = config.clone();
            let window = window.clone(); let nav = nav.clone(); let state = state.clone();
            task(move || {
                let locations: Vec<_> = chosen.iter().map(|(namespace, (base, relative))| {
                    Ok(RestoreLocation { namespace: namespace.clone(), destination: cloudsave_core::prepare_restore_location(&target, base, relative)? })
                }).collect::<cloudsave_core::Result<_>>()?;
                let report = crate::jobs::restore(&target, cloudsave_core::TransferControl::default(), |job| {
                    cloudsave_core::restore_mapped_controlled(&target, &files, &locations, job.control(), |progress| job.progress(progress))
                })?;
                if report.cancelled { return Err(cloudsave_core::Error(format!("Auftrag abgebrochen · {} Dateien wiederhergestellt", report.restored))); }
                if !report.errors.is_empty() { return Err(cloudsave_core::Error(format!("{} Dateien geladen, {} vorhandene behalten. {} Fehler:\n{}", report.restored, report.skipped, report.errors.len(), report.errors.join("\n")))); }
                let mut config = config;
                if originals {
                    config.folders.retain(|path| {
                        let name = config.settings.folder_names.get(path).cloned().unwrap_or_else(|| path.file_name().unwrap_or_default().to_string_lossy().into_owned());
                        !locations.iter().any(|location| location.namespace == name)
                    });
                    for location in &locations {
                        if location.namespace.is_empty() { continue; }
                        config.folders.push(location.destination.clone());
                        config.settings.folder_names.insert(location.destination.clone(), location.namespace.clone());
                        if let Some((_, (_, relative))) = chosen.iter().find(|(name, _)| name == &location.namespace) {
                            if !relative.as_os_str().is_empty() { config.settings.folder_home_paths.insert(location.destination.clone(), relative.clone()); }
                        }
                    }
                }
                if originals { cloudsave_core::adopt_restored(&target, &report, &locations, &Config::default_state_path())?; }
                config.settings.pending_recovery = None;
                // A clean original-location recovery can resume safely in Backup mode.
                config.settings.sync_mode = SyncMode::Backup;
                config.settings.auto_sync = originals && report.skipped == 0 && locations.iter().all(|location| !location.namespace.is_empty());
                config.save(&Config::default_path())?;
                Ok((config, report))
            }, move |result| {
                content.set_sensitive(true);
                match result {
                    Ok((config, report)) => {
                        state.error(&format!("{} Dateien wiederhergestellt · {} vorhandene Dateien behalten. {}", report.restored, report.skipped, if config.settings.auto_sync { "Live-Sync wird im Backup-Modus fortgesetzt." } else { "Live-Sync bleibt pausiert; du kannst ihn in der Übersicht einschalten." }));
                        let mut child = content.first_child();
                        while let Some(widget) = child {
                            widget.set_sensitive(false);
                            child = widget.next_sibling();
                        }
                        if let Some(status) = state.status.upgrade() { status.set_sensitive(true); }
                        let done = gtk::Button::with_label("Zur Übersicht");
                        done.add_css_class("suggested-action");
                        done.add_css_class("primary-action");
                        let window = window.clone(); let nav = nav.clone();
                        done.connect_clicked(move |_| crate::onboarding::show_dashboard(&window, &nav, &config, config.settings.auto_sync)); content.append(&done);
                    }
                    Err(error) => state.error(&format!("Wiederherstellung nicht abgeschlossen: {error}\nNichts wird hochgeladen. Du kannst die Zuordnung korrigieren und erneut versuchen.")),
                }
            });
        });
    }
    let skip = gtk::Button::with_label("Ohne Wiederherstellung fortfahren");
    content.append(&skip);
    {
        let config = config.clone();
        let window = window.clone();
        let nav = nav.clone();
        let state = state.clone();
        skip.connect_clicked(move |_| {
            let mut config = config.clone();
            config.settings.pending_recovery = None;
            config.settings.auto_sync = false;
            match config.save(&Config::default_path()) {
                Ok(()) => crate::onboarding::show_dashboard(&window, &nav, &config, false),
                Err(error) => state.error(&error.to_string()),
            }
        });
    }
    state.refresh();
    crate::onboarding::shell("PC wiederherstellen", &content, None)
}

fn suggestion(
    folder: &BackupFolder,
    inventory: &BackupInventory,
    config: &Config,
    home: Option<&PathBuf>,
) -> Option<ChosenPath> {
    if let Some(relative) = &folder.home_relative {
        return home.map(|home| (home.clone(), relative.clone()));
    }
    if let Some(path) = config.folders.iter().find(|path| {
        path.file_name()
            .is_some_and(|name| name == folder.namespace.as_str())
    }) {
        let actual_home = std::env::var_os("HOME").map(PathBuf::from);
        if let (Some(root), Some(actual_home)) = (home, actual_home) {
            if let Ok(relative) = path.strip_prefix(actual_home) {
                if !relative.as_os_str().is_empty() {
                    return Some((root.clone(), relative.to_path_buf()));
                }
            }
        }
        if std::env::var_os("FLATPAK_ID").is_some() {
            return None;
        }
        return Some((path.clone(), PathBuf::new()));
    }
    // Legacy backups only reveal their top-level name. Never invent paths for
    // root files, hidden directories, or custom/external roots in a new catalog.
    if !inventory.has_catalog && !folder.namespace.is_empty() && !folder.namespace.starts_with('.')
    {
        return home.map(|home| (home.clone(), PathBuf::from(&folder.namespace)));
    }
    None
}
