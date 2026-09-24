<div align="center">

<pre>
 ██████╗ ██╗     ██╗   ██╗████████╗████████╗ ██████╗ ███╗   ██╗██╗   ██╗
██╔════╝ ██║     ██║   ██║╚══██╔══╝╚══██╔══╝██╔═══██╗████╗  ██║╚██╗ ██╔╝
██║  ███╗██║     ██║   ██║   ██║      ██║   ██║   ██║██╔██╗ ██║ ╚████╔╝ 
██║   ██║██║     ██║   ██║   ██║      ██║   ██║   ██║██║╚██╗██║  ╚██╔╝  
╚██████╔╝███████╗╚██████╔╝   ██║      ██║   ╚██████╔╝██║ ╚████║   ██║   
 ╚═════╝ ╚══════╝ ╚═════╝    ╚═╝      ╚═╝    ╚═════╝ ╚═╝  ╚═══╝   ╚═╝   
</pre>

**Reclaim the disk space your toolchain ate without asking.**

[![CI](https://github.com/AymericChaverot/gluttony/actions/workflows/ci.yml/badge.svg)](https://github.com/AymericChaverot/gluttony/actions/workflows/ci.yml)
[![Release](https://github.com/AymericChaverot/gluttony/actions/workflows/release.yml/badge.svg)](https://github.com/AymericChaverot/gluttony/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-stable-orange.svg)](https://www.rust-lang.org/)

---

A scanner and cleaner for all the dev artefacts silently consuming your disk.

Built in Rust for speed, safety, and reliability.

</div>

---

## The Problem

`node_modules`. Cargo `target/` directories. Gradle caches. Python virtualenvs. The Go module cache. Left alone, these quietly consume tens of gigabytes. You know they're there — somewhere — but tracking them down takes more time than it's worth.

**Gluttony finds them all, shows you the damage, and cleans on your confirmation.**

## Usage

```bash
gluttony                          # Scan your home directory and show what can be reclaimed
gluttony ~/code                   # Scan a specific directory
gluttony --list                   # Every artefact with its size, last activity and path
gluttony --projects               # Breakdown per project (monorepos grouped by git root)
gluttony clean                    # Interactive picker: choose what to move to the trash
gluttony clean --older-than 3mo   # Only projects idle for at least 3 months
gluttony clean --only node --all  # Everything from one ecosystem, no picker
gluttony clean --dry-run          # Preview exact paths, touch nothing
gluttony clean --permanent        # Delete for good instead of using the trash
gluttony undo                     # Restore a previous clean session
gluttony trash                    # See what the trash holds
gluttony trash empty              # Permanently free the trash
gluttony completions zsh          # Shell completions (bash, zsh, fish, powershell, elvish)
```

### Filters

Available on both `scan` (the default command) and `clean`:

| Flag | Description |
|------|-------------|
| `--only <ECOSYSTEMS>` | Comma-separated: `node`, `rust`, `python`, `jvm`, `flutter`, `elixir`, `go`, `ruby`, `xcode` |
| `--exclude <TEXT>` | Skip paths containing this text (repeatable) |
| `--min-size <SIZE>` | Ignore artefacts smaller than this (`50MB`, `1.5G`) |
| `--older-than <AGE>` | Only projects idle for at least this long (`30d`, `2w`, `6mo`, `1y`) |

### Scan options

| Flag | Description |
|------|-------------|
| `-l`, `--list` | List every artefact with size, last activity and path |
| `-p`, `--projects` | Group artefacts by project |
| `--json` | Machine-readable output |

### Clean options

| Flag | Description |
|------|-------------|
| `-a`, `--all` | Take everything that matches, without the picker |
| `--dry-run` | Show what would be removed, touch nothing |
| `--permanent` | Delete for good instead of moving to the trash (asks you to type `delete`) |
| `-y`, `--yes` | Do not ask for confirmation |

### The picker

`gluttony clean` opens an inline picker with a live total of what is selected:

| Key | Action |
|-----|--------|
| `↑` `↓` / `j` `k` | Move |
| `space` | Toggle the current artefact |
| `a` / `n` / `i` | Select all / none / invert (visible rows) |
| `s` | Select stale artefacts (projects idle for 90+ days) |
| `/` | Filter by type or path |
| `enter` | Confirm |
| `esc` / `q` | Cancel |

### Last activity

Each artefact shows when its project was last worked on, derived from git activity (`.git/index`, `.git/logs/HEAD`) and the modification times of the project's top-level files. Projects active in the last week are highlighted, since cleaning them means a rebuild soon.

## Installation

### Quick install (recommended)

**macOS / Linux:**

```bash
curl -fsSL https://raw.githubusercontent.com/AymericChaverot/gluttony/main/scripts/install.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/AymericChaverot/gluttony/main/scripts/install.ps1 | iex
```

The scripts always download the **latest release** from GitHub.

### From source

```bash
git clone https://github.com/AymericChaverot/gluttony.git
cd gluttony
cargo install --path .
```

Requires [Rust](https://www.rust-lang.org/tools/install) (stable toolchain).

### Installation paths

| Platform | Method | Install path |
|----------|--------|-------------|
| macOS / Linux | Install script | `~/.gluttony/bin/gluttony` |
| Windows | Install script | `%USERPROFILE%\.gluttony\bin\gluttony.exe` |
| Any | `cargo install` | `~/.cargo/bin/gluttony` |

The install scripts automatically add the binary to your `PATH`. On Windows, restart your terminal after first install.

### Updating

Run the install script again — it always fetches the latest release and overwrites the existing binary.

## Targets

| Artefact | Description |
|----------|-------------|
| `node_modules/` | Node.js dependency trees |
| `.gradle/` | Gradle build caches |
| `target/` | Rust (Cargo) and Maven build output |
| `__pycache__/` | Python bytecode caches |
| `.pytest_cache/` | Pytest test caches |
| `.venv/`, `venv/` | Python virtual environments |
| `.tox/` | Tox test environments |
| `.next/` | Next.js build cache |
| `.nuxt/` | Nuxt.js build cache |
| `.turbo/` | Turborepo cache |
| `.parcel-cache/` | Parcel bundler cache |
| `build/` (Flutter) | Flutter build output |
| `_build/` (Elixir) | Elixir/Mix build output |
| Go module cache | `$GOPATH/pkg/mod` (default `~/go/pkg/mod`) |
| Ruby gems | `~/.gem/ruby` |
| Docker | Reported only, see [Docker](#docker) |
| Cargo registry | Cached crates and compiled artifacts |
| Xcode derived data | iOS/macOS build cache |

Gluttony uses smart detection to avoid false positives — it checks for project markers (e.g., `package.json`, `Cargo.toml`) and git repository ancestry to distinguish developer artefacts from application-bundled ones (VS Code, JetBrains, etc.).

## Docker

Gluttony reports Docker's disk usage (and what `docker system prune` could reclaim when the daemon is running), but **never touches it**: Docker's data lives in a VM disk image or a daemon-owned directory, and moving it would break Docker without freeing space. Prune it through Docker itself:

```bash
docker system prune -a
```

## Platform Support

| Platform | Architecture | Status |
|----------|-------------|--------|
| Linux    | x86_64      | Fully supported |
| macOS    | x86_64      | Fully supported |
| macOS    | aarch64 (Apple Silicon) | Fully supported |
| Windows  | x86_64      | Fully supported |

## CI/CD

Every push and pull request triggers the CI pipeline:

- **Format** — `cargo fmt --check`
- **Lint** — `cargo clippy -D warnings`
- **Test** — Cross-platform tests on Linux, macOS, and Windows
- **Coverage** — `cargo-llvm-cov` with 80%+ line coverage
- **Build** — Release builds for all supported platforms

Pushing a version tag (`v*`) triggers the release pipeline:

- Builds optimized binaries for all 4 platform targets
- Packages them as `.tar.gz` (Unix) or `.zip` (Windows)
- Creates a GitHub Release with auto-generated release notes
- Attaches all binaries to the release

## Development

### Build

```bash
cargo build --release
```

### Test

```bash
cargo test
```

### Lint

```bash
cargo clippy -- -D warnings
```

### Format

```bash
cargo fmt
```

### Coverage

```bash
cargo llvm-cov
```

### Creating a release

```bash
git tag v0.1.0
git push origin v0.1.0
```

The CD pipeline handles the rest.

## Trash & Undo

By default Gluttony never deletes anything outright: `gluttony clean` **moves artefacts to `~/.gluttony/trash/`** and records the session in a manifest. Keep in mind that trashed files still occupy the disk until the trash is emptied; every scan shows how much the trash holds.

```bash
gluttony undo          # pick a session and move everything back
gluttony trash         # list sessions, their size and expiry
gluttony trash empty   # free the space for good (asks you to type `empty`)
```

Sessions expire after **30 days** and are purged automatically on the next run. Restoring never overwrites: if a destination already exists (say you ran `npm install` again), that item stays in the trash and the rest is restored.

To skip the trash entirely, use `gluttony clean --permanent`.

## Configuration

| Variable | Effect |
|----------|--------|
| `NO_COLOR` | Disable colours |
| `GLUTTONY_NO_UPDATE_CHECK` | Disable the background update check (otherwise at most one request a day, 2 s timeout) |

## Architecture

```
src/
├── main.rs          Entry point, wires commands to business logic
├── cli.rs           Subcommands, flags, size/age parsers
├── ui.rs            Visual language: symbols, colours, formatting, prompts
├── picker.rs        Inline multi/single select with live totals and filtering
├── display.rs       Summary table, list, project view, JSON, removal plan
├── cleaner.rs       Selection, confirmation, parallel trash/delete with progress
├── trash.rs         Trash sessions, manifest, undo/restore, emptying
├── update.rs        Background, cached update check via the GitHub API
├── error.rs         Domain error types
└── scanner/
    ├── mod.rs       scan() orchestration, kinds, ecosystems, filters, projects
    ├── walker.rs    Parallel walk, classification dispatch, git-ancestry check
    ├── node.rs      JS ecosystem (node_modules, .next, .nuxt, .turbo, .parcel-cache)
    ├── python.rs    Python ecosystem (__pycache__, .pytest_cache, .venv, .tox)
    ├── build.rs     target/ (Cargo, Maven)
    ├── flutter.rs   Flutter build/
    ├── elixir.rs    Elixir _build/
    ├── go.rs        Go module cache
    ├── ruby.rs      Ruby gems
    └── docker.rs    Docker footprint (reported, never cleaned)

scripts/
├── install.sh       Installer for macOS and Linux
└── install.ps1      Installer for Windows (PowerShell)
```

Each module has a single responsibility. No unsafe code.

## Dependencies

| Crate | Purpose |
|-------|---------|
| [`clap`](https://crates.io/crates/clap) | Command-line argument parsing |
| [`clap_complete`](https://crates.io/crates/clap_complete) | Shell completions generation |
| [`walkdir`](https://crates.io/crates/walkdir) | Recursive directory traversal |
| [`rayon`](https://crates.io/crates/rayon) | Parallel scanning and deletion |
| [`indicatif`](https://crates.io/crates/indicatif) | Progress bars and spinners |
| [`console`](https://crates.io/crates/console) | Terminal styling, key input, the picker |
| [`ctrlc`](https://crates.io/crates/ctrlc) | Restores the cursor on interrupt |
| [`fs4`](https://crates.io/crates/fs4) | Free disk space before/after a permanent clean |
| [`ureq`](https://crates.io/crates/ureq) | HTTP client for update checks |
| [`serde`](https://crates.io/crates/serde) / [`serde_json`](https://crates.io/crates/serde_json) | Manifest, cache and `--json` output |
| [`thiserror`](https://crates.io/crates/thiserror) | Ergonomic error type derivation |

## License

[MIT](LICENSE) — Aymeric CHAVEROT
