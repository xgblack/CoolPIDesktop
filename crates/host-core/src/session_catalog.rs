//! Rebuildable, read-only index over fixed OMP session roots.
use crate::{HostError, Workbench, runtime::Result};
use rusqlite::{OptionalExtension, params};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;
const MAX_SEARCH_TEXT_BYTES: usize = 256 * 1024;
const MAX_FILES: usize = 20_000;
const MAX_DEPTH: usize = 8;
const MAX_RESULTS: usize = 100;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub session_key: String,
    pub session_id: String,
    pub cwd: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub model_provider: Option<String>,
    pub model_id: Option<String>,
    pub parent_session: Option<String>,
    pub relation_kind: String,
    pub status: String,
    pub message_count: u64,
    pub total_tokens: Option<u64>,
    pub cost: Option<f64>,
    pub trusted: bool,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub task_title: Option<String>,
    pub running: bool,
    pub unread: bool,
    pub archived: bool,
    pub parse_state: String,
    pub parse_error_code: Option<String>,
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogPage {
    pub entries: Vec<CatalogEntry>,
    pub total: u64,
    pub next_offset: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogRefresh {
    pub discovered: u64,
    pub indexed: u64,
    pub unchanged: u64,
    pub invalid: u64,
}

#[derive(Debug)]
struct SearchDoc {
    entry_id: String,
    role: String,
    content: String,
}

#[derive(Debug)]
struct ParsedSession {
    session_key: String,
    session_id: String,
    session_file: String,
    source_root: String,
    cwd: String,
    title: String,
    created_at: i64,
    updated_at: i64,
    model_provider: Option<String>,
    model_id: Option<String>,
    parent_session: Option<String>,
    relation_kind: String,
    status: String,
    message_count: u64,
    total_tokens: Option<u64>,
    cost: Option<f64>,
    trusted: bool,
    project_id: Option<String>,
    file_size: u64,
    file_mtime: i64,
    parse_state: String,
    parse_error_code: Option<String>,
    docs: Vec<SearchDoc>,
}

#[derive(Clone)]
struct TrustedRoot {
    path: PathBuf,
    project_id: String,
}

fn error(code: &str, message: impl Into<String>) -> HostError {
    HostError::new(code, message.into())
}

fn epoch(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .map(|value| {
            if value > 10_000_000_000 {
                value / 1000
            } else {
                value
            }
        })
        .or_else(|| {
            value.as_f64().map(|value| {
                if value > 10_000_000_000.0 {
                    (value / 1000.0) as i64
                } else {
                    value as i64
                }
            })
        })
        .or_else(|| {
            value
                .as_str()
                .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                .map(|value| value.timestamp())
        })
}

fn content_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Array(values) => values
            .iter()
            .filter_map(|value| {
                if value["type"] == "text" || value["type"] == "thinking" {
                    value["text"]
                        .as_str()
                        .or_else(|| value["thinking"].as_str())
                        .map(str::to_owned)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Object(values) => values
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        _ => String::new(),
    }
}

fn clean_title(value: &str) -> String {
    value
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .filter(|value| !value.is_control())
        .take(512)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn split_model(value: &str) -> (Option<String>, Option<String>) {
    match value.split_once('/') {
        Some((provider, id)) if !provider.is_empty() && !id.is_empty() => {
            (Some(provider.to_owned()), Some(id.to_owned()))
        }
        _ if !value.is_empty() => (None, Some(value.to_owned())),
        _ => (None, None),
    }
}

fn add_usage(value: &Value, total_tokens: &mut u64, cost: &mut f64, seen: &mut bool) {
    let usage = &value["usage"];
    if !usage.is_object() {
        return;
    }
    if let Some(value) = usage["totalTokens"].as_u64().or_else(|| {
        let total = usage["input"].as_u64().unwrap_or(0)
            + usage["output"].as_u64().unwrap_or(0)
            + usage["cacheRead"].as_u64().unwrap_or(0)
            + usage["cacheWrite"].as_u64().unwrap_or(0);
        (total > 0).then_some(total)
    }) {
        *total_tokens = total_tokens.saturating_add(value);
        *seen = true;
    }
    if let Some(value) = usage["cost"]["total"]
        .as_f64()
        .or_else(|| usage["cost"].as_f64())
    {
        *cost += value;
        *seen = true;
    }
}

fn session_key(path: &Path) -> String {
    uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_URL,
        path.to_string_lossy().as_bytes(),
    )
    .to_string()
}

fn parse_session(
    file: &Path,
    source_root: &Path,
    trusted_roots: &[TrustedRoot],
) -> Result<ParsedSession> {
    let metadata = fs::symlink_metadata(file)
        .map_err(|value| error("session_catalog_io", value.to_string()))?;
    if !metadata.file_type().is_file() {
        return Err(error(
            "session_catalog_invalid_file",
            "Session must be a regular file",
        ));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(error(
            "session_catalog_file_too_large",
            "Session exceeds the 64 MiB catalog limit",
        ));
    }
    let file = file
        .canonicalize()
        .map_err(|value| error("session_catalog_io", value.to_string()))?;
    let source_root = source_root
        .canonicalize()
        .map_err(|value| error("session_catalog_root", value.to_string()))?;
    if !file.starts_with(&source_root) {
        return Err(error(
            "session_catalog_root",
            "Session escaped its fixed root",
        ));
    }
    let file_mtime = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .and_then(|value| i64::try_from(value.as_nanos()).ok())
        .unwrap_or_default();
    let mut values = Vec::new();
    let mut parse_state = "ready".to_owned();
    let mut parse_error_code = None;
    let reader = BufReader::new(
        fs::File::open(&file).map_err(|value| error("session_catalog_io", value.to_string()))?,
    );
    for line in reader.lines() {
        let line = line.map_err(|value| error("session_catalog_io", value.to_string()))?;
        if line.len() > MAX_LINE_BYTES {
            parse_state = "partial".into();
            parse_error_code = Some("session_line_too_large".into());
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(&line) {
            Ok(value) => values.push(value),
            Err(_) => {
                parse_state = "partial".into();
                parse_error_code = Some("session_json_invalid".into());
            }
        }
    }
    let title_slot = values
        .first()
        .filter(|value| value["type"] == "title")
        .and_then(|value| value["title"].as_str())
        .map(clean_title)
        .filter(|value| !value.is_empty());
    let header = values
        .iter()
        .find(|value| value["type"] == "session")
        .ok_or_else(|| {
            error(
                "session_catalog_header_invalid",
                "Session header is missing",
            )
        })?;
    let session_id = header["id"]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| error("session_catalog_header_invalid", "Session id is missing"))?
        .to_owned();
    let cwd = header["cwd"].as_str().unwrap_or_default().to_owned();
    let resolved_cwd = Path::new(&cwd).canonicalize().ok();
    let matched = resolved_cwd.as_ref().and_then(|cwd| {
        trusted_roots
            .iter()
            .filter(|root| cwd.starts_with(&root.path))
            .max_by_key(|root| root.path.components().count())
    });
    let trusted = matched.is_some();
    let project_id = matched.map(|value| value.project_id.clone());
    let mut first_user = None;
    let mut last_title = None;
    let mut docs = Vec::new();
    let mut model_provider = None;
    let mut model_id = None;
    let mut message_count = 0_u64;
    let mut updated_at = epoch(header.get("timestamp")).unwrap_or_default();
    let created_at = updated_at;
    let mut total_tokens = 0_u64;
    let mut cost = 0_f64;
    let mut usage_seen = false;
    let mut last_message: Option<&Value> = None;
    for (index, value) in values.iter().enumerate() {
        updated_at = updated_at.max(epoch(value.get("timestamp")).unwrap_or_default());
        match value["type"].as_str() {
            Some("title_change") => {
                last_title = value["title"]
                    .as_str()
                    .map(clean_title)
                    .filter(|value| !value.is_empty());
            }
            Some("model_change") => {
                if let Some(model) = value["model"].as_str() {
                    (model_provider, model_id) = split_model(model);
                }
            }
            Some("model_usage") => add_usage(value, &mut total_tokens, &mut cost, &mut usage_seen),
            Some("message") => {
                message_count += 1;
                let message = &value["message"];
                let role = message["role"].as_str().unwrap_or("unknown");
                let content = content_text(&message["content"]);
                if first_user.is_none() && role == "user" && !content.trim().is_empty() {
                    first_user = Some(clean_title(&content));
                }
                if role == "assistant" {
                    if let (Some(provider), Some(model)) =
                        (message["provider"].as_str(), message["model"].as_str())
                    {
                        model_provider = Some(provider.to_owned());
                        model_id = Some(model.to_owned());
                    }
                    add_usage(message, &mut total_tokens, &mut cost, &mut usage_seen);
                }
                if trusted && matches!(role, "user" | "assistant") && !content.trim().is_empty() {
                    let content = if content.len() > MAX_SEARCH_TEXT_BYTES {
                        content[..content.floor_char_boundary(MAX_SEARCH_TEXT_BYTES)].to_owned()
                    } else {
                        content
                    };
                    docs.push(SearchDoc {
                        entry_id: value["id"]
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("entry-{index}")),
                        role: role.to_owned(),
                        content,
                    });
                }
                last_message = Some(message);
            }
            _ => {}
        }
    }
    let status = match last_message {
        Some(message) if message["role"] == "user" => "pending",
        Some(message) if message["role"] == "toolResult" => "interrupted",
        Some(message) if message["role"] == "assistant" && message["stopReason"] == "error" => {
            "error"
        }
        Some(message) if message["role"] == "assistant" && message["stopReason"] == "aborted" => {
            "aborted"
        }
        Some(message) if message["role"] == "assistant" && message["stopReason"] == "length" => {
            "interrupted"
        }
        Some(message) if message["role"] == "assistant" => "complete",
        Some(_) => "unknown",
        None => "unknown",
    };
    let nested_parent = file.parent().and_then(|parent| {
        let candidate = PathBuf::from(format!("{}.jsonl", parent.to_string_lossy()));
        candidate
            .is_file()
            .then(|| candidate.to_string_lossy().into_owned())
    });
    let header_parent = header["parentSession"].as_str().map(str::to_owned);
    let (parent_session, relation_kind) = if let Some(parent) = nested_parent {
        (Some(parent), "subagent".to_owned())
    } else if let Some(parent) = header_parent {
        (Some(parent), "fork".to_owned())
    } else {
        (None, "root".to_owned())
    };
    let title = title_slot
        .or(last_title)
        .or_else(|| {
            header["title"]
                .as_str()
                .map(clean_title)
                .filter(|value| !value.is_empty())
        })
        .or(first_user)
        .unwrap_or_else(|| "未命名会话".into());
    docs.push(SearchDoc {
        entry_id: "__metadata__".into(),
        role: "metadata".into(),
        content: format!("{title}\n{cwd}"),
    });
    Ok(ParsedSession {
        session_key: session_key(&file),
        session_id,
        session_file: file.to_string_lossy().into_owned(),
        source_root: source_root.to_string_lossy().into_owned(),
        cwd,
        title,
        created_at,
        updated_at: updated_at.max(
            metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .and_then(|value| i64::try_from(value.as_secs()).ok())
                .unwrap_or_default(),
        ),
        model_provider,
        model_id,
        parent_session,
        relation_kind,
        status: status.into(),
        message_count,
        total_tokens: usage_seen.then_some(total_tokens),
        cost: usage_seen.then_some(cost),
        trusted,
        project_id,
        file_size: metadata.len(),
        file_mtime,
        parse_state,
        parse_error_code,
        docs,
    })
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>> {
    fn walk(path: &Path, depth: usize, files: &mut Vec<PathBuf>) -> Result<()> {
        if depth > MAX_DEPTH || files.len() >= MAX_FILES {
            return Ok(());
        }
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(value) if value.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(value) => return Err(error("session_catalog_io", value.to_string())),
        };
        for entry in entries {
            let entry = entry.map_err(|value| error("session_catalog_io", value.to_string()))?;
            let metadata = entry
                .file_type()
                .map_err(|value| error("session_catalog_io", value.to_string()))?;
            if metadata.is_symlink() {
                continue;
            }
            let path = entry.path();
            if metadata.is_dir() {
                walk(&path, depth + 1, files)?;
            } else if metadata.is_file() && path.extension().is_some_and(|value| value == "jsonl") {
                files.push(path);
                if files.len() >= MAX_FILES {
                    break;
                }
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(root, 0, &mut files)?;
    files.sort();
    Ok(files)
}

fn search_expression(query: &str) -> Option<String> {
    let terms = query
        .split_whitespace()
        .filter_map(|value| {
            let clean = value.replace('"', "\"\"");
            (!clean.is_empty()).then(|| format!("\"{clean}\"*"))
        })
        .take(8)
        .collect::<Vec<_>>();
    (!terms.is_empty()).then(|| terms.join(" AND "))
}

fn display_snippet(content: &str, query: &str) -> String {
    let first = query.split_whitespace().next().unwrap_or_default();
    let position = content.find(first).unwrap_or(0);
    let start = content.floor_char_boundary(position.saturating_sub(80));
    let end = content.ceil_char_boundary((position + first.len() + 120).min(content.len()));
    let mut value = content[start..end].to_owned();
    for term in query.split_whitespace().take(8) {
        value = value.replace(term, &format!("[{term}]"));
    }
    format!(
        "{}{}{}",
        if start > 0 { "..." } else { "" },
        value,
        if end < content.len() { "..." } else { "" }
    )
}

impl Workbench {
    fn session_roots(&self) -> Vec<PathBuf> {
        let mut roots = vec![self.data.join("sessions")];
        let agent = std::env::var_os("PI_CODING_AGENT_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|value| PathBuf::from(value).join(".omp/agent"))
            });
        if let Some(agent) = agent {
            roots.push(agent.join("sessions"));
        }
        roots.sort();
        roots.dedup();
        roots
    }

    async fn trusted_catalog_roots(&self) -> Result<Vec<TrustedRoot>> {
        self.store
            .access(|connection| {
                let mut statement = connection
                    .prepare("SELECT r.path,p.id FROM project_roots r JOIN projects p ON p.id=r.project_id WHERE p.trusted=1 UNION SELECT tr.execution_path,t.project_id FROM task_roots tr JOIN tasks t ON t.id=tr.task_id WHERE t.trusted=1")
                    .map_err(|value| error("database_error", value.to_string()))?;
                let rows = statement
                    .query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })
                    .map_err(|value| error("database_error", value.to_string()))?;
                let mut roots = Vec::new();
                for row in rows {
                    let (path, project_id) =
                        row.map_err(|value| error("database_error", value.to_string()))?;
                    if let Ok(path) = PathBuf::from(path).canonicalize() {
                        roots.push(TrustedRoot { path, project_id });
                    }
                }
                Ok(roots)
            })
            .await
    }

    pub async fn refresh_session_catalog(&self, rebuild: bool) -> Result<CatalogRefresh> {
        self.refresh_session_catalog_from_roots(self.session_roots(), rebuild)
            .await
    }

    async fn refresh_session_catalog_from_roots(
        &self,
        roots: Vec<PathBuf>,
        rebuild: bool,
    ) -> Result<CatalogRefresh> {
        let trusted_roots = self.trusted_catalog_roots().await?;
        let mut discovered = Vec::new();
        let mut normalized_roots = Vec::new();
        for root in roots {
            if !root.exists() {
                continue;
            }
            let root = root
                .canonicalize()
                .map_err(|value| error("session_catalog_root", value.to_string()))?;
            for file in collect_files(&root)? {
                discovered.push((root.clone(), file));
            }
            normalized_roots.push(root);
        }
        let identities = self
            .store
            .access(|connection| {
                let mut statement = connection
                    .prepare("SELECT session_file,file_size,file_mtime FROM session_catalog")
                    .map_err(|value| error("database_error", value.to_string()))?;
                let rows = statement
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            (row.get::<_, u64>(1)?, row.get::<_, i64>(2)?),
                        ))
                    })
                    .map_err(|value| error("database_error", value.to_string()))?;
                rows.collect::<rusqlite::Result<HashMap<_, _>>>()
                    .map_err(|value| error("database_error", value.to_string()))
            })
            .await?;
        let mut parsed = Vec::new();
        let mut seen = HashSet::new();
        let mut unchanged = 0_u64;
        let mut invalid = 0_u64;
        for (root, file) in &discovered {
            let canonical = match file.canonicalize() {
                Ok(value) => value,
                Err(_) => continue,
            };
            let path = canonical.to_string_lossy().into_owned();
            seen.insert(path.clone());
            let metadata = match fs::metadata(&canonical) {
                Ok(value) => value,
                Err(_) => continue,
            };
            let mtime = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .and_then(|value| i64::try_from(value.as_nanos()).ok())
                .unwrap_or_default();
            if !rebuild && identities.get(&path) == Some(&(metadata.len(), mtime)) {
                unchanged += 1;
                continue;
            }
            match parse_session(&canonical, root, &trusted_roots) {
                Ok(value) => {
                    if value.parse_state != "ready" {
                        invalid += 1;
                    }
                    parsed.push(value);
                }
                Err(parse_error) => {
                    invalid += 1;
                    parsed.push(ParsedSession {
                        session_key: session_key(&canonical),
                        session_id: canonical
                            .file_stem()
                            .and_then(|value| value.to_str())
                            .unwrap_or("unknown")
                            .to_owned(),
                        session_file: path,
                        source_root: root.to_string_lossy().into_owned(),
                        cwd: String::new(),
                        title: "无法读取的会话".into(),
                        created_at: 0,
                        updated_at: metadata
                            .modified()
                            .ok()
                            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                            .map(|value| value.as_secs() as i64)
                            .unwrap_or_default(),
                        model_provider: None,
                        model_id: None,
                        parent_session: None,
                        relation_kind: "root".into(),
                        status: "unknown".into(),
                        message_count: 0,
                        total_tokens: None,
                        cost: None,
                        trusted: false,
                        project_id: None,
                        file_size: metadata.len(),
                        file_mtime: mtime,
                        parse_state: "invalid".into(),
                        parse_error_code: Some(parse_error.code),
                        docs: Vec::new(),
                    });
                }
            }
        }
        let discovered_count = discovered.len() as u64;
        let root_strings = normalized_roots
            .iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        self.store
            .access(move |connection| {
                let transaction = connection
                    .transaction()
                    .map_err(|value| error("database_error", value.to_string()))?;
                if rebuild {
                    transaction
                        .execute("DELETE FROM session_search_docs", [])
                        .map_err(|value| error("database_error", value.to_string()))?;
                    transaction
                        .execute("DELETE FROM session_relations", [])
                        .map_err(|value| error("database_error", value.to_string()))?;
                    transaction
                        .execute("DELETE FROM session_catalog", [])
                        .map_err(|value| error("database_error", value.to_string()))?;
                }
                for value in parsed {
                    transaction.execute(
                        "INSERT INTO session_catalog(session_key,session_id,session_file,source_root,cwd,title,created_at,updated_at,model_provider,model_id,parent_session,relation_kind,status,message_count,total_tokens,cost,trusted,project_id,file_size,file_mtime,parse_state,parse_error_code) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22) ON CONFLICT(session_file) DO UPDATE SET session_id=excluded.session_id,cwd=excluded.cwd,title=excluded.title,created_at=excluded.created_at,updated_at=excluded.updated_at,model_provider=excluded.model_provider,model_id=excluded.model_id,parent_session=excluded.parent_session,relation_kind=excluded.relation_kind,status=excluded.status,message_count=excluded.message_count,total_tokens=excluded.total_tokens,cost=excluded.cost,trusted=excluded.trusted,project_id=excluded.project_id,file_size=excluded.file_size,file_mtime=excluded.file_mtime,parse_state=excluded.parse_state,parse_error_code=excluded.parse_error_code",
                        params![value.session_key,value.session_id,value.session_file,value.source_root,value.cwd,value.title,value.created_at,value.updated_at,value.model_provider,value.model_id,value.parent_session,value.relation_kind,value.status,value.message_count,value.total_tokens,value.cost,value.trusted,value.project_id,value.file_size,value.file_mtime,value.parse_state,value.parse_error_code],
                    ).map_err(|error_value| error("database_error", error_value.to_string()))?;
                    transaction
                        .execute("DELETE FROM session_search_docs WHERE session_key=?1", [&value.session_key])
                        .map_err(|error_value| error("database_error", error_value.to_string()))?;
                    for doc in value.docs {
                        transaction.execute(
                            "INSERT INTO session_search_docs(session_key,entry_id,role,content) VALUES(?1,?2,?3,?4)",
                            params![value.session_key,doc.entry_id,doc.role,doc.content],
                        ).map_err(|error_value| error("database_error", error_value.to_string()))?;
                    }
                }
                for root in root_strings {
                    let mut statement = transaction
                        .prepare("SELECT session_file,session_key FROM session_catalog WHERE source_root=?1")
                        .map_err(|value| error("database_error", value.to_string()))?;
                    let existing = statement
                        .query_map([&root], |row| {
                            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                        })
                        .map_err(|value| error("database_error", value.to_string()))?
                        .collect::<rusqlite::Result<Vec<_>>>()
                        .map_err(|value| error("database_error", value.to_string()))?;
                    drop(statement);
                    for (file, key) in existing {
                        if !seen.contains(&file) {
                            transaction
                                .execute("DELETE FROM session_search_docs WHERE session_key=?1", [&key])
                                .map_err(|value| error("database_error", value.to_string()))?;
                            transaction
                                .execute("DELETE FROM session_catalog WHERE session_key=?1", [&key])
                                .map_err(|value| error("database_error", value.to_string()))?;
                        }
                    }
                }
                transaction
                    .execute("DELETE FROM session_relations", [])
                    .map_err(|value| error("database_error", value.to_string()))?;
                let children = {
                    let mut statement = transaction
                        .prepare("SELECT session_key,parent_session,relation_kind,cwd FROM session_catalog WHERE parent_session IS NOT NULL")
                        .map_err(|value| error("database_error", value.to_string()))?;
                    statement
                        .query_map([], |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, String>(2)?,
                                row.get::<_, String>(3)?,
                            ))
                        })
                        .map_err(|value| error("database_error", value.to_string()))?
                        .collect::<rusqlite::Result<Vec<_>>>()
                        .map_err(|value| error("database_error", value.to_string()))?
                };
                for (child, parent, kind, cwd) in children {
                    let parent_key = transaction
                        .query_row(
                            "SELECT session_key FROM session_catalog WHERE session_file=?1 OR session_id=?1 ORDER BY (cwd=?2) DESC,updated_at DESC LIMIT 1",
                            params![parent,cwd],
                            |row| row.get::<_, String>(0),
                        )
                        .optional()
                        .map_err(|value| error("database_error", value.to_string()))?;
                    if let Some(parent_key) = parent_key {
                        transaction.execute(
                            "INSERT OR REPLACE INTO session_relations(parent_key,child_key,relation_kind,discovered_at) VALUES(?1,?2,?3,strftime('%s','now'))",
                            params![parent_key,child,kind],
                        ).map_err(|value| error("database_error", value.to_string()))?;
                    }
                }
                transaction
                    .commit()
                    .map_err(|value| error("database_error", value.to_string()))
            })
            .await?;
        Ok(CatalogRefresh {
            discovered: discovered_count,
            indexed: discovered_count.saturating_sub(unchanged),
            unchanged,
            invalid,
        })
    }

    pub async fn session_catalog(
        &self,
        query: &str,
        include_archived: bool,
        offset: u64,
        limit: u64,
    ) -> Result<CatalogPage> {
        if query.len() > 256 {
            return Err(error(
                "session_search_invalid",
                "Search query exceeds 256 bytes",
            ));
        }
        let raw_query = query.trim().to_owned();
        let expression = search_expression(&raw_query);
        let searching = expression.is_some();
        let limit = usize::try_from(limit)
            .unwrap_or(MAX_RESULTS)
            .clamp(1, MAX_RESULTS);
        let offset = usize::try_from(offset).unwrap_or(usize::MAX);
        self.store
            .access(move |connection| {
                let archived = if include_archived {
                    "1=1"
                } else {
                    "v.archived_at IS NULL"
                };
                let base = "SELECT c.session_key,c.session_id,c.cwd,c.title,c.created_at,c.updated_at,c.model_provider,c.model_id,c.parent_session,c.relation_kind,c.status,c.message_count,c.total_tokens,c.cost,c.trusted,c.project_id,b.task_id,t.title,EXISTS(SELECT 1 FROM task_runs r WHERE r.task_id=b.task_id AND r.ended_at IS NULL),COALESCE(c.updated_at>v.last_seen_at,1),v.archived_at IS NOT NULL,c.parse_state,c.parse_error_code";
                let (sql, count_sql, query_params): (String, String, Vec<rusqlite::types::Value>) =
                    if let Some(expression) = expression {
                        (
                            format!("WITH matches AS (SELECT session_key,min(content) AS matched_content FROM session_search_docs WHERE session_search_docs MATCH ?1 GROUP BY session_key) {base},matches.matched_content FROM matches JOIN session_catalog c ON c.session_key=matches.session_key LEFT JOIN session_views v ON v.session_key=c.session_key LEFT JOIN session_bindings b ON b.session_file=c.session_file LEFT JOIN tasks t ON t.id=b.task_id WHERE {archived} ORDER BY c.updated_at DESC LIMIT ?2 OFFSET ?3"),
                            format!("SELECT COUNT(DISTINCT c.session_key) FROM session_search_docs JOIN session_catalog c ON c.session_key=session_search_docs.session_key LEFT JOIN session_views v ON v.session_key=c.session_key WHERE {archived} AND session_search_docs MATCH ?1"),
                            vec![expression.into(), (limit as i64).into(), (offset as i64).into()],
                        )
                    } else {
                        (
                            format!("{base},NULL FROM session_catalog c LEFT JOIN session_views v ON v.session_key=c.session_key LEFT JOIN session_bindings b ON b.session_file=c.session_file LEFT JOIN tasks t ON t.id=b.task_id WHERE {archived} ORDER BY c.updated_at DESC,c.session_key LIMIT ?1 OFFSET ?2"),
                            format!("SELECT COUNT(*) FROM session_catalog c LEFT JOIN session_views v ON v.session_key=c.session_key WHERE {archived}"),
                            vec![(limit as i64).into(), (offset as i64).into()],
                        )
                    };
                let total: u64 = if searching {
                    connection
                        .query_row(&count_sql, [&query_params[0]], |row| row.get(0))
                        .map_err(|value| error("database_error", value.to_string()))?
                } else {
                    connection
                        .query_row(&count_sql, [], |row| row.get(0))
                        .map_err(|value| error("database_error", value.to_string()))?
                };
                let mut statement = connection
                    .prepare(&sql)
                    .map_err(|value| error("database_error", value.to_string()))?;
                let mut rows = statement
                    .query_map(rusqlite::params_from_iter(query_params), |row| {
                        Ok(CatalogEntry {
                            session_key: row.get(0)?,
                            session_id: row.get(1)?,
                            cwd: row.get(2)?,
                            title: row.get(3)?,
                            created_at: row.get(4)?,
                            updated_at: row.get(5)?,
                            model_provider: row.get(6)?,
                            model_id: row.get(7)?,
                            parent_session: row.get(8)?,
                            relation_kind: row.get(9)?,
                            status: row.get(10)?,
                            message_count: row.get(11)?,
                            total_tokens: row.get(12)?,
                            cost: row.get(13)?,
                            trusted: row.get(14)?,
                            project_id: row.get(15)?,
                            task_id: row.get(16)?,
                            task_title: row.get(17)?,
                            running: row.get(18)?,
                            unread: row.get(19)?,
                            archived: row.get(20)?,
                            parse_state: row.get(21)?,
                            parse_error_code: row.get(22)?,
                            snippet: row.get(23)?,
                        })
                    })
                    .map_err(|value| error("database_error", value.to_string()))?
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(|value| error("database_error", value.to_string()))?;
                if searching {
                    for row in &mut rows {
                        row.snippet = row
                            .snippet
                            .as_deref()
                            .map(|content| display_snippet(content, &raw_query));
                    }
                }
                let consumed = offset.saturating_add(rows.len());
                Ok(CatalogPage {
                    entries: rows,
                    total,
                    next_offset: (consumed < total as usize).then_some(consumed as u64),
                })
            })
            .await
    }

    pub async fn update_session_view(
        &self,
        session_key: &str,
        last_seen_entry_id: Option<String>,
        scroll_anchor_entry_id: Option<String>,
        scroll_anchor_offset: Option<f64>,
        archived: Option<bool>,
    ) -> Result<()> {
        let session_key = session_key.to_owned();
        if scroll_anchor_offset.is_some_and(|value| !value.is_finite()) {
            return Err(error(
                "session_view_invalid",
                "Scroll offset must be finite",
            ));
        }
        let archived = archived.map(|value| if value { 1_i64 } else { 0_i64 });
        self.store
            .access(move |connection| {
                if !connection
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM session_catalog WHERE session_key=?1)",
                        [&session_key],
                        |row| row.get::<_, bool>(0),
                    )
                    .map_err(|value| error("database_error", value.to_string()))?
                {
                    return Err(error(
                        "session_catalog_missing",
                        "Session catalog entry no longer exists",
                    ));
                }
                connection.execute(
                    "INSERT INTO session_views(session_key,last_seen_entry_id,last_seen_at,scroll_anchor_entry_id,scroll_anchor_offset,archived_at) VALUES(?1,?2,strftime('%s','now'),?3,?4,CASE ?5 WHEN 1 THEN strftime('%s','now') WHEN 0 THEN NULL ELSE NULL END) ON CONFLICT(session_key) DO UPDATE SET last_seen_entry_id=COALESCE(excluded.last_seen_entry_id,session_views.last_seen_entry_id),last_seen_at=excluded.last_seen_at,scroll_anchor_entry_id=COALESCE(excluded.scroll_anchor_entry_id,session_views.scroll_anchor_entry_id),scroll_anchor_offset=COALESCE(excluded.scroll_anchor_offset,session_views.scroll_anchor_offset),archived_at=CASE WHEN ?5 IS NULL THEN session_views.archived_at ELSE excluded.archived_at END",
                    params![session_key,last_seen_entry_id,scroll_anchor_entry_id,scroll_anchor_offset,archived],
                ).map_err(|value| error("database_error", value.to_string()))?;
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn write_session(path: &Path, id: &str, cwd: &Path, parent: Option<&str>, extra: &[Value]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut output = fs::File::create(path).unwrap();
        writeln!(output, "{}", json!({"type":"title","v":1,"title":"索引标题","updatedAt":"2026-09-15T00:00:00Z","pad":""})).unwrap();
        writeln!(output, "{}", json!({"type":"session","version":3,"id":id,"cwd":cwd,"timestamp":"2026-09-15T00:00:00Z","parentSession":parent})).unwrap();
        for value in extra {
            writeln!(output, "{value}").unwrap();
        }
    }

    #[tokio::test]
    async fn rebuilds_incrementally_searches_only_trusted_content_and_tracks_lineage() {
        let root = std::env::temp_dir().join(format!("catalog-{}", uuid::Uuid::new_v4()));
        let data = root.join("data");
        let sessions = root.join("omp-sessions");
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        let workbench = Workbench::open(data).await.unwrap();
        workbench
            .store
            .register_project("P", vec![project.clone()], true)
            .await
            .unwrap();
        let parent = sessions.join("group/parent.jsonl");
        write_session(
            &parent,
            "opaque-parent",
            &project,
            None,
            &[
                json!({"type":"model_change","id":"m","parentId":null,"timestamp":"2026-09-15T00:00:01Z","model":"provider/model"}),
                json!({"type":"message","id":"u","parentId":"m","timestamp":"2026-09-15T00:00:02Z","message":{"role":"user","content":"needle trusted"}}),
                json!({"type":"message","id":"a","parentId":"u","timestamp":"2026-09-15T00:00:03Z","message":{"role":"assistant","content":[{"type":"text","text":"answer"}],"usage":{"totalTokens":42,"cost":{"total":0.25}},"stopReason":"stop"}}),
            ],
        );
        write_session(
            &sessions.join("group/child.jsonl"),
            "child",
            &project,
            Some("opaque-parent"),
            &[],
        );
        let outside = root.join("outside");
        fs::create_dir_all(&outside).unwrap();
        write_session(
            &sessions.join("other/private.jsonl"),
            "private",
            &outside,
            None,
            &[
                json!({"type":"message","id":"private","timestamp":"2026-09-15T00:00:00Z","message":{"role":"user","content":"secretneedle"}}),
            ],
        );
        let first = workbench
            .refresh_session_catalog_from_roots(vec![sessions.clone()], false)
            .await
            .unwrap();
        assert_eq!((first.discovered, first.indexed, first.invalid), (3, 3, 0));
        let second = workbench
            .refresh_session_catalog_from_roots(vec![sessions.clone()], false)
            .await
            .unwrap();
        assert_eq!((second.indexed, second.unchanged), (0, 3));
        let found = workbench
            .session_catalog("needle", false, 0, 20)
            .await
            .unwrap();
        assert_eq!(found.total, 1);
        assert_eq!(found.entries[0].total_tokens, Some(42));
        assert!(
            found.entries[0]
                .snippet
                .as_deref()
                .unwrap()
                .contains("[needle]")
        );
        assert_eq!(
            workbench
                .session_catalog("secretneedle", false, 0, 20)
                .await
                .unwrap()
                .total,
            0
        );
        let relations: u64 = workbench
            .store
            .access(|connection| {
                connection
                    .query_row("SELECT COUNT(*) FROM session_relations", [], |row| {
                        row.get(0)
                    })
                    .map_err(|value| error("database_error", value.to_string()))
            })
            .await
            .unwrap();
        assert_eq!(relations, 1);
        let key = found.entries[0].session_key.clone();
        workbench
            .update_session_view(&key, None, None, None, Some(true))
            .await
            .unwrap();
        assert_eq!(
            workbench
                .session_catalog("", false, 0, 20)
                .await
                .unwrap()
                .total,
            2
        );
        assert_eq!(
            workbench
                .session_catalog("", true, 0, 20)
                .await
                .unwrap()
                .total,
            3
        );
    }

    #[tokio::test]
    async fn isolates_broken_files_and_full_rebuild_matches_incremental_results() {
        let root = std::env::temp_dir().join(format!("catalog-broken-{}", uuid::Uuid::new_v4()));
        let data = root.join("data");
        let sessions = root.join("sessions");
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        let workbench = Workbench::open(data).await.unwrap();
        workbench
            .store
            .register_project("P", vec![project.clone()], true)
            .await
            .unwrap();
        write_session(
            &sessions.join("good.jsonl"),
            "not-a-uuid",
            &project,
            None,
            &[
                json!({"type":"message","id":"one","timestamp":"2026-09-15T00:00:00Z","message":{"role":"user","content":"stable result"}}),
            ],
        );
        fs::write(sessions.join("bad.jsonl"), "{bad\n").unwrap();
        let result = workbench
            .refresh_session_catalog_from_roots(vec![sessions.clone()], false)
            .await
            .unwrap();
        assert_eq!((result.discovered, result.invalid), (2, 1));
        let before = workbench.session_catalog("", true, 0, 20).await.unwrap();
        workbench
            .refresh_session_catalog_from_roots(vec![sessions], true)
            .await
            .unwrap();
        let after = workbench.session_catalog("", true, 0, 20).await.unwrap();
        assert_eq!(
            before
                .entries
                .iter()
                .map(|value| (&value.session_id, &value.parse_state))
                .collect::<Vec<_>>(),
            after
                .entries
                .iter()
                .map(|value| (&value.session_id, &value.parse_state))
                .collect::<Vec<_>>()
        );
    }
}
