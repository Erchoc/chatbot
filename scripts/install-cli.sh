#!/usr/bin/env bash
set -e

INSTALL_DIR="$HOME/.local/bin"
BINARY="cb"

# Build
cargo build --manifest-path packages/cli/Cargo.toml --release

# Install
mkdir -p "$INSTALL_DIR"
cp "packages/cli/target/release/$BINARY" "$INSTALL_DIR/$BINARY"

# macOS: sign with microphone entitlement to avoid repeated TCC prompts
if [ "$(uname -s)" = "Darwin" ]; then
  ENTITLEMENTS="packages/cli/entitlements.plist"
  if [ -f "$ENTITLEMENTS" ]; then
    codesign --force \
      --entitlements "$ENTITLEMENTS" \
      --sign - "$INSTALL_DIR/$BINARY" 2>/dev/null || true
  fi
fi

printf "\033[92m✓\033[0m Installed \033[1mcb\033[0m to %s/%s\n" "$INSTALL_DIR" "$BINARY"

# Remove stale binaries that would shadow the newly installed one
for STALE in "/usr/local/bin/$BINARY" "$HOME/.cargo/bin/$BINARY"; do
    if [ -f "$STALE" ]; then
        if [ "$STALE" = "/usr/local/bin/$BINARY" ]; then
            sudo rm -f "$STALE" && printf "\033[90m  removed stale %s\033[0m\n" "$STALE" || \
                printf "\033[93m⚠  Could not remove %s (needs sudo) — run: sudo rm %s\033[0m\n" "$STALE" "$STALE"
        else
            rm -f "$STALE" && printf "\033[90m  removed stale %s\033[0m\n" "$STALE"
        fi
    fi
done

# Check if ~/.local/bin is in PATH
if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
    echo ""
    printf "\033[93m⚠  %s 不在 PATH 中\033[0m\n" "$INSTALL_DIR"
    printf "\033[0m   执行以下命令立即生效:\033[0m\n"
    printf "\033[96m   export PATH=\"\$HOME/.local/bin:\$PATH\"\033[0m\n"
    echo ""
    printf "\033[0m   永久生效，追加到你的 shell 配置文件:\033[0m\n"

    if [ -n "$ZSH_VERSION" ] || [ "$(basename "$SHELL")" = "zsh" ]; then
        CONFIG_FILE="~/.zshrc"
    else
        CONFIG_FILE="~/.bashrc"
    fi

    printf "\033[96m   echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> %s\033[0m\n" "$CONFIG_FILE"
    echo ""
fi
