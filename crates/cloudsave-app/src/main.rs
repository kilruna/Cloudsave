mod dashboard;
mod onboarding;

use adw::prelude::*;
use gtk::glib;
use gtk4 as gtk;
use libadwaita as adw;

const CSS: &str = r#"
@define-color accent_color #FF7A29;
@define-color accent_bg_color #FF7A29;
@define-color accent_fg_color #1A1A1A;
@define-color window_bg_color #121212;
@define-color view_bg_color #121212;
@define-color card_bg_color #1E1E1E;
@define-color headerbar_bg_color #1A1A1A;
@define-color headerbar_border_color rgba(255, 255, 255, 0.08);
@define-color popover_bg_color #1E1E1E;

.stat-card {
    background-color: @card_bg_color;
    border-radius: 16px;
    padding: 18px 20px;
}
.stat-value {
    font-size: 30px;
    font-weight: 700;
}
.stat-label {
    font-size: 13px;
    opacity: 0.65;
}
"#;

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
    let window = adw::ApplicationWindow::new(app);
    window.set_title(Some("Cloudsave"));
    window.set_default_size(880, 700);

    let nav = adw::NavigationView::new();
    match cloudsave_core::Config::load(&cloudsave_core::Config::default_path()) {
        Ok(config) => {
            let page = dashboard::build(&window, &config, false);
            nav.push(&adw::NavigationPage::new(&page, "Übersicht"));
        }
        Err(_) => {
            onboarding::build(&window, &nav);
        }
    }

    window.set_content(Some(&nav));
    window.present();
}
