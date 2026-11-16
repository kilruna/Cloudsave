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

## Umgesetztes Design der Hauptansicht (Mockup)

Die Hauptansicht (`dashboard.rs`) folgt dem aktuellen UI-Mockup (dunkler Blau-Verlauf, schwebende Karte, lila Akzent) – eine Seite, keine Menüs:

- **Fenster:** rahmenlos (`set_decorated(false)`), Hintergrund Blau-Verlauf (`.window-bg`), zentrale Karte `.main-card` (`#151E3C`, 18 px Ecken, Rand + Schatten, blauer Glow oben rechts, zwei dekorative Wolken per cairo).
- **Kopfzeile in der Karte:** lila Wolken-Logo + „Cloudsave“ (weiß, fett) links, lila Pill-Button „Jetzt synchronisieren“ (`.sync-btn`) rechts.
- **Statusblock linksbündig:** Titel („Bereit“ / „Sync läuft“ / „Alles synchronisiert“, `.sync-title`), Unterzeile (`.sync-sub`, z. B. „143 Dateien gesichert · letzter Sync vor 2 Min“), Fortschrittsbalken (`.sync-progress`: Track transparent mit weißem Rahmen, Füllung weiß→lavender; pulsiert während des Syncs).
- **Zwei Spalten (ca. 60/40):** links „Gesicherte Ordner“ als Kartenliste (Checkbox + Ordner-Icon + Ordnername, aus der Config befüllt), „Ordner hinzufügen“-Button (`.add-btn`, FileDialog), aufklappbarer „Einstellungen“-Bereich (Pfeil + `Revealer`: Sync-Rhythmus, Benachrichtigungen, Akku-Pause, Verbindung testen); rechts Ziel-Karte (`.target-card`: Ziel-Name + Pfad/Untertitel, Zeile „Automatisch synchronisieren“ + Schalter, persistiert in die Config).
- **Farben (CSS in `crates/cloudsave-app/src/main.rs`):** Akzent Lila `#9D7CFF` über `accent_bg_color` (Button, Checkboxen, Schalter), Karten `rgba(255,255,255,0.055)`, `ColorScheme::ForceDark` erzwungen. Icons (Wolke/Ordner) werden per cairo in den Mockup-Farben gezeichnet.

## Umgesetzt (PR #4): Onboarding + Dashboard

Der Flow ist in `crates/cloudsave-app/src/onboarding.rs` als `AdwNavigationView` umgesetzt: Welcome → Ordner wählen (Checkboxen für vorhandene Standardordner, „Anderer Ordner …“ via FileDialog) → Ziel wählen (lokal aktiv; Hetzner Storage Box als „folgt in M2“ deaktiviert) → Fertig (schreibt `~/.config/cloudsave/config.toml` und startet den ersten Sync). Ohne vorhandene Config startet die App im Onboarding, sonst direkt im Dashboard (`dashboard.rs`): „Jetzt synchronisieren“ läuft `cloudsave-core::sync` im Worker-Thread, Status-Titel/-zeile + Fortschrittsbalken aktualisieren sich aus dem SQLite-State, Ordner-Checkboxen persistieren in die Config.
