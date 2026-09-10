//! OMP JSONL / v2 wire codec. Protocol reference: OMP 2c523a94 docs/rpc.md.
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use thiserror::Error;

pub const MAX_FRAME: usize = 1024 * 1024;
pub const MAX_LOGICAL: usize = 64 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum RpcError {
    #[error("invalid utf-8: {0}")] Utf8(#[from] std::str::Utf8Error),
    #[error("invalid json: {0}")] Json(#[from] serde_json::Error),
    #[error("frame exceeds limit")] FrameTooLarge,
    #[error("invalid or interrupted chunk sequence")] ChunkOrder,
    #[error("truncated frame at EOF")] Truncated,
    #[error("protocol object required")] InvalidObject,
}

struct Assembly { id: String, count: usize, length: usize, next: usize, bytes: Vec<u8> }
pub struct JsonlDecoder {
    buf: Vec<u8>, max_line: usize, max_logical: usize, assembly: Option<Assembly>,
}
impl JsonlDecoder {
    pub fn new(max_line: usize) -> Self {
        Self { buf: Vec::new(), max_line, max_logical: MAX_LOGICAL, assembly: None }
    }
    pub fn limits(&mut self, line: usize, logical: usize) -> Result<(), RpcError> {
        if line == 0 || logical == 0 { return Err(RpcError::FrameTooLarge); }
        self.max_line = line.min(MAX_FRAME); self.max_logical = logical.min(MAX_LOGICAL); Ok(())
    }
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Value>, RpcError> {
        let mut out = Vec::new();
        // Bound each physical line, not an arbitrary read containing many valid lines.
        for part in bytes.split_inclusive(|b| *b == b'\n') {
            if self.buf.len() + part.len() > self.max_line { return Err(RpcError::FrameTooLarge); }
            self.buf.extend_from_slice(part);
            if part.last() == Some(&b'\n') {
                let line = std::mem::take(&mut self.buf);
                let text = std::str::from_utf8(&line)?.trim();
                if text.is_empty() { continue; }
                let value: Value = serde_json::from_str(text)?;
                if let Some(value) = self.frame(value)? { out.push(value); }
            }
        }
        Ok(out)
    }
    fn frame(&mut self, v: Value) -> Result<Option<Value>, RpcError> {
        if !v.is_object() || !v["type"].is_string() { return Err(RpcError::InvalidObject); }
        if v["type"] != "rpc_chunk" {
            if self.assembly.is_some() { return Err(RpcError::ChunkOrder); }
            return Ok(Some(v));
        }
        let id = v["chunkId"].as_str().filter(|s| !s.is_empty()).ok_or(RpcError::ChunkOrder)?;
        let n = |key: &str| v[key].as_u64().and_then(|n| usize::try_from(n).ok()).ok_or(RpcError::ChunkOrder);
        let (index, count, length) = (n("index")?, n("count")?, n("byteLength")?);
        if length == 0 || length > self.max_logical || count == 0 || count > length || index >= count {
            return Err(RpcError::FrameTooLarge);
        }
        let bytes = STANDARD.decode(v["data"].as_str().ok_or(RpcError::ChunkOrder)?).map_err(|_| RpcError::ChunkOrder)?;
        if bytes.is_empty() { return Err(RpcError::ChunkOrder); }
        if self.assembly.is_none() {
            if index != 0 { return Err(RpcError::ChunkOrder); }
            self.assembly = Some(Assembly { id: id.into(), count, length, next: 0, bytes: Vec::new() });
        }
        let a = self.assembly.as_mut().unwrap();
        if a.id != id || a.count != count || a.length != length || a.next != index { return Err(RpcError::ChunkOrder); }
        if a.bytes.len() + bytes.len() > length { return Err(RpcError::FrameTooLarge); }
        a.bytes.extend(bytes); a.next += 1;
        if a.next != count { return Ok(None); }
        let a = self.assembly.take().unwrap();
        if a.bytes.len() != length { return Err(RpcError::Truncated); }
        let v: Value = serde_json::from_str(std::str::from_utf8(&a.bytes)?)?;
        if !v.is_object() || !v["type"].is_string() || v["type"] == "rpc_chunk" { return Err(RpcError::InvalidObject); }
        Ok(Some(v))
    }
    pub fn finish(&self) -> Result<(), RpcError> {
        if !self.buf.is_empty() || self.assembly.is_some() { Err(RpcError::Truncated) } else { Ok(()) }
    }
}

pub fn encode_request(id: &str, typ: &str, payload: Value, limit: usize) -> Result<Vec<u8>, RpcError> {
    let mut v = match payload { Value::Object(v) => v, _ => return Err(RpcError::InvalidObject) };
    v.insert("id".into(), json!(id)); v.insert("type".into(), json!(typ));
    let mut b = serde_json::to_vec(&v)?; b.push(b'\n');
    if b.len() > limit.min(MAX_FRAME) { return Err(RpcError::FrameTooLarge); } Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn line(v: Value) -> Vec<u8> { let mut b = serde_json::to_vec(&v).unwrap(); b.push(b'\n'); b }
    fn chunks(bytes: &[u8]) -> Vec<Vec<u8>> {
        let half = bytes.len()/2;
        [ &bytes[..half], &bytes[half..] ].iter().enumerate().map(|(i,b)| line(json!({"type":"rpc_chunk","chunkId":"a","index":i,"count":2,"byteLength":bytes.len(),"data":STANDARD.encode(b)}))).collect()
    }
    #[test] fn split_chinese_at_every_byte() {
        let v=json!({"type":"message_update","delta":"中文🙂"}); let bytes=line(v.clone());
        for i in 0..bytes.len() { let mut d=JsonlDecoder::new(1024); assert!(d.push(&bytes[..i]).unwrap().is_empty()); assert_eq!(d.push(&bytes[i..]).unwrap(),vec![v.clone()]); d.finish().unwrap(); }
    }
    #[test] fn many_lines_in_one_read_and_exact_limit() {
        let b=line(json!({"type":"ready"})); let mut d=JsonlDecoder::new(b.len()); assert_eq!(d.push(&b.repeat(100)).unwrap().len(),100);
        assert!(JsonlDecoder::new(b.len()-1).push(&b).is_err());
    }
    #[test] fn reassembles_large_frame() {
        let v=json!({"type":"message_update","delta":"中".repeat(400_000)}); let b=serde_json::to_vec(&v).unwrap(); let c=chunks(&b); let mut d=JsonlDecoder::new(MAX_FRAME);
        assert!(d.push(&c[0]).unwrap().is_empty()); assert_eq!(d.push(&c[1]).unwrap(),vec![v]); d.finish().unwrap();
    }
    #[test] fn rejects_order_missing_interleaving_and_size() {
        let c=chunks(br#"{"type":"ready"}"#);
        assert!(JsonlDecoder::new(1024).push(&c[1]).is_err());
        let mut d=JsonlDecoder::new(1024); d.push(&c[0]).unwrap(); assert!(d.finish().is_err()); assert!(d.push(&line(json!({"type":"ready"}))).is_err());
        let mut d=JsonlDecoder::new(1024); d.limits(1024,2).unwrap(); assert!(d.push(&c[0]).is_err());
        let mut d=JsonlDecoder::new(1024); d.push(b"{").unwrap(); assert!(d.finish().is_err());
    }
    #[test] fn rejects_bad_json_utf8_base64_and_nested_chunks() {
        for b in [b"{\n".as_slice(), b"\xff\n", b"[]\n"] { assert!(JsonlDecoder::new(1024).push(b).is_err()); }
        let v=json!({"type":"rpc_chunk","chunkId":"a","index":0,"count":1,"byteLength":1,"data":"??"}); assert!(JsonlDecoder::new(1024).push(&line(v)).is_err());
        let c=chunks(br#"{"type":"rpc_chunk"}"#); let mut d=JsonlDecoder::new(1024); d.push(&c[0]).unwrap(); assert!(d.push(&c[1]).is_err());
    }
    #[test] fn encoder_cannot_override_identity_and_enforces_limit() {
        let b=encode_request("1","prompt",json!({"id":"evil","type":"bash"}),1024).unwrap(); let v:Value=serde_json::from_slice(&b).unwrap(); assert_eq!(v["id"],"1"); assert_eq!(v["type"],"prompt"); assert!(encode_request("1","prompt",json!({}),2).is_err());
    }
}
