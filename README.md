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
gluttony           # Scan and display what can be cleaned
gluttony --clean   # Clean after interactive confirmation
```

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
| `.venv/`, `venv/` | Python virtual environments |
| Docker images | Dangling and unused images |
| Cargo registry | Cached crates and compiled artifacts |
| Xcode derived data | iOS/macOS build cache |

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
├── main.rs      Entry point — wires CLI to business logic
├── cli.rs       Argument parsing and flag handling
├── error.rs     Domain error types
├── scanner.rs   Recursive filesystem scanner with artefact detection
├── cleaner.rs   Confirmation flow and deletion logic
└── update.rs    Auto-update version check via GitHub API

scripts/
├── install.sh   Installer for macOS and Linux
└── install.ps1  Installer for Windows (PowerShell)
```

Each module has a single responsibility. No unsafe code.

## Dependencies

| Crate | Purpose |
|-------|---------|
| [`clap`](https://crates.io/crates/clap) | Command-line argument parsing |
| [`walkdir`](https://crates.io/crates/walkdir) | Recursive directory traversal |
| [`indicatif`](https://crates.io/crates/indicatif) | Progress bars and spinners |
| [`ureq`](https://crates.io/crates/ureq) | HTTP client for update checks |
| [`serde`](https://crates.io/crates/serde) / [`serde_json`](https://crates.io/crates/serde_json) | JSON deserialization for GitHub API |

## License

[MIT](LICENSE) — Aymeric CHAVEROT
