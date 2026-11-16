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
   im System-Schlüsselbund und startet den ersten Sync. D-Bus-Zugriffe laufen im
   Worker; Fehler halten die Einrichtung offen und sind sichtbar.

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

**Sicherungen wiederherstellen** öffnet eine eigene Seite mit Zurück-Navigation:

1. Endpunkt wählen und **Sicherungen laden**.
2. Gewünschte Dateien mit relativen Pfaden und Größen markieren.
3. **Wiederherstellungsordner wählen** über den nativen Datei-Dialog.
4. **Wiederherstellen** lädt die Dateien im Worker unter Erhalt der Ordnerstruktur.

Dateien werden zunächst temporär geschrieben und erst nach vollständigem Download
übernommen. Vorhandene Dateien werden nicht überschrieben; unsichere Pfade und
Symlinks in Zielpfaden werden zurückgewiesen. Fehler nennen die betroffenen Dateien.

## Fenster und Testgrenzen

Nur die äußere Anwendungs-Titelleiste enthält Fensterbuttons. Seitentitel und
Zurück-Navigation bleiben erhalten. Die native GTK-Suite verwendet echte Widgets,
Dateien und lokale Ziele unter Xvfb. Reale Flatpak-Portal-Freigaben und native
Wayland-Fensterbedienung benötigen weiterhin eine manuelle Abnahme.

## Noch geplant

Genauer Transferfortschritt in Dateien/Bytes, Abbrechen, Desktop-Benachrichtigungen,
Bandbreitenlimit und Pausen nach Akku-/Netzwerkzustand sind noch nicht Teil der UI.
