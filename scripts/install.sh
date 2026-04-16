#!/usr/bin/env bash
# cb install script
# Usage: curl -fsSL https://chatbot.longye.site/install.sh | bash
set -euo pipefail

REPO="erchoc/chatbot"
BIN_NAME="cb"
INSTALL_DIR="$HOME/.local/bin"

# ── Detect platform ───────────────────────────────────────────────────────────

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

# ── Resolve latest version ────────────────────────────────────────────────────

echo "  Fetching latest release..."
VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep '"tag_name"' | sed 's/.*"tag_name": "\(.*\)".*/\1/')

if [ -z "$VERSION" ]; then
  echo "Failed to fetch latest version" >&2
  exit 1
fi

echo "  Version: $VERSION"

# ── Download ──────────────────────────────────────────────────────────────────

URL="https://github.com/${REPO}/releases/download/${VERSION}/${ARTIFACT}"
TMP=$(mktemp)

echo "  Downloading $ARTIFACT..."
if ! curl -fsSL "$URL" -o "$TMP"; then
  echo "Download failed: $URL" >&2
  rm -f "$TMP"
  exit 1
fi

# ── Install ───────────────────────────────────────────────────────────────────

mkdir -p "$INSTALL_DIR"
chmod +x "$TMP"

# Remove stale copies from other locations
rm -f "/usr/local/bin/$BIN_NAME" 2>/dev/null || true
rm -f "$HOME/.cargo/bin/$BIN_NAME" 2>/dev/null || true

mv "$TMP" "$INSTALL_DIR/$BIN_NAME"

echo ""
echo "  ✓ Installed $BIN_NAME $VERSION → $INSTALL_DIR/$BIN_NAME"

# ── PATH check ───────────────────────────────────────────────────────────────

if ! echo ":$PATH:" | grep -q ":$INSTALL_DIR:"; then
  SHELL_NAME=$(basename "${SHELL:-bash}")
  case "$SHELL_NAME" in
    zsh)  RC="$HOME/.zshrc" ;;
    bash) RC="$HOME/.bashrc" ;;
    *)    RC="$HOME/.profile" ;;
  esac

  echo ""
  echo "  \033[33m⚠  $INSTALL_DIR is not in PATH\033[0m"
  echo "  Run the following then restart your terminal:"
  echo ""
  echo "  \033[1mexport PATH=\"\$HOME/.local/bin:\$PATH\"\033[0m"
  echo ""
  echo "  Or add it to $RC permanently:"
  echo ""
  echo "  \033[1mecho 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> $RC\033[0m"
fi

echo ""
echo "  Run \033[1mcb config\033[0m to get started."
