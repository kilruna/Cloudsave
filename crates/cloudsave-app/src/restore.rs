use adw::prelude::*;
use cloudsave_core::{BackupFile, Target};
use gtk::glib;
use gtk4 as gtk;
use libadwaita as adw;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::mpsc,
    time::Duration,
};

type Files = Rc<RefCell<Vec<(BackupFile, gtk::CheckButton)>>>;

fn background<T: Send + 'static>(
    task: impl FnOnce() -> cloudsave_core::Result<T> + Send + 'static,
    done: impl Fn(cloudsave_core::Result<T>) + 'static,
) {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(task());
    });
    glib::timeout_add_local(Duration::from_millis(100), move || match rx.try_recv() {
        Ok(result) => {
            done(result);
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(mpsc::TryRecvError::Disconnected) => {
            done(Err(cloudsave_core::Error(
                "Aktion abgebrochen. Bitte erneut versuchen.".into(),
            )));
            glib::ControlFlow::Break
        }
    });
}

fn update_action(
    button: &gtk::Button,
    files: &Files,
    destination: &RefCell<Option<PathBuf>>,
    loaded: &RefCell<Option<Target>>,
    busy: &Cell<bool>,
) {
    button.set_sensitive(
        !busy.get()
            && destination.borrow().is_some()
            && loaded.borrow().is_some()
            && files.borrow().iter().any(|(_, check)| check.is_active()),
    );
}

pub fn page(window: &gtk::Window, targets: &[Target]) -> adw::NavigationPage {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.set_margin_top(20);
    root.set_margin_bottom(20);
    root.set_margin_start(24);
    root.set_margin_end(24);
    let hint=gtk::Label::new(Some("Wähle ein Sicherungsziel, lade die Dateiliste und markiere die gewünschten Dateien. Vorhandene Dateien im Wiederherstellungsordner werden nicht überschrieben."));
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    root.append(&hint);
    let names: Vec<_> = targets.iter().map(cloudsave_core::target_label).collect();
    let labels: Vec<_> = names.iter().map(String::as_str).collect();
    let target = gtk::DropDown::from_strings(&labels);
    target.set_enable_search(true);
    root.append(&target);
    let load = gtk::Button::with_label("Sicherungen laden");
    load.set_sensitive(!targets.is_empty());
    root.append(&load);
    let status = gtk::Label::new(None);
    status.set_wrap(true);
    status.set_selectable(true);
    status.set_xalign(0.0);
    root.append(&status);
    if targets.is_empty() {
        status.set_text("Kein Sicherungsziel eingerichtet.");
    }
    let list = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    root.append(&scroll);
    let choose = gtk::Button::with_label("Wiederherstellungsordner wählen …");
    root.append(&choose);
    let folder_label = gtk::Label::new(Some("Noch kein Wiederherstellungsordner gewählt"));
    folder_label.set_wrap(true);
    folder_label.set_xalign(0.0);
    root.append(&folder_label);
    let restore = gtk::Button::with_label("Wiederherstellen");
    restore.add_css_class("suggested-action");
    restore.set_sensitive(false);
    root.append(&restore);
    let files: Files = Rc::new(RefCell::new(Vec::new()));
    let destination = Rc::new(RefCell::new(None::<PathBuf>));
    let loaded = Rc::new(RefCell::new(None::<Target>));
    let busy = Rc::new(Cell::new(false));
    {
        let files = files.clone();
        let list = list.clone();
        let loaded = loaded.clone();
        let restore = restore.clone();
        let status = status.clone();
        target.connect_selected_notify(move |_| {
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            files.borrow_mut().clear();
            *loaded.borrow_mut() = None;
            restore.set_sensitive(false);
            status.set_text("");
        });
    }
    {
        let files = files.clone();
        let destination = destination.clone();
        let loaded = loaded.clone();
        let busy = busy.clone();
        let target = target.clone();
        let status = status.clone();
        let list = list.clone();
        let restore = restore.clone();
        let root = root.downgrade();
        let targets = targets.to_vec();
        load.connect_clicked(move |_| {
            let Some(selected) = targets.get(target.selected() as usize).cloned() else {
                return;
            };
            let Some(root) = root.upgrade() else {
                return;
            };
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            files.borrow_mut().clear();
            *loaded.borrow_mut() = None;
            restore.set_sensitive(false);
            busy.set(true);
            root.set_sensitive(false);
            status.set_text("Sicherungen werden gelesen …");
            let job_target = selected.clone();
            let files = files.clone();
            let destination = destination.clone();
            let loaded = loaded.clone();
            let busy = busy.clone();
            let status = status.clone();
            let list = list.clone();
            let restore = restore.clone();
            background(
                move || cloudsave_core::list_backups(&job_target),
                move |result| {
                    busy.set(false);
                    root.set_sensitive(true);
                    match result {
                        Ok(entries) => {
                            let count = entries.len();
                            for file in entries {
                                let size = file
                                    .size
                                    .map(|size| format!("{size} B"))
                                    .unwrap_or_else(|| "Größe unbekannt".into());
                                let check = gtk::CheckButton::with_label(&format!(
                                    "{} · {size}",
                                    file.path.display()
                                ));
                                let weak_files = Rc::downgrade(&files);
                                let destination = destination.clone();
                                let loaded = loaded.clone();
                                let busy = busy.clone();
                                let button = restore.downgrade();
                                check.connect_toggled(move |_| {
                                    if let (Some(files), Some(button)) =
                                        (weak_files.upgrade(), button.upgrade())
                                    {
                                        update_action(
                                            &button,
                                            &files,
                                            &destination,
                                            &loaded,
                                            &busy,
                                        );
                                    }
                                });
                                list.append(&check);
                                files.borrow_mut().push((file, check));
                            }
                            *loaded.borrow_mut() = Some(selected.clone());
                            status.set_text(&format!(
                                "{count} Sicherungsdatei(en) gefunden. Bitte Dateien auswählen."
                            ));
                        }
                        Err(error) => status.set_text(&format!(
                            "Sicherungen konnten nicht gelesen werden: {error}"
                        )),
                    }
                    update_action(&restore, &files, &destination, &loaded, &busy);
                },
            );
        });
    }
    {
        let window = window.downgrade();
        let destination = destination.clone();
        let files = files.clone();
        let loaded = loaded.clone();
        let busy = busy.clone();
        let restore = restore.clone();
        let folder_label = folder_label.clone();
        choose.connect_clicked(move |button| {
            let Some(window) = window.upgrade() else {
                return;
            };
            button.set_sensitive(false);
            let destination = destination.clone();
            let files = files.clone();
            let loaded = loaded.clone();
            let busy = busy.clone();
            let restore = restore.clone();
            let folder_label = folder_label.clone();
            let button = button.downgrade();
            gtk::FileDialog::new().select_folder(
                Some(&window),
                None::<&gtk::gio::Cancellable>,
                move |result| {
                    if let Some(button) = button.upgrade() {
                        button.set_sensitive(true);
                    }
                    if let Ok(file) = result {
                        if let Some(path) = file.path() {
                            folder_label.set_text(&path.display().to_string());
                            *destination.borrow_mut() = Some(path);
                        }
                    }
                    update_action(&restore, &files, &destination, &loaded, &busy);
                },
            );
        });
    }
    {
        let files = files.clone();
        let loaded = loaded.clone();
        let destination = destination.clone();
        let busy = busy.clone();
        let root = root.downgrade();
        let status = status.clone();
        restore.connect_clicked(move |button| {
            let (Some(target), Some(destination_path), Some(root)) = (
                loaded.borrow().clone(),
                destination.borrow().clone(),
                root.upgrade(),
            ) else {
                return;
            };
            let selection: Vec<_> = files
                .borrow()
                .iter()
                .filter(|(_, check)| check.is_active())
                .map(|(file, _)| file.clone())
                .collect();
            if selection.is_empty() {
                return;
            }
            busy.set(true);
            root.set_sensitive(false);
            status.set_text("Dateien werden wiederhergestellt …");
            let files = files.clone();
            let loaded = loaded.clone();
            let destination = destination.clone();
            let busy = busy.clone();
            let button = button.clone();
            let status = status.clone();
            background(
                move || {
                    crate::jobs::restore(&target, |job| {
                        cloudsave_core::restore_with_progress(
                            &target,
                            &selection,
                            &destination_path,
                            |progress| job.progress(progress),
                        )
                    })
                },
                move |result| {
                    busy.set(false);
                    root.set_sensitive(true);
                    match result {
                        Ok(report) if report.errors.is_empty() => status.set_text(&format!(
                            "{} Datei(en) wiederhergestellt · {} B",
                            report.restored, report.bytes
                        )),
                        Ok(report) => status.set_text(&format!(
                            "{} Datei(en) wiederhergestellt · {} Fehler:\n{}",
                            report.restored,
                            report.errors.len(),
                            report.errors.join("\n")
                        )),
                        Err(error) => {
                            status.set_text(&format!("Wiederherstellung fehlgeschlagen: {error}"))
                        }
                    }
                    update_action(&button, &files, &destination, &loaded, &busy);
                },
            );
        });
    }
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_show_start_title_buttons(false);
    header.set_show_end_title_buttons(false);
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&root));
    adw::NavigationPage::new(&toolbar, "Sicherungen wiederherstellen")
}
