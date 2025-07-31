# GitHub Repository Setup Guide

## 1. Create Main Repository: `exec/rune`

1. Create new repository on GitHub: `exec/rune`
2. Initialize with the files in this directory
3. Set up repository settings:
   - Description: "A modern CLI text editor that bridges the gap between nano's simplicity and advanced features"
   - Topics: `rust`, `editor`, `terminal`, `cli`, `nano`, `text-editor`
   - License: MIT

## 2. Create Homebrew Tap: `exec/homebrew-rune`

1. Create new repository: `exec/homebrew-rune`
2. Follow the detailed instructions in `HOMEBREW_SETUP.md`

## 3. Required GitHub Secrets

In the main repository (`exec/rune`), add these secrets:

### Required Secrets:
- `HOMEBREW_TAP_TOKEN`: Personal Access Token with write access to `exec/homebrew-rune`

To create the token:
1. Go to GitHub Settings → Developer settings → Personal access tokens
2. Generate new token (classic)
3. Select scopes: `repo` (full repository access)
4. Copy the token and add it as a secret in the main repository

## 4. GitHub Actions Overview

### CI Workflow (`.github/workflows/ci.yml`)
- Runs on every push and PR
- Checks formatting, clippy lints, tests, and builds
- Uses cargo caching for faster builds

### Release Workflow (`.github/workflows/release.yml`)
- Triggers on git tags (e.g., `v0.1.0`)
- Builds for multiple Linux platforms + FreeBSD/NetBSD
- Creates GitHub releases with binaries
- Automatically updates Homebrew formula

## 5. Creating Your First Release

```bash
# In the main repository
git tag v0.1.0
git push origin v0.1.0
```

This will:
1. Trigger the release workflow
2. Build binaries for all platforms
3. Create a GitHub release
4. Update the Homebrew formula automatically

## 6. Supported Platforms

The GitHub Actions will build for:
- Linux x86_64 (glibc and musl)
- Linux aarch64 (ARM64) (glibc and musl)  
- Linux armv7 (ARM 32-bit)
- Linux i686 (32-bit x86)
- FreeBSD x86_64
- NetBSD x86_64

## 7. Installation Methods After Setup

Users will be able to install via:

```bash
# Homebrew
brew tap exec/rune
brew install rune

# Binary download
curl -L https://github.com/exec/rune/releases/latest/download/rune-linux-x86_64.tar.gz | tar xz
sudo mv rune /usr/local/bin/

# Cargo
cargo install --git https://github.com/exec/rune
```

## 8. File Structure Summary

```
rune/
├── .github/workflows/          # CI/CD automation
│   ├── ci.yml                 # Build and test on PR/push
│   └── release.yml            # Release builds and Homebrew update
├── src/                       # Source code
│   ├── main.rs               # Main editor implementation
│   └── syntax.rs             # Syntax highlighting
├── Cargo.toml                # Rust package manifest
├── README.md                 # Main documentation
├── CHANGELOG.md              # Version history
├── LICENSE                   # MIT license
├── HOMEBREW_SETUP.md         # Homebrew tap instructions
└── homebrew-formula.rb       # Template for Homebrew formula
```

The project is ready for production use and distribution!