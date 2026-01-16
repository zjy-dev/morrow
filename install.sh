#!/bin/bash
set -e

REPO="zjy-dev/morrow"
INSTALL_DIR="${MORROW_INSTALL_DIR:-$HOME/.local/bin}"
CONFIG_DIR=""

detect_platform() {
    local os arch
    os=$(uname -s | tr '[:upper:]' '[:lower:]')
    arch=$(uname -m)

    case "$os" in
        linux) os="linux" ;;
        darwin) os="darwin" ;;
        *) echo "Unsupported OS: $os"; exit 1 ;;
    esac

    case "$arch" in
        x86_64|amd64) arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *) echo "Unsupported architecture: $arch"; exit 1 ;;
    esac

    echo "${os}-${arch}"
}

get_config_dir() {
    case "$(uname -s)" in
        Darwin)
            echo "$HOME/Library/Application Support/morrow"
            ;;
        *)
            echo "$HOME/.config/morrow"
            ;;
    esac
}

main() {
    echo "Installing morrow..."

    platform=$(detect_platform)
    echo "Detected platform: $platform"

    # Create install directory
    mkdir -p "$INSTALL_DIR"

    # Get latest release
    if command -v curl &> /dev/null; then
        latest=$(curl -sL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')
    elif command -v wget &> /dev/null; then
        latest=$(wget -qO- "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')
    else
        echo "Error: curl or wget required"
        exit 1
    fi

    if [ -z "$latest" ]; then
        echo "Error: Could not determine latest version"
        exit 1
    fi

    echo "Latest version: $latest"

    # Download binary
    url="https://github.com/$REPO/releases/download/$latest/morrow-$platform"
    echo "Downloading from: $url"

    if command -v curl &> /dev/null; then
        curl -sL "$url" -o "$INSTALL_DIR/morrow"
    else
        wget -q "$url" -O "$INSTALL_DIR/morrow"
    fi

    chmod +x "$INSTALL_DIR/morrow"

    # Download default config if not exists
    CONFIG_DIR=$(get_config_dir)
    if [ ! -f "$CONFIG_DIR/config.yaml" ]; then
        echo "Downloading default configuration..."
        mkdir -p "$CONFIG_DIR"
        config_url="https://raw.githubusercontent.com/$REPO/main/config.example.yaml"
        if command -v curl &> /dev/null; then
            curl -sL "$config_url" -o "$CONFIG_DIR/config.yaml"
        else
            wget -q "$config_url" -O "$CONFIG_DIR/config.yaml"
        fi
        echo "Default config created at: $CONFIG_DIR/config.yaml"
    else
        echo "Config file already exists at: $CONFIG_DIR/config.yaml"
    fi

    echo ""
    echo "morrow installed to: $INSTALL_DIR/morrow"
    echo ""

    # Check if in PATH
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        echo "Add this to your shell profile (.bashrc, .zshrc, etc.):"
        echo ""
        echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
        echo ""
    fi

    echo "Run 'morrow --help' to get started."
    echo "Run 'morrow config init' to customize your configuration."
}

main "$@"
