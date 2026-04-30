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

`node_modules`. Gradle caches. Cargo build artefacts. Dangling Docker images. Pip wheels. Maven local repositories. Left alone, these quietly consume tens of gigabytes. You know they're there — somewhere — but tracking them down takes more time than it's worth.

**Gluttony finds them all, shows you the damage, and cleans on your confirmation.**

## Usage

```bash
gluttony                  # Scan and display what can be cleaned
gluttony --list           # Show every detected path individually
gluttony --clean          # Interactive cherry-pick: select which artefacts to remove
gluttony --clean --all    # Remove everything (double confirmation required)
gluttony --dry-run        # Preview what would be removed without deleting anything
gluttony --path ~/code    # Scan a specific directory instead of home
gluttony --completions bash  # Generate shell completions (bash, zsh, fish, powershell, elvish)
```

### Flags

| Flag | Description |
|------|-------------|
| `--list` | List every detected path individually with size and type |
| `--clean` | Enter interactive mode to cherry-pick artefacts for deletion |
| `--clean --all` | Remove all detected artefacts without cherry-picking (asks twice) |
| `--dry-run` | Preview exact paths that would be removed without deleting |
| `--path <PATH>` | Root directory to scan (default: home directory) |
| `--completions <SHELL>` | Print shell completions and exit |

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
| `target/` | Rust and Maven build output |
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
| Docker images | Dangling and unused images |
| Cargo registry | Cached crates and compiled artifacts |
| Xcode derived data | iOS/macOS build cache |

Gluttony uses smart detection to avoid false positives — it checks for project markers (e.g., `package.json`, `Cargo.toml`) and git repository ancestry to distinguish developer artefacts from application-bundled ones (VS Code, JetBrains, etc.).

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

## Architecture

```
src/
├── main.rs          Entry point — wires CLI to business logic
├── cli.rs           Argument parsing and flag handling
├── display.rs       Result formatting, table display, and path rendering
├── error.rs         Domain error types
├── cleaner.rs       Interactive selection, confirmation flow, and parallel deletion
├── update.rs        Auto-update version check via GitHub API
└── scanner/
    ├── mod.rs       Public API, scan() orchestration, ArtifactKind/Artifact types
    ├── walker.rs    Parallel walk logic, classification dispatch, git-ancestry check
    ├── node.rs      JS ecosystem (node_modules, .next, .nuxt, .turbo, .parcel-cache)
    ├── python.rs    Python ecosystem (__pycache__, .pytest_cache, .venv, .tox)
    ├── build.rs     target/ (Cargo + Maven)
    ├── flutter.rs   Flutter build/
    ├── elixir.rs    Elixir _build/
    └── docker.rs    Docker data paths

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
| [`console`](https://crates.io/crates/console) | Terminal styling and colors |
| [`dialoguer`](https://crates.io/crates/dialoguer) | Interactive multi-select prompts |
| [`ureq`](https://crates.io/crates/ureq) | HTTP client for update checks |
| [`serde`](https://crates.io/crates/serde) / [`serde_json`](https://crates.io/crates/serde_json) | JSON deserialization for GitHub API |
| [`thiserror`](https://crates.io/crates/thiserror) | Ergonomic error type derivation |

## License

[MIT](LICENSE) — Aymeric CHAVEROT
