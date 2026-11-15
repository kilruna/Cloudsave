use std::path::PathBuf;
use std::process::ExitCode;

use cloudsave_core::{sync, Config};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("sync") => {
            let config_path = args
                .get(2)
                .map(PathBuf::from)
                .unwrap_or_else(Config::default_path);
            let config = match Config::load(&config_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!(
                        "Config nicht gefunden/lesbar ({}): {}\nTipp: config.toml anlegen, z. B. unter {}",
                        config_path.display(),
                        e,
                        Config::default_path().display()
                    );
                    return ExitCode::FAILURE;
                }
            };
            let state_path = Config::default_state_path();
            match sync(&config, &state_path) {
                Ok(report) => {
                    println!(
                        "Sync fertig: {} hochgeladen ({} B), {} gelöscht, {} unverändert, {} Fehler",
                        report.uploaded, report.bytes, report.deleted, report.skipped, report.errors.len()
                    );
                    for err in &report.errors {
                        eprintln!("Fehler: {}", err);
                    }
                    if report.errors.is_empty() {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    }
                }
                Err(e) => {
                    eprintln!("Sync fehlgeschlagen: {}", e);
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!("Usage: cloudsave-cli sync [CONFIG_PFAD]");
            ExitCode::FAILURE
        }
    }
}
