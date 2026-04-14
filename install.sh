#!/bin/bash
# Rune Editor Installation Script
# Usage: curl -fsSL https://rune.byexec.com/install.sh | bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'  
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# GitHub repository info
GITHUB_REPO="exec/rune"
BINARY_NAME="rune"

# Detect OS and architecture
detect_platform() {
    local os arch

    case "$(uname -s)" in
        Linux*)  os="linux" ;;
        Darwin*) os="macos" ;;
        FreeBSD*) os="freebsd" ;;
        NetBSD*) os="netbsd" ;;
        *)       echo -e "${RED}Error: Unsupported operating system: $(uname -s)${NC}" >&2; exit 1 ;;
    esac

    case "$(uname -m)" in
        x86_64)  arch="x86_64" ;;
        arm64)   arch="aarch64" ;;
        aarch64) arch="aarch64" ;;
        armv7l)  arch="armv7" ;;
        i386|i686) arch="i686" ;;
        *)       echo -e "${RED}Error: Unsupported architecture: $(uname -m)${NC}" >&2; exit 1 ;;
    esac

    echo "${os}-${arch}"
}

# Get latest release version from GitHub API
get_latest_version() {
    local api_response
    api_response=$(curl -s "https://api.github.com/repos/${GITHUB_REPO}/releases/latest")
    echo "$api_response" | grep '"tag_name":' | sed -E 's/.*"tag_name": "([^"]+)".*/\1/'
}

# Download and install binary
install_rune() {
    local platform version download_url install_dir binary_path temp_dir

    platform=$(detect_platform)
    version=$(get_latest_version)

    if [ -z "$version" ]; then
        echo -e "${RED}Error: Could not fetch latest version${NC}" >&2
        exit 1
    fi

    echo -e "${YELLOW}Installing Rune Editor ${version} for ${platform}...${NC}"

    download_url="https://github.com/${GITHUB_REPO}/releases/download/${version}/${BINARY_NAME}-${platform}.tar.gz"

    # Determine install directory
    if [ -w "/usr/local/bin" ]; then
        install_dir="/usr/local/bin"
    else
        install_dir="$HOME/.local/bin"
        mkdir -p "$install_dir"
    fi

    # Create temporary directory for extraction
    temp_dir=$(mktemp -d)

    echo -e "${YELLOW}Downloading from: ${download_url}${NC}"

    # Download and extract tarball
    if ! curl -fsSL "$download_url" | tar -xzf - -C "$temp_dir"; then
        echo -e "${RED}Error: Failed to download or extract binary${NC}" >&2
        rm -rf "$temp_dir"
        exit 1
    fi

    # Move binary to install location
    binary_path="${install_dir}/${BINARY_NAME}"
    mv "${temp_dir}/${BINARY_NAME}" "$binary_path"
    rm -rf "$temp_dir"

    # Make executable
    chmod +x "$binary_path"

    echo -e "${GREEN}✓ Rune Editor installed to ${binary_path}${NC}"

    # Check if install_dir is in PATH
    if ! echo "$PATH" | grep -q "$install_dir"; then
        echo -e "${YELLOW}Warning: ${install_dir} is not in your PATH${NC}"
        echo -e "${YELLOW}Add this to your shell profile:${NC}"
        echo -e "${YELLOW}  export PATH=\"${install_dir}:\$PATH\"${NC}"
    fi

    # Verify installation
    if command -v rune >/dev/null 2>&1; then
        echo -e "${GREEN}✓ Installation successful! Run 'rune' to start.${NC}"
    else
        echo -e "${YELLOW}Installation complete. You may need to restart your shell or update PATH.${NC}"
    fi
}

# Main
main() {
    echo -e "${GREEN}Rune Editor Installer${NC}"
    echo "====================="
    
    # Check dependencies
    if ! command -v curl >/dev/null 2>&1; then
        echo -e "${RED}Error: curl is required but not installed${NC}" >&2
        exit 1
    fi
    
    install_rune
}

main "$@"