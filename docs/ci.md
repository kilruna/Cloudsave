# CI-Plan für Cloudsave

Plan für eine saubere, schrittweise aufgebaute CI (GitHub Actions). Jede Phase ist einzeln implementierbar und landet als eigener Workflow unter `.github/workflows/`. App-ID und Runtime folgen der bestehenden Doku: `org.cloudsave.Cloudsave` auf GNOME-Runtime.

## 1. Ziele

- Jeder Pull Request bekommt schnelle, verlässliche Checks: Doku-Lint, Rust (fmt/clippy/test), Flatpak-Build.
- Reproduzierbar: Toolchain über `rust-toolchain.toml` gepinnt, Actions über Major-Tags gepinnt (aktualisiert via Dependabot), Crates für den Flatpak-Build vendored.
- Sicher: least-privilege `permissions`, keine Secrets im Repo, laufende Runs bei neuem Push abbrechen.
- Schlank: Path-Filter, damit Doku-Änderungen keinen Rust-Build auslösen (und umgekehrt).

## 2. Grundsätze

- **Ein Workflow pro Thema**, klarer `name:` – so sind die Check-Namen im PR selbsterklärend.
- Überall `permissions: contents: read`; `contents: write` nur im Release-Workflow.
- Überall `concurrency` mit `cancel-in-progress: true` (beim Release-Workflow `false`).
- Triggers: `push` + `pull_request`, jeweils mit `paths`-Filter.
- Gleiche Befehle lokal wie in CI (siehe Abschnitt 9, Makefile).

## 3. Workflow-Übersicht

| Workflow | Trigger | Jobs | Phase |
|---|---|---|---|
| `markdownlint.yml` | push/PR (nur `*.md`, Config, Workflow) | Markdown-Lint | 0 – bereits umgesetzt |
| `rust.yml` | push/PR (nur `crates/**`, `Cargo.*`, `rust-toolchain.toml`) | fmt, clippy, test, deny | 1 – umgesetzt |
| `flatpak.yml` | push/PR (nur `flatpak/**`, `crates/**`, `data/**`, `Cargo.*`) | Flatpak-Build + Artifact | 2 – umgesetzt |
| `release.yml` | Tag `v*` | Bundle bauen, SHA256, GitHub Release | 3 – mit M3 |
| Dependabot | wöchentlich | PRs für Actions + Cargo-Crates | 1 – mit M0 |

## 4. Phase 0 – Doku-Lint (umgesetzt)

`markdownlint.yml` + `.markdownlint.json` sind bereits eingerichtet und grün (PR-Check „Markdown-Lint"). Regeln MD013/MD040/MD060 sind deaktiviert (lange Tabellenzeilen, ASCII-Diagramme, kompakter Tabellenstil). Neue Doku-Dateien werden automatisch mitgelintet.

## 5. Phase 1 – Rust-CI (umgesetzt, zusammen mit dem M0-Skelett)

Bereits angelegt: `rust-toolchain.toml` (Channel `stable`), `deny.toml` für `cargo-deny` (Lizenz-Allowlist, yanked = deny), minimales Workspace-Skelett (`crates/cloudsave-core`), Makefile und Dependabot.

Gerüst für `.github/workflows/rust.yml`:

```yaml
name: Rust

on:
  push:
  pull_request:
    paths:
      - 'crates/**'
      - 'Cargo.toml'
      - 'Cargo.lock'
      - 'rust-toolchain.toml'
      - 'deny.toml'
      - '.github/workflows/rust.yml'

permissions:
  contents: read

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  fmt:
    name: fmt
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo fmt --all -- --check

  clippy:
    name: clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          shared-key: ci
      - run: cargo clippy --workspace --all-targets --locked -- -D warnings

  test:
    name: test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
      - uses: taiki-e/install-action@nextest
      - uses: Swatinem/rust-cache@v2
        with:
          shared-key: ci
      - run: cargo nextest run --workspace --locked

  deny:
    name: deny
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
      - uses: taiki-e/install-action@cargo-deny
      - run: cargo deny check
```

Hinweise:

- `dtolnay/rust-toolchain@stable` installiert die Toolchain; die rustup-Shims ehren anschließend automatisch die gepinnte `rust-toolchain.toml`.
- `Swatinem/rust-cache@v2` mit `shared-key: ci` teilt den Cargo-Cache (registry + target) zwischen den Jobs – spart etliche Minuten pro Run.
- `--locked` stellt sicher, dass CI exakt gegen den committeten `Cargo.lock` baut.
- Tests über `cargo nextest` (schneller, bessere Ausgabe); Plain-`cargo test` ist als Fallback ebenso ok.
- `deny` prüft Lizenzen und Security-Advisories der Abhängigkeiten (`deny.toml` im Repo).

## 6. Phase 2 – Flatpak-Build-CI (umgesetzt)

`.github/workflows/flatpak.yml` – baut das Flatpak in einem fertigen Flatpak-Container und lädt das Bundle als Artifact hoch (zum manuellen Testen aus dem PR). Manifest: `flatpak/org.cloudsave.Cloudsave.yml` (GNOME 50), App-Skelett: `crates/cloudsave-app` (Binary `cloudsave`), Data-Dateien unter `data/`:

```yaml
name: Flatpak

on:
  push:
  pull_request:
    paths:
      - 'flatpak/**'
      - 'crates/**'
      - 'Cargo.lock'
      - '.github/workflows/flatpak.yml'

permissions:
  contents: read

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  build:
    name: flatpak-builder
    runs-on: ubuntu-latest
    container:
      image: ghcr.io/flathub-infra/flatpak-github-actions:gnome-50
      options: --privileged
    steps:
      - uses: actions/checkout@v7
      - name: Vendored Cargo-Sources aktuell
        run: |
          flatpak-cargo-generator Cargo.lock -o flatpak/cargo-sources.json
          git diff --exit-code -- flatpak/cargo-sources.json
      - uses: flatpak/flatpak-github-actions/flatpak-builder@v6
        with:
          manifest-path: flatpak/org.cloudsave.Cloudsave.yml
          bundle: cloudsave.flatpak
          cache-key: flatpak-builder-${{ github.sha }}
      - uses: actions/upload-artifact@v4
        with:
          name: cloudsave.flatpak
          path: cloudsave.flatpak
          retention-days: 14
```

Hinweise:

- Container-Image und GNOME-Version an das Manifest anpassen (z. B. `gnome-50`); [flathub-infra/actions-images](https://github.com/flathub-infra/actions-images) listet verfügbare Tags.
- `flatpak/cargo-sources.json` (Output von `flatpak-cargo-generator`) wird committhet; der Schritt „Vendored Cargo-Sources aktuell“ stellt sicher, dass er bei `Cargo.lock`-Änderungen neu erzeugt wurde.
- Actions: [flatpak/flatpak-github-actions](https://github.com/flatpak/flatpak-github-actions) (`flatpak-builder@v6`).

## 7. Phase 3 – Release & Flathub (mit M3)

`.github/workflows/release.yml`, läuft nur auf Tags `v*`:

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

permissions:
  contents: write

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: false

jobs:
  release:
    name: Release
    runs-on: ubuntu-latest
    environment: release
    container:
      image: ghcr.io/flathub-infra/flatpak-github-actions:gnome-50
      options: --privileged
    steps:
      - uses: actions/checkout@v7
      - uses: flatpak/flatpak-github-actions/flatpak-builder@v6
        with:
          manifest-path: flatpak/org.cloudsave.Cloudsave.yml
          bundle: cloudsave.flatpak
      - run: sha256sum cloudsave.flatpak > cloudsave.flatpak.sha256
      - uses: softprops/action-gh-release@v2
        with:
          files: |
            cloudsave.flatpak
            cloudsave.flatpak.sha256
```

- Das GitHub-`environment: release` kann in den Repo-Einstellungen Required Reviewers bekommen – so wird kein Release ohne Freigabe gebaut.
- Flathub-Veröffentlichung läuft separat über das Flathub-Repo (`flathub/org.cloudsave.Cloudsave`), das auf den Tag im GitHub-Repo zeigt; optional direkter Deploy per `flatpak/flatpak-github-actions/flat-manager@v6` mit Token aus dem Secret `FLATHUB_TOKEN`.

## 8. Abhängigkeiten aktuell halten

`.github/dependabot.yml`:

```yaml
version: 2
updates:
  - package-ecosystem: github-actions
    directory: /
    schedule:
      interval: weekly
  - package-ecosystem: cargo
    directory: /
    schedule:
      interval: weekly
```

Damit werden Action-Versionen (Major-Tags) und Cargo-Crates wöchentlich als PR vorgeschlagen.

## 9. Lokale Parität

`Makefile` im Repo-Root, damit lokal exakt dieselben Befehle laufen wie in CI:

<!-- markdownlint-disable MD010 -->
```make
.PHONY: ci flatpak-build

ci:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets --locked -- -D warnings
	cargo nextest run --workspace --locked
	cargo deny check

flatpak-build:
	flatpak-builder --user --force-clean _build flatpak/org.cloudsave.Cloudsave.yml
```
<!-- markdownlint-enable MD010 -->

## 10. Repo-Einstellungen (nicht Teil des Git-Repos)

- Branch Protection für den Default-Branch: Pull Request erforderlich, Force-Push verboten, Status Checks required: `Markdown-Lint`, `fmt`, `clippy`, `test` (später `flatpak-builder`, `deny`).
- Secret Scanning + Push Protection aktivieren.
- Secrets: Für die Basis-CI werden keine Secrets benötigt. Erst für Flathub-Deploys: `FLATHUB_TOKEN` als Repository-Secret, nur im Environment `release` freigegeben. Niemals in Dateien oder Logs.

## 11. Phasenplan & Abnahmekriterien

| Phase | Inhalt | Abnahmekriterium |
|---|---|---|
| 0 | `markdownlint.yml` + `.markdownlint.json` | PR-Check grün – erfüllt |
| 1 | `rust-toolchain.toml`, `deny.toml`, `rust.yml`, `dependabot.yml`, Makefile | erster Code-PR hat grüne Checks fmt/clippy/test/deny |
| 2 | `flatpak.yml`, `flatpak/cargo-sources.json` | PR mit Manifest-Änderung baut `cloudsave.flatpak` als Artifact |
| 3 | `release.yml` + Environment `release` | Tag `v0.1.0` erzeugt GitHub Release mit `.flatpak` + SHA256 |

Optional später: CodeQL (Rust-Support) als weiterer Check, Nightly/MSRV-Matrix für Tests.
