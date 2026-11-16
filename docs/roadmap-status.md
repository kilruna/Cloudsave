# Umsetzungsstand der nächsten Roadmap-Schritte

Die Aufgaben werden getrennt getestet und als einzelne PRs integriert.

- [x] Zugangsdaten im System-Schlüsselbund (Secret Service, GNOME Keyring/KWallet); bestehende Passwortdateien werden erst nach erfolgreicher Übernahme entfernt. Einrichtung zeigt Fehler an und speichert im Hintergrund.
- [x] Wählbarer Backup-Modus ohne Löschweitergabe. Neue Profile starten im Backup-Modus; vorhandene Profile behalten die bisherige Spiegelung. Der Modus ist im Dashboard umstellbar.
- [x] Abbruchsichere Uploads: lokale Tempdatei mit Größenprüfung/fsync/atomarem Ersetzen; WebDAV mit exklusivem temporärem PUT und MOVE. Fehlgeschlagene Übertragungen ersetzen keine gültige Sicherung.
- [x] Ausschlussmuster: ein Glob pro Zeile im Dashboard. Namen ohne / gelten für jede Pfadkomponente; Pfade mit / relativ zur Quelle. Ausgeschlossene Bestandsdateien werden auch bei Spiegelung nicht gelöscht.
- [x] Ereignisbasierte Änderungserkennung: notify/inotify im Worker, drei Sekunden Ruhezeit, bestehendes Intervall als Scan-Fallback. Auto-Sync AUS bleibt wirksam; Quellen und Ausschlüsse werden beim Ändern neu registriert.
- [x] Sicherungen durchsuchen und ausgewählte Dateien wiederherstellen: lokale Ziele und WebDAV, Dateiauswahl im Dashboard, Zielordner per Dialog, atomarer Download ohne Überschreiben bestehender Dateien.

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
- Bandbreitenlimit, Akku-/Netzwerkregeln und Desktop-Benachrichtigungen.
- Policies pro Quellordner, Verschlüsselung, Übersetzungen und Flathub-Release.
- Manuelle Abnahme mit realer installierter Flatpak, Desktopportal und Wayland;
  automatische Tests auf Xvfb ersetzen diese Bedienprüfung nicht.
