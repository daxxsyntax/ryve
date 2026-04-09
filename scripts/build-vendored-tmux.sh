#!/usr/bin/env bash
# Build the vendored tmux binary from source.
#
# Usage:
#   ./scripts/build-vendored-tmux.sh [--prefix <install-dir>]
#
# By default the binary is placed at vendor/tmux/bin/tmux.
# Pass --prefix to override (e.g. for CI artifact staging).
#
# Prerequisites (install via your system package manager):
#   macOS:  brew install autoconf automake libevent pkg-config
#   Linux:  apt install build-essential autoconf automake libevent-dev libncurses-dev bison pkg-config
#
# Pinned version is read from vendor/tmux/VERSION.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
VERSION_FILE="$REPO_ROOT/vendor/tmux/VERSION"
VERSION="$(tr -d '[:space:]' < "$VERSION_FILE")"

PREFIX="$REPO_ROOT/vendor/tmux"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix) PREFIX="$2"; shift 2 ;;
    *) echo "Unknown arg: $1" >&2; exit 1 ;;
  esac
done

BIN_DIR="$PREFIX/bin"
BUILD_DIR="$(mktemp -d)"
trap 'rm -rf "$BUILD_DIR"' EXIT

echo "==> Building tmux $VERSION"
echo "    source:  https://github.com/tmux/tmux/releases/download/$VERSION/tmux-$VERSION.tar.gz"
echo "    output:  $BIN_DIR/tmux"

# Download
TARBALL="$BUILD_DIR/tmux-$VERSION.tar.gz"
curl -fsSL "https://github.com/tmux/tmux/releases/download/$VERSION/tmux-$VERSION.tar.gz" \
  -o "$TARBALL"

# Extract
tar xzf "$TARBALL" -C "$BUILD_DIR"

# Build
cd "$BUILD_DIR/tmux-$VERSION"
./configure --prefix="$BUILD_DIR/install" --enable-static 2>&1 | tail -5
make -j"$(nproc 2>/dev/null || sysctl -n hw.ncpu)" 2>&1 | tail -5

# Install the binary only
mkdir -p "$BIN_DIR"
cp tmux "$BIN_DIR/tmux"
chmod +x "$BIN_DIR/tmux"

echo "==> Installed tmux $VERSION at $BIN_DIR/tmux"
"$BIN_DIR/tmux" -V
