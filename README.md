# Cloudsave

Cloudsave ist eine Flatpak-Desktop-App für Linux, mit der du ausgewählte Ordner automatisch auf lokale Laufwerke oder ein WebDAV-Ziel sicherst, etwa eine **Hetzner Storage Box** oder Nextcloud.

**[Cloudsave für Linux herunterladen (.flatpak, x86_64)](https://github.com/kilruna/Cloudsave/releases/latest/download/cloudsave-x86_64.flatpak)**

**Die Idee in einem Satz:** Ordner anhaken → Ziel wählen → fertig. Cloudsave erkennt selbstständig, welche Dateien sich geändert haben, und lädt nur diese hoch – ressourcenschonend im Hintergrund.

## Schnell testen und Feedback geben

Cloudsave ist eine frühe Testversion. Verwende für den ersten Versuch einen kleinen
Testordner und ein getrenntes Sicherungsziel. Prüfe unter **Einstellungen**, dass
der Modus **Backup (keine Löschungen)** gewählt ist: Neue Profile verwenden ihn
automatisch, bestehende Profile können weiterhin auf Spiegelung stehen.

### Installieren oder aktualisieren

Voraussetzungen: Linux mit grafischer Sitzung, Flatpak und für WebDAV ein
verfügbarer System-Schlüsselbund (GNOME Keyring oder KWallet mit Secret Service).

Lade `cloudsave-x86_64.flatpak` über den Downloadlink oben herunter. Auf der
[Release-Seite](https://github.com/kilruna/Cloudsave/releases/latest) findest du
die Versionshinweise und die SHA-256-Prüfsumme.
Öffne ein Terminal im Downloadordner und führe diese Befehle aus.

```bash
flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install --user --reinstall ./cloudsave-x86_64.flatpak
flatpak run org.cloudsave.Cloudsave
```

Eine bereits laufende Instanz vorher über **Einstellungen → App beenden** schließen.
Dieses Update erhält die vorhandene Konfiguration. Die Installation zeigt die
angeforderten Berechtigungen an; Home wird für Ordnerliste und Quellen lesbar gemacht.

Unter **Gesicherte Ordner** pausiert das Häkchen die Sicherung eines Ordners.
Der Minus-Button entfernt ihn aus der Liste. Bei aktivem Live-Sync entfernt
Cloudsave nach Bestätigung auch die zugeordneten Kopien an den Endpunkten.
Die lokalen Originale bleiben erhalten.
Der Minus-Button auf einer Zielkarte beendet die Sicherung an diesem Endpunkt.
Vorhandene Sicherungskopien am Ziel bleiben erhalten.

### Kurzer Funktionstest

1. **Einrichten:** Einen Ordner mit einigen Testdateien auswählen und einen anderen lokalen Ordner als Sicherungsziel wählen. Das Ziel darf nicht innerhalb der Quelle liegen. **Sync einrichten & starten** klicken und die gesicherten Dateiinhalte vergleichen.
2. **Automatik:** Eine Testdatei ändern und die automatische Übertragung prüfen. Live-Sync startet normalerweise nach etwa drei Sekunden Ruhezeit. **Zusätzlicher Kontrollscan (Minuten)** fängt verpasste Dateisystemereignisse auf. Auto-Sync ausschalten: Weitere Änderungen dürfen erst nach **Jetzt synchronisieren** übertragen werden.
3. **Ausschlüsse:** Unter Einstellungen **Dateien und Ordner ausschließen** öffnen und **Temporäre Dateien** anklicken; die Auswahl wird sofort gespeichert. Eine neue `.tmp`-Datei in der Quelle anlegen und **Jetzt synchronisieren** klicken. Diese Datei darf nicht im Ziel erscheinen. Weitere Gruppen erfassen Cache-Ordner, temporäre Ordner, System-Vorschaudateien und Entwicklungs-Caches. **Ordner auswählen …** übernimmt einen Unterordner aus einer Quelle; **Eigene Regel hinzufügen …** öffnet ein kleines Eingabefenster, etwa für `*.bak` oder `Projekte/Cache/**`. Regeln gelten für alle Quellen; vorhandene ausgeschlossene Sicherungen bleiben erhalten. Ohne Auswahl werden keine Standardregeln automatisch aktiviert.
4. **Backup-Modus:** Eine bereits gesicherte Testdatei lokal löschen und synchronisieren. Im Backup-Modus bleibt ihre Sicherung erhalten. Den Spiegelungsmodus ausschließlich mit Testdaten ausprobieren; er gibt echte Dateilöschungen an das Ziel weiter.
5. **Wiederherstellen:** Nach einer Neuinstallation auf dem Startbildschirm **Vorhandene Sicherung wiederherstellen** wählen und den Endpunkt verbinden. Auch die normale Einrichtung prüft den Endpunkt vor dem ersten Sync. Der Assistent bietet ganze Ordner **an den bisherigen Orten** oder **in einem separaten Ordner** an; einzelne Dateien müssen dafür nicht ausgewählt werden. Zielpfade prüfen und Wiederherstellung starten. Im Flatpak den Home-Ordner bei Bedarf einmal für Schreibzugriff über den Dialog bestätigen. Für einzelne Dateien im Dashboard **Wiederherstellen** öffnen und bei Bedarf nach Dateiname oder Pfad suchen.
6. **Optional WebDAV:** Mit einem Testverzeichnis auf dem Server wiederholen. Nach **App beenden** und Neustart müssen gespeicherte Zugangsdaten über den Schlüsselbund verfügbar sein; dieser muss gegebenenfalls entsperrt werden. Bei Hetzner müssen WebDAV und äußere Erreichbarkeit aktiviert sein. Subaccounts benötigen ihre eigene Domain, z. B. `https://uXXXXX-sub1.your-storagebox.de` zum Benutzer `uXXXXX-sub1`.

### Feedback melden

Erstelle ein [GitHub-Issue](https://github.com/kilruna/Cloudsave/issues/new), auch für
unklare Bedienung oder Darstellungsprobleme. Diese Angaben helfen beim Nachstellen:

```text
Version: Link zum verwendeten GitHub-Release
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
- **Autostart & Hintergrund:** Optional startet Cloudsave minimiert beim Systemstart (`--background`). Mit aktivierter Hintergrundoption verbirgt **×** das Fenster. Die orange Wolke im Systemabschnitt bleibt auch bei geöffnetem Fenster verfügbar. Ein normaler Klick öffnet auf kompatiblen Panels das Menü mit aktiven Uploads/Downloads und den letzten acht Jobs dieser Sitzung. **Cloudsave öffnen** zeigt dasselbe Fenster wieder; **Beenden** oder **App beenden** in den Einstellungen beendet die App vollständig. Ohne Hintergrundoption beendet bereits **×** die App.
- **Ressourcenschonend:** Datei-Ereignisse lösen gebündelte Syncs aus; ein geplanter Scan dient als Fallback. Übertragungen laufen außerhalb des UI-Threads.

Das Symbol verwendet den StatusNotifierItem-Standard, den etwa KDE Plasma
unterstützt. Bei GNOME ist dafür eine AppIndicator-/StatusNotifier-Erweiterung
erforderlich. Ohne passenden Systemabschnitt lässt sich ein verborgenes Fenster
über den App-Starter erneut öffnen.

## Ziele (Protokolle)

Verfügbar: **WebDAV (HTTPS)** – z. B. Hetzner Storage Box (`https://uXXXXX.your-storagebox.de`), Nextcloud, ownCloud – sowie **lokale Ordner / USB-Laufwerke**. Geplant: SFTP, SSH (rsync), FTP(S), S3-kompatibler Object Storage (z. B. Hetzner Object Storage), BorgBackup. Details und Priorisierung siehe [docs/protocols.md](docs/protocols.md).

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
Eine noch offene Wiederherstellung blockiert Sync auch nach einem Neustart.
Nach vollständiger Wiederherstellung an die bisherigen Orte setzt Cloudsave
Live-Sync im Backup-Modus fort. Bei vorhandenen lokalen Dateien oder einem
separaten Ausgabeordner bleibt die Automatik pausiert. Alte Sicherungen ohne
Ordnerzuordnung erhalten bestätigbare Vorschläge aus ihren obersten Ordnernamen;
externe oder unbekannte Zielpfade lassen sich im Dialog zuweisen.

[Bedienung und aktueller Roadmap-Stand](docs/roadmap-status.md) ·
[GTK-Regressionstests](tests/ui/README.md)
