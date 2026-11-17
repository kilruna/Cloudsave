# Umsetzungsstand der nächsten Roadmap-Schritte

Die Aufgaben werden getrennt getestet und als einzelne PRs integriert.

- [x] Zugangsdaten im System-Schlüsselbund (Secret Service, GNOME Keyring/KWallet); bestehende Passwortdateien werden erst nach erfolgreicher Übernahme entfernt. Einrichtung zeigt Fehler an und speichert im Hintergrund.
- [x] Wählbarer Backup-Modus ohne Löschweitergabe. Neue Profile starten im Backup-Modus; vorhandene Profile behalten die bisherige Spiegelung. Der Modus ist im Dashboard umstellbar.
- [x] Abbruchsichere Uploads: lokale Tempdatei mit Größenprüfung/fsync/atomarem Ersetzen; WebDAV mit exklusivem temporärem PUT und MOVE. Fehlgeschlagene Übertragungen ersetzen keine gültige Sicherung.
- [x] Ausschlussmuster: anklickbare Standardgruppen, eigener Musterdialog und native Ordnerauswahl. Namen ohne / gelten für jede Pfadkomponente; Pfade mit / relativ zur Quelle. Ausgeschlossene Bestandsdateien werden auch bei Spiegelung nicht gelöscht.
- [x] Ereignisbasierte Änderungserkennung: notify/inotify im Worker, drei Sekunden Ruhezeit, bestehendes Intervall als Scan-Fallback. Auto-Sync AUS bleibt wirksam; Quellen und Ausschlüsse werden beim Ändern neu registriert.
- [x] Sicherungen durchsuchen und ausgewählte Dateien wiederherstellen: lokale Ziele und WebDAV, Dateiauswahl im Dashboard, Zielordner per Dialog, atomarer Download ohne Überschreiben bestehender Dateien.
- [x] Ganze Ordner im Einrichtungsassistenten wiederherstellen: Endpunktprüfung vor dem ersten Upload, bisherige Home-Orte oder separater Ausgabeordner, fortsetzbar nach Neustart. Neue Sicherungen speichern eine portable Ordnerzuordnung; alte erhalten bestätigbare Vorschläge. Fehler/Konflikte lassen Auto-Sync pausiert.
- [x] Orange Wolke im Tray mit aktiven Aufträgen, Upload-/Downloadphase und den letzten acht Jobs über App-Neustarts hinweg. Öffnen und Beenden bleiben im Menü erreichbar.
- [x] Tray-Symbol mit sichtbaren Zuständen für bereit, laufende Aufträge, pausierte Automatik, Fehler und erfolgreichen Sync. Fensterkopf, Tray und Flatpak-Launcher verwenden dieselbe Wolke.
- [x] Fortschritt für Übertragungen mit übertragenen Bytes pro Datei, Geschwindigkeit und geschätzter Restzeit bei bekannter Dateigröße. Aufträge können im Tray pausiert, fortgesetzt und abgebrochen werden. Die Pause greift nach der laufenden Datei.
- [x] WebDAV wiederholt vorübergehend fehlgeschlagene Dateioperationen nach zwei und fünf Sekunden. Das Tray zeigt die Wartezeit. Wenn alle Versuche an einem Ziel scheitern, gehen andere Endpunkte weiter.
- [x] Desktop-Benachrichtigungen für fehlgeschlagene Sync- und Wiederherstellungsaufträge. Bei einem weiter bestehenden Fehler wird die Meldung nicht wiederholt.

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
- Bandbreitenlimit und Akku-/Netzwerkregeln.
- Policies pro Quellordner, Verschlüsselung, Übersetzungen und Flathub-Release.
- Manuelle Abnahme mit realer installierter Flatpak, Desktopportal und Wayland;
  automatische Tests auf Xvfb ersetzen diese Bedienprüfung nicht.

## Nächste Schritte nach Priorität

1. **Bazzite-Abnahme:** installierte Flatpak unter KDE/Wayland mit echten
   Portal-Freigaben, Schlüsselbund und Storage Box prüfen. Wiederherstellung auf
   einem frischen Profil einschließlich Konflikten testen und danach Issue #10
   abschließen, wenn die Abnahme besteht.
2. **Tray-Status und einheitliches App-Icon:** umgesetzt. Die Flatpak exportiert
   dieselbe orange Wolke wie Fensterkopf und Tray als SVG und in fünf PNG-Größen.
   Symbol und Tooltip aktualisieren sich ohne App-Neustart. Ein erfolgreicher
   Endpunkt verdeckt keinen Fehler eines anderen Endpunkts im selben Sync-Lauf.
   Die Anzeige nach frischer Installation und Update auf Bazzite gehört zur
   Desktop-Abnahme.
3. **Übertragungen kontrollieren:** Fortschritt, Pausieren, Fortsetzen und
   Abbrechen sind umgesetzt. Bereits abgeschlossene Dateien bleiben bei einem
   Abbruch erhalten.
4. **Netzwerkfehler behandeln:** umgesetzt. WebDAV-Transfers beginnen beim
   Wiederholen mit einer neuen temporären Datei. Anmelde- und lokale
   Schreibfehler werden direkt gemeldet.
5. **Alltagskomfort und Veröffentlichung:** die letzten acht Aufträge bleiben
   nach dem Neustart im Tray sichtbar. Eine Desktop-Meldung führt bei einem
   Fehler zurück zur App. Die Flathub-Veröffentlichung bleibt offen.

Die Umsetzung erfolgt über einzelne PRs mit CI vor dem Merge.

## Vorbereitung einer Flathub-Einreichung

- AppStream beschreibt lokale und WebDAV-Sicherungen, nennt die deutsche
  Oberfläche und enthält Projektlinks, eine Altersfreigabe und einen Screenshot.
  [AppStream-Vorgaben](https://docs.flathub.org/docs/for-app-authors/metainfo-guidelines/)
- Die derzeitige App-ID `org.cloudsave.Cloudsave` setzt die Kontrolle über
  `cloudsave.org` voraus. Für eine über GitHub abgeleitete ID kommt
  `io.github.kilruna.Cloudsave` infrage. Eine Änderung der ID braucht eine
  Migration für bestehende Flatpak-Profile, Schlüsselbund-Einträge und Autostart.
  [Anforderungen an die App-ID](https://docs.flathub.org/docs/for-app-authors/requirements/#application-id)
- Der unbeschränkte Sitzungs-D-Bus-Zugriff und die ausdrückliche
  Portal-Freigabe im aktuellen Projektpaket blockieren den Flathub-Linter.
  Die benötigten Tray-Verbindungen und Portal-Funktionen müssen mit engeren
  Freigaben auf den unterstützten Desktops geprüft werden.
  [Flathub-Linter](https://docs.flathub.org/docs/for-app-authors/linter/)
- Flathub verlangt eine aktuelle Runtime, nachvollziehbare Versions-Tags und
  eine erkennbare Wartungshistorie. Das Einreichungsmanifest und der
  Einreichungs-PR werden von den Maintainers erstellt. KI-Beiträge zum Projekt
  müssen bei der Einreichung offengelegt werden.
  [Flathub-Anforderungen](https://docs.flathub.org/docs/for-app-authors/requirements/)
