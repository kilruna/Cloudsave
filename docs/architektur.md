# Tatsächlicher Implementierungsstand

Die sechs Roadmap-Erweiterungen sind im Core und in der GTK-App umgesetzt;
Details und verbleibende Aufgaben: [roadmap-status.md](roadmap-status.md).
Der nachfolgende Architekturentwurf bleibt das Zielbild für die weiteren Phasen.

Aktuell läuft der Sync in Workern der GTK-App. `ChangeWatcher` registriert notify-
Watcher in einem eigenen Thread, bündelt Ereignisse und ignoriert App-State/Ziele.
Ein periodischer Scan bleibt aktiv. Zugangsdaten liegen im Secret Service;
Legacy-Passwortdateien werden nach erfolgreicher Übernahme entfernt.

Lokale Uploads verwenden exklusive Tempdateien, Größenprüfung, fsync und Rename;
WebDAV verwendet exklusiven temporären PUT und MOVE. Der Backup-Modus erhält lokal
entfernte Dateien; Spiegelung löscht nur nach vollständigem erfolgreichem Quellscan.
Ausschlüsse werden auch vor Löschentscheidungen beachtet.

`Backend::list` und `Backend::download` ermöglichen Wiederherstellung von lokalen
und WebDAV-Zielen. DAV-Listen werden rekursiv mit Depth 1 innerhalb desselben
Origins/Zielpfads eingelesen. Downloads verwenden Tempdateien und
`persist_noclobber`; relative Pfade und Symlink-Eltern werden geprüft.

Die Flatpak erhält `home:ro` für die tatsächliche Home-Ordnerliste und Leserechte
für ausgewählte Quellen; zusätzliche Schreibziele kommen über den Datei-Dialog.
Ein separater D-Bus-Daemon, rsync/SFTP/S3 und genauer Transferfortschritt sind geplant.

---

# Architektur

## Komponenten-Übersicht

```
┌────────────────────────────┐      D-Bus (zbus)      ┌──────────────────────────────┐
│  cloudsave-gui             │ ◄────────────────────► │  cloudsave-daemon            │
│  GTK 4 + libadwaita        │   Status, Kommandos    │  systemd-User-Service /      │
│  (Flatpak-App)             │                        │  D-Bus-aktivierbar           │
└────────────────────────────┘                        │  ├─ inotify-Watcher (notify) │
                                                      │  ├─ Sync-Queue + Worker-Pool │
                                                      │  ├─ State-DB (SQLite)        │
                                                      │  └─ Remote-Backends          │
                                                      └──────────┬───────────────────┘
                                                                 ▼
                                        SFTP / WebDAV / rsync / S3 / lokal / …
```

- **`cloudsave-core` (Library):** Protokoll-Abstraktion (`RemoteBackend`-Trait), Datei-Diff gegen den State, Hashing, Konfig-Parsing. Unabhängig von UI und D-Bus, dadurch gut testbar.
- **`cloudsave-daemon`:** Hintergrundprozess. Nimmt Datei-Events entgegen, pflegt die State-DB und führt Uploads/Downloads über das passende Backend aus. Läuft als User-Service und schläft zwischen Ereignissen.
- **`cloudsave-gui`:** GTK-4-/libadwaita-Oberfläche. Reine Anzeige + Kommandos an den Daemon, keine eigene Sync-Logik.
- **`cloudsave-cli` (später):** Gleichen Core für headless Nutzung.

Wichtige Crates: `gtk4`, `libadwaita`, `ashpd` (Portale), `zbus` (D-Bus), `notify` + `notify-debouncer-full` (inotify), `rusqlite` (State), `blake3` (Hashes), `russh` (SFTP), `reqwest` (WebDAV/S3), `serde`/`toml` (Konfig), `tracing` (Logging), `tokio` (Async-Runtime).

## Änderungserkennung (ressourcenschonend)

1. **inotify rekursiv** über alle gewählten Ordner (`notify`-Crate) – ereignisgesteuert, praktisch ohne CPU-Verbrauch im Leerlauf.
2. **Debounce:** Events werden gesammelt (z. B. 3–5 s), dann als Batch verarbeitet (`notify-debouncer-full`). Spart häufiges Anlaufen bei Speichervorgängen.
3. **Diff gegen State:** Vergleich mit SQLite-State (Pfad, mtime, Größe). Bei mtime+größe gleich: optional BLAKE3-Hash, um „angefasste, aber unveränderte" Dateien zu überspringen.
4. **Fallback-Scan:** Ein seltener, leichter Scan (default alle 15 Min., konfigurierbar/abschaltbar) vergleicht mtime/Größe und fängt verlorene Events ab (z. B. inotify-Queue-Überlauf, Änderungen außerhalb der App). **Kein dauerhaftes Polling im Normalbetrieb.**
5. Hinweis: Über das Document Portal exponierte Pfade (`/run/user/$UID/doc/…`) sind FUSE-Mounts; inotify-Events darüber sind nicht garantiert. Deshalb ist der Fallback-Scan kein Optional-Feature, und in den Einstellungen gibt es optional „Zugriff auf gesamtes Home-Verzeichnis" für zuverlässiges inotify.

## Sync-Ablauf

1. Event/Scan markiert Datei im State als „dirty" (neu/geändert/gelöscht).
2. Worker-Pool (begrenzte Parallelität, default 3 – Hetzner erlaubt max. 10 gleichzeitige Verbindungen) nimmt die Queue ab.
3. Pro Datei: Remote-Stat (Größe/mtime) vergleichen → bei Abweichung Upload in temporären Namen, danach atomares Umbenennen (abgebrochene Uploads landen nicht als kaputte Datei).
4. Erfolg → State aktualisieren; Fehler → Retry mit exponential Backoff, danach Klartext-Fehler in UI + Benachrichtigung.
5. Lokal gelöschte Dateien werden auf dem Ziel gelöscht (Spiegelsemantik im MVP).
6. Uploads streamen (kein Einlesen kompletter Dateien in den RAM); optional Bandbreitenlimit.

## State & Konfiguration

- **Konfig:** TOML in `~/.var/app/org.cloudsave.Cloudsave/config/` (Ordner, Ziel, Intervalle, Flags).
- **State:** SQLite in `~/.var/app/org.cloudsave.Cloudsave/data/` – pro Datei: Pfad, mtime, Größe, optionaler Hash, Sync-Status, Remote-Informationen. Indiziert für schnelle Scans.
- **Zugangsdaten:** Nie in Klartext-Dateien, sondern im Secret Service (org.freedesktop.secrets, via ashpd). SSH-Keys als Keyfile-Pfad oder über ssh-agent.

## Flatpak-Packaging

- **Runtime/SDK:** `org.gnome.Platform` / `org.gnome.Sdk` (GNOME 47), Rust über `sdk-extensions: org.freedesktop.Sdk.Extension.rust-stable`. Rust-Crates werden mit `cargo vendor` + `flatpak-cargo-generator` vendored.
- **App-ID:** `org.cloudsave.Cloudsave`, Command `cloudsave`.
- **finish-args (so restriktiv wie möglich – kein `--filesystem=home`):** Portale statt breiter Rechte:

```json
"finish-args": [
  "--socket=session-bus",
  "--filesystem=xdg-run/doc",
  "--talk-name=org.freedesktop.portal.FileChooser",
  "--talk-name=org.freedesktop.portal.Documents",
  "--talk-name=org.freedesktop.portal.Notification",
  "--talk-name=org.freedesktop.Notifications",
  "--talk-name=org.freedesktop.secrets",
  "--talk-name=org.freedesktop.systemd1"
]
```

- **Ordnerzugriff über das Document Portal:** Beim Auswählen wird `portal.Documents.AddFull(..., persistent=true)` aufgerufen. Die gewählten Ordner sind dann persistent unter `/run/user/$UID/doc/…` erreichbar – auch nach Neustart, ohne pauschalen Home-Zugriff.
- **Hintergrunddienst:** D-Bus-aktivierbarer Service (`org.cloudsave.Cloudsave.service` mit `Exec=cloudsave-daemon`), Autostart der GUI über XDG-Autostart-Eintrag in der Sandbox; der Daemon startet bedarfsgesteuert bzw. beim Login.
- **Veröffentlichung:** Flathub (Build + Review über flatpak-builder in CI).

## Ressourcenschonung (Zusammenfassung)

- Ereignisgesteuert (inotify) statt Polling; Daemon ist zwischen Events idle (tokio, keine Busy-Loops).
- Debounce + Batch-Verarbeitung; begrenzte Upload-Parallelität; Backoff bei Fehlern (kein Dauer-Retry-Sturm).
- Pause bei Akkubetrieb (< 20 %, UPower über D-Bus) und bei getakteter Verbindung (NetworkManager) – abschaltbar.
- Niedrige I/O-Priorität (ionice idle), Streaming-Uploads, indizierte SQLite-State.

## Sicherheit

- Zugangsdaten nur im Secret Service; optional SSH-Key statt Passwort.
- TLS (1.2+) für WebDAV/S3 mit Zertifikatsprüfung (optionale eigene CA).
- Keine Telemetrie, keine Analyse-Tracker; Logging nur lokal (journald via `tracing`).
- Flatpak-Sandbox mit Portalen; Prinzip der geringsten Rechte.

## Tests & CI

- **Unit-Tests:** State/Diff, Hash-Fälle, Konfig-Parsing, Backend-Mocks.
- **Integrationstests:** lokales Ziel (schnell), WebDAV-Testserver und SFTP via Docker in CI.
- **CI (GitHub Actions):** `cargo fmt --check`, `cargo clippy`, `cargo test`, danach Flatpak-Build mit `flatpak-builder` + Smoke-Test (`cloudsave --version`, Daemon startet über D-Bus).
- **Manuelle QA:** Checkliste Einrichtung → Änderung → Sync → Restore auf frischer VM/Container.

## Umgesetzt (M1-Teil: Sync-Engine + lokales Ziel)

`cloudsave-core` enthält die lauffähige Engine: `Config` (TOML unter `~/.config/cloudsave/config.toml`), `State` (SQLite, `~/.local/state/cloudsave/state.db`: path/size/mtime/hash/synced_at), `Backend`-Trait mit `LocalBackend` (spiegelt `<Ziel>/<Ordnername>/<relativer Pfad>`) und `engine::sync()` (Scan via walkdir → Diff gegen State über size+mtime → Upload mit BLAKE3-Hash im Stream → Löschen von lokal entfernten Dateien; Report mit uploaded/deleted/skipped/bytes/errors). Erster manueller Sync über `cloudsave-cli sync [CONFIG]` (`make sync`). Integrationstests in `crates/cloudsave-core/tests/sync.rs` (Roundtrip, Delta, Delete, fehlender Ordner).
