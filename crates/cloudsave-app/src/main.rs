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

    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let title = gtk::Label::new(Some("Cloudsave"));
    title.add_css_class("title-2");
    header.set_title_widget(Some(&title));
    let sync_btn = gtk::Button::with_label("Jetzt synchronisieren");
    sync_btn.add_css_class("suggested-action");
    sync_btn.add_css_class("pill");
    header.pack_end(&sync_btn);
    toolbar.add_top_bar(&header);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
    content.set_margin_start(24);
    content.set_margin_end(24);
    content.set_margin_top(16);
    content.set_margin_bottom(24);
    content.set_halign(gtk::Align::Center);
    content.set_width_request(720);
    content.set_vexpand(true);

    let banner = adw::Banner::new("Alles synchronisiert · letzter Sync vor 2 Minuten");
    banner.set_revealed(true);
    content.append(&banner);

    let stats = gtk::Grid::new();
    stats.set_column_spacing(16);
    stats.set_hexpand(true);
    stats.attach(&stat_card("0", "Dateien gesichert"), 0, 0, 1, 1);
    stats.attach(&stat_card("0 B", "auf dem Ziel belegt"), 1, 0, 1, 1);
    stats.attach(&stat_card("vor 2 Min", "letzter Sync"), 2, 0, 1, 1);
    content.append(&stats);

    let folders = adw::PreferencesGroup::builder()
        .title("Gesicherte Ordner")
        .build();
    for (name, path, active) in [
        ("Dokumente", "~/Dokumente", true),
        ("Bilder", "~/Bilder", true),
        ("Musik", "~/Musik", false),
        ("Videos", "~/Videos", false),
        ("Schreibtisch", "~/Schreibtisch", false),
    ] {
        let row = adw::ActionRow::builder().title(name).subtitle(path).build();
        let check = gtk::CheckButton::new();
        check.set_active(active);
        check.set_valign(gtk::Align::Center);
        row.set_activatable_widget(Some(&check));
        row.add_suffix(&check);
        folders.add(&row);
    }
    let add_row = adw::ActionRow::builder()
        .title("Anderer Ordner hinzufügen …")
        .build();
    folders.add(&add_row);
    content.append(&folders);

    let target = adw::PreferencesGroup::builder().title("Ziel").build();
    let target_row = adw::ActionRow::builder()
        .title("Hetzner Storage Box")
        .subtitle("u123456.your-storagebox.de · WebDAV (443)")
        .build();
    let test_btn = gtk::Button::with_label("Verbindung testen");
    test_btn.set_valign(gtk::Align::Center);
    target_row.add_suffix(&test_btn);
    target.add(&target_row);
    let auto = adw::SwitchRow::builder()
        .title("Automatisch synchronisieren")
        .active(true)
        .build();
    target.add(&auto);
    content.append(&target);

    let settings = adw::PreferencesGroup::builder()
        .title("Einstellungen")
        .build();
    let rhythm = adw::ComboRow::builder().title("Sync-Rhythmus").build();
    let model = gtk::StringList::new(&["Bei jeder Änderung", "Stündlich", "Täglich"]);
    rhythm.set_model(Some(&model));
    rhythm.set_selected(0);
    settings.add(&rhythm);
    let notify = adw::SwitchRow::builder()
        .title("Benachrichtigung bei Fehlern")
        .active(true)
        .build();
    settings.add(&notify);
    let battery = adw::SwitchRow::builder()
        .title("Pausieren bei niedrigem Akku")
        .build();
    settings.add(&battery);
    content.append(&settings);

    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scrolled.set_child(Some(&content));
    scrolled.set_vexpand(true);
    toolbar.set_content(Some(&scrolled));

    window.set_content(Some(&toolbar));
    window.present();
}

fn stat_card(value: &str, label: &str) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
    card.add_css_class("stat-card");
    card.set_hexpand(true);
    let v = gtk::Label::new(Some(value));
    v.add_css_class("stat-value");
    v.set_xalign(0.0);
    let l = gtk::Label::new(Some(label));
    l.add_css_class("stat-label");
    l.set_xalign(0.0);
    card.append(&v);
    card.append(&l);
    card
}
