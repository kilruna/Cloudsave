# Unterstützte Protokolle / Ziele

Übersicht über potenziell sinnvolle Protokolle für Cloudsave. Der Fokus liegt auf der **Hetzner Storage Box** (typischerweise `uXXXXX@uXXXXX.your-storagebox.de`), daneben auf gängigen Alternativen.

## Hetzner Storage Box – Eckdaten

| Zugang | Protokoll | Port | Hinweis |
|---|---|---|---|
| SFTP / SCP | `sftp -P 22 uXXXXX@uXXXXX.your-storagebox.de` | **22** immer aktiv; zusätzlich **23** nach Aktivierung von „SSH-Support" (deutlich schneller als Port 22) | Port 22 kann nicht deaktiviert werden; SSH-Key-Login möglich |
| FTP / FTPS | legacy | 21 | immer aktiv |
| rsync | `rsync -e "ssh -p 23" ...` | **23** | erfordert „SSH-Support" (Hetzner Console → Einstellungen ändern) |
| BorgBackup | `borg` über SSH | 23 | ebenfalls „SSH-Support" |
| WebDAV (HTTPS) | `https://uXXXXX.your-storagebox.de` | 443 | in der Hetzner Console aktivieren |
| Samba/CIFS | LAN | 445/139 | in der Hetzner Console aktivieren |

Weitere Eckdaten: max. **10 gleichzeitige Verbindungen** pro Account (Upload-Parallelität in Cloudsave daher default 3), SSH-Key-Authentifizierung ohne Passwort für SCP/SFTP/rsync/Borg, Sub-Accounts (`uXXXXX-subN`) sehen nur ihr eigenes Unterverzeichnis, **kein S3** auf der Storage Box (S3-kompatibel ist Hetzner Object Storage, ein separates Produkt). Serverseitige Snapshots der Box selbst sind als zusätzliche Sicherungsebene möglich.

Quellen: [Hetzner Docs – Storage Box (de)](https://docs.hetzner.com/de/storage/storage-box/), [Hetzner Docs – Storage Box Übersicht](https://docs.hetzner.com/de/storage/storage-box/general/)

## Protokoll-Liste

| # | Protokoll | Hetzner Storage Box | Sinnvoller Einsatzzweck | Stärken | Schwächen | Priorität |
|---|---|---|---|---|---|---|
| 1 | **rsync über SSH** | ja (Port 23) | Primäres MVP-Ziel | Bewährtes Delta-Verfahren (nur Änderungen übertragen), Resume bei Abbruch, geringer Eigenentwicklungsaufwand | Benötigt `ssh` + `rsync` im Bundle; Credentials-Handling im Sandbox | **P1** |
| 2 | **SFTP** (reiner Rust-Client) | ja (Port 22 und 23) | Universeller verschlüsselter Transport | Sicher, überall verfügbar, GUI-Clients kompatibel | Ohne rsync kein natives Delta – eigene Diff-Logik nötig | **P1** als Transport, P2 als reiner Upload |
| 3 | **Lokaler Ordner / externes Laufwerk** | – | USB-Platte, gemountetes NAS, Test-Ziel | Trivial, offline, ideal für Tests und Entwicklung | Kein Remote-Backup | **P1** (zweites MVP-Ziel) |
| 4 | **WebDAV (HTTPS)** | ja (Port 443) | Nextcloud, ownCloud, Storage Box, andere Anbieter | HTTP-basiert, gut für Restore/Datei-Ansicht, breit unterstützt | Kein natives Delta, Sync über Datei-Vergleich | **P2** |
| 5 | **S3-kompatibel** | nein (Hetzner Object Storage separat) | Hetzner Object Storage, MinIO, AWS S3 | Versioning, Multipart-Uploads, skalierbar, gute SDKs | Kein klassisches „Ordner"-Gefühl, ggf. Kosten pro Request | **P2** |
| 6 | **FTPS** | ja | Legacy-Systeme, alte NAS | Weit verbreitet | Veraltet, Zertifikats-/TLS-Konfiguration aufwendig | **P3** |
| 7 | **BorgBackup** | ja (über SSH, Port 23) | Deduplizierende, verschlüsselte Backups | Platzsparend durch Dedup, Ende-zu-Ende-Verschlüsselung | Komplex, eigener Server-Teil, höherer Wartungsaufwand | **P3** (Phase 3) |
| 8 | **Samba/CIFS** | ja | Nur LAN/NAS | Einfaches Mounten | Unsicher über WAN, nicht für Internet-Backup geeignet | **P3** |

## Empfehlung

- **MVP (Phase 1):** rsync über SSH gegen die Hetzner Storage Box + lokales Ziel. rsync übernimmt das Delta-Verfahren und Resumes – in Rust von Grund auf neu zu bauen wäre Verschwendung. `ssh` und `rsync` werden im Flatpak mitgeliefert; Zugangsdaten kommen aus dem Secret Service und werden dem SSH-Client über einen Askpass-Helfer übergeben (nie als Klartext auf Platte).
- **Phase 2:** WebDAV (Nextcloud & Storage Box) und S3-kompatibler Object Storage als zusätzliche Backends, dazu ein reiner Rust-SFTP-Client (z. B. auf Basis von `russh`) als leichtgewichtige Alternative.
- **Alternative Strategie:** Statt jedes Protokoll selbst zu implementieren, könnte man **rclone** als eingebettete Engine nutzen und alle unterstützten Ziele „gratis" bekommen. Trade-off: Fremdbinary im Bundle und weniger native Kontrolle vs. deutlich schnellerer Abdeckung vieler Protokolle. Empfehlung: mit nativen Rust-Backends (rsync/SSH, lokal, WebDAV, S3) starten; rclone erst in Betracht ziehen, wenn die Liste der Ziele schnell wachsen soll.

Alle Backends sitzen hinter einem gemeinsamen Rust-Trait (`Backend`: `list`, `upload`, `download`, `delete`), sodass die Protokoll-Wahl für UI und Sync-Engine transparent ist.
