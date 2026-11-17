# UX-Flow: Einrichtung, Sicherung und Wiederherstellung

Cloudsave bleibt ein Ein-Wege-Backup: ausgewählte Quellen werden an ein oder
mehrere Ziele gesichert. Die Wiederherstellung ist eine ausdrückliche Aktion.

## Einrichtung

1. **Willkommen** → **Einrichtung starten**.
2. **Ordner wählen** zeigt sichtbare, tatsächlich vorhandene direkte Home-Unterordner
   alphabetisch ohne automatische Vorauswahl. Dateien und versteckte Ordner sind
   ausgeblendet. Andere Ordner können über den Datei-Dialog hinzugefügt werden;
   doppelte Pfade werden nicht doppelt aufgenommen. Ohne Auswahl bleibt Weiter aus.
3. **Ziel wählen**: lokaler Ordner per wiederverwendbarem Auswahlbutton oder WebDAV
   mit syntaktisch geprüfter HTTPS-URL, Benutzername und Passwort. Verbindungstests
   sind separat möglich. Für Hetzner-Subaccounts gehören Benutzername und Domain
   des Subaccounts zusammen, z. B. `uXXXXX-sub1.your-storagebox.de`.
4. **Fertig** zeigt die Auswahl. **Sync einrichten & starten** speichert Zugangsdaten
   im System-Schlüsselbund und prüft zuerst den Endpunkt auf vorhandene Sicherungen.
   Bei einem leeren Ziel startet der erste Sync. Vorhandene Daten öffnen stattdessen
   den Wiederherstellungsassistenten. D-Bus und Netzwerk laufen im Worker; Fehler
   halten die Einrichtung offen und sind sichtbar.

Neue Profile beginnen im Backup-Modus ohne Löschweitergabe. Abbrechen eines
Datei-Dialogs erhält die vorherige gültige Auswahl.

## Dashboard

- **Jetzt synchronisieren** bleibt auch bei ausgeschaltetem Auto-Sync verfügbar.
- Status zeigt Erfolg oder vollständige kopierbare Fehler mit Quell-/Zielpfaden
  und Wiederholen-Hinweis. Bei Fehlern erscheint kein voller Erfolgsbalken.
- Ordner lassen sich einzeln abwählen oder über den Dialog ergänzen. Abwählen oder
  ein nicht lesbarer Quellbaum löscht keine vorhandenen Sicherungen.
- Mehrere lokale/WebDAV-Endpunkte können hinzugefügt werden.
- Auto-Sync erkennt Dateiänderungen nach drei Sekunden Ruhezeit. Der Zeitplan
  bleibt Scan-Fallback und versucht fehlgeschlagene Beobachtungen erneut.
- Einstellungen enthalten Intervall, Ausschlussmuster, Backup-/Spiegelungsmodus,
  Autostart, Hintergrundbetrieb, Verbindungstest und Beenden.
- Ausschlussmuster werden vor dem Speichern geprüft. Bestehende ausgeschlossene
  Sicherungen bleiben auch bei Spiegelung erhalten.
- Spiegelung ist ausdrücklich beschriftet: absichtlich lokal gelöschte Dateien
  werden im Ziel entfernt. Bestehende Profile behalten ihre bisherige Einstellung.

Lokale Quellen und Ziele dürfen nicht ineinander liegen. Temporäre Dateien,
App-State und lokale Zieländerungen erzeugen keine Beobachtungsschleifen.

## Wiederherstellung

Auf einem neuen PC führt **Vorhandene Sicherung wiederherstellen** vom Start direkt
zur Anmeldung am Endpunkt. Es müssen keine lokalen Quellordner existieren.

1. Vorhandene Dateien und gespeicherte Ordnerzuordnung erkennen.
2. **An die bisherigen Orte wiederherstellen** oder **In einen separaten Ordner
   wiederherstellen** wählen. Ganze Ordner sind vorausgewählt; die Zielpfade sind
   sichtbar und einzeln über einen Dialog änderbar.
3. Das Flatpak lässt den Home-Ordner für Schreibzugriff über den nativen Dialog
   bestätigen. Externe oder unbekannte Orte brauchen eine ausdrückliche Auswahl.
   Eine separate Ausgabe erfordert immer die Auswahl eines Ausgabeordners.
4. **Ausgewählte Ordner wiederherstellen** lädt alle Dateien der gewählten Ordner.
   Die ursprünglichen Home-Pfade stammen aus der portablen Ordnerzuordnung.
   Bei alten Sicherungen ohne Zuordnung werden gleichnamige gewählte Quellen oder
   Home-Unterordner vorgeschlagen; die Oberfläche kennzeichnet diese Vorschläge.
5. Nach vollständiger Wiederherstellung folgt **Zur Übersicht**. Originalordner
   werden als Quellen übernommen; ihre Servernamen bleiben auch bei lokal anders
   benannten Zielordnern stabil. Erfolgreiche Downloads werden im Sync-Status
   erfasst, damit der nächste Lauf sie nicht sofort erneut hochlädt.

Während einer offenen Wiederherstellung blockiert auch die Engine den Sync;
ein App-Neustart kehrt in den Assistenten zurück. Fehler bleiben wiederholbar.
Vorhandene lokale Dateien werden behalten und gezählt. Bei Konflikten, separater
Ausgabe oder bewusstem Überspringen bleibt Auto-Sync aus. Eine vollständige
Wiederherstellung an die bisherigen Orte setzt Live-Sync im Backup-Modus fort.
Spiegelung wird dabei nicht automatisch wieder eingeschaltet.

### Einzelne Dateien

**Sicherungen wiederherstellen** öffnet eine eigene Seite mit Zurück-Navigation:

1. Endpunkt wählen und **Sicherungen laden**.
2. Gewünschte Dateien mit relativen Pfaden und Größen markieren.
3. **Wiederherstellungsordner wählen** über den nativen Datei-Dialog.
4. **Wiederherstellen** lädt die Dateien im Worker unter Erhalt der Ordnerstruktur.

Dateien werden zunächst temporär geschrieben und erst nach vollständigem Download
übernommen. Vorhandene Dateien werden nicht überschrieben; unsichere Pfade und
Symlinks in Zielpfaden werden zurückgewiesen. Fehler nennen die betroffenen Dateien.

## Systemabschnitt

Header und Tray zeichnen dieselbe orange Wolke. Das Tray liefert ARGB-Bilddaten in
vier Größen; es benötigt keinen Zugriff des Panels auf Flatpak-Icondateien.
Ein normaler Klick öffnet über StatusNotifierItem/DBusMenu auf kompatiblen Panels
das Menü. Es enthält aktive Aufträge mit Phase oder Dateipfad, die letzten acht
Aufträge der Sitzung mit Uhrzeit und Ergebnis sowie **Cloudsave öffnen** und
**Beenden**. Die Dateizähler bezeichnen geprüfte Dateien, keine geschätzte
Byte-Queue. Upload-/Downloadphasen stammen direkt aus den Dateioperationen.
Aufträge werden seriell ausgeführt; der Tray erzeugt keinen zweiten Sync-Worker.

## Fenster und Testgrenzen

Nur die äußere Anwendungs-Titelleiste enthält Fensterbuttons. Seitentitel und
Zurück-Navigation bleiben erhalten. Die native GTK-Suite verwendet echte Widgets,
Dateien und lokale Ziele unter Xvfb. Reale Flatpak-Portal-Freigaben und native
Wayland-Fensterbedienung benötigen weiterhin eine manuelle Abnahme.

## Noch geplant

Genauer Transferfortschritt in Dateien/Bytes, Abbrechen, Desktop-Benachrichtigungen,
Bandbreitenlimit und Pausen nach Akku-/Netzwerkzustand sind noch nicht Teil der UI.
