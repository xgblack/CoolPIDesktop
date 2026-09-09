use serde::{Deserialize,Serialize};use serde_json::Value;use thiserror::Error;
#[derive(Debug,Error)]pub enum RpcError{#[error("invalid utf-8: {0}")]Utf8(#[from]std::str::Utf8Error),#[error("invalid json: {0}")]Json(#[from]serde_json::Error),#[error("frame exceeds limit")]FrameTooLarge,#[error("chunk out of order")]ChunkOrder,#[error("truncated frame")]Truncated}
#[derive(Debug,Clone,Serialize,Deserialize)]pub struct RpcMessage{pub id:Option<String>,#[serde(flatten)]pub body:Value}
pub struct JsonlDecoder{buf:Vec<u8>,max_line:usize}impl JsonlDecoder{pub fn new(max_line:usize)->Self{Self{buf:vec![],max_line}}pub fn push(&mut self,b:&[u8])->Result<Vec<RpcMessage>,RpcError>{self.buf.extend_from_slice(b);if self.buf.len()>self.max_line{return Err(RpcError::FrameTooLarge)}let mut o=vec![];while let Some(i)=self.buf.iter().position(|x|*x==b'\n'){let l=self.buf.drain(..=i).collect::<Vec<_>>();let s=std::str::from_utf8(&l[..l.len()-1])?.trim();if !s.is_empty(){o.push(serde_json::from_str(s)?);}}Ok(o)}}
#[cfg(test)]
mod tests {
 use super::*;
 #[test] fn split(){ let mut d=JsonlDecoder::new(100); assert!(d.push(b"{").unwrap().is_empty()); assert_eq!(d.push(b"\"id\":\"1\"}\n").unwrap().len(),1); }
 #[test] fn oversized(){ assert!(matches!(JsonlDecoder::new(2).push(b"123"),Err(RpcError::FrameTooLarge))); }
}
