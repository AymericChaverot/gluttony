#!/bin/sh
# Gluttony installer for macOS and Linux
# Usage: curl -fsSL https://raw.githubusercontent.com/AymericChaverot/gluttony/main/scripts/install.sh | sh

set -e

REPO="AymericChaverot/gluttony"
INSTALL_DIR="$HOME/.gluttony/bin"
BINARY_NAME="gluttony"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { printf "${GREEN}[*]${NC} %s\n" "$1"; }
warn()  { printf "${YELLOW}[!]${NC} %s\n" "$1"; }
error() { printf "${RED}[x]${NC} %s\n" "$1"; exit 1; }

# Detect OS and architecture
detect_target() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"

    case "$OS" in
        Linux)
            case "$ARCH" in
                x86_64) echo "x86_64-unknown-linux-gnu" ;;
                *)      error "Unsupported architecture: $ARCH. Only x86_64 is supported on Linux." ;;
            esac
            ;;
        Darwin)
            case "$ARCH" in
                arm64|aarch64) echo "aarch64-apple-darwin" ;;
                x86_64)        echo "x86_64-apple-darwin" ;;
                *)             error "Unsupported architecture: $ARCH" ;;
            esac
            ;;
        *)
            error "Unsupported OS: $OS. Use scripts/install.ps1 for Windows."
            ;;
    esac
}

# Fetch the latest release version
fetch_latest_version() {
    URL="https://api.github.com/repos/${REPO}/releases/latest"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$URL" | grep '"tag_name"' | head -1 | sed -E 's/.*"tag_name":\s*"([^"]+)".*/\1/'
    elif command -v wget >/dev/null 2>&1; then
        wget -qO- "$URL" | grep '"tag_name"' | head -1 | sed -E 's/.*"tag_name":\s*"([^"]+)".*/\1/'
    else
        error "Neither curl nor wget found. Please install one of them."
    fi
}

# Download and install
install() {
    TARGET="$(detect_target)"
    info "Detected platform: $TARGET"

    info "Fetching latest release..."
    VERSION="$(fetch_latest_version)"
    if [ -z "$VERSION" ]; then
        error "Could not determine latest version. Check your internet connection or if the repository has releases."
    fi
    info "Latest version: $VERSION"

    ASSET="${BINARY_NAME}-${TARGET}.tar.gz"
    URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET}"
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT

    info "Downloading $ASSET..."
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$URL" -o "${TMP}/${ASSET}" || error "Download failed. Check if a release exists for your platform: $TARGET"
    else
        wget -q "$URL" -O "${TMP}/${ASSET}" || error "Download failed. Check if a release exists for your platform: $TARGET"
    fi

    info "Extracting..."
    tar xzf "${TMP}/${ASSET}" -C "$TMP"

    mkdir -p "$INSTALL_DIR"
    mv "${TMP}/${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"
    chmod +x "${INSTALL_DIR}/${BINARY_NAME}"
    info "Installed $BINARY_NAME $VERSION to $INSTALL_DIR"

    # PATH configuration
    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*)
            info "$INSTALL_DIR is already in your PATH." ;;
        *)
            warn "$INSTALL_DIR is not in your PATH."

            SHELL_CONFIG=""
            case "${SHELL:-}" in
                */zsh)  SHELL_CONFIG="$HOME/.zshrc" ;;
                */bash)
                    if [ -f "$HOME/.bash_profile" ]; then
                        SHELL_CONFIG="$HOME/.bash_profile"
                    else
                        SHELL_CONFIG="$HOME/.bashrc"
                    fi
                    ;;
                */fish) SHELL_CONFIG="$HOME/.config/fish/config.fish" ;;
            esac

            if [ -n "$SHELL_CONFIG" ]; then
                printf '\n# Added by gluttony installer\n' >> "$SHELL_CONFIG"
                case "$SHELL_CONFIG" in
                    *fish*)
                        printf 'set -gx PATH $PATH %s\n' "$INSTALL_DIR" >> "$SHELL_CONFIG" ;;
                    *)
                        printf 'export PATH="$PATH:%s"\n' "$INSTALL_DIR" >> "$SHELL_CONFIG" ;;
                esac
                info "Added $INSTALL_DIR to PATH in $SHELL_CONFIG"
                warn "Run 'source $SHELL_CONFIG' or restart your terminal to apply."
            else
                warn "Could not detect shell config. Add this line manually to your shell profile:"
                printf '  export PATH="$PATH:%s"\n' "$INSTALL_DIR"
            fi
            ;;
    esac
}

# Verify installation
verify() {
    if command -v "$BINARY_NAME" >/dev/null 2>&1; then
        info "Verification: $($BINARY_NAME --version)"
    else
        FULL_PATH="${INSTALL_DIR}/${BINARY_NAME}"
        if [ -x "$FULL_PATH" ]; then
            info "Verification: $($FULL_PATH --version)"
        fi
    fi
    printf "\n${GREEN}Installation complete.${NC} Run '${BINARY_NAME} --help' to get started.\n\n"
}

main() {
    printf "\n"
    printf "  ╔═╗╦  ╦ ╦╔╦╗╔╦╗╔═╗╔╗╔╦ ╦\n"
    printf "  ║ ╦║  ║ ║ ║  ║ ║ ║║║║╚╦╝\n"
    printf "  ╚═╝╩═╝╚═╝ ╩  ╩ ╚═╝╝╚╝ ╩ \n"
    printf "  Installer\n\n"

    install
    verify
}

main
