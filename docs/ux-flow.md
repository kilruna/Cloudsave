# UX-Flow: Vom Start bis zum eingerichteten Sync

Leitlinie: **absolute Userfreundlichkeit**. Ein linearer Flow, maximal drei Schritte bis zum laufenden Sync, keine verschachtelten Menüs. Alles Wesentliche ist auf einer Seite sichtbar.

## Der Flow

```
Start der App
│
├─ Noch nicht eingerichtet
│    └─ Welcome-Seite: ein Satz, was Cloudsave macht
│       → großer Button [Einrichtung starten]
│
├─ Schritt 1: Ordner wählen
│    └─ Datei-Dialog (Flatpak-Portal), mehrere Ordner auf einmal auswählbar
│       + Schnellwahl: ☐ Dokumente  ☐ Bilder  ☐ Musik  ☐ Videos  ☐ Schreibtisch
│       → [Weiter]
│
├─ Schritt 2: Ziel wählen
│    └─ Typ: SFTP / WebDAV / lokaler Ordner
│       Felder: Server, Port, Benutzer, Passwort oder SSH-Key, Remote-Pfad
│       → [Verbindung testen] → Rückmeldung in Klartext („Verbunden ✓" / „Passwort falsch“)
│       → [Weiter]
│
└─ Schritt 3: Fertig
     └─ „Alles eingerichtet – der erste Sync startet jetzt."
        → Hauptansicht
```

## Hauptansicht (nach der Einrichtung)

Eine Seite, drei Bereiche – keine Menü-Tiefe:

1. **Status-Banner oben:** „Alles synchronisiert ✓ · letzter Sync vor 2 Min.“ oder „Sync läuft … 12/143 Dateien“ oder „Fehler: Server nicht erreichbar – [Erneut versuchen]".
2. **Mitte – zwei Karten:**
   - *Gesicherte Ordner:* Liste mit Checkboxen zum An-/Abhaken, jeweils letzter Stand.
   - *Ziel:* Server-Adresse, Buttons [Jetzt synchronisieren] und Schalter [Automatisch synchronisieren an/aus].
3. **Unten – Einstellungen** (ein aufklappbarer Bereich, keine Untermenüs): Sync-Modus (sofort bei Änderung / stündlich / täglich), Pause bei Akkubetrieb, Pause bei getakteter Verbindung, Benachrichtigungen, Bandbreitenlimit.

## Gestaltungsregeln

- **Ein offensichtlicher nächster Schritt** pro Bildschirm; der primäre Button ist immer klar hervorgehoben.
- **Klartext statt Technik-Jargon:** Fehler werden übersetzt („Passwort falsch" statt „SSH_AUTH_FAILED“), mit konkreter Aktion („Zugangsdaten ändern" / „Erneut versuchen").
- **Keine versteckten Funktionen:** Alles, was es gibt, ist in Hauptansicht oder im einen Einstellungsbereich erreichbar.
- **Wenig frågen, sinnvolle Defaults:** Automatischer Sync an, Deletes spiegeln, Benachrichtigungen nur bei Fehlern.
- **Barrierefrei und dunkelmodusfähig:** durch libadwaita automatisch; volle Tastaturbedienung; Tooltips nur beim ersten Start.
- **Onboarding ohne Overhead:** Die Welcome-Seite erklärt in einem Satz das Prinzip und startet direkt in Schritt 1 – kein Tour-Marathon.

## Zustände, die angezeigt werden müssen

- Leerlauf/alles synchronisiert · Sync läuft (mit Fortschritt) · Pausiert (Akku/getaktet/manuell) · Fehler (mit Klartext + Aktion) · Ersteinrichtung läuft (erster Vollsync).

## Umgesetztes Design der Hauptansicht (M1-Skelett)

Die Hauptansicht ist als **dunkles Dashboard** im Stil der Referenz gestaltet (dunkler Hintergrund, Karten, orange Akzent) und trotzdem auf einen Blick erfassbar – eine Seite, keine Menüebenen:

- **Farben (CSS-Overrides in `crates/cloudsave-app/src/main.rs`):** Hintergrund `#121212`, Karten `#1E1E1E` (16 px Ecken), Akzent Orange `#FF7A29` (über `accent_bg_color` – färbt den primären Button „Jetzt synchronisieren“ in der Headerbar orange), Farbschema per `ColorScheme::ForceDark` erzwungen.
- **Headerbar:** Titel „Cloudsave“ + rechts der orange **pill**-Button „Jetzt synchronisieren“.
- **Status-Banner oben:** `AdwBanner` („Alles synchronisiert · letzter Sync vor …“).
- **Stat-Karten (3er-Raster):** große Zahl + kleines Label – „Dateien gesichert“, „auf dem Ziel belegt“, „letzter Sync“ (eigene CSS-Klasse `.stat-card`).
- **Gruppen als Karten** (`AdwPreferencesGroup`): „Gesicherte Ordner“ (Zeilen mit Checkbox rechts, plus Zeile „Anderer Ordner hinzufügen …“), „Ziel“ (Hetzner Storage Box + Button „Verbindung testen“ + Schalter „Automatisch synchronisieren“), „Einstellungen“ (Sync-Rhythmus als `AdwComboRow`, Schalter für Fehler-Benachrichtigung und Akku-Pause).

Der Ersteinrichtungs-Flow (Welcome → Ordner → Ziel → fertig) wird als Nächstes auf derselben Basis umgesetzt.
