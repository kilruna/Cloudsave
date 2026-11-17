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

#[derive(Clone, Copy)]
pub(crate) enum Icon {
    Cloud,
    Folder,
    File,
    Storage,
    Clock,
    Server,
    Check,
    Warning,
    Refresh,
}

fn rounded_rect(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    let pi = std::f64::consts::PI;
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -pi / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, pi / 2.0);
    cr.arc(x + r, y + h - r, r, pi / 2.0, pi);
    cr.arc(x + r, y + r, r, pi, pi * 1.5);
    cr.close_path();
}

fn draw_cloud_at(
    cr: &gtk::cairo::Context,
    cx: f64,
    cy: f64,
    scale: f64,
    color: (f64, f64, f64, f64),
) {
    let (red, green, blue, alpha) = color;
    cr.set_source_rgba(red, green, blue, alpha);
    cr.set_line_width(4.0 * scale);
    cr.set_line_cap(gtk::cairo::LineCap::Round);
    cr.move_to(cx - 18.0 * scale, cy + 20.0 * scale);
    cr.curve_to(
        cx - 39.0 * scale,
        cy + 19.0 * scale,
        cx - 36.0 * scale,
        cy - 6.0 * scale,
        cx - 18.0 * scale,
        cy - 5.0 * scale,
    );
    cr.curve_to(
        cx - 17.0 * scale,
        cy - 28.0 * scale,
        cx + 16.0 * scale,
        cy - 27.0 * scale,
        cx + 18.0 * scale,
        cy - 5.0 * scale,
    );
    cr.curve_to(
        cx + 39.0 * scale,
        cy - 7.0 * scale,
        cx + 39.0 * scale,
        cy + 20.0 * scale,
        cx + 18.0 * scale,
        cy + 20.0 * scale,
    );
    cr.close_path();
    let _ = cr.stroke();
}

fn draw_folder(cr: &gtk::cairo::Context, w: f64, h: f64, red: f64, green: f64, blue: f64) {
    let s = w.min(h) / 64.0;
    cr.set_source_rgba(red, green, blue, 1.0);
    cr.set_line_width(3.5 * s);
    cr.set_line_join(gtk::cairo::LineJoin::Round);
    cr.move_to(7.0 * s, 23.0 * s);
    cr.line_to(7.0 * s, 15.0 * s);
    cr.line_to(25.0 * s, 15.0 * s);
    cr.line_to(32.0 * s, 23.0 * s);
    let _ = cr.stroke();
    rounded_rect(cr, 7.0 * s, 23.0 * s, 50.0 * s, 30.0 * s, 4.0 * s);
    let _ = cr.stroke();
}

pub(crate) fn icon(kind: Icon, size: i32, red: f64, green: f64, blue: f64) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_size_request(size, size);
    area.set_valign(gtk::Align::Center);
    area.set_can_target(false);
    set_icon(&area, kind, red, green, blue);
    area
}

pub(crate) fn set_icon(area: &gtk::DrawingArea, kind: Icon, red: f64, green: f64, blue: f64) {
    area.set_draw_func(move |_, cr, w, h| {
        let w = f64::from(w);
        let h = f64::from(h);
        match kind {
            Icon::Cloud => draw_cloud_at(cr, w / 2.0, h * 0.42, w / 64.0, (red, green, blue, 1.0)),
            Icon::Folder => draw_folder(cr, w, h, red, green, blue),
            _ => {
                cr.scale(w / 64.0, h / 64.0);
                cr.set_source_rgb(red, green, blue);
                cr.set_line_width(3.5);
                cr.set_line_cap(gtk::cairo::LineCap::Round);
                cr.set_line_join(gtk::cairo::LineJoin::Round);
                match kind {
                    Icon::File => {
                        cr.move_to(18.0, 8.0);
                        cr.line_to(38.0, 8.0);
                        cr.line_to(51.0, 22.0);
                        cr.line_to(51.0, 56.0);
                        cr.line_to(15.0, 56.0);
                        cr.line_to(15.0, 8.0);
                        cr.close_path();
                        cr.move_to(37.0, 8.0);
                        cr.line_to(37.0, 23.0);
                        cr.line_to(51.0, 23.0);
                    }
                    Icon::Storage => {
                        cr.move_to(10.0, 16.0);
                        cr.curve_to(10.0, 5.0, 54.0, 5.0, 54.0, 16.0);
                        cr.curve_to(54.0, 27.0, 10.0, 27.0, 10.0, 16.0);
                        cr.line_to(10.0, 47.0);
                        cr.curve_to(10.0, 59.0, 54.0, 59.0, 54.0, 47.0);
                        cr.line_to(54.0, 16.0);
                        for y in [31.0, 41.0] {
                            cr.move_to(10.0, y);
                            cr.curve_to(10.0, y + 11.0, 54.0, y + 11.0, 54.0, y);
                        }
                    }
                    Icon::Clock | Icon::Check | Icon::Warning => {
                        cr.arc(32.0, 32.0, 24.0, 0.0, std::f64::consts::TAU);
                        match kind {
                            Icon::Clock => {
                                cr.move_to(32.0, 16.0);
                                cr.line_to(32.0, 32.0);
                                cr.line_to(44.0, 39.0);
                            }
                            Icon::Check => {
                                cr.move_to(19.0, 32.0);
                                cr.line_to(28.0, 41.0);
                                cr.line_to(45.0, 23.0);
                            }
                            _ => {
                                cr.move_to(32.0, 18.0);
                                cr.line_to(32.0, 34.0);
                                cr.move_to(32.0, 45.0);
                                cr.line_to(32.0, 46.0);
                            }
                        }
                    }
                    Icon::Server => {
                        rounded_rect(cr, 7.0, 11.0, 50.0, 42.0, 6.0);
                        cr.move_to(7.0, 32.0);
                        cr.line_to(57.0, 32.0);
                        for y in [22.0, 43.0] {
                            cr.move_to(16.0, y);
                            cr.line_to(18.0, y);
                            cr.move_to(27.0, y);
                            cr.line_to(47.0, y);
                        }
                    }
                    Icon::Refresh => {
                        cr.arc(32.0, 32.0, 21.0, 0.6, 5.8);
                        cr.move_to(52.0, 12.0);
                        cr.line_to(52.0, 25.0);
                        cr.line_to(39.0, 25.0);
                    }
                    _ => {}
                }
                let _ = cr.stroke();
            }
        }
    });
    area.queue_draw();
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
        let provider = gtk::CssProvider::new();
        provider.load_from_string(CSS);
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
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
    window.set_default_size(1180, 820);
    window.add_css_class("cloudsave-window");
    window.set_decorated(true);
    let header = adw::HeaderBar::new();
    follow_window_controls(&header);
    header.add_css_class("cloudsave-header");
    let brand = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    let mark = gtk::DrawingArea::new();
    mark.set_size_request(44, 44);
    mark.set_can_target(false);
    match brand::pixbuf(128) {
        Ok(cloud) => mark.set_draw_func(move |_, cr, width, height| {
            cr.scale(f64::from(width) / 128.0, f64::from(height) / 128.0);
            cr.set_source_pixbuf(&cloud, 0.0, 0.0);
            let _ = cr.paint();
        }),
        Err(error) => eprintln!("App-Icon nicht verfügbar: {error}"),
    }
    mark.add_css_class("brand-mark");
    brand.append(&mark);
    let name = gtk::Label::new(Some("Cloudsave"));
    name.add_css_class("app-title");
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

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("window-bg");
    root.set_hexpand(true);
    root.set_vexpand(true);
    window.set_child(Some(&root));

    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("main-card");
    card.set_halign(gtk::Align::Fill);
    card.set_valign(gtk::Align::Fill);
    card.set_vexpand(true);
    root.append(&card);

    let nav = adw::NavigationView::new();
    nav.set_hexpand(true);
    nav.set_vexpand(true);
    card.append(&nav);

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
