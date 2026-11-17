# Umsetzungsstand der nächsten Roadmap-Schritte

Die Aufgaben werden getrennt getestet und als einzelne PRs integriert.

- [x] Zugangsdaten im System-Schlüsselbund (Secret Service, GNOME Keyring/KWallet); bestehende Passwortdateien werden erst nach erfolgreicher Übernahme entfernt. Einrichtung zeigt Fehler an und speichert im Hintergrund.
- [x] Wählbarer Backup-Modus ohne Löschweitergabe. Neue Profile starten im Backup-Modus; vorhandene Profile behalten die bisherige Spiegelung. Der Modus ist im Dashboard umstellbar.
- [x] Abbruchsichere Uploads: lokale Tempdatei mit Größenprüfung/fsync/atomarem Ersetzen; WebDAV mit exklusivem temporärem PUT und MOVE. Fehlgeschlagene Übertragungen ersetzen keine gültige Sicherung.
- [x] Ausschlussmuster: anklickbare Standardgruppen, eigener Musterdialog und native Ordnerauswahl. Namen ohne / gelten für jede Pfadkomponente; Pfade mit / relativ zur Quelle. Ausgeschlossene Bestandsdateien werden auch bei Spiegelung nicht gelöscht.
- [x] Ereignisbasierte Änderungserkennung: notify/inotify im Worker, drei Sekunden Ruhezeit, bestehendes Intervall als Scan-Fallback. Auto-Sync AUS bleibt wirksam; Quellen und Ausschlüsse werden beim Ändern neu registriert.
- [x] Sicherungen durchsuchen und ausgewählte Dateien wiederherstellen: lokale Ziele und WebDAV, Dateiauswahl im Dashboard, Zielordner per Dialog, atomarer Download ohne Überschreiben bestehender Dateien.
- [x] Ganze Ordner im Einrichtungsassistenten wiederherstellen: Endpunktprüfung vor dem ersten Upload, bisherige Home-Orte oder separater Ausgabeordner, fortsetzbar nach Neustart. Neue Sicherungen speichern eine portable Ordnerzuordnung; alte erhalten bestätigbare Vorschläge. Fehler/Konflikte lassen Auto-Sync pausiert.
- [x] Orange Wolke im Tray mit aktiven Aufträgen, Upload-/Downloadphase und den letzten acht Jobs der laufenden Sitzung. Öffnen und Beenden bleiben im Menü erreichbar.

Für den Schlüsselbund muss ein Secret-Service-Anbieter in der Desktopsitzung laufen
und gegebenenfalls entsperrt werden. Cloudsave schreibt keine neuen Passwortdateien.
Die explizite Umgebungsvariable `CLOUDSAVE_WEBDAV_PASSWORD` bleibt ein flüchtiger
Override für CLI/isolierte Tests und wird nicht gespeichert. Passwörter werden
bytegenau übernommen, einschließlich Leerzeichen.

WebDAV-Ziele müssen MOVE unterstützen. Bei einem Verbindungsabbruch mit unklarem
Serverzustand kann eine zufällige `.cloudsave-upload-*.part`-Datei zurückbleiben;
die bisherige Zieldatei wird während des PUT nicht verändert.

## Wiederherstellen

Im Dashboard **Sicherungen wiederherstellen** öffnen, einen Endpunkt auswählen und
**Sicherungen laden** klicken. Gewünschte Dateien markieren, einen Zielordner
wählen und **Wiederherstellen** starten. Die ursprüngliche Ordnerstruktur bleibt
unter diesem Ziel erhalten. Bestehende Dateien und Symlinks werden nicht ersetzt.
Teilweise fehlgeschlagene Wiederherstellungen nennen die betroffenen Dateien.
WebDAV-Verzeichnislisten bleiben auf denselben Server und den konfigurierten
Zielpfad beschränkt. Temporäre Upload-Dateien werden nicht als Backups angeboten.

## Weiter offen

- SFTP/rsync und S3 als zusätzliche Backends.
- Übertragungsfortschritt in Dateien/Bytes, Abbrechen und Netzwerk-Retries.
- Sichtbarer Status direkt am Tray-Symbol: laufender Sync, pausierte Automatik und Fehler auch ohne geöffnetes Menü unterscheiden; erfolgreicher Sync erhält einen eindeutigen Zustand.
- Gemeldete Icon-Abweichung in der App-Übersicht beheben: einheitliche orange Wolke in App-Übersicht/Launcher, Fenstertitelleiste und Tray; Statusvarianten behalten dasselbe Grundsymbol.
- Job-Historie über App-Neustarts hinweg.
- Bandbreitenlimit, Akku-/Netzwerkregeln und Desktop-Benachrichtigungen.
- Policies pro Quellordner, Verschlüsselung, Übersetzungen und Flathub-Release.
- Manuelle Abnahme mit realer installierter Flatpak, Desktopportal und Wayland;
  automatische Tests auf Xvfb ersetzen diese Bedienprüfung nicht.

## Nächste Schritte nach Priorität

1. **Bazzite-Abnahme:** installierte Flatpak unter KDE/Wayland mit echten
   Portal-Freigaben, Schlüsselbund und Storage Box prüfen. Wiederherstellung auf
   einem frischen Profil einschließlich Konflikten testen und danach Issue #10
   abschließen, wenn die Abnahme besteht.
2. **Tray-Status und einheitliches App-Icon:** die gemeldete Abweichung in der
   App-Übersicht reproduzieren und beheben. Eine gemeinsame Icon-Vorlage für
   Launcher, exportiertes Flatpak-Icon, Fenstertitelleiste und Tray verwenden;
   installierte Assets und mögliche Icon-Caches bei der Prüfung berücksichtigen.
   Der Status muss bereits direkt am Tray-Symbol erkennbar sein:
   - Bereit/noch kein Sync darf nicht als erfolgreiche Sicherung erscheinen.
   - Laufender Sync erhält einen klaren Übertragungsindikator.
   - Pausierte Automatik erhält eine erkennbare Pausenmarkierung.
   - Fehler erhalten ein Warnzeichen; ein erfolgreicher anderer Endpunkt darf
     einen noch offenen Fehler nicht verdecken.
   - Ein erfolgreicher Sync erhält einen eindeutigen Erfolgszustand.
   Form/Symbol und Tooltip ergänzen die Farbe. Das Grundsymbol bleibt die orange
   Wolke; Statusänderungen aktualisieren den Tray ohne App-Neustart. Auf Bazzite
   sowohl nach frischer Installation als auch nach einem Update prüfen.
3. **Übertragungen kontrollieren:** echter Datei-/Byte-Fortschritt mit
   Geschwindigkeit und Restzeitschätzung, anschließend Pausieren und Abbrechen.
4. **Netzwerkfehler behandeln:** automatische Wiederholungen mit Wartezeiten und
   verständlicher Anzeige bei nicht erreichbaren Endpunkten.
5. **Alltagskomfort und Veröffentlichung:** dauerhafte Job-Historie,
   Fehlerbenachrichtigungen und Flathub. Zusätzliche Protokolle folgen danach.

Diese Punkte sind geplant. Die Umsetzung erfolgt über einzelne PRs mit CI vor
dem Merge; die vorhandene Job-Anzeige im Tray ersetzt den Statusindikator am
Symbol und die Desktop-Abnahme noch nicht.
