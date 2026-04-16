#!/usr/bin/env bash
# setup.sh — 编译并启动远程语音聊天机器人 (Rust)
# 用法: ./setup.sh [-y]
#   -y  跳过所有交互确认

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
YES=false
DEV=false
DEBUG=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    -y|--yes) YES=true; shift ;;
    --dev) DEV=true; shift ;;
    --debug) DEBUG=true; shift ;;
    *) echo "未知参数: $1"; exit 1 ;;
  esac
done

info() { echo "→ $*"; }
ok()   { echo "✅ $*"; }
die()  { echo "❌ $*" >&2; exit 1; }

# ── 检查 Rust 工具链 ──────────────────────────────────────
# 确保 cargo 在 PATH 中
[[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"
export PATH="$HOME/.cargo/bin:$PATH"

if ! command -v cargo &>/dev/null; then
  die "未检测到 cargo，请先安装 Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
fi
info "Rust 已安装：$(rustc --version)"

# ── 加载用户环境变量（zshenv 中可能有 DOUBAO_* 等） ─────
[[ -f "$HOME/.zshenv" ]] && source "$HOME/.zshenv"

# ── .env 检查 ─────────────────────────────────────────────
ENV_FILE="$SCRIPT_DIR/.env"
EXAMPLE_FILE="$SCRIPT_DIR/.env.example"
if [[ ! -f "$ENV_FILE" ]]; then
  if [[ -f "$EXAMPLE_FILE" ]]; then
    info "未找到 .env，从 .env.example 复制..."
    cp "$EXAMPLE_FILE" "$ENV_FILE"
    echo ""
    echo "⚠️  请编辑 $ENV_FILE 填写配置后重新运行。"
    echo "   必填: AI_API_KEY, AI_BASE_URL, AI_MODEL, DOUBAO_APP_ID, DOUBAO_ACCESS_TOKEN"
    exit 0
  else
    die "未找到 .env 文件，请创建并填写配置"
  fi
fi

# ── 编译（dev 模式，首次较慢） ────────────────────────────
info "编译项目 (dev 模式)..."
cd "$SCRIPT_DIR"
cargo build 2>&1
ok "编译完成"

# ── 启动 ──────────────────────────────────────────────────
echo ""
echo "=========================================="
echo "  启动远程语音聊天机器人 (Rust)..."
echo "  Ctrl+C 退出"
echo "=========================================="
echo ""

export SCRIPT_DIR
EXTRA_ARGS=""
$DEBUG && EXTRA_ARGS="-- --debug"

if $DEV && command -v cargo-watch &>/dev/null; then
  info "dev 模式：文件变更自动重编译重启"
  exec cargo watch -x "run $EXTRA_ARGS" 2>&1
else
  exec cargo run $EXTRA_ARGS 2>&1
fi
