# Umsetzungsstand der nächsten Roadmap-Schritte

Die Aufgaben werden getrennt getestet und als einzelne PRs integriert.

- [x] Zugangsdaten im System-Schlüsselbund (Secret Service, GNOME Keyring/KWallet); bestehende Passwortdateien werden erst nach erfolgreicher Übernahme entfernt. Einrichtung zeigt Fehler an und speichert im Hintergrund.
- [x] Wählbarer Backup-Modus ohne Löschweitergabe. Neue Profile starten im Backup-Modus; vorhandene Profile behalten die bisherige Spiegelung. Der Modus ist im Dashboard umstellbar.
- [x] Abbruchsichere Uploads: lokale Tempdatei mit Größenprüfung/fsync/atomarem Ersetzen; WebDAV mit exklusivem temporärem PUT und MOVE. Fehlgeschlagene Übertragungen ersetzen keine gültige Sicherung.
- [ ] Ausschlussmuster für nicht zu sichernde Dateien und Ordner.
- [ ] Ereignisbasierte Änderungserkennung mit Bündelung und Scan-Fallback.
- [ ] Sicherungen durchsuchen und ausgewählte Dateien wiederherstellen.

Für den Schlüsselbund muss ein Secret-Service-Anbieter in der Desktopsitzung laufen
und gegebenenfalls entsperrt werden. Cloudsave schreibt keine neuen Passwortdateien.
Die explizite Umgebungsvariable `CLOUDSAVE_WEBDAV_PASSWORD` bleibt ein flüchtiger
Override für CLI/isolierte Tests und wird nicht gespeichert. Passwörter werden
bytegenau übernommen, einschließlich Leerzeichen.

WebDAV-Ziele müssen MOVE unterstützen. Bei einem Verbindungsabbruch mit unklarem
Serverzustand kann eine zufällige `.cloudsave-upload-*.part`-Datei zurückbleiben;
die bisherige Zieldatei wird während des PUT nicht verändert.
