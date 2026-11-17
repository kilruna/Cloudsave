# Cloudsave

Cloudsave ist eine Flatpak-Desktop-App für Linux, mit der du ausgewählte Ordner automatisch auf lokale Laufwerke oder ein WebDAV-Ziel sicherst, etwa eine **Hetzner Storage Box** oder Nextcloud.

**Die Idee in einem Satz:** Ordner anhaken → Ziel wählen → fertig. Cloudsave erkennt selbstständig, welche Dateien sich geändert haben, und lädt nur diese hoch – ressourcenschonend im Hintergrund.

## Schnell testen und Feedback geben

Cloudsave ist eine frühe Testversion. Verwende für den ersten Versuch einen kleinen
Testordner und ein getrenntes Sicherungsziel. Prüfe unter **Einstellungen**, dass
der Modus **Backup (keine Löschungen)** gewählt ist: Neue Profile verwenden ihn
automatisch, bestehende Profile können weiterhin auf Spiegelung stehen.

### Installieren oder aktualisieren

Voraussetzungen: Linux mit grafischer Sitzung, Flatpak und für WebDAV ein
verfügbarer System-Schlüsselbund (GNOME Keyring oder KWallet mit Secret Service).

1. Öffne die [Flatpak-Builds auf GitHub](https://github.com/kilruna/Cloudsave/actions/workflows/flatpak.yml?query=branch%3Avibe%2Fcloudsave-plan-1926b5) und wähle den neuesten **erfolgreichen** Lauf des Hauptbranches `vibe/cloudsave-plan-1926b5`.
2. Lade unten unter **Artifacts** das Paket **cloudsave.flatpak** herunter. Dafür ist eine GitHub-Anmeldung erforderlich. Die Pakete bleiben 14 Tage verfügbar.
3. Entpacke das ZIP und öffne ein Terminal im Ordner mit der Datei `cloudsave.flatpak`. Führe dort aus:

```bash
flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install --user --reinstall ./cloudsave.flatpak
flatpak run org.cloudsave.Cloudsave
```

Eine bereits laufende Instanz vorher über **Einstellungen → App beenden** schließen.
Dieses Update erhält die vorhandene Konfiguration. Die Installation zeigt die
angeforderten Berechtigungen an; Home wird für Ordnerliste und Quellen lesbar gemacht.

### Kurzer Funktionstest

1. **Einrichten:** Einen Ordner mit einigen Testdateien auswählen und einen anderen lokalen Ordner als Sicherungsziel wählen. Das Ziel darf nicht innerhalb der Quelle liegen. **Sync einrichten & starten** klicken und die gesicherten Dateiinhalte vergleichen.
2. **Automatik:** Eine Testdatei ändern und die automatische Übertragung prüfen. Normalerweise startet sie nach etwa drei Sekunden Ruhezeit; bei eingeschränkter Dateisystem-/Portal-Erkennung greift das eingestellte Scan-Intervall. Auto-Sync ausschalten: Weitere Änderungen dürfen erst nach **Jetzt synchronisieren** übertragen werden.
3. **Ausschlüsse:** Unter Einstellungen `*.tmp` eintragen und **Ausschlüsse speichern** klicken. Eine neue `.tmp`-Datei in der Quelle anlegen und **Jetzt synchronisieren** klicken. Diese Datei darf nicht im Ziel erscheinen.
4. **Backup-Modus:** Eine bereits gesicherte Testdatei lokal löschen und synchronisieren. Im Backup-Modus bleibt ihre Sicherung erhalten. Den Spiegelungsmodus ausschließlich mit Testdaten ausprobieren; er gibt echte Dateilöschungen an das Ziel weiter.
5. **Wiederherstellen:** **Sicherungen wiederherstellen → Sicherungen laden**, Dateien markieren und einen dritten, leeren Zielordner wählen. Inhalte vergleichen. Beim erneuten Wiederherstellen bleiben vorhandene Dateien unverändert.
6. **Optional WebDAV:** Mit einem Testverzeichnis auf dem Server wiederholen. Nach **App beenden** und Neustart müssen gespeicherte Zugangsdaten über den Schlüsselbund verfügbar sein; dieser muss gegebenenfalls entsperrt werden. Bei Hetzner müssen WebDAV und äußere Erreichbarkeit aktiviert sein. Subaccounts benötigen ihre eigene Domain, z. B. `https://uXXXXX-sub1.your-storagebox.de` zum Benutzer `uXXXXX-sub1`.

### Feedback melden

Erstelle ein [GitHub-Issue](https://github.com/kilruna/Cloudsave/issues/new), auch für
unklare Bedienung oder Darstellungsprobleme. Diese Angaben helfen beim Nachstellen:

```text
Build: Link zum verwendeten GitHub-Actions-Lauf
System: Distribution/Version, Desktop (GNOME/KDE/…), Wayland oder X11
Sicherung: lokales Ziel oder WebDAV; Backup- oder Spiegelungsmodus
Schritte: Was wurde nacheinander angeklickt/geändert?
Erwartet: Was sollte passieren?
Tatsächlich: Was ist passiert? Genaue Fehlermeldung/Screenshot ergänzen.
```

Installationsdetails liefert `flatpak info org.cloudsave.Cloudsave`. Für zusätzliche
Terminalmeldungen die App zuerst über **App beenden** schließen und dann mit
`flatpak run org.cloudsave.Cloudsave` im Terminal starten. Passwörter, Tokens und
private Pfade in Meldungen oder Screenshots bitte schwärzen.

## Highlights

- **Einfache Einrichtung:** Flacher Flow in drei Schritten, keine verschachtelten Menüs – vom Start bis zum eingerichteten Sync.
- **Mehrere Quellordner:** Beliebig viele Verzeichnisse pro Ziel – per Datei-Auswahl hinzufügen und jederzeit per Häkchen an- bzw. abwählen.
- **Automatische Änderungserkennung:** Kombination aus System-Ereignissen (inotify) und gelegentlichem Scan – kein dauerhaftes Durchsuchen der Platte im Hintergrund.
- **Delta-Sync:** Nur geänderte Dateien werden übertragen, nicht der ganze Ordner.
- **Sichere Zugangsdaten:** Passwörter liegen im System-Schlüsselbund. Alte Passwortdateien werden nach erfolgreicher Übernahme entfernt.
- **Wiederherstellung nach Neuinstallation:** Auf dem Startbildschirm **Vorhandene Sicherung wiederherstellen** wählen, das bisherige lokale Ziel oder die WebDAV-Zugangsdaten angeben und den Endpunkt prüfen lassen. Gefundene Sicherungsdateien lassen sich samt Ordnerstruktur in einen gewählten lokalen Ordner zurückholen. Dafür ist keine alte Cloudsave-Konfiguration oder Zustandsdatenbank nötig.
- **Autostart & Hintergrund:** Optional startet Cloudsave minimiert beim Systemstart (`--background`). Mit aktivierter Hintergrundoption verbirgt **×** das Fenster; das Symbol im Systemabschnitt bleibt auch bei geöffnetem Fenster verfügbar. Ein Klick oder **Cloudsave öffnen** im Symbolmenü öffnet dasselbe Fenster wieder. **Beenden** im Symbolmenü oder **App beenden** in den Einstellungen beendet die App vollständig. Ohne Hintergrundoption beendet bereits **×** die App.
- **Ressourcenschonend:** Datei-Ereignisse lösen gebündelte Syncs aus; ein geplanter Scan dient als Fallback. Übertragungen laufen außerhalb des UI-Threads.

Das Symbol verwendet den StatusNotifierItem-Standard, den etwa KDE Plasma
unterstützt. Bei GNOME ist dafür eine AppIndicator-/StatusNotifier-Erweiterung
erforderlich. Ohne passenden Systemabschnitt lässt sich ein verborgenes Fenster
über den App-Starter erneut öffnen.

Bei jeder Endpunkt-Einrichtung prüft Cloudsave vor dem ersten Sync, ob bereits
Sicherungsdateien vorhanden sind, und bietet gegebenenfalls die Wiederherstellung
an. Bei Zugriffsfehlern bleibt die Einrichtung offen und kann erneut versucht
werden. Vorhandene lokale Dateien werden beim Wiederherstellen nicht
überschrieben. Wähle etwa dein Home-Verzeichnis als Wiederherstellungsordner,
damit `Documents/datei.txt` wieder unter `~/Documents/datei.txt` landet.
Zurückgeholte Ordner werden als Quellen eingetragen; Auto-Sync bleibt zunächst
ausgeschaltet. Prüfe die Dateien und aktiviere ihn anschließend nach Bedarf.
Die Wiederherstellung umfasst die gesicherten Dateien und deren Ordnerstruktur;
frühere App-Einstellungen werden dabei nicht aus dem Sicherungsziel rekonstruiert.

## Ziele (Protokolle)

Verfügbar: **WebDAV (HTTPS)** – z. B. Hetzner Storage Box (`https://uXXXXX.your-storagebox.de`), Nextcloud, ownCloud – sowie **lokale Ordner / USB-Laufwerke**. Geplant: SFTP, SSH (rsync), FTP(S), S3-kompatibler Object Storage (z. B. Hetzner Object Storage), BorgBackup. Details und Priorisierung siehe [docs/protocols.md](docs/protocols.md).

Bei der WebDAV-Einrichtung wird eine Serveradresse ohne Protokoll automatisch um
`https://` ergänzt. Die URL wird vor dem Fortfahren geprüft; ausdrücklich
angegebenes HTTP oder andere Protokolle werden abgewiesen. Ein Verbindungstest
ist optional.

## Technik

- **Rust** als Sprache – Workspace mit Sync-Engine (`cloudsave-core`), GTK-Oberfläche (`cloudsave-app`) und CLI (`cloudsave-cli`)
- **GTK 4 / libadwaita** für eine native, einfache Bedienung
- **Flatpak** (GNOME-SDK); Home-Lesezugriff für die Ordnerliste und Quellen, zusätzliche Ordner und Schreibziele über das Dokumenten-Portal
- **SQLite** für den Sync-Status, **D-Bus/Secret Service** für den System-Schlüsselbund

## Dokumentation

- [docs/plan.md](docs/plan.md) – Plan, Feature-Set, Architektur-Überblick, Meilensteine, Risiken
- [docs/protocols.md](docs/protocols.md) – Protokoll-Liste mit Priorisierung (Hetzner Storage Box im Fokus)
- [docs/architektur.md](docs/architektur.md) – Komponenten, Sync-Ablauf, Flatpak-Packaging, Sicherheit, Tests/CI
- [docs/ux-flow.md](docs/ux-flow.md) – flacher Einrichtungs-Flow und Hauptansicht
- [docs/ci.md](docs/ci.md) – CI-Plan und -Umsetzung (Doku-Lint + Rust-CI laufen bereits grün)

## Status

Die App unterstützt lokale Sicherungsziele und WebDAV (z. B. Hetzner Storage Box),
mehrere Endpunkte, echte Home-Ordnerauswahl, Zugangsdaten im System-Schlüsselbund,
Backup-/Spiegelungsmodus, Ausschlussmuster, automatische Änderungserkennung und
Wiederherstellung einzelner Dateien. Uploads werden vor dem Ersetzen einer alten
Sicherung auf einen temporären Pfad geschrieben.

Neue Profile verwenden den Backup-Modus ohne Löschweitergabe. Vorhandene Profile
behalten ihre bisherige Spiegelung; der Modus ist unter Einstellungen umstellbar.
Beim Wiederherstellen bleiben vorhandene lokale Dateien erhalten.

[Bedienung und aktueller Roadmap-Stand](docs/roadmap-status.md) ·
[GTK-Regressionstests](tests/ui/README.md)
