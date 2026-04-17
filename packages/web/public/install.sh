#!/bin/bash
# Install the `cb` voice assistant from GitHub Releases.
#
# Usage:
#   curl -fsSL https://chatbot.longye.site/install.sh | bash
#
# Env overrides:
#   CB_VERSION=v0.1.0-beta.3   Pin a specific release tag (default: newest)
#   CB_CHANNEL=stable|any      stable = skip pre-releases, any = include them
#                              (default: any — we're pre-1.0)
#   CB_INSTALL_DIR=/path       Override install location (default: ~/.local/bin)

set -euo pipefail

REPO="erchoc/chatbot"
BIN_NAME="cb"
INSTALL_DIR="${CB_INSTALL_DIR:-$HOME/.local/bin}"
CHANNEL="${CB_CHANNEL:-any}"

OS=$(uname -s)
ARCH=$(uname -m)

case "$OS" in
  Darwin)
    ARTIFACT="cb-macos-universal"
    ;;
  Linux)
    case "$ARCH" in
      x86_64)  ARTIFACT="cb-linux-x86_64" ;;
      aarch64) ARTIFACT="cb-linux-arm64" ;;
      *) echo "Unsupported Linux arch: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $OS" >&2
    exit 1
    ;;
esac

# ── Resolve version ──────────────────────────────────────────────────────
# Priority: CB_VERSION env → first release from /releases (respects channel)
#
# /releases/latest is avoided because GitHub skips pre-releases there; during
# 0.x we ship pre-releases continuously and the stable endpoint 404s.
VERSION="${CB_VERSION:-}"
if [ -z "$VERSION" ]; then
  echo "  Fetching latest release..."
  if [ "$CHANNEL" = "stable" ]; then
    VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
      | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": "\(.*\)".*/\1/' || true)
  else
    # First entry from /releases list; prereleases are ordered newest-first.
    VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases?per_page=1" \
      | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": "\(.*\)".*/\1/' || true)
  fi
fi

if [ -z "$VERSION" ]; then
  echo "Failed to fetch release version (CHANNEL=$CHANNEL)" >&2
  echo "Try pinning: CB_VERSION=v0.1.0-beta.3 curl -fsSL ... | bash" >&2
  exit 1
fi

echo "  Version: $VERSION"

URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARTIFACT}"
TMP=$(mktemp)

echo "  Downloading $ARTIFACT..."
if ! curl -fsSL "$URL" -o "$TMP"; then
  echo "Download failed: $URL" >&2
  rm -f "$TMP"
  exit 1
fi

mkdir -p "$INSTALL_DIR"
chmod +x "$TMP"

if [ "$OS" = "Darwin" ]; then
  codesign --verify "$TMP" 2>/dev/null || codesign --force --sign - "$TMP" 2>/dev/null || true
fi

rm -f "/usr/local/bin/$BIN_NAME" 2>/dev/null || true
rm -f "$HOME/.cargo/bin/$BIN_NAME" 2>/dev/null || true

mv "$TMP" "$INSTALL_DIR/$BIN_NAME"

echo ""
echo "  ✓ Installed $BIN_NAME $VERSION → $INSTALL_DIR/$BIN_NAME"

if ! echo ":$PATH:" | grep -q ":$INSTALL_DIR:"; then
  SHELL_NAME=$(basename "${SHELL:-bash}")
  case "$SHELL_NAME" in
    zsh)  RC="$HOME/.zshrc" ;;
    bash) RC="$HOME/.bashrc" ;;
    *)    RC="$HOME/.profile" ;;
  esac

  echo ""
  printf "  \033[33m⚠  %s is not in PATH\033[0m\n" "$INSTALL_DIR"
  echo "  Run the following then restart your terminal:"
  echo ""
  printf "  \033[1mexport PATH=\"%s:\$PATH\"\033[0m\n" "$INSTALL_DIR"
  echo ""
  echo "  Or add it to $RC permanently:"
  echo ""
  printf "  \033[1mecho 'export PATH=\"%s:\$PATH\"' >> %s\033[0m\n" "$INSTALL_DIR" "$RC"
fi

echo ""
printf "  Run \033[1mcb config\033[0m to get started.\n"
