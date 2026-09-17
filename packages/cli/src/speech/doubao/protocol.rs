//! 豆包 openspeech v3 二进制帧。
//!
//! ```text
//! byte0: 高 4 位协议版本(1) | 低 4 位 header 长度(单位 4B，=1)
//! byte1: 高 4 位消息类型   | 低 4 位 flags
//! byte2: 高 4 位序列化(0 无 / 1 JSON) | 低 4 位压缩(0 无 / 1 gzip)
//! byte3: 保留
//! 之后：[i32 sequence（flags 含 SEQ 位时）] u32 payload_size payload
//! 错误帧：u32 error_code u32 msg_size msg
//! ```
//! 参考：<https://www.volcengine.com/docs/6561/1354869>
use std::io::{Read, Write};

use anyhow::Result;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;

pub const MSG_FULL_CLIENT: u8 = 0b0001;
pub const MSG_AUDIO_ONLY: u8 = 0b0010;
pub const MSG_FULL_SERVER: u8 = 0b1001;
pub const MSG_ERROR: u8 = 0b1111;

pub const FLAG_NONE: u8 = 0b0000;
pub const FLAG_HAS_SEQ: u8 = 0b0001;
pub const FLAG_LAST: u8 = 0b0010;

pub const SER_NONE: u8 = 0;
pub const SER_JSON: u8 = 1;
pub const COMP_NONE: u8 = 0;
pub const COMP_GZIP: u8 = 1;

pub fn gzip(data: &[u8]) -> Result<Vec<u8>> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(data)?;
    Ok(enc.finish()?)
}

pub fn gunzip(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    GzDecoder::new(data).read_to_end(&mut out)?;
    Ok(out)
}

/// 编一帧客户端消息（无 sequence 段）。`compression == COMP_GZIP` 时对 payload 做 gzip。
pub fn encode(msg_type: u8, flags: u8, serialization: u8, compression: u8, payload: &[u8]) -> Result<Vec<u8>> {
    let body: std::borrow::Cow<'_, [u8]> = if compression == COMP_GZIP {
        std::borrow::Cow::Owned(gzip(payload)?)
    } else {
        std::borrow::Cow::Borrowed(payload)
    };
    let mut frame = Vec::with_capacity(8 + body.len());
    frame.push(0x10 | 0x01);
    frame.push((msg_type << 4) | (flags & 0x0f));
    frame.push((serialization << 4) | (compression & 0x0f));
    frame.push(0);
    frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
    frame.extend_from_slice(&body);
    Ok(frame)
}

/// 解出来的服务端帧（payload 已解压）。
#[derive(Debug, Default)]
pub struct ServerFrame {
    pub msg_type: u8,
    pub flags: u8,
    pub sequence: Option<i32>,
    pub is_json: bool,
    pub payload: Vec<u8>,
}

impl ServerFrame {
    /// 服务端标记的最后一包（flags 的 LAST 位，或负 sequence）。
    pub fn is_last(&self) -> bool {
        self.flags & FLAG_LAST != 0 || self.sequence.map(|s| s < 0).unwrap_or(false)
    }
}

fn u32_at(buf: &[u8], off: usize) -> Option<u32> {
    buf.get(off..off + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// 解一帧服务端消息。错误帧直接返回 `Err`，其余帧的 payload 已按压缩位解压。
/// 长度不足的帧不 panic，返回空 payload 的帧。
pub fn decode(buf: &[u8]) -> Result<ServerFrame> {
    if buf.len() < 4 {
        anyhow::bail!("response too short ({}B)", buf.len());
    }
    let header_len = ((buf[0] & 0x0f) as usize) * 4;
    let msg_type = buf[1] >> 4;
    let flags = buf[1] & 0x0f;
    let is_json = (buf[2] >> 4) == SER_JSON;
    let gz = (buf[2] & 0x0f) == COMP_GZIP;
    let mut off = header_len;

    if msg_type == MSG_ERROR {
        let code = u32_at(buf, off).unwrap_or(0);
        let size = u32_at(buf, off + 4).unwrap_or(0) as usize;
        let raw = buf.get(off + 8..off + 8 + size).unwrap_or(&[]);
        let msg = if gz { gunzip(raw).unwrap_or_else(|_| raw.to_vec()) } else { raw.to_vec() };
        anyhow::bail!("server error code={code}: {}", String::from_utf8_lossy(&msg));
    }

    let mut sequence = None;
    if flags & FLAG_HAS_SEQ != 0 || flags == 0b0011 {
        sequence = u32_at(buf, off).map(|v| v as i32);
        off += 4;
    }
    let payload = match u32_at(buf, off) {
        Some(size) => {
            off += 4;
            let raw = buf.get(off..off + size as usize).unwrap_or(&[]);
            if gz && !raw.is_empty() { gunzip(raw)? } else { raw.to_vec() }
        }
        None => Vec::new(),
    };
    Ok(ServerFrame { msg_type, flags, sequence, is_json, payload })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_then_decode_roundtrip_with_gzip() {
        let payload = r#"{"result":{"text":"你好"}}"#.as_bytes();
        let mut frame = encode(MSG_FULL_SERVER, FLAG_HAS_SEQ, SER_JSON, COMP_GZIP, payload).unwrap();
        // 手工插入 sequence 段模拟服务端帧
        frame.splice(4..4, 7_i32.to_be_bytes());
        let f = decode(&frame).unwrap();
        assert_eq!(f.msg_type, MSG_FULL_SERVER);
        assert_eq!(f.sequence, Some(7));
        assert!(!f.is_last());
        assert_eq!(f.payload, payload);
    }

    #[test]
    fn negative_sequence_marks_last() {
        let mut frame = encode(MSG_FULL_SERVER, 0b0011, SER_JSON, COMP_NONE, b"{}").unwrap();
        frame.splice(4..4, (-1_i32).to_be_bytes());
        assert!(decode(&frame).unwrap().is_last());
    }

    #[test]
    fn error_frame_becomes_err() {
        let mut frame = vec![0x11, (MSG_ERROR << 4), 0x10, 0x00];
        frame.extend_from_slice(&45000001_u32.to_be_bytes());
        let msg = b"invalid param";
        frame.extend_from_slice(&(msg.len() as u32).to_be_bytes());
        frame.extend_from_slice(msg);
        let err = decode(&frame).unwrap_err().to_string();
        assert!(err.contains("45000001") && err.contains("invalid param"));
    }

    #[test]
    fn truncated_frame_does_not_panic() {
        let f = decode(&[0x11, MSG_FULL_SERVER << 4, 0x10, 0x00, 0, 0]).unwrap();
        assert!(f.payload.is_empty());
    }
}
