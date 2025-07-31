# Homebrew Tap Setup

This document explains how to set up the separate homebrew tap repository.

## Create the Homebrew Tap Repository

1. Create a new repository: `exec/homebrew-rune`
2. The repository name MUST follow the pattern `homebrew-*` for Homebrew to recognize it

## Repository Structure

```
exec/homebrew-rune/
├── README.md
├── rune.rb              # The formula file
└── .github/
    └── workflows/
        └── update-formula.yml
```

## Files to Create

### README.md
```markdown
# Homebrew Rune

Homebrew tap for [rune](https://github.com/exec/rune) - A modern CLI text editor.

## Installation

```bash
brew tap exec/rune
brew install rune
```

## Development

The formula is automatically updated by GitHub Actions when new releases are published.
```

### rune.rb
```ruby
class Rune < Formula
  desc "A modern CLI text editor that bridges the gap between nano's simplicity and advanced features"
  homepage "https://github.com/exec/rune"
  version "0.1.0"
  
  if OS.mac?
    if Hardware::CPU.arm?
      url "https://github.com/exec/rune/releases/download/v0.1.0/rune-darwin-aarch64.tar.gz"
      sha256 "PLACEHOLDER"
    else
      url "https://github.com/exec/rune/releases/download/v0.1.0/rune-darwin-x86_64.tar.gz" 
      sha256 "PLACEHOLDER"
    end
  elsif OS.linux?
    if Hardware::CPU.arm?
      url "https://github.com/exec/rune/releases/download/v0.1.0/rune-linux-aarch64.tar.gz"
      sha256 "PLACEHOLDER"
    else
      url "https://github.com/exec/rune/releases/download/v0.1.0/rune-linux-x86_64.tar.gz"
      sha256 "PLACEHOLDER"
    end
  end

  def install
    bin.install "rune"
  end

  test do
    output = shell_output("#{bin}/rune --help")
    assert_match "A modern CLI text editor", output
  end
end
```

### .github/workflows/update-formula.yml
```yaml
name: Update Formula

on:
  repository_dispatch:
    types: [update-formula]

jobs:
  update:
    runs-on: ubuntu-latest
    
    steps:
    - uses: actions/checkout@v4
      with:
        token: ${{ secrets.GITHUB_TOKEN }}
        
    - name: Download release assets
      run: |
        VERSION="${{ github.event.client_payload.version }}"
        TAG="${{ github.event.client_payload.tag }}"
        
        # Download all release assets to get SHA256 hashes
        curl -sL "https://github.com/exec/rune/releases/download/${TAG}/rune-darwin-x86_64.tar.gz" -o darwin-x86_64.tar.gz
        curl -sL "https://github.com/exec/rune/releases/download/${TAG}/rune-darwin-aarch64.tar.gz" -o darwin-aarch64.tar.gz
        curl -sL "https://github.com/exec/rune/releases/download/${TAG}/rune-linux-x86_64.tar.gz" -o linux-x86_64.tar.gz
        curl -sL "https://github.com/exec/rune/releases/download/${TAG}/rune-linux-aarch64.tar.gz" -o linux-aarch64.tar.gz
        
        # Calculate SHA256 hashes
        DARWIN_X86_64_SHA=$(sha256sum darwin-x86_64.tar.gz | cut -d' ' -f1)
        DARWIN_AARCH64_SHA=$(sha256sum darwin-aarch64.tar.gz | cut -d' ' -f1)
        LINUX_X86_64_SHA=$(sha256sum linux-x86_64.tar.gz | cut -d' ' -f1)
        LINUX_AARCH64_SHA=$(sha256sum linux-aarch64.tar.gz | cut -d' ' -f1)
        
        # Update formula
        cat > rune.rb << EOF
        class Rune < Formula
          desc "A modern CLI text editor that bridges the gap between nano's simplicity and advanced features"
          homepage "https://github.com/exec/rune"
          version "${VERSION}"
          
          if OS.mac?
            if Hardware::CPU.arm?
              url "https://github.com/exec/rune/releases/download/${TAG}/rune-darwin-aarch64.tar.gz"
              sha256 "${DARWIN_AARCH64_SHA}"
            else
              url "https://github.com/exec/rune/releases/download/${TAG}/rune-darwin-x86_64.tar.gz"
              sha256 "${DARWIN_X86_64_SHA}"
            end
          elsif OS.linux?
            if Hardware::CPU.arm?
              url "https://github.com/exec/rune/releases/download/${TAG}/rune-linux-aarch64.tar.gz"
              sha256 "${LINUX_AARCH64_SHA}"
            else
              url "https://github.com/exec/rune/releases/download/${TAG}/rune-linux-x86_64.tar.gz"
              sha256 "${LINUX_X86_64_SHA}"
            end
          end

          def install
            bin.install "rune"
          end

          test do
            output = shell_output("#{bin}/rune --help")
            assert_match "A modern CLI text editor", output
          end
        end
        EOF
        
    - name: Commit and push changes
      run: |
        git config --local user.email "action@github.com"
        git config --local user.name "GitHub Action"
        git add rune.rb
        git commit -m "Update formula to version ${{ github.event.client_payload.version }}" || exit 0
        git push
```

## GitHub Secrets Required

In the main repository (exec/rune), add this secret:
- `HOMEBREW_TAP_TOKEN`: A GitHub Personal Access Token with write access to the homebrew-rune repository

## Testing

After setting up:
1. Create a test release in exec/rune
2. The workflow should automatically update the homebrew formula
3. Test installation: `brew tap exec/rune && brew install rune`