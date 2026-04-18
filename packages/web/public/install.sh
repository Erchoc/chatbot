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
# Priority: CB_VERSION env → GitHub API (authenticated if GITHUB_TOKEN is
# set, otherwise anonymous — 60 req/hr per IP).
#
# /releases/latest is avoided by default because GitHub skips pre-releases
# there; during 0.x we ship pre-releases continuously and the stable
# endpoint 404s. Set CB_CHANNEL=stable to opt in to that behavior.
VERSION="${CB_VERSION:-}"
if [ -z "$VERSION" ]; then
  echo "  Fetching latest release..."

  if [ "$CHANNEL" = "stable" ]; then
    API_URL="https://api.github.com/repos/${REPO}/releases/latest"
  else
    API_URL="https://api.github.com/repos/${REPO}/releases?per_page=1"
  fi

  # Capture both body and status so we can distinguish rate-limit (403)
  # from no-release-yet (404) from other transport failures.
  #
  # We build the curl invocation via if/else rather than a "${ARR[@]}" array
  # because macOS ships bash 3.2, which errors under `set -u` when expanding
  # an empty array — even `"${AUTH_ARGS[@]}"` trips it.
  API_BODY=$(mktemp)
  if [ -n "${GITHUB_TOKEN:-}" ]; then
    API_STATUS=$(curl -sSL -o "$API_BODY" -w '%{http_code}' \
      -H "Authorization: Bearer $GITHUB_TOKEN" "$API_URL" || echo '000')
  else
    API_STATUS=$(curl -sSL -o "$API_BODY" -w '%{http_code}' "$API_URL" || echo '000')
  fi

  if [ "$API_STATUS" = "200" ]; then
    VERSION=$(grep '"tag_name"' "$API_BODY" | head -1 \
      | sed 's/.*"tag_name": "\(.*\)".*/\1/' || true)
  elif [ "$API_STATUS" = "403" ]; then
    echo "" >&2
    echo "  ⚠  GitHub API rate limit hit (HTTP 403)." >&2
    if [ -z "${GITHUB_TOKEN:-}" ]; then
      echo "     Anonymous requests are capped at 60/hr per IP." >&2
      echo "     Fix: export GITHUB_TOKEN=<your-pat>  (any scope works for read)" >&2
      echo "          https://github.com/settings/tokens" >&2
    else
      echo "     Your GITHUB_TOKEN is likely expired or revoked." >&2
    fi
    echo "     Or pin a version: CB_VERSION=v0.1.0-beta.5 curl ... | bash" >&2
    rm -f "$API_BODY"
    exit 1
  elif [ "$API_STATUS" = "404" ] && [ "$CHANNEL" = "stable" ]; then
    echo "  No stable release yet. Re-run with CB_CHANNEL=any or pin CB_VERSION." >&2
    rm -f "$API_BODY"
    exit 1
  else
    echo "  GitHub API returned HTTP $API_STATUS from $API_URL" >&2
    rm -f "$API_BODY"
    exit 1
  fi
  rm -f "$API_BODY"
fi

if [ -z "$VERSION" ]; then
  echo "Failed to parse release version from API response." >&2
  echo "Pin a version: CB_VERSION=v0.1.0-beta.5 curl -fsSL ... | bash" >&2
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
