# Warmes Dashboard mit Orange-Akzenten

Die Gestaltung orientiert sich am Mockup: dunkle, warme Flächen, ein
orangefarbenes Statusband, drei Kennzahlenkarten, große Bereiche für Quellen
und Ziele sowie Einstellungen über die gesamte Breite. Verläufe, dünne warme
Konturen und dezente Schatten erzeugen den Glow. Hover- und Fokuszustände sind
sichtbar; es gibt keine dauerhaft pulsierende Dekoration.

![Dashboard mit Statusband, Kennzahlen und zwei Bereichen](images/dashboard-orange.png)

Die Bilder zeigen die echte native GTK-App mit isolierten Testdateien und einem
lokalen Ziel. Die Zahlen sind aus den tatsächlich ausgeführten Sicherungen
gelesen. Die Darstellung im installierten Flatpak und unter nativem Wayland
bleibt zusätzlich am Desktop zu prüfen.

## Inhalt und Zustände

- Dateizahl und Größe bezeichnen erfasste Sicherungskopien über alle Ziele, nicht
  die gesamte Belegung eines WebDAV-Servers.
- Die Zeitkarte zeigt die letzte erfasste Dateiübertragung. Ein reiner Scan ohne
  Übertragung wird nicht als neuer Transfer ausgegeben.
- Grün markiert einen erfolgreich abgeschlossenen Sync. Fehler erhalten ein
  eigenes Warnsymbol, eine andere Kontur und weiterhin ihre vollständige
  Meldung mit Pfad und Hinweisen zum Wiederholen.
- Lange Pfade werden in der Übersicht gekürzt und sind im Tooltip vollständig
  verfügbar. Die Auswahl der Quellen und das Verhalten der Synchronisierung
  bleiben erhalten.
- Jeder Endpunkt hat eine eigene Kachel. Verbindungsprüfung und Wiederherstellung
  beziehen sich ausschließlich auf das dort angezeigte Ziel; Rückmeldungen zur
  Verbindung bleiben in derselben Kachel.
- **Weiteren Endpunkt hinzufügen** steht unterhalb aller Endpunkt-Kacheln.
  Auto-Sync ist eine gemeinsame Einstellung und steht separat mit dem Hinweis
  **Für alle Endpunkte**. Es werden keine unabhängigen Schalter pro Ziel
  vorgetäuscht.

![Zwei getrennte Endpunkt-Kacheln mit Hinzufügen darunter](images/dashboard-orange-endpoints.png)

![Fehler mit vollständiger Meldung und Warnsymbol](images/dashboard-orange-error.png)

## Schmale Fenster und Bedienung

Unter 860 Pixeln wechseln Statusband, Kennzahlen und Inhaltsbereiche in eine
vertikale Anordnung. Die gesamte Seite einschließlich Einstellungen kann
vertikal gescrollt werden. Fokusrahmen erleichtern die Tastaturbedienung;
Outline-Symbole werden direkt gezeichnet und hängen nicht vom Icon-Theme ab.

![Unterer Bereich bei 560 Pixeln Fensterbreite](images/dashboard-orange-narrow.png)

Die bestehende GTK-Suite prüft außerdem echte Statistikwerte vor und nach dem
Sync sowie ein tatsächlich auf 560 Pixel verkleinertes Fenster (UI-32).
UI-33 prüft getrennte Endpunkt-Kacheln, die Position der Hinzufügen-Aktion,
die gemeinsame Auto-Sync-Einstellung und die zielbezogene Verbindungsprüfung
und Wiederherstellung mit zwei unterschiedlichen Testzielen.
