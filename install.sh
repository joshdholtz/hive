#!/usr/bin/env bash
set -euo pipefail

# Hive installer
# Usage: curl -fsSL https://raw.githubusercontent.com/joshdholtz/hive/main/install.sh | bash

VERSION="0.1.0"
REPO="joshdholtz/hive"
INSTALL_DIR="${HIVE_INSTALL_DIR:-$HOME/.local/bin}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

info() { echo -e "${BLUE}==>${NC} $*"; }
ok() { echo -e "${GREEN}==>${NC} $*"; }
warn() { echo -e "${YELLOW}==>${NC} $*"; }
error() { echo -e "${RED}==>${NC} $*" >&2; }

# Detect OS
OS="$(uname -s)"
case "$OS" in
  Darwin*) OS="macos" ;;
  Linux*)  OS="linux" ;;
  *)       error "Unsupported OS: $OS"; exit 1 ;;
esac

info "Installing hive v${VERSION}..."

# Create install directory
mkdir -p "$INSTALL_DIR"

# Download hive script
info "Downloading hive..."
if command -v curl &>/dev/null; then
  curl -fsSL "https://raw.githubusercontent.com/${REPO}/main/hive" -o "${INSTALL_DIR}/hive"
elif command -v wget &>/dev/null; then
  wget -qO "${INSTALL_DIR}/hive" "https://raw.githubusercontent.com/${REPO}/main/hive"
else
  error "curl or wget required"
  exit 1
fi

chmod +x "${INSTALL_DIR}/hive"

# Check if install dir is in PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
  warn "$INSTALL_DIR is not in your PATH"
  echo ""
  echo "Add this to your shell profile (~/.bashrc, ~/.zshrc, etc.):"
  echo ""
  echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
  echo ""
fi

# Check dependencies
echo ""
info "Checking dependencies..."

missing=()
command -v yq &>/dev/null || missing+=("yq")
command -v tmux &>/dev/null || missing+=("tmux")
command -v tmuxp &>/dev/null || missing+=("tmuxp")

if [[ ${#missing[@]} -gt 0 ]]; then
  warn "Missing dependencies: ${missing[*]}"
  echo ""
  echo "Install with:"
  if [[ "$OS" == "macos" ]]; then
    echo "  brew install yq tmux"
    echo "  pip install tmuxp"
  else
    echo "  # yq: https://github.com/mikefarah/yq#install"
    echo "  # tmux: sudo apt install tmux"
    echo "  # tmuxp: pip install tmuxp"
  fi
  echo ""
fi

# Optional deps
if ! command -v fswatch &>/dev/null; then
  info "Optional: install fswatch for efficient file watching"
  [[ "$OS" == "macos" ]] && echo "  brew install fswatch"
fi

echo ""
ok "Installed hive to ${INSTALL_DIR}/hive"
echo ""
echo "Get started:"
echo "  cd your-project"
echo "  hive init"
echo "  hive up"
echo ""
