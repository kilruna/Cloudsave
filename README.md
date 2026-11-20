# Cloudsave

Cloudsave is a Flatpak desktop app for Linux that automatically backs up selected folders to a local drive or a WebDAV server such as a **Hetzner Storage Box** or Nextcloud.

**[Download Cloudsave for Linux (.flatpak, x86_64)](https://github.com/kilruna/Cloudsave/releases/latest/download/cloudsave-x86_64.flatpak)**

Pick the folders you want to back up, choose a destination, and Cloudsave takes care of the rest. It detects file changes and copies them in the background.

## Install Cloudsave

You need Linux on an Intel or AMD 64-bit PC, a graphical desktop, and Flatpak. For WebDAV, you also need a system keyring such as GNOME Keyring or KWallet with the Secret Service interface. The app interface is in German.

Download `cloudsave-x86_64.flatpak` using the link above. The [release page](https://github.com/kilruna/Cloudsave/releases/latest) provides the release notes and SHA-256 checksum. Open a terminal in your Downloads folder and run these commands.

```bash
flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install --user --reinstall ./cloudsave-x86_64.flatpak
flatpak run org.cloudsave.Cloudsave
```

If Cloudsave is already running, quit it from **Einstellungen → App beenden** (Settings → Quit app) before updating. Updates keep your existing configuration. The installer shows the requested permissions, including read access to your home directory so Cloudsave can list your folders and read the files you select.

## Try Cloudsave

Cloudsave is an early release. Start with a small test folder and a separate backup destination. Under **Einstellungen** (Settings), check that the backup mode is **Backup (keine Löschungen)**. New profiles use this mode automatically. Existing profiles may still use mirroring.

1. **Set up a backup.** Select a folder containing a few test files, then choose a separate local destination outside the source folder. Click **Sync einrichten & starten** (Set up and start sync) and compare the backed-up files with the originals.
2. **Try automatic sync.** Edit a test file and check that Cloudsave copies the change. Live sync usually starts after about three seconds of inactivity. The periodic scan setting (**Zusätzlicher Kontrollscan (Minuten)**) catches missed file events. With automatic sync off, new changes are copied only after you click **Jetzt synchronisieren** (Sync now).
3. **Exclude files and folders.** In Settings, open **Dateien und Ordner ausschließen** (Exclude files and folders) and select **Temporäre Dateien** (Temporary files). Cloudsave saves the rule immediately. Create a `.tmp` file in the source folder and sync. The file should be absent from the destination. Other presets cover cache folders, temporary folders, system thumbnails, and development caches. **Ordner auswählen …** (Choose folder) adds a subfolder from a source. **Eigene Regel hinzufügen …** (Add custom rule) accepts patterns such as `*.bak` or `Projects/Cache/**`. Rules apply to all sources and preserve existing backups of excluded files. Presets are opt-in.
4. **Check backup mode.** Delete a test file that was already backed up, then sync. In backup mode its copy stays at the destination. Try mirror mode with test data in a separate destination. It propagates file deletions.
5. **Restore a backup.** After a fresh install, select **Vorhandene Sicherung wiederherstellen** (Restore an existing backup) on the welcome screen and connect the destination. Standard setup also checks the destination before the first sync. The assistant can restore whole folders to their original locations or to a separate folder. Check the output paths before starting. Flatpak may ask you to grant write access to your home folder. To restore individual files, open **Wiederherstellen** (Restore) from the dashboard and search by filename or path.
6. **Try WebDAV.** Repeat the test with a test directory on your server. Quit and reopen Cloudsave to check that the saved credentials are available through your keyring. You may need to unlock it. For Hetzner Storage Box, enable WebDAV and external reachability. Subaccounts use their own hostname, such as `https://uXXXXX-sub1.your-storagebox.de` for user `uXXXXX-sub1`.

Under **Gesicherte Ordner** (Backed-up folders), clear a checkbox to pause backups for that folder. Use the minus button to remove it from the list. With live sync on, Cloudsave asks for confirmation before removing that folder's matching copies from the destinations. Your local originals are preserved. The minus button on a destination card stops backups to that destination and leaves its existing copies in place.

### Report feedback

Open a [GitHub issue](https://github.com/kilruna/Cloudsave/issues/new) to report a bug, a confusing interaction, or a display problem. These details help reproduce it.

```text
Version: link to the GitHub release you installed
System: distribution and version, desktop (GNOME, KDE, etc.), Wayland or X11
Backup: local or WebDAV destination, backup or mirror mode
Steps: what did you do, in order?
Expected: what should have happened?
Actual: what happened? Include the exact error or a screenshot if available.
```

Run `flatpak info org.cloudsave.Cloudsave` for installation details. For terminal messages, quit Cloudsave and launch it with `flatpak run org.cloudsave.Cloudsave`. Redact passwords, tokens, and private paths in reports and screenshots.

## Features

- **Guided setup.** Select your folders and a destination, then start the first sync from the setup assistant.
- **Multiple sources and destinations.** Add as many source folders as you need, and use their checkboxes to pause or resume backups.
- **Automatic change detection.** Cloudsave combines filesystem events with a periodic scan and batches changes before transferring them.
- **Incremental transfers.** Cloudsave copies new or changed files and repairs missing copies of files still present in the source.
- **Credentials in your keyring.** Cloudsave stores WebDAV passwords in the system keyring and removes legacy password files after successful migration.
- **Autostart and background sync.** Cloudsave can start hidden with `--background`. When background mode is enabled, closing the window hides it while sync continues. On supported desktops, the orange tray icon opens a menu with active transfers and the eight most recent jobs. Use **Cloudsave öffnen** (Open Cloudsave) to show the same window, or **Beenden** (Quit) to exit. With background mode off, closing the window exits the app.
- **Responsive during transfers.** Uploads and downloads run outside the UI thread.

The tray icon uses StatusNotifierItem, which KDE Plasma supports. GNOME requires an AppIndicator or StatusNotifier extension. You can also reopen a hidden window from your application launcher.

## Backup destinations

Cloudsave supports **local folders and USB drives** and **WebDAV over HTTPS**, including Hetzner Storage Box, Nextcloud, and ownCloud. Planned options include SFTP, SSH with rsync, FTP(S), S3-compatible object storage, and BorgBackup. [Protocol roadmap](docs/protocols.md)

## Technology

- **Rust** workspace with a sync engine (`cloudsave-core`), a desktop app (`cloudsave-app`), and a command-line tool (`cloudsave-cli`)
- **GTK 4 and libadwaita** for the desktop interface
- **Flatpak** with the GNOME SDK, read access to the home directory, and document-portal access to additional folders and writable destinations
- **SQLite** for sync state and **D-Bus / Secret Service** for credentials

## Project documentation

- [Project plan](docs/plan.md)
- [Supported and planned protocols](docs/protocols.md)
- [Architecture and tests](docs/architektur.md)
- [Setup and dashboard flow](docs/ux-flow.md)
- [Continuous integration](docs/ci.md)
- [Roadmap and current status](docs/roadmap-status.md)
- [GTK regression tests](tests/ui/README.md)

## Data handling

Cloudsave uploads to a temporary path before replacing an existing backup file. New profiles keep backup copies when source files are deleted. Profiles already set to mirroring keep that setting, which you can change in Settings.

Restoring files preserves existing local files. An unfinished restore pauses sync across restarts. After a successful restore to the original locations, live sync resumes in backup mode. It stays paused if files already exist or if you restore to a separate output folder. For older backups without saved folder mappings, Cloudsave suggests matches based on their top-level folder names and lets you assign external or unknown destinations yourself.
