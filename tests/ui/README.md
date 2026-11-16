# GTK regression tests for issue #10

`run_ui_tests.py` and the test-only `gtk_qa_bridge.c` originate from the
[report and complete test code in issue #10](https://github.com/kilruna/Cloudsave/issues/10).
The bridge activates real GTK widgets on the main thread. The tests use the real
sync engine, isolated HOME/XDG profiles, synthetic files, and two local targets.
No private data or credentials are needed.

The original scenarios are retained, with UI-03 updated for discovery of real home
folders: existing English/custom names are offered alphabetically without automatic
selection; files, hidden folders, nested descendants, and nonexistent standard
folders are excluded. UI-22/23 cover empty and unavailable home directories.
Source deselection and source loss now
include a process restart to verify persisted state. URL validation includes
empty, whitespace, malformed, HTTP, FTP, and valid HTTPS cases. UI-05 accepts the
explicit browse control required by the report, instead of requiring a radio
button that is already active to emit `toggled` again.

## Installed Flatpak

Requires the newly built and installed `org.cloudsave.Cloudsave`, an X11/XWayland
display, Python 3 with GI/Gdk 3, a C compiler, `xdotool`, and `xwininfo`.

```sh
mkdir -p work/ui-tmp
TMPDIR="$PWD/work/ui-tmp" /usr/bin/python3 tests/ui/run_ui_tests.py \
  --output "$PWD/work/flatpak-ui-results"
```

The suite sets HOME/XDG **inside** the sandbox because Flatpak overrides host
XDG environment values. It verifies the running process environment before
performing actions and disables real home access, session bus access, network, and autostart writes.
Do not substitute a real profile or real targets. Each output directory must be new.

## Native build

For a build environment without Flatpak namespaces, the explicit `--binary`
option tests the native executable. Its metadata says `mode: native`; this does
not certify an installed Flatpak. Run in a disposable environment with networking
isolated by the caller. Debian 13 provides suitable GTK 4.18/libadwaita 1.7 packages:

```sh
apt-get install build-essential pkg-config libgtk-4-dev libadwaita-1-dev \
  python3-gi gir1.2-gtk-3.0 xvfb xauth xdotool x11-utils dbus-x11
cargo build --workspace --locked
mkdir -p work/ui-tmp
TMPDIR="$PWD/work/ui-tmp" GSK_RENDERER=cairo xvfb-run -a \
  /usr/bin/python3 tests/ui/run_ui_tests.py --binary target/debug/cloudsave \
  --output "$PWD/work/native-ui-results"
```

The native mode adds three scenarios using GTK's actual fallback folder chooser:
selection, cancellation, reselection, radio switching, window controls on all
wizard pages, an explicit first sync, and adding a second endpoint. This uses
widget actions plus `xdotool` to open/submit the chooser's location entry.
It does not simulate chooser success or replace the filesystem/backend.

The production Flatpak grants read-only access to the user's home so setup can
enumerate its immediate visible folders. The harness explicitly revokes that
permission and grants only its synthetic fixture directory instead. No recursive
home scan or automatic source selection is performed by setup.

Exit status: **0** all checks passed, **1** product failures, **2** harness/setup
error. Outputs include JSON results, binary SHA-256, widget trees, screenshots,
and stderr logs. The generated bridge library is ignored by Git.

## Coverage and remaining manual checks

| Finding | Fix and regression coverage |
|---|---|
| F1 | Persist per-file source ownership; mirror deletes only for selected, fully scanned sources. UI-14/15 plus Core tests for unavailable/deselected sources, partial scan permissions, restart, legacy migration, namespace collisions, and intentional deletes. |
| F2 | Exclusive randomized temporary file with RAII cleanup. UI-13 and repeated Core sentinel/cleanup checks. |
| F3 | Explicit reusable browse button, remembered local path, cancellation preserves previous selection. UI-05/19/21. |
| F4 | Normal startup honors `auto_sync`; explicit setup still starts its first sync. UI-09/11/20. |
| F5 | Complete errors, source/target paths, and retry guidance are visible and selectable; error progress resets. UI-16, Core destination-write and HTTP-401 tests. |
| F6 | Core rejects empty targets; UI shows setup required and disables sync. UI-17 and Core empty-target/valid-empty-source tests. |
| F7 | Parse URL and require HTTPS/host; inline errors; no mandatory live connection test. UI-07/21 and Core syntax table. |
| F8 | Page headers hide their window controls and keep back/title navigation; application titlebar owns controls. UI-02/19/21. |

Legacy state has no source ownership. Unknown entries are conservatively retained;
observing a file in a later scan records its owner, enabling subsequent intentional
delete mirroring. No automatic purge of deselected sources is introduced.

These tests do **not** establish native Wayland dragging/maximizing, real Flatpak
portal selection/cancellation or grants, Secret Service, production autostart, or
authenticated remote WebDAV transfers. The HTTP-401 test uses a local mock server.
Before declaring the installed Flatpak verified, build/install the new revision,
run the default suite against that installation, and manually test the real
portal and Wayland window operations requested by the report.

## Roadmap-Regressionsabdeckung

UI-24 prüft einen nicht verfügbaren Schlüsselbund (Fehler sichtbar, keine neue
Klartextdatei/Config). UI-25 prüft Backup-/Spiegelungsmodus über einen Neustart und
zwei Ziele. UI-26 prüft Ausschlussmuster und ungültige Eingaben. UI-27 prüft echte
Dateiänderungen mit automatischem Transfer und Auto-Sync AUS. UI-28 verwendet die
Wiederherstellungsansicht und den tatsächlichen GTK-Zielordnerdialog für Unicode-
Dateien und prüft den Schutz vorhandener Dateien.

Der echte Secret-Service-Roundtrip ist zusätzlich als gezielt ausführbarer Core-
Integrationstest vorhanden (isolierter, entsperrter GNOME Keyring/KWallet nötig):

```sh
cargo test -p cloudsave-core system_keyring_roundtrip -- --ignored
```

Die normalen Core-Tests ersetzen für Migration/Fehler nur den Credential-Store;
Sync-/Restore-Dateioperationen und die lokalen HTTP-/Dateisystemtests sind echt.
