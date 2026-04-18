#!/usr/bin/env bash
# If invoked as `sh scripts/verify-install-channels.sh` (i.e. /bin/sh),
# macOS feeds the file to bash-in-POSIX-mode, which disables process
# substitution and makes line 175 die with a syntax error before any
# of our code runs. Detect any restricted mode (no BASH_VERSION, or
# POSIXLY_CORRECT set, or SHELLOPTS mentions posix) and re-exec under
# real bash so the user can run `sh scripts/verify-install-channels.sh`
# without thinking about it.
if [ -z "${BASH_VERSION:-}" ] \
   || [ -n "${POSIXLY_CORRECT:-}" ] \
   || [ "${BASH##*/}" = "sh" ]; then
  if command -v bash >/dev/null 2>&1; then
    exec bash "$0" "$@"
  else
    echo "This script needs bash (not /bin/sh). Install bash and retry." >&2
    exit 1
  fi
fi

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
#   EXPECTED_VERSION=v0.1.0-beta.5 scripts/verify-install-channels.sh
#   SKIP_BREW=1 SKIP_NPM=1 scripts/verify-install-channels.sh  # curl only
#
# Exit codes:
#   0  all channels passed, no anomalies
#   1  at least one channel failed OR anomalies detected
#
# The script never calls `cb chat`, `cb`, `cb config` (interactive), or
# `cb install` — those either consume API quota, block on stdin, or touch
# system permissions.
#
# Compatibility: plain bash 3.2 (macOS default). No associative arrays.

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
EXPECTED_VERSION="${EXPECTED_VERSION:-}"
INSTALL_SH_URL="${INSTALL_SH_URL:-https://chatbot.longye.site/install.sh}"
NPM_PACKAGE="${NPM_PACKAGE:-@erchoc/chatbot}"
BREW_TAP="${BREW_TAP:-erchoc/tap}"
BREW_FORMULA="${BREW_FORMULA:-cb}"
CONFIG_FILE="$HOME/.config/chatbot/config.toml"
DEV_CB_PATH="$HOME/.local/bin/cb"
DEV_CB_BACKUP="$HOME/.local/bin/cb.devbackup-$$"
SKIP_CURL="${SKIP_CURL:-0}"
SKIP_NPM="${SKIP_NPM:-0}"
SKIP_BREW="${SKIP_BREW:-0}"

# Result storage via prefixed flat variables (bash 3.2 compatible).
#   R_<channel>_<field>="✓" | "✗" | "⚠" | "-"
#   I_<channel>_path / I_<channel>_version / I_<channel>_help — detail strings
CHANNELS=""           # space-separated list
ANOMALIES=()

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

set_result() {  # channel field value
  local var="R_$1_$2"
  eval "$var=\"\$3\""
}

get_result() {  # channel field — prints value (or '-' if unset)
  local var="R_$1_$2"
  eval "printf '%s' \"\${$var:--}\""
}

set_info() {  # channel field value
  local var="I_$1_$2"
  eval "$var=\"\$3\""
}

get_info() {
  local var="I_$1_$2"
  eval "printf '%s' \"\${$var:-}\""
}

register_channel() {
  CHANNELS="$CHANNELS $1"
}

# Probe the current cb (whichever channel just installed it).
# Capture command output to a variable before piping through grep/head —
# `cb config show` is long enough that `grep -q` closes stdin early, which
# sends SIGPIPE to the Rust process and trips its default panic handler
# (exit code 101). Capturing first avoids the pipe entirely.
probe_cb() {
  local channel="$1"
  hash -r 2>/dev/null || true

  local path
  path=$(command -v cb 2>/dev/null || true)
  if [ -z "$path" ]; then
    set_result "$channel" install "${CROSS}"
    anomaly "$channel: cb not found in PATH after install"
    return 1
  fi
  set_info "$channel" path "$path"
  set_result "$channel" install "${CHECK}"

  local version_out version
  version_out=$(cb --version 2>&1)
  version=$(echo "$version_out" | head -1)
  set_info "$channel" version "$version"
  if [ -n "$EXPECTED_VERSION" ]; then
    local expected_bare="${EXPECTED_VERSION#v}"
    case "$version" in
      *"$expected_bare"*) set_result "$channel" version "${CHECK}" ;;
      *)                  set_result "$channel" version "${CROSS}"
                          anomaly "$channel: --version = \"$version\", expected \"$expected_bare\"" ;;
    esac
  else
    set_result "$channel" version "${CHECK}"
  fi

  local help_out help_line
  help_out=$(cb --help 2>&1)
  help_line=$(echo "$help_out" | grep -m1 '^Usage:' || true)
  set_info "$channel" help "$help_line"
  case "$help_line" in
    "Usage: cb"|"Usage: cb "*) set_result "$channel" help "${CHECK}" ;;
    *)                         set_result "$channel" help "${WARN}"
                               anomaly "$channel: Usage line reads \"$help_line\" (clap fell back to argv[0])" ;;
  esac

  local cfg_out
  cfg_out=$(cb config show 2>&1)
  if echo "$cfg_out" | grep -qE '当前配置|current config|Current config'; then
    set_result "$channel" config "${CHECK}"
  else
    set_result "$channel" config "${CROSS}"
    anomaly "$channel: cb config show did not print expected header"
  fi
}

# ── Setup ──────────────────────────────────────────────────────────────────
echo
title "══════ cb install channel verification ══════"
hr
note "Expected version:  ${EXPECTED_VERSION:-<any>}"
note "Config file:       $CONFIG_FILE"
note "Dev cb path:       $DEV_CB_PATH"

CONFIG_SHA_BEFORE=$(file_sha "$CONFIG_FILE")
HAD_DEV_CB=0
if [ -f "$DEV_CB_PATH" ]; then
  note "Backing up dev cb → $DEV_CB_BACKUP"
  mv "$DEV_CB_PATH" "$DEV_CB_BACKUP"
  HAD_DEV_CB=1
fi

# Flag other cb binaries in PATH that might shadow the test binary.
SHADOWED=""
while IFS= read -r other; do
  if [ -n "$other" ] && [ "$other" != "$DEV_CB_PATH" ] && [ "$other" != "$DEV_CB_BACKUP" ]; then
    SHADOWED="$SHADOWED\n    $other"
  fi
done < <(type -a cb 2>/dev/null | awk '{print $NF}' | sort -u)
if [ -n "$SHADOWED" ]; then
  echo
  note "Other cb binaries in PATH (may shadow tests):"
  printf "$SHADOWED\n"
fi

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
  # Pass CB_VERSION down to install.sh so we skip the unauthenticated
  # GitHub API call (which can 403 under rate limits during repeated
  # testing). If EXPECTED_VERSION isn't set we let install.sh resolve.
  if CB_VERSION="${EXPECTED_VERSION:-}" bash <(curl -fsSL "$INSTALL_SH_URL") >/tmp/verify-curl.out 2>&1; then
    register_channel curl
    probe_cb curl || true
    rm -f "$DEV_CB_PATH"
    hash -r
    if ! command -v cb >/dev/null 2>&1; then
      set_result curl uninstall "${CHECK}"
    else
      set_result curl uninstall "${CROSS}"
      anomaly "curl: cb still resolvable after rm — shadowed by $(command -v cb)"
    fi
  else
    register_channel curl
    set_result curl install "${CROSS}"
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
    register_channel npm
    probe_cb npm || true
    if npm uninstall -g "$NPM_PACKAGE" >>/tmp/verify-npm.out 2>&1; then
      hash -r
      set_result npm uninstall "${CHECK}"
    else
      set_result npm uninstall "${CROSS}"
      anomaly "npm: uninstall failed. Log: /tmp/verify-npm.out"
    fi
  else
    register_channel npm
    set_result npm install "${CROSS}"
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
  if brew list --formula 2>/dev/null | grep -qx "$BREW_FORMULA"; then
    note "Removing stale cellar of $BREW_FORMULA..."
    brew uninstall "$BREW_FORMULA" >/dev/null 2>&1 || true
  fi
  brew untap "$BREW_TAP" >/dev/null 2>&1 || true
  brew tap "$BREW_TAP" >/dev/null 2>&1

  if brew install "${BREW_TAP}/${BREW_FORMULA}" >/tmp/verify-brew.out 2>&1; then
    register_channel brew
    probe_cb brew || true
    if brew uninstall "$BREW_FORMULA" >>/tmp/verify-brew.out 2>&1; then
      hash -r
      set_result brew uninstall "${CHECK}"
    else
      set_result brew uninstall "${CROSS}"
      anomaly "brew: uninstall failed. Log: /tmp/verify-brew.out"
    fi
  else
    register_channel brew
    set_result brew install "${CROSS}"
    anomaly "brew: install failed. Log: /tmp/verify-brew.out"
  fi
fi

# ── Summary table ─────────────────────────────────────────────────────────
echo
title "══════ Summary ══════"
printf '\n  %-8s │ %-9s │ %-9s │ %-6s │ %-8s │ %s\n' \
  "channel" "install" "version" "help" "config" "uninstall"
printf '  %s─┼─%s─┼─%s─┼─%s─┼─%s─┼─%s\n' \
  "--------" "---------" "---------" "------" "--------" "---------"

# Note: ANSI codes add width. Pad the payload with printf %s (not %-Ns) so
# the layout looks right regardless of color codes.
for ch in $CHANNELS; do
  printf '  %-8s │    %s      │    %s      │   %s    │    %s     │    %s\n' \
    "$ch" \
    "$(get_result $ch install)" \
    "$(get_result $ch version)" \
    "$(get_result $ch help)" \
    "$(get_result $ch config)" \
    "$(get_result $ch uninstall)"
done
echo

# Detailed per-channel info
for ch in $CHANNELS; do
  printf '  %s%s%s\n' "${C_BOLD}" "$ch" "${C_RESET}"
  p=$(get_info $ch path);    [ -n "$p" ] && printf '    path:    %s\n' "$p"
  v=$(get_info $ch version); [ -n "$v" ] && printf '    version: %s\n' "$v"
  h=$(get_info $ch help);    [ -n "$h" ] && printf '    help:    %s\n' "$h"
done

# ── Config integrity ──────────────────────────────────────────────────────
CONFIG_SHA_AFTER=$(file_sha "$CONFIG_FILE")
if [ "$CONFIG_SHA_BEFORE" = "$CONFIG_SHA_AFTER" ]; then
  printf '\n  %s config untouched (sha256 unchanged)\n' "${CHECK}"
else
  printf '\n  %s config changed during run!\n' "${CROSS}"
  printf '      before: %s\n' "$CONFIG_SHA_BEFORE"
  printf '      after:  %s\n' "$CONFIG_SHA_AFTER"
  anomaly "config.toml sha256 changed during verification (should never happen)"
fi

# ── Anomalies ─────────────────────────────────────────────────────────────
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
