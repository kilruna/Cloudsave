# Cloudsave – Plan

Flatpak-Desktop-App für Linux (Rust), die ausgewählte Ordner automatisch und ressourcenschonend auf ein Cloud-Ziel sichert – z. B. eine Hetzner Storage Box. Leitlinie: **absolute Userfreundlichkeit**, flacher Flow, keine verschachtelten Menüs.

## Aktueller Stand

Onboarding mit echten Home-Ordnern, lokale Ziele/WebDAV, mehrere Endpunkte,
System-Schlüsselbund, wählbarer Backup-/Spiegelungsmodus, abbruchsichere Uploads,
Ausschlussmuster, Änderungserkennung mit Debounce/Fallback und Wiederherstellung
sind implementiert. Details, Bedienung und verbleibende Aufgaben stehen im
[Umsetzungsstand](roadmap-status.md). Die folgenden Phasen beschreiben das Zielbild;
SFTP/rsync, S3, Benachrichtigungen, genauer Transferfortschritt und Akku-/Bandbreitenregeln
sind noch offen. Die installierte Flatpak benötigt weiterhin die manuelle Portal-/Wayland-Abnahme.

## 1. Vision & Design-Entscheidungen

- **„Ordner anhaken → Ziel wählen → fertig."** Die komplette Ersteinrichtung ist ein Flow aus max. 3 Schritten.
- Cloudsave ist ein **Ein-Wege-Backup** (lokal → remote) mit Restore-Funktion – keine bidirektionale Synchronisation wie bei Syncthing. Das hält Konflikte und UI einfach.
- **Ressourcenschonung als Architekturprinzip:** ereignisgesteuert (inotify) statt dauerhaft scannend; der Daemon schläft zwischen Syncs.
- **Sicherheit:** Zugangsdaten im Secret Service (libsecret), TLS überall, Lesezugriff auf Home für die Ordnerliste (`--filesystem=home:ro`); zusätzliche Ordner und Schreibziele über den Datei-Dialog.

## 2. Feature-Set (phasenweise)

### Kern-Features (direkt aus der Anforderung)

- **Ordner anhaken:** Beliebige Verzeichnisse aus dem Home-Bereich auswählen (Mehrfachauswahl mit Häkchen); im Hauptfenster ist jeder Ordner einzeln per Häkchen an- bzw. abwählbar.
- **Beliebiges Ziel:** Hetzner Storage Box (Voreinstellung), WebDAV/Nextcloud, S3-kompatibler Speicher, lokaler Ordner/externes Laufwerk – Details in [protocols.md](protocols.md).
- **Automatische Änderungserkennung:** Das System meldet Datei-Ereignisse (anlegen, ändern, löschen, verschieben); nach einer kurzen Ruhezeit wird genau das Delta nachgesynct.
- **Ressourcenschonend:** Ereignisgesteuert statt dauerhaft scannend – zwischen Syncs schläft der Dienst, es gibt kein Sekunden-Polling und optional ein Bandbreiten-Limit.

### Phase 1 – MVP

| ID | Feature |
|---|---|
| F1 | Ersteinrichtung: mehrere Quellordner per Datei-Auswahl (Portal, Mehrfachauswahl) hinzufügen, inkl. Standardvorschlägen (`~/Dokumente`, `~/Bilder`, `~/Schreibtisch`); Ordnerliste mit Häkchen zum An-/Abwählen |
| F2 | Ziel-Assistent mit Kacheln: **Hetzner Storage Box** (voreingestellt), lokaler Ordner/externes Laufwerk; Zugangsdaten sicher im Secret Service speichern |
| F3 | „Jetzt synchronisieren" + Fortschrittsanzeige + Benachrichtigung bei Abschluss/Fehler |
| F4 | SQLite-Zustands-DB: pro Datei letzter Stand (Pfad, Größe, mtime, Hash), letzter Sync, Fehlerlog |
| F5 | Einfacher Zeitplan (z. B. stündlich) als Fallback zur Ereigniserkennung |

### Phase 2

| ID | Feature |
|---|---|
| F6 | Automatische Änderungserkennung: inotify (`notify`-Crate) mit Debounce (z. B. 60 s Ruhezeit) + periodischer Vollscan (alle 6 h und beim Start) als Fallback |
| F7 | Weitere Backends: WebDAV, S3-kompatibel (siehe [protocols.md](protocols.md)) |
| F8 | Restore: Remote-Dateien ansehen und zurückspielen |
| F9 | Ausschlussmuster (z. B. `.cache`, `*.tmp`), Konfliktbehandlung (beide Versionen behalten) |
| F10 | Bandbreiten-Limit, Pausieren bei niedrigem Akku (UPower) |
| F11 | Nur-Uploads, keine Löschweitergabe (Backup-Sicherheit); optional später konfigurierbar |

### Phase 3

- Mehrere Ziele sind umgesetzt; getrennte Backup-Policies pro Ordner-Set sind noch geplant
- Clientseitige Verschlüsselung (z. B. `age`)
- BorgBackup-Backend (Dedup + Verschlüsselung)
- Übersetzungen, Tray-Icon, Flathub-Veröffentlichung

### Bewusst keine Ziele (anfangs)

Bidirektionale Synchronisation, Versionierung im Remote, Freigaben/Sharing, Mobile Apps.

## 3. Architektur (Rust-Workspace)

Details zu Komponenten, Sync-Ablauf, Flatpak-Packaging, Sicherheit und CI in [architektur.md](architektur.md).

```
cloudsave/ (Cargo-Workspace)
├── cloudsave-core/     # UI-unabhängige Sync-Engine (testbar)
│   ├── config          # serde + toml, XDG-konform (~/.config/cloudsave/config.toml)
│   ├── state           # rusqlite: ~/.local/state/cloudsave/state.db
│   ├── watch           # notify (inotify) + Debouncer
│   ├── scan            # walkdir: mtime+size-Vergleich, optional SHA-256
│   ├── backend         # Trait `Backend { list, upload, download, delete }`
│   │   ├── local       # std::fs
│   │   ├── rsync_ssh   # System-rsync über SSH (MVP für Storage Box, Port 23)
│   │   ├── webdav      # dav-client (Phase 2)
│   │   └── s3          # S3-API (Phase 2)
│   └── engine          # Scan → Diff → Upload-Queue (Concurrency-Limit, Retries, Progress-Events)
├── cloudsave-app/      # GTK 4 + libadwaita, einzelnes Hauptfenster + Onboarding
└── cloudsave-daemon/   # Hintergrundprozess, D-Bus-Service org.cloudsave.Cloudsave
```

- **Async-Runtime:** tokio (Timer, Netzwerk, Nebenläufigkeit der Uploads).
- **IPC UI ↔ Daemon:** D-Bus (zbus) auf dem Session-Bus – Methoden `SyncNow`, `GetStatus`; Signale `Progress`, `Finished`.
- **Konfiguration:** `~/.config/cloudsave/config.toml`; **Zustand:** `~/.local/state/cloudsave/state.db` (SQLite).
- **Änderungserkennung:** inotify rekursiv auf jedem Quellordner; Datei-Ereignisse (anlegen/ändern/löschen/verschieben) landen in einer Debounce-Warteschlange; danach **ein** Sync-Lauf. Fallback: Vollscan alle 6 h + beim Start (mtime+size gegen State-DB, Hash nur bei Mehrdeutigkeit). Kein Polling in kurzen Intervallen.
- **Protokoll-Abstraktion:** Alle Ziele implementieren denselben `Backend`-Trait – neue Protokolle sind reine Core-Erweiterungen, UI merkt nichts davon.

## 4. UX-Flow (Ersteinrichtung)

Ausführlicher Flow (Schritte, Hauptansicht, Zustände, Gestaltungsregeln) in [ux-flow.md](ux-flow.md).

1. **Willkommen:** „Welche Ordner möchtest du sichern?" → Datei-Auswahl (Dokumenten-Portal, Mehrfachauswahl per Häkchen, persistenter Zugriff wird vom Portal gespeichert).
2. **Ziel wählen:** Kacheln – Hetzner Storage Box (voreingestellt, Host `uXXXXX.your-storagebox.de`, Port 23), WebDAV, S3, lokaler Ordner → Zugangsdaten (im Secret Service gespeichert).
3. **Zusammenfassung + „Sync einrichten"** → erster Sync startet, Abschluss per Benachrichtigung.

Danach gibt es **ein einziges Hauptfenster**: Status, Ordnerliste (jeder Ordner mit Häkchen an-/abwählbar), Ziel, letzter Sync, großer „Jetzt synchronisieren"-Button. Alle Einstellungen sind flach erreichbar – keine verschachtelten Menüs.

## 5. Flatpak & Sandbox

- Manifest `org.cloudsave.Cloudsave.yml` gegen GNOME Platform / Freedesktop SDK, Rust über `org.freedesktop.Sdk.Extension.rust-stable` (oder cargo in den Sources).
- `finish-args`: `--share=network`, `--socket=session-bus`, `--talk-name=org.freedesktop.Notifications`, `--talk-name=org.freedesktop.secrets`, Portal-Zugriff (`org.freedesktop.portal.Documents` / FileChooser mit persist), `--filesystem=xdg-config/autostart` für den Autostart des Daemons.
- Aktuell `--filesystem=home:ro` für das Einlesen vorhandener Home-Ordner und ausgewählter Quellen. Zusätzliche Freigaben und lokale Schreibziele werden über das Dokumenten-Portal gewählt.
- SSH/rsync für die Storage Box werden im Bundle mitgeliefert; Credentials kommen aus dem Secret Service und gelangen per Askpass-Helfer an den SSH-Client – keine Klartext-Keys auf Platte.

## 6. Ressourcenschonung

- inotify statt Dauer-Scan; Debounce bündelt Datei-Ereignisse zu einem Sync-Lauf.
- Daemon schläft zwischen Ereignissen (epoll/tokio-Timer), kein Sekunden-Polling.
- Optionales Bandbreiten-Limit und CPU-Nice für Uploads; Pausieren bei niedrigem Akku.
- Nur geänderte Dateien werden übertragen (Delta), Uploads mit Retry und Fortschritts-Events.

## 7. Meilensteine

| Meilenstein | Inhalt |
|---|---|
| **M0** | Repo-Gerüst: Cargo-Workspace, CI (fmt, clippy `-D warnings`, test), Flatpak-Manifest baut, GTK-Hello-World, Doku, Lizenz |
| **M1 (MVP)** | F1–F5: Onboarding, Backends `local` + `rsync-ssh` (Storage Box), manueller Sync + stündlicher Timer, State-DB, Benachrichtigungen, Fortschritt in der UI |
| **M2** | F6–F11: inotify + Debounce, WebDAV/S3, Restore-Ansicht, Ausschlussmuster, Bandbreiten-Limit, Akku-Pause |
| **M3** | Phase-3-Features, Übersetzungen, Flathub |

## 8. Tests & Qualität

- Unit-Tests in `cloudsave-core` (Diff/Scan/State/Debounce) mit `tempfile`.
- Integrationstest: Roundtrip gegen das lokale Backend (upload → ändern → nur Delta → restore).
- Manueller Test gegen eine echte Hetzner Storage Box vor jedem Release.
- CI (GitHub Actions): `cargo fmt --check`, `clippy -- -D warnings`, `cargo test`; Flatpak-Build als eigener CI-Job (später).

## 9. Risiken

- Dokumenten-Portal: persistenter Ordner-Zugriff muss sauber funktionieren (sonst Fallback: `flatpak override` bzw. einmalige `--filesystem=home`-Freigabe).
- inotify-Limits bei sehr großen Bäumen → Fallback auf periodischen Scan.
- SSH-Credential-Handling im Sandbox (Askpass) muss gründlich getestet werden.
- Rate-Limits der Storage Box → Retries mit Backoff.

## 10. Protokolle

Die Liste und Priorisierung der unterstützten Protokolle (mit Hetzner-Storage-Box-Fokus) liegt in [protocols.md](protocols.md).
