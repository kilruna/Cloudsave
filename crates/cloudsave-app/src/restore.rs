use adw::prelude::*;
use cloudsave_core::{BackupFile, Target};
use gtk::glib;
use gtk4 as gtk;
use libadwaita as adw;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc,
    time::Duration,
};

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

struct RestoreRow {
    file: BackupFile,
    button: gtk::CheckButton,
    search_path: String,
}

struct RestoreGroup {
    container: gtk::Box,
    rows: Vec<RestoreRow>,
}

struct RestoreView {
    list: gtk::Box,
    search: gtk::SearchEntry,
    status: gtk::Label,
    feedback: gtk::Label,
    choose: gtk::Button,
    action: gtk::Button,
    groups: RefCell<Vec<RestoreGroup>>,
    loaded: RefCell<Option<Target>>,
    destination: RefCell<Option<PathBuf>>,
    busy: Cell<bool>,
    request: Cell<u64>,
}

impl RestoreView {
    fn set_feedback(&self, text: &str) {
        self.feedback.set_text(text);
        self.feedback.set_visible(!text.is_empty());
    }

    fn update_selection(&self) {
        let query = self.search.text().trim().to_lowercase();
        let groups = self.groups.borrow();
        let mut visible = 0;
        let mut selected = 0;
        let mut hidden_selected = 0;
        for group in groups.iter() {
            let mut found = false;
            for row in &group.rows {
                let matches = row.search_path.contains(&query);
                row.button.set_visible(matches);
                found |= matches;
                visible += usize::from(matches);
                let checked = row.button.is_active();
                selected += usize::from(checked);
                hidden_selected += usize::from(checked && !matches);
            }
            group.container.set_visible(found);
        }
        if !groups.is_empty() {
            let count = match visible {
                0 => "Keine Dateien entsprechen der Suche".into(),
                1 => "1 Sicherungsdatei gefunden".into(),
                _ => format!("{visible} Sicherungsdateien gefunden"),
            };
            let selection = if hidden_selected > 0 {
                format!("{selected} ausgewählt ({hidden_selected} außerhalb der Suche)")
            } else {
                format!("{selected} ausgewählt")
            };
            self.status.set_text(&format!("{count} · {selection}"));
        }
        self.action.set_sensitive(
            selected > 0
                && !self.busy.get()
                && self.destination.borrow().is_some()
                && self.loaded.borrow().is_some(),
        );
    }

    fn set_files(self: &Rc<Self>, entries: Vec<BackupFile>) {
        if entries.is_empty() {
            self.status.set_text("Keine Sicherungsdateien gefunden.");
            return;
        }
        let mut folders = BTreeMap::<_, Vec<_>>::new();
        for file in entries {
            let folder = file.path.parent().unwrap_or(Path::new("")).to_path_buf();
            folders.entry(folder).or_default().push(file);
        }
        let mut groups = self.groups.borrow_mut();
        for (folder, files) in folders {
            let container = gtk::Box::new(gtk::Orientation::Vertical, 4);
            container.add_css_class("restore-group");
            let title = if folder.as_os_str().is_empty() {
                "Dateien im Sicherungsziel".into()
            } else {
                folder.display().to_string()
            };
            let heading = path_label(&title);
            heading.add_css_class("heading");
            container.append(&heading);
            let rows = files
                .into_iter()
                .map(|file| {
                    let name = file
                        .path
                        .file_name()
                        .unwrap_or(file.path.as_os_str())
                        .to_string_lossy();
                    let filename = path_label(&name);
                    filename.set_hexpand(true);
                    let size = file.size.map_or_else(
                        || "Größe unbekannt".into(),
                        |size| crate::jobs::format_bytes(size as f64),
                    );
                    let size = gtk::Label::new(Some(&size));
                    size.add_css_class("dim-label");
                    let contents = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                    contents.append(&filename);
                    contents.append(&size);
                    let button = gtk::CheckButton::new();
                    button.set_child(Some(&contents));
                    button.update_property(&[gtk::accessible::Property::Label(
                        &file.path.to_string_lossy(),
                    )]);
                    let view = Rc::downgrade(self);
                    button.connect_toggled(move |_| {
                        if let Some(view) = view.upgrade() {
                            view.update_selection();
                        }
                    });
                    container.append(&button);
                    RestoreRow {
                        search_path: file.path.to_string_lossy().to_lowercase(),
                        file,
                        button,
                    }
                })
                .collect();
            self.list.append(&container);
            groups.push(RestoreGroup { container, rows });
        }
    }

    fn load(self: &Rc<Self>, selected: Target, reload: &gtk::Button) {
        let request = self.request.get().wrapping_add(1);
        self.request.set(request);
        self.busy.set(true);
        self.groups.borrow_mut().clear();
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        *self.loaded.borrow_mut() = None;
        self.action.set_sensitive(false);
        self.choose.set_sensitive(false);
        self.search.set_sensitive(false);
        reload.set_sensitive(false);
        self.set_feedback("");
        self.status.set_text("Sicherungen werden gelesen …");

        let job_target = selected.clone();
        let view = Rc::downgrade(self);
        let reload = reload.downgrade();
        background(
            move || cloudsave_core::list_backups(&job_target),
            move |result| {
                let Some(view) = view.upgrade() else {
                    return;
                };
                if view.request.get() != request {
                    return;
                }
                view.busy.set(false);
                if let Some(reload) = reload.upgrade() {
                    reload.set_sensitive(true);
                }
                match result {
                    Ok(entries) => {
                        view.choose.set_sensitive(!entries.is_empty());
                        view.search.set_sensitive(!entries.is_empty());
                        view.set_files(entries);
                        *view.loaded.borrow_mut() = Some(selected.clone());
                    }
                    Err(error) => {
                        view.status
                            .set_text("Sicherungen konnten nicht gelesen werden.");
                        view.set_feedback(&error.to_string());
                    }
                }
                view.update_selection();
            },
        );
    }

    fn restore(self: &Rc<Self>, root: &gtk::Box) {
        let (Some(target), Some(destination)) = (
            self.loaded.borrow().clone(),
            self.destination.borrow().clone(),
        ) else {
            return;
        };
        let selection: Vec<_> = self
            .groups
            .borrow()
            .iter()
            .flat_map(|group| &group.rows)
            .filter(|row| row.button.is_active())
            .map(|row| row.file.clone())
            .collect();
        if selection.is_empty() || self.busy.get() {
            return;
        }
        self.busy.set(true);
        root.set_sensitive(false);
        self.set_feedback(
            "Dateien werden wiederhergestellt … Pause und Abbruch im Systemabschnitt.",
        );
        let view = Rc::downgrade(self);
        let root = root.downgrade();
        background(
            move || {
                crate::jobs::restore(&target, cloudsave_core::TransferControl::default(), |job| {
                    cloudsave_core::restore_controlled(
                        &target,
                        &selection,
                        &destination,
                        job.control(),
                        |progress| job.progress(progress),
                    )
                })
            },
            move |result| {
                let (Some(view), Some(root)) = (view.upgrade(), root.upgrade()) else {
                    return;
                };
                view.busy.set(false);
                root.set_sensitive(true);
                let message = match result {
                    Ok(report) if report.cancelled => format!(
                        "Wiederherstellung abgebrochen · {} Datei(en) wiederhergestellt",
                        report.restored
                    ),
                    Ok(report) if report.errors.is_empty() => format!(
                        "{} Datei(en) wiederhergestellt · {}",
                        report.restored,
                        crate::jobs::format_bytes(report.bytes as f64)
                    ),
                    Ok(report) => format!(
                        "{} Datei(en) wiederhergestellt · {} Fehler\n{}",
                        report.restored,
                        report.errors.len(),
                        report.errors.join("\n")
                    ),
                    Err(error) => format!("Wiederherstellung fehlgeschlagen\n{error}"),
                };
                view.set_feedback(&message);
                view.update_selection();
            },
        );
    }
}

fn path_label(path: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(path));
    label.set_xalign(0.0);
    label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    label.set_max_width_chars(40);
    label.set_tooltip_text(Some(path));
    label
}

fn target_factory() -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        if let Some(item) = item.downcast_ref::<gtk::ListItem>() {
            item.set_child(Some(&path_label("")));
        }
    });
    factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        if let (Some(name), Some(label)) = (
            item.item().and_downcast::<gtk::StringObject>(),
            item.child().and_downcast::<gtk::Label>(),
        ) {
            label.set_text(&name.string());
            label.set_tooltip_text(Some(&name.string()));
        }
    });
    factory
}

pub fn page(window: &gtk::Window, targets: &[Target]) -> adw::NavigationPage {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(20);
    root.set_margin_end(20);

    let names: Vec<_> = targets.iter().map(cloudsave_core::target_label).collect();
    let labels: Vec<_> = names.iter().map(String::as_str).collect();
    let target = gtk::DropDown::from_strings(&labels);
    target.set_factory(Some(&target_factory()));
    target.set_list_factory(Some(&target_factory()));
    target.set_hexpand(true);
    target.update_property(&[gtk::accessible::Property::Label("Sicherungsziel")]);
    let target_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    match labels.as_slice() {
        [] => {}
        [name] => {
            let label = path_label(name);
            label.set_hexpand(true);
            target_row.append(&label);
        }
        _ => target_row.append(&target),
    }
    let reload = gtk::Button::with_label("Erneut laden");
    reload.set_sensitive(!targets.is_empty());
    target_row.append(&reload);
    root.append(&target_row);

    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Dateiname oder Pfad suchen"));
    search.set_sensitive(false);
    root.append(&search);

    let status = gtk::Label::new(None);
    status.set_xalign(0.0);
    status.set_wrap(true);
    root.append(&status);
    if targets.is_empty() {
        status.set_text("Kein Sicherungsziel eingerichtet.");
    }

    let list = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    root.append(&scroll);

    let feedback = gtk::Label::new(None);
    feedback.set_xalign(0.0);
    feedback.set_wrap(true);
    feedback.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    feedback.set_selectable(true);
    feedback.set_visible(false);
    let feedback_scroll = gtk::ScrolledWindow::new();
    feedback_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    feedback_scroll.set_max_content_height(94);
    feedback_scroll.set_propagate_natural_height(true);
    feedback_scroll.set_child(Some(&feedback));
    feedback
        .bind_property("visible", &feedback_scroll, "visible")
        .sync_create()
        .build();
    root.append(&feedback_scroll);

    let choose = gtk::Button::with_label("Wiederherstellungsordner wählen …");
    choose.set_sensitive(false);
    root.append(&choose);
    let folder_label = path_label("");
    folder_label.set_visible(false);
    root.append(&folder_label);
    let action = gtk::Button::with_label("Wiederherstellen");
    action.add_css_class("suggested-action");
    action.set_sensitive(false);
    action.set_tooltip_text(Some(
        "Vorhandene Dateien im Ausgabeordner bleiben erhalten.",
    ));
    root.append(&action);

    let view = Rc::new(RestoreView {
        list,
        search: search.clone(),
        status,
        feedback,
        choose: choose.clone(),
        action: action.clone(),
        groups: RefCell::new(Vec::new()),
        loaded: RefCell::new(None),
        destination: RefCell::new(None),
        busy: Cell::new(false),
        request: Cell::new(0),
    });
    {
        let view = view.clone();
        let target = target.clone();
        let targets = targets.to_vec();
        reload.connect_clicked(move |button| {
            if let Some(selected) = targets.get(target.selected() as usize) {
                view.load(selected.clone(), button);
            }
        });
    }
    {
        let reload = reload.downgrade();
        target.connect_selected_notify(move |_| {
            if let Some(reload) = reload.upgrade() {
                reload.emit_clicked();
            }
        });
    }
    let weak_view = Rc::downgrade(&view);
    search.connect_search_changed(move |_| {
        if let Some(view) = weak_view.upgrade() {
            view.update_selection();
        }
    });
    {
        let view = Rc::downgrade(&view);
        let window = window.downgrade();
        choose.connect_clicked(move |button| {
            let Some(window) = window.upgrade() else {
                return;
            };
            button.set_sensitive(false);
            let view = view.clone();
            let folder_label = folder_label.clone();
            let button = button.downgrade();
            gtk::FileDialog::new().select_folder(
                Some(&window),
                None::<&gtk::gio::Cancellable>,
                move |result| {
                    let Some(view) = view.upgrade() else {
                        return;
                    };
                    if let Some(button) = button.upgrade() {
                        button.set_sensitive(!view.busy.get() && !view.groups.borrow().is_empty());
                    }
                    if let Some(path) = result.ok().and_then(|file| file.path()) {
                        let path_text = path.display().to_string();
                        folder_label.set_text(&path_text);
                        folder_label.set_tooltip_text(Some(&path_text));
                        folder_label.set_visible(true);
                        *view.destination.borrow_mut() = Some(path);
                        view.update_selection();
                    }
                },
            );
        });
    }
    {
        let root = root.downgrade();
        let view = Rc::downgrade(&view);
        action.connect_clicked(move |_| {
            if let (Some(root), Some(view)) = (root.upgrade(), view.upgrade()) {
                view.restore(&root);
            }
        });
    }

    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_show_start_title_buttons(false);
    header.set_show_end_title_buttons(false);
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&root));
    if !targets.is_empty() {
        reload.emit_clicked();
    }
    adw::NavigationPage::new(&toolbar, "Sicherungen wiederherstellen")
}
