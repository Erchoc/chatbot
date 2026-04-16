#!/usr/bin/env bash
# setup.sh — 安装依赖、下载模型、启动语音聊天机器人
# 用法: ./setup.sh [-y]
#   -y  跳过所有交互确认

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
YES=false

# ── 参数解析 ──────────────────────────────────────────────
CHAT_ARGS=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    -y|--yes) YES=true; shift ;;
    --debug) CHAT_ARGS+=("--debug"); shift ;;
    --list-devices) CHAT_ARGS+=("--list-devices"); shift ;;
    --device) CHAT_ARGS+=("--device" "$2"); shift 2 ;;
    *) echo "未知参数: $1"; exit 1 ;;
  esac
done

# ── 工具函数 ──────────────────────────────────────────────
info() { echo "→ $*"; }
ok()   { echo "✅ $*"; }
die()  { echo "❌ $*" >&2; exit 1; }

# 取消所有代理变量（避免 SOCKS 代理干扰安装）
no_proxy() {
  env -u http_proxy -u https_proxy -u HTTP_PROXY -u HTTPS_PROXY \
      -u all_proxy  -u ALL_PROXY \
      "$@"
}

# ── 检查 uv ───────────────────────────────────────────────
if ! command -v uv &>/dev/null; then
  die "未检测到 uv，请先安装：curl -LsSf https://astral.sh/uv/install.sh | sh"
fi
info "uv 已安装：$(uv --version)"

# ── 检查系统依赖（pyaudio 编译需要 portaudio） ────────────
_portaudio_installed() {
  command -v brew    &>/dev/null && brew list portaudio &>/dev/null && return 0
  command -v dpkg    &>/dev/null && dpkg -s portaudio19-dev &>/dev/null && return 0
  pkg-config --exists portaudio-2.0 2>/dev/null && return 0
  return 1
}
if ! _portaudio_installed; then
  if command -v brew &>/dev/null; then
    info "安装系统依赖 portaudio..."
    brew install portaudio
  elif command -v apt-get &>/dev/null; then
    info "安装系统依赖 portaudio19-dev..."
    sudo apt-get install -y portaudio19-dev
  else
    die "缺少 portaudio 开发库，请手动安装后重试"
  fi
fi

# ── 同步依赖（uv 自动管理 .venv 和 uv.lock） ──────────────
info "同步 Python 依赖..."
cd "$SCRIPT_DIR"
no_proxy uv sync
ok "依赖同步完成"

# ── .env 检查 ─────────────────────────────────────────────
ENV_FILE="$SCRIPT_DIR/.env"
EXAMPLE_FILE="$SCRIPT_DIR/.env.example"
if [[ ! -f "$ENV_FILE" ]]; then
  if [[ -f "$EXAMPLE_FILE" ]]; then
    info "未找到 .env，从 .env.example 复制..."
    cp "$EXAMPLE_FILE" "$ENV_FILE"
    echo ""
    echo "⚠️  请编辑 $ENV_FILE 填写 AI_API_KEY / AI_BASE_URL / AI_MODEL，然后重新运行。"
    exit 0
  else
    die "未找到 .env 文件，请创建并填写 AI_API_KEY / AI_BASE_URL / AI_MODEL"
  fi
fi

# ── 下载模型（download.py 自行判断缓存，幂等） ─────────────
info "检查本地模型缓存..."
no_proxy uv run python "$SCRIPT_DIR/download.py"

# ── 启动聊天 ───────────────────────────────────────────────
echo ""
echo "=========================================="
echo "  启动语音聊天机器人..."
echo "  Ctrl+C 退出"
echo "=========================================="
echo ""

exec uv run python "$SCRIPT_DIR/chat.py" "${CHAT_ARGS[@]}"
