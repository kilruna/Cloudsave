use cloudsave_core::{autostart_enabled, set_autostart};

#[test]
fn autostart_desktop_file_written_and_removed() {
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_CONFIG_HOME", tmp.path());
    assert!(!autostart_enabled());

    set_autostart(true).unwrap();
    assert!(autostart_enabled());
    let content =
        std::fs::read_to_string(tmp.path().join("autostart/org.cloudsave.Cloudsave.desktop"))
            .unwrap();
    assert!(content.contains("Exec=flatpak run org.cloudsave.Cloudsave --background"));

    set_autostart(false).unwrap();
    assert!(!autostart_enabled());

    std::env::remove_var("XDG_CONFIG_HOME");
}
