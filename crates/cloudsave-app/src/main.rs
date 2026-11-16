mod dashboard;
mod onboarding;

use adw::prelude::*;
use gtk::glib;
use gtk4 as gtk;
use libadwaita as adw;

const CSS: &str = r#"
@define-color accent_color #9D7CFF;
@define-color accent_bg_color #9D7CFF;
@define-color accent_fg_color #FFFFFF;

.window-bg {
    background: linear-gradient(150deg, #3E7BDB 0%, #1E56B0 38%, #24338D 100%);
}

.main-card {
    background:
        radial-gradient(circle at 78% -12%, rgba(96, 165, 250, 0.40), rgba(59, 130, 246, 0.12) 42%, transparent 62%),
        #151E3C;
}

.app-title {
    font-size: 20px;
    font-weight: 700;
    color: #FFFFFF;
}

.sync-btn {
    background-color: #9D7CFF;
    color: #FFFFFF;
    font-weight: 600;
    border-radius: 999px;
    padding: 8px 18px;
}
.sync-btn:hover {
    background-color: #AE92FF;
}

.sync-title {
    font-size: 30px;
    font-weight: 700;
    color: #FFFFFF;
}
.sync-sub {
    font-size: 14px;
    color: #A9B1CE;
}

.sync-progress trough {
    min-height: 10px;
    background-color: rgba(255, 255, 255, 0.16);
    border: 1px solid rgba(255, 255, 255, 0.55);
    border-radius: 999px;
}
.sync-progress trough progress {
    min-height: 10px;
    background: linear-gradient(90deg, #FFFFFF, #C7C9FE);
    border-radius: 999px;
}

.section-title {
    font-size: 18px;
    font-weight: 700;
    color: #FFFFFF;
}

.card {
    background-color: rgba(255, 255, 255, 0.055);
    border-radius: 14px;
}
.target-card {
    padding: 22px;
}

.folder-row {
    padding: 13px 16px;
}
.row-label {
    font-size: 15px;
    color: #F2F4FF;
}
.row-sep {
    min-height: 1px;
    background-color: rgba(255, 255, 255, 0.08);
}

.add-btn {
    background-color: #2A2D3A;
    color: #FFFFFF;
    border-radius: 10px;
    padding: 9px 14px;
}
.add-btn:hover {
    background-color: #353A4A;
}
.add-btn label {
    font-size: 14px;
}

.target-title {
    font-size: 20px;
    font-weight: 700;
    color: #FFFFFF;
}
.target-sub {
    font-size: 14px;
    color: #9AA3C0;
}
.target-row-label {
    font-size: 15px;
    color: #F2F4FF;
}

.expander-btn {
    padding: 14px 16px;
}
.expander-btn label {
    font-size: 15px;
    font-weight: 600;
    color: #F2F4FF;
}
"#;

#[derive(Clone, Copy)]
pub(crate) enum Icon {
    Cloud,
    Folder,
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
    for (dx, dy, r) in [
        (-18.0, 6.0, 13.0),
        (-2.0, -4.0, 17.0),
        (15.0, 4.0, 14.0),
        (4.0, 10.0, 15.0),
    ] {
        cr.arc(
            cx + dx * scale,
            cy + dy * scale,
            r * scale,
            0.0,
            std::f64::consts::TAU,
        );
    }
    rounded_rect(
        cr,
        cx - 26.0 * scale,
        cy + 4.0 * scale,
        52.0 * scale,
        14.0 * scale,
        7.0 * scale,
    );
    let _ = cr.fill();
}

fn draw_folder(cr: &gtk::cairo::Context, w: f64, h: f64, red: f64, green: f64, blue: f64) {
    let s = w.min(h) / 64.0;
    cr.set_source_rgba(red, green, blue, 1.0);
    rounded_rect(cr, 6.0 * s, 12.0 * s, 24.0 * s, 12.0 * s, 4.0 * s);
    let _ = cr.fill();
    rounded_rect(cr, 6.0 * s, 20.0 * s, 52.0 * s, 32.0 * s, 5.0 * s);
    let _ = cr.fill();
}

pub(crate) fn icon(kind: Icon, size: i32, red: f64, green: f64, blue: f64) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_size_request(size, size);
    area.set_valign(gtk::Align::Center);
    area.set_draw_func(move |_, cr, w, h| {
        let w = f64::from(w);
        let h = f64::from(h);
        match kind {
            Icon::Cloud => draw_cloud_at(cr, w / 2.0, h * 0.42, w / 64.0, (red, green, blue, 1.0)),
            Icon::Folder => draw_folder(cr, w, h, red, green, blue),
        }
    });
    area
}

fn main() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id("org.cloudsave.Cloudsave")
        .build();

    app.connect_startup(|_| {
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
    });

    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &adw::Application) {
    let background = std::env::args().any(|a| a == "--background");
    let window = gtk::ApplicationWindow::new(app);
    window.set_title(Some("Cloudsave"));
    window.set_default_size(1120, 760);
    window.set_decorated(false);

    {
        let window = window.clone();
        window.connect_close_request(move |w| {
            let hide = cloudsave_core::Config::load(&cloudsave_core::Config::default_path())
                .map(|c| c.settings.run_in_background)
                .unwrap_or(true);
            if hide {
                w.hide();
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
    root.append(&card);

    let nav = adw::NavigationView::new();
    nav.set_hexpand(true);
    nav.set_vexpand(true);
    card.append(&nav);

    match cloudsave_core::Config::load(&cloudsave_core::Config::default_path()) {
        Ok(config) => {
            let page = dashboard::build(window.upcast_ref(), &config, true);
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
