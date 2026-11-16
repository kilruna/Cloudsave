# Cloudsave

Cloudsave ist eine Flatpak-Desktop-App für Linux, mit der du beliebige Ordner aus deinem Home-Verzeichnis – z. B. `~/Bilder` oder `~/Dokumente` – automatisch auf ein Cloud-Ziel sicherst, etwa eine **Hetzner Storage Box**, eine Nextcloud (WebDAV) oder S3-kompatiblen Speicher.

**Die Idee in einem Satz:** Ordner anhaken → Ziel wählen → fertig. Cloudsave erkennt selbstständig, welche Dateien sich geändert haben, und lädt nur diese hoch – ressourcenschonend im Hintergrund.

## Highlights

- **Einfache Einrichtung:** Flacher Flow in drei Schritten, keine verschachtelten Menüs – vom Start bis zum eingerichteten Sync.
- **Mehrere Quellordner:** Beliebig viele Verzeichnisse pro Ziel – per Datei-Auswahl hinzufügen und jederzeit per Häkchen an- bzw. abwählen.
- **Automatische Änderungserkennung:** Kombination aus System-Ereignissen (inotify) und gelegentlichem Scan – kein dauerhaftes Durchsuchen der Platte im Hintergrund.
- **Delta-Sync:** Nur geänderte Dateien werden übertragen, nicht der ganze Ordner.
- **Sichere Zugangsdaten:** Passwörter liegen nie in der `config.toml`, sondern in einer Datei mit `0600`-Berechtigung im App-Config-Verzeichnis (Fallback: Umgebungsvariable).
- **Autostart & Hintergrund:** Optional startet Cloudsave minimiert beim Systemstart (`--background`) und läuft nach dem Schließen des Fensters im Hintergrund weiter; der Auto-Sync läuft im einstellbaren Intervall (Standard: 5 Minuten) und überträgt nur Geändertes.
- **Ressourcenschonend:** Zwischen den Syncs wird nur stat-geprüft, nicht die ganze Platte gelesen; laufender Sync belegt keine UI.

## Ziele (Protokolle)

Verfügbar: **WebDAV (HTTPS)** – z. B. Hetzner Storage Box (`https://uXXXXX.your-storagebox.de`), Nextcloud, ownCloud – sowie **lokale Ordner / USB-Laufwerke**. Geplant: SFTP, SSH (rsync), FTP(S), S3-kompatibler Object Storage (z. B. Hetzner Object Storage), BorgBackup. Details und Priorisierung siehe [docs/protocols.md](docs/protocols.md).

## Technik

- **Rust** als Sprache – Workspace mit getrennter Sync-Engine (`cloudsave-core`), GTK-Oberfläche (`cloudsave-app`) und Hintergrund-Dienst (`cloudsave-daemon`)
- **GTK 4 / libadwaita** für eine native, einfache Bedienung
- **Flatpak** (GNOME-SDK); Ordner-Zugriff über das Dokumenten-Portal statt pauschalem Home-Zugriff
- **SQLite** für den Sync-Status, **D-Bus** für die Kommunikation zwischen UI und Daemon

## Dokumentation

- [docs/plan.md](docs/plan.md) – Plan, Feature-Set, Architektur-Überblick, Meilensteine, Risiken
- [docs/protocols.md](docs/protocols.md) – Protokoll-Liste mit Priorisierung (Hetzner Storage Box im Fokus)
- [docs/architektur.md](docs/architektur.md) – Komponenten, Sync-Ablauf, Flatpak-Packaging, Sicherheit, Tests/CI
- [docs/ux-flow.md](docs/ux-flow.md) – flacher Einrichtungs-Flow und Hauptansicht
- [docs/ci.md](docs/ci.md) – CI-Plan und -Umsetzung (Doku-Lint + Rust-CI laufen bereits grün)

## Status

Frühe Planungsphase. Der ausführliche Plan mit Feature-Set, Architektur, UX-Flow und Meilensteinen liegt in [docs/plan.md](docs/plan.md). CI-Grundgerüst (Phase 0–2, siehe [docs/ci.md](docs/ci.md)) ist eingerichtet und grün; das M0-Skelett (Workspace `crates/cloudsave-core` + `crates/cloudsave-app`) baut in CI ein Flatpak-Bundle (`flatpak/org.cloudsave.Cloudsave.yml`, GNOME 50).
