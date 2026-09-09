use serde::{Deserialize,Serialize};
use serde_json::Value;
use thiserror::Error;
#[derive(Debug,Error)] pub enum RpcError { #[error("invalid utf-8: {0}")] Utf8(#[from] std::str::Utf8Error), #[error("invalid json: {0}")] Json(#[from] serde_json::Error), #[error("frame exceeds limit")] FrameTooLarge }
#[derive(Debug,Clone,Serialize,Deserialize)] pub struct RpcMessage { pub id: Option<String>, #[serde(flatten)] pub body: Value }
pub struct JsonlDecoder { buf: Vec<u8>, max_line: usize }
impl JsonlDecoder { pub fn new(max_line:usize)->Self{Self{buf:Vec::new(),max_line}} pub fn push(&mut self, bytes:&[u8])->Result<Vec<RpcMessage>,RpcError>{self.buf.extend_from_slice(bytes); if self.buf.len()>self.max_line*2{return Err(RpcError::FrameTooLarge)}; let mut out=Vec::new(); while let Some(i)=self.buf.iter().position(|b|*b==b'\n'){let line=self.buf.drain(..=i).collect::<Vec<_>>(); let s=std::str::from_utf8(&line[..line.len()-1])?.trim(); if !s.is_empty(){out.push(serde_json::from_str(s)?);}} if self.buf.len()>self.max_line{return Err(RpcError::FrameTooLarge)} Ok(out)} }
#[cfg(test)] mod tests { use super::*; #[test] fn handles_split_utf8(){let mut d=JsonlDecoder::new(1024); let x=r#"{"id":"1","type":"ready"}
"#; let a=x.as_bytes(); assert!(d.push(&a[..5]).unwrap().is_empty()); assert_eq!(d.push(&a[5..]).unwrap().len(),1);} }
