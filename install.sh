#!/bin/bash
# Rune Editor Installation Script
# Usage: curl -fsSL https://exec.github.io/rune/install.sh | bash

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
        Linux*)  os="unknown-linux-gnu" ;;
        Darwin*) os="apple-darwin" ;;
        *)       echo -e "${RED}Error: Unsupported operating system$(uname -s)${NC}" >&2; exit 1 ;;
    esac
    
    case "$(uname -m)" in
        x86_64)  arch="x86_64" ;;
        arm64)   arch="aarch64" ;;
        aarch64) arch="aarch64" ;;
        *)       echo -e "${RED}Error: Unsupported architecture $(uname -m)${NC}" >&2; exit 1 ;;
    esac
    
    echo "${arch}-${os}"
}

# Get latest release version from GitHub API
get_latest_version() {
    local api_response
    api_response=$(curl -s "https://api.github.com/repos/${GITHUB_REPO}/releases/latest")
    echo "$api_response" | grep '"tag_name":' | sed -E 's/.*"tag_name": "([^"]+)".*/\1/'
}

# Download and install binary
install_rune() {
    local platform version download_url install_dir binary_path
    
    platform=$(detect_platform)
    version=$(get_latest_version)
    
    if [ -z "$version" ]; then
        echo -e "${RED}Error: Could not fetch latest version${NC}" >&2
        exit 1
    fi
    
    echo -e "${YELLOW}Installing Rune Editor ${version} for ${platform}...${NC}"
    
    download_url="https://github.com/${GITHUB_REPO}/releases/download/${version}/${BINARY_NAME}-${platform}"
    
    # Determine install directory
    if [ -w "/usr/local/bin" ]; then
        install_dir="/usr/local/bin"
    else
        install_dir="$HOME/.local/bin"
        mkdir -p "$install_dir"
    fi
    
    binary_path="${install_dir}/${BINARY_NAME}"
    
    echo -e "${YELLOW}Downloading from: ${download_url}${NC}"
    
    # Download binary
    if ! curl -fsSL "$download_url" -o "$binary_path"; then
        echo -e "${RED}Error: Failed to download binary${NC}" >&2
        exit 1
    fi
    
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