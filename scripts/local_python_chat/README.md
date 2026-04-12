# chatbot-py

本地运行的中文语音对话机器人。STT + TTS 完全本地推理，仅 LLM 调用远程 API。

```
麦克风 ──▶ Whisper (STT) ──▶ LLM (流式) ──▶ Kokoro (TTS) ──▶ 扬声器
```

- **STT**：[faster-whisper](https://github.com/SYSTRAN/faster-whisper) — 本地 Whisper small，中文识别
- **LLM**：任意 OpenAI 兼容接口（DeepSeek / OpenAI / Ollama 等），流式输出实时显示
- **TTS**：[Kokoro-82M](https://huggingface.co/hexgrad/Kokoro-82M-v1.1-zh) — 本地中文语音合成
- **打断**：说话中途开口即可打断助手播放
- **性能**：每轮对话结束后打印 STT / LLM 首字延迟 / TTS / 总耗时

> 支持 macOS、Linux

---

## 环境要求

- Python 3.10+
- 麦克风 + 扬声器

**macOS** 需要先安装 PortAudio：

```bash
brew install portaudio
```

**Linux** 需要安装 PortAudio 和 ALSA 开发库：

```bash
# Debian / Ubuntu
sudo apt install portaudio19-dev python3-dev

# Arch
sudo pacman -S portaudio
```

---

## 快速启动

**1. 安装依赖**

```bash
pip install -r requirements.txt
```

**2. 配置环境变量**

```bash
cp .env.example .env
```

编辑 `.env`：

```env
AI_BASE_URL=https://api.deepseek.com
AI_API_KEY=sk-xxxxxxxxxxxxxxxx
AI_MODEL=deepseek-chat
```

支持任何 OpenAI 兼容服务：

| 服务 | AI_BASE_URL | AI_MODEL |
|------|-------------|---------------|
| DeepSeek | `https://api.deepseek.com` | `deepseek-chat` |
| OpenAI | `https://api.openai.com/v1` | `gpt-4o` |
| Ollama（本地） | `http://localhost:11434/v1` | `qwen2.5` |

**3. 下载模型（首次使用）**

```bash
python download_models.py
```

默认走国内镜像 `hf-mirror.com`，下载约 570MB。切换官方源：在 `.env` 里设置 `HF_ENDPOINT=https://huggingface.co`。

模型缓存后，之后运行无需网络（LLM API 除外）。

**4. 运行**

```bash
python chatbot.py
```

---

## 运行效果

启动阶段：

```
[14:23:01] Python 启动，开始加载依赖...

🚀 启动语音助手  (依赖加载耗时 3.2s)

   加载 Whisper STT... ✅
   加载 Kokoro TTS... ✅
   加载中文分词... ✅
   连接 LLM... ✅
   🔇 校准环境噪音...  噪音:142 阈值:426

==========================================
  🤖 语音助手已就绪
  💡 说话即可对话，对话中说话可打断
  ⛔ Ctrl+C 退出
==========================================
```

对话中（LLM 流式实时打印，最后一行为性能统计）：

```
🎤 说话吧...
   🟢 检测到语音...

   🗣️ 你: 今天天气怎么样？
   🤖 助手: 我没有实时天气数据，不过你可以看看窗外！有什么我能帮你的吗？
   STT 0.8s │ LLM首字 0.3s · 18tok · 31tok/s │ TTS 1.1s │ 总计 2.2s
```

---

## 命令行参数

```bash
# 列出所有可用麦克风设备
python chatbot.py --list-devices

# 指定麦克风设备编号（编号从 --list-devices 获取）
python chatbot.py --device 2
```

---

## 主要参数

`chatbot.py` 顶部配置区可调整：

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `WHISPER_MODEL` | `small` | Whisper 模型大小（tiny / base / small / medium） |
| `SPEECH_START_CHUNKS` | `5` | 连续多少帧响亮才确认开口（越大越不灵敏） |
| `MIN_LOUD_CHUNKS` | `8` | 整段录音至少多少响亮帧才送 STT |
| `SILENCE_SECONDS` | `0.8` | 停顿多久判断说完 |
| `TTS_SPEED` | `1.3` | 语速倍率 |

---

## 常见问题

**Q: 键盘声 / 环境噪音误触发**

调大 `SPEECH_START_CHUNKS`（如改为 `7`），需要更持续的声音才触发录音。

**Q: 说话没有被识别**

调小 `SPEECH_START_CHUNKS` 或 `MIN_LOUD_CHUNKS`，或在更安静的环境下重启，让噪音校准更准确。

**Q: PyAudio 安装报错**

macOS 需要先 `brew install portaudio`，Linux 需要先安装 `portaudio19-dev`，见上方环境要求。

**Q: 模型下载失败**

在 `.env` 里设置 `HF_ENDPOINT=https://huggingface.co` 切换到官方源，或检查网络是否能访问 `hf-mirror.com`。

**Q: 想换其他 LLM**

只需修改 `.env` 里的三个变量，代码无需改动。
