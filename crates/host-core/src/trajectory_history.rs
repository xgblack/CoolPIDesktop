//! Read-only, bounded projection of an OMP session JSONL file.
use crate::runtime::{HostError, Result};
use crate::task::UsageSummary;
#[derive(Debug, Clone, serde::Serialize)]
pub struct Image {
    pub id: String,
    pub mime: String,
    pub label: String,
}
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all="camelCase")]
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
    (Value::String(s.chars().take(MAX_VALUE).collect()), true)
}
fn num(v: Option<&Value>) -> Option<f64> {
    v.and_then(|x| x.as_f64().or_else(|| x.as_u64().map(|n| n as f64)))
}
fn stamp(v: Option<&Value>) -> Option<f64> {
    num(v).map(|n| if n > 10_000_000_000.0 { n } else { n * 1000.0 })
}
fn usage(v: Option<&Value>) -> Option<UsageSummary> {
    v.cloned().and_then(|x| serde_json::from_value(x).ok())
}
fn image_refs(v: &Value) -> Vec<Image> {
    let mut out = Vec::new();
    fn walk(v: &Value, out: &mut Vec<Image>, n: &mut usize) {
        if *n >= 64 {
            return;
        }
        match v {
            Value::Object(o) => {
                if let Some(data) = o.get("data").and_then(Value::as_str) {
                    if data.starts_with("blob:sha256:") {
                        let mime = o
                            .get("mimeType")
                            .and_then(Value::as_str)
                            .unwrap_or("application/octet-stream");
                        out.push(Image {
                            id: data[12..].to_string(),
                            mime: mime.to_string(),
                            label: o
                                .get("label")
                                .and_then(Value::as_str)
                                .unwrap_or("image")
                                .to_string(),
                        });
                        *n += 1;
                    }
                }
                for x in o.values() {
                    walk(x, out, n);
                }
            }
            Value::Array(a) => {
                for x in a {
                    walk(x, out, n);
                }
            }
            _ => {}
        }
    }
    walk(v, &mut out, &mut 0);
    out
}
fn make_record(
    id: String,
    kind: &str,
    name: String,
    status: &str,
    v: Value,
    parent: Option<String>,
    warnings: &mut Vec<String>,
) -> Record {
    let (content, truncated) = bounded(v.clone(), warnings);
    let o = v.as_object();
    Record {
        id,
        aliases: Vec::new(),
        kind: kind.into(),
        turn: o.and_then(|x| x.get("turn")).and_then(Value::as_u64),
        step: o.and_then(|x| x.get("step")).and_then(Value::as_u64),
        parent_id: parent,
        tool_call_id: o
            .and_then(|x| x.get("toolCallId").or_else(|| x.get("tool_call_id")).or_else(|| x.get("id")))
            .and_then(Value::as_str)
            .map(str::to_string),
        name,
        status: status.into(),
        content,
        input: o
            .and_then(|x| x.get("args").or_else(|| x.get("input")))
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
        duration_ms: o.and_then(|x| num(x.get("durationMs"))),
        ttft_ms: o.and_then(|x| num(x.get("ttftMs").or_else(|| x.get("ttft")))),
        timing_source: Some("omp".into()),
        usage: o.and_then(|x| usage(x.get("usage"))),
        error: o.and_then(|x| x.get("error")).map(text),
        images: image_refs(&v),
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
    let mut lines = raw
        .lines()
        .map(|s| Ok::<String, std::io::Error>(s.to_string()));
    validate_header(&mut lines, expected_session)?;
    let mut records = Vec::new();
    let mut warnings = Vec::new();
    let mut tools: HashMap<String, usize> = HashMap::new();
    let all: Vec<_> = lines.collect();
    let mut lines = all.into_iter().peekable();
    while let Some(line) = lines.next() {
        let line = match line {
            Ok(x) => x,
            Err(e) => return Err(err("session_invalid", e.to_string())),
        };
        if line.len() > MAX_LINE {
            return Err(err("session_invalid", "Session line exceeds 16 MiB"));
        }
        let v: Value = match serde_json::from_str(&line) {
            Ok(x) => x,
            Err(_) if lines.peek().is_none() && !raw.ends_with('\n') => {
                warnings.push("ignored incomplete trailing JSON line".into());
                continue;
            }
            Err(_) => return Err(err("session_invalid", "Malformed completed JSONL entry")),
        };
        let typ = v["type"].as_str().unwrap_or("");
        let id = v["id"].as_str().unwrap_or("").to_string();
        if typ == "message" {
            let m = v.get("message").cloned().unwrap_or(v.clone());
            let role = m["role"].as_str().unwrap_or("context");
            let kind = match role {
                "user" => "user",
                "assistant" => "assistant",
                _ => "context",
            };
            let mut r = make_record(
                if id.is_empty() {
                    format!("message:{}", records.len())
                } else {
                    id
                },
                kind,
                role.into(),
                "succeeded",
                m.clone(),
                v["parentId"].as_str().map(str::to_string),
                &mut warnings,
            );
            r.started_at = stamp(v.get("timestamp").or_else(|| m.get("timestamp")));
            r.images = image_refs(&m);
            records.push(r);
            if role == "assistant" {
                if let Some(arr) = m.get("content").and_then(Value::as_array) {
                    for c in arr {
                        if c["type"] == "toolCall" || c["type"] == "tool_use" {
                            if let Some(cid) = c
                                .get("id")
                                .or_else(|| c.get("toolCallId"))
                                .and_then(Value::as_str)
                            {
                                let tr = make_record(
                                    format!("tool:{}", cid),
                                    "tool",
                                    c.get("name")
                                        .and_then(Value::as_str)
                                        .unwrap_or("tool")
                                        .into(),
                                    "running",
                                    c.clone(),
                                    Some(records.last().unwrap().id.clone()),
                                    &mut warnings,
                                );
                                tools.insert(cid.into(), records.len());
                                records.push(tr);
                            }
                        }
                    }
                }
            }
        } else if typ.contains("tool") {
            let cid = v.get("toolCallId").and_then(Value::as_str).unwrap_or("");
            if let Some(&idx) = tools.get(cid) {
                let r = &mut records[idx];
                if v.get("result").is_some() {
                    r.output = v.get("result").cloned();
                    r.status = if v["isError"].as_bool().unwrap_or(false) {
                        "failed".into()
                    } else {
                        "succeeded".into()
                    };
                    r.completed_at = stamp(v.get("timestamp").or_else(|| v.get("completedAt")));
                } else {
                    r.input = v.get("args").cloned();
                    r.started_at = stamp(v.get("timestamp"));
                }
            } else {
                records.push(make_record(
                    format!("tool:{}", if cid.is_empty() { id.as_str() } else { cid }),
                    "tool",
                    v.get("toolName")
                        .and_then(Value::as_str)
                        .unwrap_or("tool")
                        .into(),
                    "unknown",
                    v,
                    None,
                    &mut warnings,
                ));
            }
        } else if typ.contains("compaction") || typ == "context" {
            records.push(make_record(
                if id.is_empty() {
                    format!("context:{}", records.len())
                } else {
                    id
                },
                "compaction",
                typ.into(),
                "succeeded",
                v,
                None,
                &mut warnings,
            ));
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
    if !record.images.iter().any(|i| i.id == image_id)
        || image_id.len() != 64
        || !image_id.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(err(
            "session_invalid",
            "Image reference is not owned by record",
        ));
    }
    let mime = record
        .images
        .iter()
        .find(|i| i.id == image_id)
        .map(|i| i.mime.as_str())
        .unwrap_or("");
    if !matches!(
        mime,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    ) {
        return Err(err("session_invalid", "Unsupported image type"));
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
    let p = found.ok_or_else(|| err("session_missing", "Referenced image blob is missing"))?;
    let m = fs::metadata(&p).map_err(|e| err("session_missing", e.to_string()))?;
    if m.len() > MAX_IMAGE {
        return Err(err("session_invalid", "Image exceeds 8 MiB"));
    }
    let data = fs::read(p).map_err(|e| err("session_missing", e.to_string()))?;
    Ok(json!({"mime":mime,"data":base64::engine::general_purpose::STANDARD.encode(data)}))
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}
