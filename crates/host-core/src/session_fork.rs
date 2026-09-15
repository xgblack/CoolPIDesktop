//! Fork a finalized conversation boundary into a new OMP v3 session.
//! No RPC session-changing command is sent to the source runtime.
use crate::{HostError, RpcQuery, Workbench, runtime::Result, session::session_header, store::TaskRecord};
use crate::session_export::{blob_path, read_file};
use base64::Engine;
use serde_json::{Value, json};
use std::{collections::{HashMap, HashSet}, io::Write, path::Path};

const LIMIT: u64 = 64 * 1024 * 1024;
fn invalid(message: impl std::fmt::Display) -> HostError { HostError::new("session_fork_failed", message) }

fn branch_entries(file: &Path, expected: &str, timestamp: f64) -> Result<(Value, Vec<Value>)> {
    let header = session_header(file)?;
    if header["id"].as_str() != Some(expected) { return Err(invalid("会话身份与任务绑定不一致")); }
    if header["version"] != 3 { return Err(invalid("仅支持当前 OMP v3 会话格式的 Fork")); }
    let raw = read_file(file, LIMIT)?;
    let all: Vec<Value> = raw.split(|b| *b == b'\n').filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).map_err(|_| invalid("会话记录尚未完整保存"))).collect::<Result<_>>()?;
    let entries: Vec<_> = all.into_iter().filter(|v| v["type"] != "session" && v["type"] != "title").collect();
    let mut by_id = HashMap::new();
    for entry in &entries {
        if let Some(id) = entry["id"].as_str() {
            if by_id.insert(id, entry).is_some() { return Err(invalid("会话含重复记录 ID")); }
        }
    }
    let mut seen = HashSet::new();
    let mut chain = Vec::new();
    let mut node = entries.iter().rev().find(|v| v["id"].is_string());
    while let Some(entry) = node {
        let id = entry["id"].as_str().ok_or_else(|| invalid("会话记录缺少 ID"))?;
        if !seen.insert(id) { return Err(invalid("会话记录存在循环引用")); }
        chain.push(entry.clone());
        node = match entry["parentId"].as_str() {
            Some(parent) => Some(*by_id.get(parent).ok_or_else(|| invalid("会话父记录缺失"))?),
            None => None,
        };
    }
    chain.reverse();
    let targets: Vec<_> = chain.iter().enumerate().filter(|(_, e)| e["type"] == "message" && e["message"]["role"] == "assistant" && e["message"]["timestamp"].as_f64() == Some(timestamp)).map(|(i, _)| i).collect();
    if targets.len() != 1 { return Err(invalid("所选回复不在当前会话分支中，请刷新历史")); }
    let index = targets[0];
    let message = &chain[index]["message"];
    if matches!(message["stopReason"].as_str(), Some("error" | "aborted" | "toolUse"))
        || message["content"].as_array().is_some_and(|blocks| blocks.iter().any(|b| matches!(b["type"].as_str(), Some("toolCall" | "tool_use"))))
        || chain[index+1..].iter().find(|e| e["type"] == "message").is_some_and(|e| e["message"]["role"] != "user")
        || !chain[..index].iter().any(|e| e["type"] == "message" && e["message"]["role"] == "user")
    { return Err(invalid("只能从已结束轮次的最终回复创建 Fork")); }
    chain.truncate(index + 1);
    // session_init is out-of-band in OMP; retain its original system prompt snapshot.
    let init: Vec<_> = entries.iter().filter(|e| e["type"] == "session_init" && !chain.iter().any(|c| c == *e)).cloned().collect();
    chain.splice(0..0, init);
    Ok((header, chain))
}

fn write_fork(file: &Path, expected: &str, timestamp: f64, destination: &Path, session: &str, title: &str, roots: &[std::path::PathBuf]) -> Result<()> {
    let (_, mut entries) = branch_entries(file, expected, timestamp)?;
    // Resolve inherited images into this file so the fork never depends on a source-local blob directory.
    fn replace(slot: Option<&mut Value>, url: bool, source: &Path, cache: &mut HashMap<String, Vec<u8>>, total: &mut u64) -> Result<()> {
        let Some(Value::String(value)) = slot else { return Ok(()); };
        let Some(hash) = value.strip_prefix("blob:sha256:") else { return Ok(()); };
        if hash.len() != 64 || !hash.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) { return Err(invalid("会话包含无效图片引用")); }
        if !cache.contains_key(hash) {
            let bytes = read_file(&blob_path(source, hash)?, LIMIT - *total)?;
            *total += bytes.len() as u64;
            cache.insert(hash.to_owned(), bytes);
        }
        let bytes = &cache[hash];
        *value = if url { String::from_utf8(bytes.clone()).map_err(|_| invalid("图片 URL 资源无效"))? }
            else { base64::engine::general_purpose::STANDARD.encode(bytes) };
        Ok(())
    }
    fn resolve(value: &mut Value, key: &str, source: &Path, cache: &mut HashMap<String, Vec<u8>>, total: &mut u64) -> Result<()> {
        match value {
            Value::Array(values) => for value in values { resolve(value, key, source, cache, total)?; },
            Value::Object(values) => {
                if key == "images" || key == "frames" || key == "content" && values.get("type").is_some_and(|v| v == "image") {
                    replace(values.get_mut("data"), false, source, cache, total)?;
                }
                if values.get("type").is_some_and(|v| v == "image_generation_call") { replace(values.get_mut("result"), false, source, cache, total)?; }
                replace(values.get_mut("image_url"), true, source, cache, total)?;
                if key == "image_url" { replace(values.get_mut("url"), true, source, cache, total)?; }
                for (key, value) in values { resolve(value, key, source, cache, total)?; }
            },
            _ => {}
        }
        Ok(())
    }
    let mut cache = HashMap::new();
    let mut total = 0;
    for entry in &mut entries { resolve(entry, "", file, &mut cache, &mut total)?; }
    let now: chrono::DateTime<chrono::Utc> = std::time::SystemTime::now().into();
    let header = json!({"type":"session","version":3,"id":session,"timestamp":now.to_rfc3339(),"cwd":roots[0],"additionalDirectories":&roots[1..],"title":title,"parentSession":file});
    let mut bytes = serde_json::to_vec(&header).map_err(invalid)?;
    bytes.push(b'\n');
    for entry in entries {
        let line = serde_json::to_vec(&entry).map_err(invalid)?;
        if line.len() > 16 * 1024 * 1024 { return Err(invalid("Fork 中的单条消息超过 16 MiB")); }
        bytes.extend(line);bytes.push(b'\n');
        if bytes.len() as u64 > LIMIT { return Err(invalid("Fork 会话超过 64 MiB")); }
    }
    std::fs::create_dir(destination.parent().ok_or_else(|| invalid("无效目标目录"))?).map_err(invalid)?;
    let mut output = std::fs::OpenOptions::new().create_new(true).write(true).open(destination).map_err(invalid)?;
    output.write_all(&bytes).and_then(|_| output.sync_all()).map_err(invalid)
}

impl Workbench {
    pub async fn fork_task(&self, id: &str, timestamp: f64) -> Result<TaskRecord> {
        let _guard = self.gate.lock().await;
        if !timestamp.is_finite() || timestamp < 0.0 { return Err(invalid("无效的回复时间戳")); }
        self.check_handoff(id).await?;
        let roots = self.store.validate_task_roots(id).await?;
        let source = self.store.task(id).await?;
        if let Ok(snapshot) = self.runtime.snapshot(id).await {
            if !matches!(snapshot.status.as_str(), "stopped" | "failed") {
                if !matches!(snapshot.status.as_str(), "ready" | "idle" | "interrupted") || !snapshot.pending_ui.is_empty() {
                    return Err(HostError::new("task_busy", "请等待当前生成或审批结束后 Fork"));
                }
                let state = self.runtime.query(id, RpcQuery::GetState, json!({})).await?;
                if state["isStreaming"] != false || state["isCompacting"] != false || state["queuedMessageCount"] != 0 {
                    return Err(HostError::new("task_busy", "请等待生成、压缩和排队消息结束后 Fork"));
                }
            }
        }
        let file = source.session_file.clone().ok_or_else(|| invalid("会话尚未保存"))?;
        let expected = source.session_id.clone().ok_or_else(|| invalid("会话尚未绑定"))?;
        let task_id = uuid::Uuid::new_v4().to_string();
        let session_id = uuid::Uuid::new_v4().to_string();
        let title = format!("{} · Fork", source.title.chars().take(100).collect::<String>());
        let sessions = self.data.join("sessions");
        tokio::fs::create_dir_all(&sessions).await.map_err(invalid)?;
        let destination = sessions.join(&task_id).join(format!("{session_id}.jsonl"));
        let (dest, session, name, dirs) = (destination.clone(), session_id.clone(), title.clone(), roots.clone());
        tokio::task::spawn_blocking(move || write_fork(&file, &expected, timestamp, &dest, &session, &name, &dirs)).await.map_err(invalid)??;
        let destination = tokio::fs::canonicalize(destination).await.map_err(invalid)?;
        self.store.insert_fork(source, task_id, title, roots, session_id, destination).await
    }
}
