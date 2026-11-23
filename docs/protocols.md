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
| 9 | **SCP** | ja (Port 22/23, wie SFTP) | Schneller Einzel-Upload, wenn kein rsync-Lauf nötig ist | Einfach, bewährt, liegt `ssh` bei | Kein Delta, kein Resume bei Abbruch | **P3** |
| 10 | **FTP** (unverschlüsselt) | ja (Port 21, immer aktiv) | Nur noch Legacy-Altlasten | Überall vorhanden | Klartext-Passwörter, unsicher, NAT/Firewall-Probleme | **P4** (nur Legacy) |
| 11 | **rsync.net** | nein (separater Anbieter) | Deduplizierendes rsync-Ziel mit serverseitigen Snapshots | Bewährtes rsync-Protokoll, Snapshots inklusive | Kosten, proprietärer Anbieter | **P3** |
| 12 | **Backblaze B2** | nein (S3-kompatibel) | Günstiger Object Storage | S3-API, Multipart-Uploads, niedrige Speicherkosten | Kein „Ordner“-Gefühl, Egress-Kosten beachten | **P3** (über S3-Backend) |
| 13 | **Nextcloud / ownCloud** | nein (eigenes Hosting; Storage Box nur per WebDAV) | Self-hosted Cloud als Ziel | WebDAV-Standard, oft bereits im Haushalt vorhanden | Eigener Betrieb und Wartung | **P2** (über WebDAV-Backend) |
| 14 | **rclone** (eingebettete Engine) | ja (unterstützt alle genannten Ziele) | Viele Ziele auf einmal, ohne jedes Protokoll selbst zu implementieren | 70+ Backends, Delta-Verfahren, clientseitige Verschlüsselung | Fremdbinary im Bundle, mehr Bundle-Größe, weniger native Kontrolle | **P3** (Option bei schnell wachsender Zielliste) |
| 15 | **NFS** | nein | NAS im LAN als gemountetes Ziel | Transparentes Mount, kein Protokoll-Client nötig | Nur LAN, unsicher über WAN, Mount-Abhängigkeit | **P3** |
| 16 | **Google Drive** | nein | Consumer-Cloud (15 GB gratis) | Weit verbreitet, gute Erreichbarkeit | Nur Vendor-API + OAuth2 (kein Datei-Protokoll), API-Quotas, Flatpak braucht Online-Accounts/Portal | **P4** |
| 17 | **Dropbox** | nein | Consumer-Cloud | Einfach, Delta-API | Nur Vendor-API + OAuth2, keine echte Ordnerstruktur wie lokal | **P4** |
| 18 | **Microsoft OneDrive** | nein | Consumer-Cloud im Microsoft-Ökosystem | Verbreitet bei MS-Accounts | Nur Microsoft-Graph-API + OAuth2, dünner Linux-Support | **P4** |
| 19 | **pCloud** | nein | Consumer-Cloud mit Lifetime-Plänen | WebDAV in Bezahlplänen, langfristig günstig | Gratis-Tarif ohne WebDAV; sonst Vendor-API + OAuth2 | **P4** |

## Bewusst ausgeschlossen

- **iCloud Drive:** unter Linux nicht nutzbar (kein offizieller Client, keine öffentliche API).
- **Bidirektionale Synchronisation** (Syncthing-Modell): bewusst kein Ziel, siehe „Bewusst keine Ziele“ in [plan.md](plan.md).
- **Plain FTP über WAN:** nur für Legacy-Fälle (Zeile 10), nicht für produktive Internet-Backups.

## Empfehlung

- **MVP (Phase 1):** rsync über SSH gegen die Hetzner Storage Box + lokales Ziel. rsync übernimmt das Delta-Verfahren und Resumes – in Rust von Grund auf neu zu bauen wäre Verschwendung. `ssh` und `rsync` werden im Flatpak mitgeliefert; Zugangsdaten kommen aus dem Secret Service und werden dem SSH-Client über einen Askpass-Helfer übergeben (nie als Klartext auf Platte).
- **Phase 2:** WebDAV (Storage Box, Nextcloud/ownCloud) und S3-kompatibler Object Storage (Hetzner Object Storage, MinIO, Backblaze B2) als zusätzliche Backends, dazu ein reiner Rust-SFTP-Client (z. B. auf Basis von `russh`) als leichtgewichtige Alternative; SCP (Zeile 9) fällt dabei nebenbei ab.
- **Phase 3:** BorgBackup, Samba/CIFS und NFS fürs LAN, rsync.net. **rclone** (Zeile 14) als eingebettete Engine in Betracht ziehen, wenn die Zielliste schnell wachsen soll – Trade-off: Fremdbinary im Bundle und weniger native Kontrolle vs. deutlich schnellere Abdeckung vieler Protokolle.
- **P4 / bei Bedarf:** Consumer-Clouds (Google Drive, Dropbox, OneDrive, pCloud) sind nur über Vendor-APIs mit OAuth2 erreichbar – im Flatpak flach umsetzbar (z. B. über GNOME Online Accounts), aber mit eigenem API-, Portal- und Wartungsaufwand pro Anbieter.

Alle Backends sitzen hinter einem gemeinsamen Rust-Trait (`Backend`: `list`, `upload`, `download`, `delete`), sodass die Protokoll-Wahl für UI und Sync-Engine transparent ist.
