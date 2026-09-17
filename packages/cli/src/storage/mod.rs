//! 基础设施层 · 本地持久化
//!
//! 所有落盘到 `~/.config/chatbot/` 的运行时数据都在这里：
//! - `history`：对话历史（JSON，按会话一文件）
//! - `events`：结构化事件日志（JSONL，按日期切分）
pub mod events;
pub mod history;
