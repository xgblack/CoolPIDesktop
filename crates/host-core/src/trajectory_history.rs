//! Read-only, bounded projection of an OMP session JSONL file.
use crate::runtime::{HostError, Result};
use crate::task::UsageSummary;
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Image {
    pub id: String,
    pub mime: String,
    pub label: String,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub id: String,
    pub aliases: Vec<String>,
    pub kind: String,
    pub turn: Option<u64>,
    pub step: Option<u64>,
    pub parent_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub name: String,
    pub status: String,
    pub content: Value,
    pub input: Option<Value>,
    pub output: Option<Value>,
    pub model: Option<String>,
    pub started_at: Option<f64>,
    pub completed_at: Option<f64>,
    pub duration_ms: Option<f64>,
    pub ttft_ms: Option<f64>,
    pub timing_source: Option<String>,
    pub usage: Option<UsageSummary>,
    pub error: Option<String>,
    pub images: Vec<Image>,
    #[serde(skip)]
    pub(crate) resources: Vec<(Image, String)>,
    pub truncated: bool,
}
use base64::Engine;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const MAX_FILE: u64 = 64 * 1024 * 1024;
const MAX_LINE: usize = 16 * 1024 * 1024;
const MAX_VALUE: usize = 1024 * 1024;
const MAX_IMAGE: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct History {
    pub records: Vec<Record>,
    pub revision: String,
    pub warnings: Vec<String>,
}

fn err(code: &str, msg: impl Into<String>) -> HostError {
    HostError::new(code, msg.into())
}
fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(text).collect::<Vec<_>>().join("\n"),
        Value::Object(o) => o.get("text").map(text).unwrap_or_else(|| v.to_string()),
        _ => v.to_string(),
    }
}
fn bounded(v: Value, warnings: &mut Vec<String>) -> (Value, bool) {
    let s = v.to_string();
    if s.len() <= MAX_VALUE {
        return (v, false);
    }
    warnings.push("trajectory value truncated at 1 MiB".into());
    (
        Value::String(s[..s.floor_char_boundary(MAX_VALUE)].to_string()),
        true,
    )
}
fn num(v: Option<&Value>) -> Option<f64> {
    v.and_then(|x| x.as_f64().or_else(|| x.as_u64().map(|n| n as f64)))
}
fn stamp(v: Option<&Value>) -> Option<f64> {
    let v = v?;
    v.as_f64().or_else(|| {
        v.as_str()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.timestamp_millis() as f64)
    })
}
fn usage(v: Option<&Value>) -> Option<UsageSummary> {
    let v = v?;
    Some(UsageSummary {
        input_tokens: v["input"].as_u64().or(v["inputTokens"].as_u64()),
        output_tokens: v["output"].as_u64().or(v["outputTokens"].as_u64()),
        reasoning_tokens: v["reasoning"].as_u64().or(v["reasoningTokens"].as_u64()),
        cache_read_tokens: v["cacheRead"].as_u64().or(v["cacheReadTokens"].as_u64()),
        cache_write_tokens: v["cacheWrite"].as_u64().or(v["cacheWriteTokens"].as_u64()),
        total_tokens: v["totalTokens"].as_u64(),
        cost: v["cost"]["total"].as_f64().or(v["cost"].as_f64()),
        ..Default::default()
    })
}
fn image_payloads(v: &Value) -> Vec<(Image, String)> {
    fn walk(v: &Value, out: &mut Vec<(Image, String)>) {
        if out.len() >= 64 {
            return;
        }
        match v {
            Value::Object(o) => {
                let data = o
                    .get("data")
                    .and_then(Value::as_str)
                    .filter(|_| {
                        o.get("type").is_some_and(|v| v == "image") || o.contains_key("mimeType")
                    })
                    .or_else(|| {
                        o.get("image_url")
                            .and_then(|v| v.as_str().or_else(|| v["url"].as_str()))
                    });
                if let Some(data) = data {
                    let (mime, payload) = if let Some((header, payload)) = data
                        .strip_prefix("data:")
                        .and_then(|d| d.split_once(";base64,"))
                    {
                        (header, payload)
                    } else {
                        (
                            o.get("mimeType")
                                .and_then(Value::as_str)
                                .unwrap_or("application/octet-stream"),
                            data,
                        )
                    };
                    if payload.starts_with("blob:sha256:") || mime.starts_with("image/") {
                        let id = payload
                            .strip_prefix("blob:sha256:")
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("inline:{}", out.len()));
                        out.push((
                            Image {
                                id,
                                mime: mime.into(),
                                label: o
                                    .get("label")
                                    .and_then(Value::as_str)
                                    .unwrap_or("图片")
                                    .into(),
                            },
                            payload.into(),
                        ));
                        return;
                    }
                }
                for value in o.values() {
                    walk(value, out);
                }
            }
            Value::Array(a) => {
                for value in a {
                    walk(value, out);
                }
            }
            _ => {}
        }
    }
    let mut out = vec![];
    walk(v, &mut out);
    out
}
fn strip_image_data(v: &mut Value) {
    match v {
        Value::Object(o) => {
            if o.get("type").is_some_and(|t| t == "image") || o.contains_key("mimeType") {
                if let Some(Value::String(data)) = o.get_mut("data") {
                    if !data.starts_with("blob:") {
                        *data = "[图片通过资源接口读取]".into();
                    }
                }
            }
            for value in o.values_mut() {
                strip_image_data(value);
            }
        }
        Value::Array(a) => {
            for value in a {
                strip_image_data(value);
            }
        }
        _ => {}
    }
}
pub(crate) fn make_record(
    id: String,
    kind: &str,
    name: String,
    status: &str,
    v: Value,
    parent: Option<String>,
    warnings: &mut Vec<String>,
) -> Record {
    let resources = image_payloads(&v);
    let mut v = v;
    strip_image_data(&mut v);
    let (content, truncated) = bounded(v.get("content").cloned().unwrap_or(v.clone()), warnings);
    let o = v.as_object();
    Record {
        id,
        aliases: Vec::new(),
        kind: kind.into(),
        turn: o.and_then(|x| x.get("turn")).and_then(Value::as_u64),
        step: o.and_then(|x| x.get("step")).and_then(Value::as_u64),
        parent_id: parent,
        tool_call_id: o
            .and_then(|x| x.get("toolCallId").or_else(|| x.get("tool_call_id")))
            .and_then(Value::as_str)
            .map(str::to_string),
        name,
        status: status.into(),
        content,
        input: o
            .and_then(|x| {
                x.get("arguments")
                    .or_else(|| x.get("args"))
                    .or_else(|| x.get("input"))
            })
            .cloned(),
        output: o
            .and_then(|x| x.get("result").or_else(|| x.get("output")))
            .cloned(),
        model: o
            .and_then(|x| x.get("model"))
            .and_then(Value::as_str)
            .map(str::to_string),
        started_at: o.and_then(|x| stamp(x.get("startedAt").or_else(|| x.get("timestamp")))),
        completed_at: o.and_then(|x| stamp(x.get("completedAt"))),
        duration_ms: o.and_then(|x| num(x.get("durationMs").or_else(|| x.get("duration")))),
        ttft_ms: o.and_then(|x| num(x.get("ttftMs").or_else(|| x.get("ttft")))),
        timing_source: Some("omp".into()),
        usage: o.and_then(|x| usage(x.get("usage"))),
        error: o.and_then(|x| x.get("error")).map(text),
        images: resources.iter().map(|(i, _)| i.clone()).collect(),
        resources,
        truncated,
    }
}

fn validate_header(
    lines: &mut impl Iterator<Item = std::io::Result<String>>,
    expected: &str,
) -> Result<()> {
    let first = lines
        .next()
        .ok_or_else(|| err("session_invalid", "Missing session header"))?
        .map_err(|e| err("session_invalid", e.to_string()))?;
    let a: Value =
        serde_json::from_str(&first).map_err(|e| err("session_invalid", e.to_string()))?;
    let h = if a["type"] == "title" {
        lines
            .next()
            .ok_or_else(|| err("session_invalid", "Missing session header"))?
            .map_err(|e| err("session_invalid", e.to_string()))?
            .pipe(|s| serde_json::from_str(&s).map_err(|e| err("session_invalid", e.to_string())))?
    } else {
        a
    };
    if h["type"] != "session" || h["id"].as_str() != Some(expected) || h["cwd"].as_str().is_none() {
        return Err(err(
            "session_mismatch",
            "Session identity does not match expected session",
        ));
    }
    if h.get("version")
        .is_some_and(|v| !matches!(v.as_u64(), Some(1..=3)))
    {
        return Err(err("session_incompatible", "Unsupported session format"));
    }
    Ok(())
}

pub fn read(file: &Path, expected_session: &str) -> Result<History> {
    let meta = fs::symlink_metadata(file).map_err(|e| err("session_missing", e.to_string()))?;
    if !meta.file_type().is_file() {
        return Err(err("session_invalid", "Session must be a regular file"));
    }
    if meta.len() > MAX_FILE {
        return Err(err(
            "session_invalid",
            "Session exceeds 64 MiB trajectory limit",
        ));
    }
    let rev = format!(
        "{}:{}",
        meta.len(),
        meta.modified()
            .ok()
            .and_then(|x| x.duration_since(UNIX_EPOCH).ok())
            .map(|x| x.as_nanos())
            .unwrap_or(0)
    );
    let mut raw = String::new();
    fs::File::open(file)
        .map_err(|e| err("session_missing", e.to_string()))?
        .take(MAX_FILE + 1)
        .read_to_string(&mut raw)
        .map_err(|e| err("session_invalid", e.to_string()))?;
    if raw.len() as u64 > MAX_FILE {
        return Err(err(
            "session_invalid",
            "Session exceeds 64 MiB trajectory limit",
        ));
    }
    if raw.lines().any(|line| line.len() > MAX_LINE) {
        return Err(err("session_invalid", "Session line exceeds 16 MiB"));
    }
    let mut lines = raw
        .lines()
        .map(|s| Ok::<String, std::io::Error>(s.to_string()));
    validate_header(&mut lines, expected_session)?;
    let mut records: Vec<Record> = Vec::new();
    let mut warnings = Vec::new();
    let mut tools: HashMap<String, usize> = HashMap::new();
    let mut branches: HashMap<String, (u64, u64)> = HashMap::new();
    let mut turn = 0;
    let mut step = 0;
    let all: Vec<_> = lines.collect();
    let mut lines = all.into_iter().peekable();
    let mut entries = vec![];
    while let Some(line) = lines.next() {
        let line = line.map_err(|e| err("session_invalid", e.to_string()))?;
        if line.len() > MAX_LINE {
            return Err(err("session_invalid", "Session line exceeds 16 MiB"));
        }
        match serde_json::from_str::<Value>(&line) {
            Ok(v) => entries.push(v),
            Err(_) if lines.peek().is_none() && !raw.ends_with('\n') => {
                warnings.push("忽略尚未写完的会话尾行".into())
            }
            Err(_) => return Err(err("session_invalid", "Malformed completed JSONL entry")),
        }
    }
    let by_id: HashMap<_, _> = entries
        .iter()
        .filter_map(|v| v["id"].as_str().map(|id| (id, v)))
        .collect();
    let mut ancestry = std::collections::HashSet::new();
    let mut node = entries.iter().rev().find(|v| v["id"].is_string());
    while let Some(v) = node {
        let Some(id) = v["id"].as_str() else {
            break;
        };
        if !ancestry.insert(id.to_owned()) {
            return Err(err("session_invalid", "Cyclic session ancestry"));
        }
        node = v["parentId"].as_str().and_then(|id| by_id.get(id).copied());
    }
    // Legacy fixtures without parentId retain source order; real OMP trees select the active leaf.
    let tree = entries.iter().any(|v| v.get("parentId").is_some());
    for v in &entries {
        if tree && v["id"].as_str().is_some_and(|id| !ancestry.contains(id)) {
            continue;
        }
        let typ = v["type"].as_str().unwrap_or("");
        let id = v["id"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("entry:{}", records.len()));
        let parent = v["parentId"].as_str();
        if let Some((t, s)) = parent.and_then(|p| branches.get(p)) {
            turn = *t;
            step = *s;
        }
        let m = if typ == "message" { &v["message"] } else { &v };
        let role = m["role"].as_str().unwrap_or(typ);
        if role == "toolResult" || typ == "toolResult" {
            let cid = m["toolCallId"].as_str().unwrap_or("");
            let idx = if let Some(idx) = tools.get(cid) {
                *idx
            } else {
                let mut r = make_record(
                    format!("tool:{cid}"),
                    "tool",
                    m["toolName"].as_str().unwrap_or("tool").into(),
                    "unknown",
                    json!({}),
                    None,
                    &mut warnings,
                );
                r.tool_call_id = Some(cid.into());
                r.turn = (turn > 0).then_some(turn);
                r.step = (step > 0).then_some(step);
                records.push(r);
                tools.insert(cid.into(), records.len() - 1);
                records.len() - 1
            };
            let r = &mut records[idx];
            r.aliases.push(id.clone());
            let mut output = m
                .get("content")
                .or_else(|| m.get("result"))
                .cloned()
                .unwrap_or(Value::Null);
            for (mut image, data) in image_payloads(&output) {
                if image.id.starts_with("inline:") {
                    image.id = format!("inline:{}", r.resources.len());
                }
                r.images.push(image.clone());
                r.resources.push((image, data));
            }
            strip_image_data(&mut output);
            let (output, truncated) = bounded(output, &mut warnings);
            r.output = Some(output);
            r.truncated |= truncated;
            r.status = if m["isError"] == true {
                "failed"
            } else {
                "succeeded"
            }
            .into();
            r.completed_at = stamp(m.get("timestamp").or_else(|| v.get("timestamp")));
            r.duration_ms = num(m.get("durationMs").or_else(|| m.get("duration")));
            if let (Some(end), Some(d)) = (r.completed_at, r.duration_ms) {
                r.started_at = Some(end - d);
            }
            let owner = r.clone();
            if let Some(results) = m["details"]["results"].as_array() {
                for result in results {
                    let Some(child_id) = result["id"].as_str() else {
                        continue;
                    };
                    let mut child = make_record(
                        format!("subagent:{child_id}"),
                        "tool",
                        result["agent"].as_str().unwrap_or("subagent").into(),
                        if result["aborted"] == true {
                            "cancelled"
                        } else if result["exitCode"].as_i64().is_some_and(|v| v != 0) {
                            "failed"
                        } else {
                            "succeeded"
                        },
                        json!({"content":result["description"],"input":result["task"],"output":result["output"],"durationMs":result["durationMs"],"usage":result["usage"],"model":result["resolvedModel"]}),
                        Some(owner.id.clone()),
                        &mut warnings,
                    );
                    child.turn = owner.turn;
                    child.step = owner.step;
                    child.error = result["error"].as_str().map(str::to_owned);
                    records.push(child);
                }
            }
        } else if typ == "message" {
            if role == "user" {
                turn += 1;
                step = 0;
            }
            if role == "assistant" {
                step += 1;
            }
            let kind = match role {
                "user" => "user",
                "assistant" => "assistant",
                _ => "context",
            };
            let mut r = make_record(
                id.clone(),
                kind,
                role.into(),
                match m["stopReason"].as_str() {
                    Some("error") => "failed",
                    Some("aborted") => "cancelled",
                    _ => "succeeded",
                },
                m.clone(),
                None,
                &mut warnings,
            );
            r.turn = (turn > 0).then_some(turn);
            r.step = (step > 0).then_some(step);
            r.started_at = stamp(m.get("timestamp")).or_else(|| stamp(v.get("timestamp")));
            if let Some(time) = stamp(m.get("timestamp")) {
                r.aliases.push(format!("message:{role}:{time}"));
            }
            r.error = m
                .get("errorMessage")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if r.completed_at.is_none() {
                if let (Some(start), Some(d)) = (r.started_at, r.duration_ms) {
                    r.completed_at = Some(start + d);
                }
            }
            records.push(r);
            if role == "assistant" {
                if let Some(arr) = m["content"].as_array() {
                    for c in arr {
                        if c["type"] != "toolCall" && c["type"] != "tool_use" {
                            continue;
                        }
                        let Some(cid) = c["id"].as_str() else {
                            continue;
                        };
                        let mut tr = make_record(
                            format!("tool:{cid}"),
                            "tool",
                            c["name"].as_str().unwrap_or("tool").into(),
                            "unknown",
                            c.clone(),
                            Some(id.clone()),
                            &mut warnings,
                        );
                        tr.tool_call_id = Some(cid.into());
                        tr.turn = (turn > 0).then_some(turn);
                        tr.step = Some(step);
                        tools.insert(cid.into(), records.len());
                        records.push(tr);
                    }
                }
            }
        } else if matches!(
            typ,
            "compaction"
                | "branch_summary"
                | "custom_message"
                | "custom"
                | "model_change"
                | "thinking_level_change"
                | "context"
        ) {
            let mut r = make_record(
                id.clone(),
                if typ == "compaction" {
                    "compaction"
                } else {
                    "context"
                },
                typ.into(),
                "succeeded",
                v.clone(),
                None,
                &mut warnings,
            );
            r.content = v
                .get("summary")
                .or_else(|| v.get("content"))
                .cloned()
                .unwrap_or(v.clone());
            records.push(r);
        }
        branches.insert(id, (turn, step));
    }
    // Bound each separately exposed field as well as content.
    for r in &mut records {
        for v in [&mut r.input, &mut r.output].into_iter().flatten() {
            let (value, truncated) = bounded(v.take(), &mut warnings);
            *v = value;
            r.truncated |= truncated;
        }
    }
    Ok(History {
        records,
        revision: rev,
        warnings,
    })
}

pub fn image(
    file: &Path,
    expected_session: &str,
    record_id: &str,
    image_id: &str,
) -> Result<Value> {
    let h = read(file, expected_session)?;
    let record = h
        .records
        .iter()
        .find(|r| r.id == record_id)
        .ok_or_else(|| err("session_invalid", "Trajectory record not found"))?;
    let resources = &record.resources;
    let (descriptor, payload) = resources
        .iter()
        .find(|(i, _)| i.id == image_id)
        .ok_or_else(|| err("session_invalid", "Image reference is not owned by record"))?;
    let mime = descriptor.mime.as_str();
    if !matches!(
        mime,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    ) {
        return Err(err("session_invalid", "Unsupported image type"));
    }
    if image_id.starts_with("inline:") {
        if payload.len() as u64 > MAX_IMAGE * 4 / 3 + 4 {
            return Err(err("session_invalid", "Image exceeds 8 MiB"));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .map_err(|_| err("session_invalid", "Invalid inline image"))?;
        if bytes.len() as u64 > MAX_IMAGE {
            return Err(err("session_invalid", "Image exceeds 8 MiB"));
        }
        return Ok(json!({"mime":mime,"data":payload}));
    }
    if image_id.len() != 64 || !image_id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(err("session_invalid", "Invalid blob identity"));
    }
    let mut dir = file.parent().map(PathBuf::from);
    let mut found = None;
    while let Some(d) = dir {
        let p = d.join("blobs").join(image_id);
        if p.is_file() {
            found = Some(p);
            break;
        }
        dir = d.parent().map(PathBuf::from);
    }
    if found.is_none() {
        let root = std::env::var_os("PI_CODING_AGENT_DIR")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".omp/agent")));
        if let Some(root) = root {
            let candidate = root.join("blobs").join(image_id);
            if candidate.exists() {
                found = Some(candidate);
            }
        }
    }
    let p = found.ok_or_else(|| err("session_missing", "Referenced image blob is missing"))?;
    if !fs::symlink_metadata(p.parent().unwrap())
        .map_err(|e| err("session_missing", e.to_string()))?
        .file_type()
        .is_dir()
    {
        return Err(err(
            "session_invalid",
            "Blob directory must not be a symlink",
        ));
    }
    let m = fs::symlink_metadata(&p).map_err(|e| err("session_missing", e.to_string()))?;
    if !m.file_type().is_file() || m.len() > MAX_IMAGE {
        return Err(err("session_invalid", "Image exceeds 8 MiB"));
    }
    let mut data = Vec::new();
    fs::File::open(p)
        .map_err(|e| err("session_missing", e.to_string()))?
        .take(MAX_IMAGE + 1)
        .read_to_end(&mut data)
        .map_err(|e| err("session_missing", e.to_string()))?;
    if data.len() as u64 > MAX_IMAGE {
        return Err(err("session_invalid", "Image exceeds 8 MiB"));
    }
    Ok(json!({"mime":mime,"data":base64::engine::general_purpose::STANDARD.encode(data)}))
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub records: Vec<Record>,
    pub next_cursor: Option<String>,
    pub after_cursor: Option<String>,
    pub total_records: usize,
    pub revision: String,
    pub warnings: Vec<String>,
}
/// Cursors reference immutable entry identities, so appending does not invalidate older pages.
pub fn page(history: History, cursor: Option<&str>, after: bool) -> Result<Page> {
    let total = history.records.len();
    let position = cursor
        .map(|id| {
            history
                .records
                .iter()
                .position(|r| r.id == id || r.aliases.iter().any(|a| a == id))
                .ok_or_else(|| err("stale_cursor", "轨迹游标已失效，请重新加载"))
        })
        .transpose()?;
    let (start, end) = match (position, after) {
        (Some(i), true) => (i, (i + 50).min(total)),
        (Some(i), false) => (i.saturating_sub(50), i),
        _ => (total.saturating_sub(50), total),
    };
    let mut records = history.records[start..end].to_vec();
    for r in &mut records {
        strip_image_data(&mut r.content);
        if let Some(v) = &mut r.input {
            strip_image_data(v);
        }
        if let Some(v) = &mut r.output {
            strip_image_data(v);
        }
    }
    Ok(Page {
        next_cursor: if start > 0 {
            records.first().map(|r| r.id.clone())
        } else {
            None
        },
        after_cursor: records.last().map(|r| r.id.clone()),
        records,
        total_records: total,
        revision: history.revision,
        warnings: history.warnings,
    })
}
