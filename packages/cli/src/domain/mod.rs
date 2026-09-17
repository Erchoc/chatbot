//! 领域层 · 纯逻辑
//!
//! 这一层**不做任何 IO**（无网络、无文件、无终端输出、无系统调用），
//! 只依赖标准库和纯计算 crate（如 `pinyin`），因此全部可以直接单元测试。
//!
//! - `wake`：唤醒词会话状态机 + 唤醒词 / 退下短语匹配（含拼音同音）
//! - `sentence`：LLM 流式 token 按句切分，供 TTS 批量合成
//! - `metrics`：单轮对话耗时指标
//! - `semver`：版本号比较（含预发布规则）
//! - `health`：守护进程日志的健康诊断规则
pub mod health;
pub mod metrics;
pub mod semver;
pub mod sentence;
pub mod wake;
