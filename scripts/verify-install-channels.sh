#!/bin/bash
# Three-channel smoke test for the `cb` voice assistant CLI.
#
# For each of: curl install.sh | npm | brew
#   1. Install
#   2. Verify: install path, --version string, --help Usage line, cb config
#      show reads the user's config
#   3. Uninstall cleanly
#
# Any existing dev binary at ~/.local/bin/cb is moved aside and restored at
# the end. The user's ~/.config/chatbot/config.toml is hashed before and
# after — any drift is flagged.
#
# Usage:
#   scripts/verify-install-channels.sh
#   EXPECTED_VERSION=v0.1.0-beta.4 scripts/verify-install-channels.sh
#   SKIP_BREW=1 SKIP_NPM=1 scripts/verify-install-channels.sh  # curl only
#
# Exit codes:
#   0  all channels passed, no anomalies
#   1  at least one channel failed OR anomalies detected
#
# The script never calls `cb chat`, `cb`, `cb config` (interactive), or
# `cb install` — those either consume API quota, block on stdin, or touch
# system permissions.

set -u
set -o pipefail

# ── ANSI colors ────────────────────────────────────────────────────────────
C_RED=$'\033[91m'
C_GREEN=$'\033[92m'
C_YELLOW=$'\033[93m'
C_DIM=$'\033[90m'
C_BOLD=$'\033[1m'
C_RESET=$'\033[0m'
CHECK="${C_GREEN}✓${C_RESET}"
CROSS="${C_RED}✗${C_RESET}"
WARN="${C_YELLOW}⚠${C_RESET}"

# ── Config ─────────────────────────────────────────────────────────────────
EXPECTED_VERSION="${EXPECTED_VERSION:-}"   # e.g. v0.1.0-beta.4; empty = don't check
INSTALL_SH_URL="${INSTALL_SH_URL:-https://chatbot.longye.site/install.sh}"
NPM_PACKAGE="${NPM_PACKAGE:-@erchoc/chatbot}"
BREW_TAP="${BREW_TAP:-erchoc/tap}"
BREW_FORMULA="${BREW_FORMULA:-cb}"
CONFIG_FILE="$HOME/.config/chatbot/config.toml"
DEV_CB_PATH="$HOME/.local/bin/cb"
DEV_CB_BACKUP="$HOME/.local/bin/cb.devbackup-$$"
# Channels to skip via env flags
SKIP_CURL="${SKIP_CURL:-0}"
SKIP_NPM="${SKIP_NPM:-0}"
SKIP_BREW="${SKIP_BREW:-0}"

# ── Per-channel result storage ─────────────────────────────────────────────
declare -A RESULTS           # key: "curl:install" = "✓" or "✗" or "-"
declare -a CHANNELS
declare -a ANOMALIES
declare -A INSTALL_PATH
declare -A INSTALLED_VERSION
declare -A HELP_USAGE

# ── Helpers ────────────────────────────────────────────────────────────────
hr() { printf '%s\n' "${C_DIM}─────────────────────────────────────${C_RESET}"; }
title() { printf '%s%s%s\n' "${C_BOLD}" "$*" "${C_RESET}"; }
note() { printf '  %s\n' "$*"; }
anomaly() { ANOMALIES+=("$*"); }

file_sha() {
  if [ -f "$1" ]; then
    shasum -a 256 "$1" 2>/dev/null | awk '{print $1}'
  else
    echo "<missing>"
  fi
}

# Set RESULTS[$channel:$step] to ✓/✗/- (dash = skipped/n-a)
mark() {
  local channel="$1" step="$2" value="$3"
  RESULTS["$channel:$step"]="$value"
}

# Run cb from the current PATH, capture relevant output
probe_cb() {
  local channel="$1"
  hash -r 2>/dev/null || true

  local path version help_line config_ok="✗"
  path=$(command -v cb 2>/dev/null || true)
  if [ -z "$path" ]; then
    mark "$channel" install "${CROSS}"
    anomaly "$channel: cb not found in PATH after install"
    return 1
  fi
  INSTALL_PATH[$channel]="$path"
  mark "$channel" install "${CHECK}"

  version=$(cb --version 2>&1 | head -1)
  INSTALLED_VERSION[$channel]="$version"
  if [ -n "$EXPECTED_VERSION" ]; then
    local expected_bare="${EXPECTED_VERSION#v}"   # v0.1.0-beta.4 → 0.1.0-beta.4
    if [[ "$version" == *"$expected_bare"* ]]; then
      mark "$channel" version "${CHECK}"
    else
      mark "$channel" version "${CROSS}"
      anomaly "$channel: --version reports \"$version\", expected to contain \"$expected_bare\""
    fi
  else
    mark "$channel" version "${CHECK}"
  fi

  # Usage: line should render as "Usage: cb" — not "cb-darwin" or similar
  help_line=$(cb --help 2>&1 | grep -m1 '^Usage:' || true)
  HELP_USAGE[$channel]="$help_line"
  if [[ "$help_line" == "Usage: cb "* ]] || [[ "$help_line" == "Usage: cb" ]]; then
    mark "$channel" help "${CHECK}"
  else
    mark "$channel" help "${WARN}"
    anomaly "$channel: Usage line reads \"$help_line\" (clap fell back to argv[0])"
  fi

  # Config read: `cb config show` exits 0 and output contains the config header
  if cb config show 2>&1 | grep -q '当前配置\|current config\|Current config'; then
    mark "$channel" config "${CHECK}"
    config_ok="✓"
  else
    mark "$channel" config "${CROSS}"
    anomaly "$channel: cb config show did not print expected header"
  fi
  : "$config_ok"
}

# ── Setup ──────────────────────────────────────────────────────────────────
echo
title "══════ cb install channel verification ══════"
hr
note "Expected version:  ${EXPECTED_VERSION:-<any>}"
note "Config file:       $CONFIG_FILE"
note "Dev cb path:       $DEV_CB_PATH"

CONFIG_SHA_BEFORE=$(file_sha "$CONFIG_FILE")
if [ -f "$DEV_CB_PATH" ]; then
  note "Backing up dev cb → $DEV_CB_BACKUP"
  mv "$DEV_CB_PATH" "$DEV_CB_BACKUP"
  HAD_DEV_CB=1
else
  HAD_DEV_CB=0
fi

# Also surface other cb binaries in PATH that we're not managing — they
# could shadow the channel being tested.
echo
note "Other cb binaries in PATH (if any will shadow tests):"
while IFS= read -r other; do
  [ -n "$other" ] && [ "$other" != "$DEV_CB_PATH" ] && printf '    %s\n' "$other"
done < <(type -a cb 2>/dev/null | awk '{print $NF}' | sort -u)

restore_dev_cb() {
  if [ "$HAD_DEV_CB" = "1" ] && [ -f "$DEV_CB_BACKUP" ]; then
    mv "$DEV_CB_BACKUP" "$DEV_CB_PATH"
    note "${C_DIM}restored dev cb → $DEV_CB_PATH${C_RESET}"
  fi
  hash -r 2>/dev/null || true
}
trap restore_dev_cb EXIT

# ── Channel 1: curl ────────────────────────────────────────────────────────
if [ "$SKIP_CURL" = "1" ]; then
  note "${C_DIM}skipping curl channel (SKIP_CURL=1)${C_RESET}"
else
  echo
  title "─── curl ──────────────────────────────"
  if bash <(curl -fsSL "$INSTALL_SH_URL") >/tmp/verify-curl.out 2>&1; then
    CHANNELS+=("curl")
    probe_cb curl || true
    # uninstall = delete the binary
    rm -f "$DEV_CB_PATH"
    hash -r
    if ! command -v cb >/dev/null 2>&1; then
      mark curl uninstall "${CHECK}"
    else
      mark curl uninstall "${CROSS}"
      anomaly "curl: cb still resolvable after rm $DEV_CB_PATH — shadowed by $(command -v cb)"
    fi
  else
    CHANNELS+=("curl")
    mark curl install "${CROSS}"
    anomaly "curl: install.sh failed. Log: /tmp/verify-curl.out"
  fi
fi

# ── Channel 2: npm ─────────────────────────────────────────────────────────
if [ "$SKIP_NPM" = "1" ]; then
  note "${C_DIM}skipping npm channel (SKIP_NPM=1)${C_RESET}"
else
  echo
  title "─── npm ──────────────────────────────"
  if npm i -g "$NPM_PACKAGE" >/tmp/verify-npm.out 2>&1; then
    CHANNELS+=("npm")
    probe_cb npm || true
    if npm uninstall -g "$NPM_PACKAGE" >>/tmp/verify-npm.out 2>&1; then
      hash -r
      mark npm uninstall "${CHECK}"
    else
      mark npm uninstall "${CROSS}"
      anomaly "npm: uninstall failed. Log: /tmp/verify-npm.out"
    fi
  else
    CHANNELS+=("npm")
    mark npm install "${CROSS}"
    anomaly "npm: install failed. Log: /tmp/verify-npm.out"
  fi
fi

# ── Channel 3: brew ────────────────────────────────────────────────────────
if [ "$SKIP_BREW" = "1" ]; then
  note "${C_DIM}skipping brew channel (SKIP_BREW=1)${C_RESET}"
elif ! command -v brew >/dev/null 2>&1; then
  note "${C_DIM}brew not installed — skipping${C_RESET}"
else
  echo
  title "─── brew ─────────────────────────────"
  # Pre-clean any stale cellar left over from an older beta
  if brew list --formula 2>/dev/null | grep -qx "$BREW_FORMULA"; then
    note "Removing stale cellar of $BREW_FORMULA..."
    brew uninstall "$BREW_FORMULA" >/dev/null 2>&1 || true
  fi
  # Refresh the tap so Formula/cb.rb points at the latest version
  brew untap "$BREW_TAP" >/dev/null 2>&1 || true
  brew tap "$BREW_TAP" >/dev/null 2>&1

  if brew install "${BREW_TAP}/${BREW_FORMULA}" >/tmp/verify-brew.out 2>&1; then
    CHANNELS+=("brew")
    probe_cb brew || true
    if brew uninstall "$BREW_FORMULA" >>/tmp/verify-brew.out 2>&1; then
      hash -r
      mark brew uninstall "${CHECK}"
    else
      mark brew uninstall "${CROSS}"
      anomaly "brew: uninstall failed. Log: /tmp/verify-brew.out"
    fi
  else
    CHANNELS+=("brew")
    mark brew install "${CROSS}"
    anomaly "brew: install failed. Log: /tmp/verify-brew.out"
  fi
fi

# ── Summary table ─────────────────────────────────────────────────────────
echo
title "══════ Summary ══════"
printf '\n  %-8s │ %s │ %s │ %s │ %s │ %s\n' \
  "channel" "install" "version" "help" "config" "uninstall"
printf '  %-8s─┼─%s─┼─%s─┼─%s─┼─%s─┼─%s\n' \
  "--------" "-------" "-------" "----" "------" "---------"
for ch in "${CHANNELS[@]}"; do
  printf '  %-8s │    %s    │    %s    │   %s   │    %s    │    %s\n' \
    "$ch" \
    "${RESULTS[$ch:install]:--}" \
    "${RESULTS[$ch:version]:--}" \
    "${RESULTS[$ch:help]:--}" \
    "${RESULTS[$ch:config]:--}" \
    "${RESULTS[$ch:uninstall]:--}"
done
echo

# Detailed per-channel info
for ch in "${CHANNELS[@]}"; do
  printf '  %s%s%s\n' "${C_BOLD}" "$ch" "${C_RESET}"
  [ -n "${INSTALL_PATH[$ch]:-}" ]      && printf '    path:    %s\n' "${INSTALL_PATH[$ch]}"
  [ -n "${INSTALLED_VERSION[$ch]:-}" ] && printf '    version: %s\n' "${INSTALLED_VERSION[$ch]}"
  [ -n "${HELP_USAGE[$ch]:-}" ]        && printf '    help:    %s\n' "${HELP_USAGE[$ch]}"
done

# ── Config integrity check ────────────────────────────────────────────────
CONFIG_SHA_AFTER=$(file_sha "$CONFIG_FILE")
if [ "$CONFIG_SHA_BEFORE" = "$CONFIG_SHA_AFTER" ]; then
  printf '\n  %s config untouched (sha256 unchanged)\n' "${CHECK}"
else
  printf '\n  %s config changed during run!\n' "${CROSS}"
  printf '      before: %s\n' "$CONFIG_SHA_BEFORE"
  printf '      after:  %s\n' "$CONFIG_SHA_AFTER"
  anomaly "config.toml sha256 changed during verification (should never happen)"
fi

# ── Anomalies section ─────────────────────────────────────────────────────
echo
if [ ${#ANOMALIES[@]} -eq 0 ]; then
  printf '%s  no anomalies detected.\n' "${CHECK}"
  exit 0
else
  title "${C_YELLOW}Anomalies / notes:${C_RESET}"
  for a in "${ANOMALIES[@]}"; do
    printf '  %s %s\n' "${WARN}" "$a"
  done
  exit 1
fi
