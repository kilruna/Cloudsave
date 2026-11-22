mod brand;
mod dashboard;
mod exclusion_picker;
mod jobs;
mod onboarding;
mod recovery;
mod restore;
mod tray;

use adw::prelude::*;
use gtk::{gdk::prelude::GdkCairoContextExt, glib};
use gtk4 as gtk;
use libadwaita as adw;

const CSS: &str = include_str!("style.css");

fn without_app_icon(layout: &str) -> String {
    layout
        .split(':')
        .map(|side| {
            side.split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty() && !matches!(*item, "icon" | "menu" | "appmenu"))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join(":")
}

fn follow_window_controls(header: &adw::HeaderBar) {
    if let Some(settings) = gtk::Settings::default() {
        let layout = settings
            .gtk_decoration_layout()
            .unwrap_or_else(|| ":minimize,maximize,close".into());
        header.set_decoration_layout(Some(&without_app_icon(&layout)));
        let header = header.downgrade();
        settings.connect_gtk_decoration_layout_notify(move |settings| {
            if let Some(header) = header.upgrade() {
                let layout = settings
                    .gtk_decoration_layout()
                    .unwrap_or_else(|| ":minimize,maximize,close".into());
                header.set_decoration_layout(Some(&without_app_icon(&layout)));
            }
        });
    }
}

fn wrapping_button(text: &str) -> gtk::Button {
    let button = gtk::Button::with_label(text);
    if let Some(label) = button.child().and_downcast::<gtk::Label>() {
        label.set_wrap(true);
        label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        label.set_justify(gtk::Justification::Center);
    }
    button
}

fn main() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id("org.cloudsave.Cloudsave")
        .flags(gtk::gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    app.add_main_option(
        "background",
        glib::Char(0),
        glib::OptionFlags::NONE,
        glib::OptionArg::None,
        "Im Hintergrund starten",
        None,
    );

    let tray = std::rc::Rc::new(std::cell::RefCell::new(None));
    let startup_tray = tray.clone();
    app.connect_startup(move |app| {
        gtk::Window::set_default_icon_name("org.cloudsave.Cloudsave");
        jobs::load_history();
        let open = gtk::gio::SimpleAction::new("open", None);
        let window_app = app.downgrade();
        open.connect_activate(move |_, _| {
            if let Some(app) = window_app.upgrade() {
                app.activate();
            }
        });
        app.add_action(&open);
        let provider = gtk::CssProvider::new();
        provider.load_from_string(CSS);
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
        match tray::Tray::new(app) {
            Ok(item) => *startup_tray.borrow_mut() = Some(item),
            Err(error) => eprintln!("Systemabschnitt nicht verfügbar: {error}"),
        }
    });
    app.connect_shutdown(move |_| {
        tray.borrow_mut().take();
    });

    app.connect_activate(|app| open_window(app, false));
    app.connect_command_line(|app, command_line| {
        let background = command_line.options_dict().contains("background");
        open_window(app, background);
        glib::ExitCode::SUCCESS
    });
    app.run()
}

// Activation (launcher, tray, or a second process) must reuse the same window
// and its sync worker. Hidden windows remain registered with GtkApplication,
// keeping the background process alive until an explicit quit or window destroy.
fn open_window(app: &adw::Application, background: bool) {
    if let Some(window) = app.windows().first() {
        if !background {
            window.present();
        }
        return;
    }
    build_ui(app, background);
}

fn build_ui(app: &adw::Application, background: bool) {
    let window = gtk::ApplicationWindow::new(app);
    window.set_title(Some("Cloudsave"));
    window.set_default_size(1060, 740);
    window.set_decorated(true);
    let header = adw::HeaderBar::new();
    follow_window_controls(&header);
    header.add_css_class("cloudsave-header");
    let brand = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let mark = gtk::DrawingArea::new();
    mark.set_size_request(32, 32);
    mark.set_valign(gtk::Align::Center);
    mark.set_can_target(false);
    match brand::pixbuf(128) {
        Ok(cloud) => mark.set_draw_func(move |_, cr, width, height| {
            cr.scale(f64::from(width) / 128.0, f64::from(height) / 128.0);
            cr.set_source_pixbuf(&cloud, 0.0, 0.0);
            let _ = cr.paint();
        }),
        Err(error) => eprintln!("App-Icon nicht verfügbar: {error}"),
    }
    brand.append(&mark);
    let name = gtk::Label::new(Some("Cloudsave"));
    name.add_css_class("title-4");
    brand.append(&name);
    header.pack_start(&brand);
    header.set_title_widget(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    window.set_titlebar(Some(&header));

    {
        let window = window.clone();
        window.connect_close_request(move |w| {
            let hide = cloudsave_core::Config::load(&cloudsave_core::Config::default_path())
                .map(|c| c.settings.run_in_background)
                .unwrap_or(true);
            if hide {
                w.set_visible(false);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
    }

    let nav = adw::NavigationView::new();
    window.set_child(Some(&nav));

    match cloudsave_core::Config::load(&cloudsave_core::Config::default_path()) {
        Ok(config) if config.settings.pending_recovery.is_some() => {
            jobs::set_auto_sync(false);
            recovery::resume(window.upcast_ref(), &nav, &config);
        }
        Ok(config) => {
            let page = dashboard::build(
                window.upcast_ref(),
                &nav,
                &config,
                config.settings.auto_sync,
            );
            nav.push(&adw::NavigationPage::new(&page, "Übersicht"));
        }
        Err(_) => {
            onboarding::build(window.upcast_ref(), &nav);
        }
    }

    if !background {
        window.present();
    }
}

#[cfg(test)]
mod header_tests {
    use super::without_app_icon;

    #[test]
    fn native_app_icons_are_removed_without_reordering_window_controls() {
        for (layout, expected) in [
            ("icon:minimize,maximize,close", ":minimize,maximize,close"),
            ("menu,close:minimize,maximize", "close:minimize,maximize"),
            ("close,minimize,maximize:", "close,minimize,maximize:"),
            ("icon,menu,appmenu:close", ":close"),
        ] {
            assert_eq!(without_app_icon(layout), expected);
        }
    }
}
