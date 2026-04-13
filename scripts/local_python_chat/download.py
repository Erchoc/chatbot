"""
首次使用前运行此脚本，下载并缓存所需模型。
之后 chat.py 完全离线运行，不再需要网络。

下载源：默认走国内镜像 hf-mirror.com
切换官方：在 .env 里设置 HF_ENDPOINT=https://huggingface.co

用法：
  python download.py           # 跳过已缓存的模型
  python download.py --force   # 强制重新下载所有模型
"""

import argparse
import os
import sys
import warnings
warnings.filterwarnings("ignore", category=FutureWarning)
warnings.filterwarnings("ignore", category=UserWarning)

from dotenv import load_dotenv
load_dotenv()

parser = argparse.ArgumentParser()
parser.add_argument("--force", action="store_true", help="删除缓存，强制重新下载所有模型")
args = parser.parse_args()

# 下载源：.env 未设置时默认走国内镜像
endpoint = os.environ.setdefault("HF_ENDPOINT", "https://hf-mirror.com")
print(f"下载源：{endpoint}\n")

# ── 缓存检查 ──────────────────────────────────────────────────────────────────

def is_cached(repo_id: str) -> bool:
    """直接检查 HuggingFace 本地缓存目录，不发出任何网络请求。"""
    from pathlib import Path
    hf_home = os.environ.get("HF_HOME", os.path.join(Path.home(), ".cache", "huggingface"))
    cache_dir = Path(hf_home) / "hub"
    folder = "models--" + repo_id.replace("/", "--")
    blobs = cache_dir / folder / "blobs"
    return blobs.is_dir() and any(blobs.iterdir())

def purge_cache(repo_id: str) -> None:
    """删除指定 repo 的本地 HuggingFace 缓存。"""
    try:
        from huggingface_hub import scan_cache_dir
        cache_info = scan_cache_dir()
        for repo in cache_info.repos:
            if repo.repo_id == repo_id:
                revisions = [r.commit_hash for r in repo.revisions]
                delete_strategy = cache_info.delete_revisions(*revisions)
                delete_strategy.execute()
                print(f"   🗑  已清除缓存：{repo_id}")
                return
    except Exception as e:
        print(f"   ⚠️  清除缓存失败（{e}），继续尝试下载", file=sys.stderr)

# ── 下载步骤 ──────────────────────────────────────────────────────────────────

from huggingface_hub import snapshot_download

def step(label: str, repo_id: str):
    if args.force:
        purge_cache(repo_id)
    elif is_cached(repo_id):
        print(f"⏭  {label} 已缓存，跳过\n")
        return

    print(f"⬇️  {label}...", flush=True)
    try:
        snapshot_download(repo_id)
        print(f"✅ {label} 完成\n")
    except Exception as e:
        print(f"\n❌ {label} 下载失败：{e}", file=sys.stderr)
        print(f"   请检查网络，或在 .env 中设置 HF_ENDPOINT 切换下载源", file=sys.stderr)
        sys.exit(1)

step("Whisper small（语音识别，~244MB）", "Systran/faster-whisper-small")
step("Kokoro-82M（语音合成，~330MB）",   "hexgrad/Kokoro-82M-v1.1-zh")

print("所有模型已就绪，可以运行 chat.py 了。")
