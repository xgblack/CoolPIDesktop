use crate::{
    broker::{EventBroker, ObserverInfo},
    runtime::{self, DEADLINE, HostError, Result, RuntimeInfo},
};
use omp_rpc::{JsonlDecoder, MAX_FRAME, MAX_LOGICAL, encode_request};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::{Child, ChildStdin},
    sync::{Mutex, RwLock, mpsc, oneshot},
    time::{Instant, timeout},
};
use uuid::Uuid;

const EVENT_BYTES: usize = 4 * 1024 * 1024;
const MAX_TASKS: usize = 32;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostEvent {
    pub task_id: String,
    pub run_id: String,
    pub seq: u64,
    pub event_type: String,
    pub payload: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolActivity {
    pub id: String,
    pub name: String,
    pub status: String,
    pub args: Value,
    pub result: Option<Value>,
    pub seq: u64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSnapshot {
    pub task_id: String,
    pub run_id: String,
    pub seq: u64,
    pub status: String,
    pub events: VecDeque<HostEvent>,
    pub text: String,
    pub truncated: bool,
    pub runtime: RuntimeInfo,
    pub error: Option<HostError>,
    pub pending_ui: Vec<Value>,
    #[serde(default)]
    pub tools: Vec<ToolActivity>,
    #[serde(default)]
    pub usage: UsageSummary,
}
fn bounded_value(value: &Value) -> Value {
    const LIMIT: usize = 64 * 1024;
    if serde_json::to_vec(value).map_or(true, |bytes| bytes.len() > LIMIT) {
        json!({"truncated":true,"message":"Tool data exceeds the 64 KiB workbench limit; the full record remains in OMP"})
    } else { value.clone() }
}
fn update_usage(snapshot: &mut TaskSnapshot, typ: &str, data: &Value) {
    if typ == "get_state" {
        let context = &data["contextUsage"];
        snapshot.usage.context_tokens = context["tokens"].as_u64();
        snapshot.usage.context_window = context["contextWindow"].as_u64();
        snapshot.usage.context_percent = context["percent"].as_f64();
    }
    if typ == "get_session_stats" {
        let tokens = &data["tokens"];
        snapshot.usage.input_tokens = tokens["input"].as_u64();
        snapshot.usage.output_tokens = tokens["output"].as_u64();
        snapshot.usage.reasoning_tokens = tokens["reasoning"].as_u64();
        snapshot.usage.cache_read_tokens = tokens["cacheRead"].as_u64();
        snapshot.usage.cache_write_tokens = tokens["cacheWrite"].as_u64();
        snapshot.usage.total_tokens = tokens["total"].as_u64();
        snapshot.usage.cost = data["cost"].as_f64();
    }
}
fn update_tool(snapshot: &mut TaskSnapshot, typ: &str, event: &Value) {
    let Some(id) = event["toolCallId"].as_str() else { return };
    match typ {
        "tool_execution_start" => {
            if let Some(tool) = snapshot.tools.iter_mut().find(|tool| tool.id == id) {
                tool.name = event["toolName"].as_str().unwrap_or("工具").into();
                tool.status = "running".into(); tool.args = bounded_value(&event["args"]); tool.result = None; tool.seq = snapshot.seq;
            } else if snapshot.tools.len() < 64 {
                snapshot.tools.push(ToolActivity { id: id.into(), name: event["toolName"].as_str().unwrap_or("工具").into(), status: "running".into(), args: bounded_value(&event["args"]), result: None, seq: snapshot.seq });
            } else { snapshot.truncated = true; }
        }
        "tool_execution_update" => {
            if let Some(tool) = snapshot.tools.iter_mut().find(|tool| tool.id == id) { tool.status = "running".into(); tool.result = Some(bounded_value(&event["partialResult"])); tool.seq = snapshot.seq; }
        }
        "tool_execution_end" => {
            let status = if event["isError"] == true { "failed" } else if event["result"]["details"]["reason"] == "aborted" { "cancelled" } else { "succeeded" };
            if let Some(tool) = snapshot.tools.iter_mut().find(|tool| tool.id == id) { tool.status = status.into(); tool.result = Some(bounded_value(&event["result"])); tool.seq = snapshot.seq; }
            else if snapshot.tools.len() < 64 { snapshot.tools.push(ToolActivity { id: id.into(), name: event["toolName"].as_str().unwrap_or("工具").into(), status: status.into(), args: Value::Null, result: Some(bounded_value(&event["result"])), seq: snapshot.seq }); }
            else { snapshot.truncated = true; }
        }
        _ => {}
    }
}
impl TaskSnapshot {
    fn event(&mut self, typ: &str, payload: Value) -> HostEvent {
        self.seq += 1;
        let mut event = HostEvent {
            task_id: self.task_id.clone(),
            run_id: self.run_id.clone(),
            seq: self.seq,
            event_type: typ.into(),
            payload,
        };
        if serde_json::to_vec(&event).map_or(usize::MAX, |v| v.len()) > EVENT_BYTES / 2 {
            event.payload = json!({"truncated":true,"message":"Event exceeds display buffer; original session remains in OMP"});
            self.truncated = true;
        }
        self.events.push_back(event.clone());
        while self.events.len() > 256
            || self
                .events
                .iter()
                .map(|e| serde_json::to_vec(e).map_or(EVENT_BYTES, |b| b.len()))
                .sum::<usize>()
                > EVENT_BYTES
        {
            self.events.pop_front();
            self.truncated = true;
        }
        event
    }
    fn status(&mut self, status: &str) -> HostEvent {
        self.status = status.into();
        self.event("status", json!({"status":status}))
    }
    fn fail(&mut self, e: HostError) -> HostEvent {
        self.error = Some(e.clone());
        self.status = "failed".into();
        self.event("error", json!(e))
    }
}
struct Task {
    snapshot: Arc<RwLock<TaskSnapshot>>,
    tx: mpsc::Sender<Action>,
}
enum Action {
    Query {
        typ: &'static str,
        payload: Value,
        reply: oneshot::Sender<Result<Value>>,
    },
    Request {
        typ: &'static str,
        payload: Value,
        reply: oneshot::Sender<Result<()>>,
    },
    Stop(oneshot::Sender<Result<()>>),
}
#[derive(Clone, Default)]
pub struct LaunchOptions {
    pub run_id: Option<String>,
    pub root: PathBuf,
    pub additional: Vec<PathBuf>,
    pub model: Option<String>,
    pub session_dir: Option<PathBuf>,
    pub session_file: Option<PathBuf>,
    pub expected_session: Option<String>,
}
#[derive(Clone)]
pub struct TaskManager {
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    saved: Arc<Mutex<Option<PathBuf>>>,
    snapshots: Arc<RwLock<HashMap<String, Arc<RwLock<TaskSnapshot>>>>>,
    broker: EventBroker,
    pub(crate) root: PathBuf,
    model: Option<String>,
}
impl TaskManager {
    pub fn new(root: PathBuf) -> Self {
        Self {
            tasks: Default::default(),
            saved: Default::default(),
            snapshots: Default::default(),
            broker: EventBroker::new(512),
            root,
            model: None,
        }
    }
    pub fn with_model(mut self, model: String) -> Self {
        self.model = Some(model);
        self
    }
    async fn resolve(&self, explicit: Option<&str>) -> Result<PathBuf> {
        let saved = self.saved.lock().await;
        runtime::resolve_executable(explicit.map(Path::new), saved.as_deref())
    }
    pub async fn runtime_status(&self, explicit: Option<String>) -> RuntimeInfo {
        let path = match self.resolve(explicit.as_deref()).await {
            Ok(p) => p,
            Err(e) => return RuntimeInfo::error(e, explicit.unwrap_or_default()),
        };
        let result = async {
            let info = runtime::probe(&path).await?;
            let mut wire = Wire::spawn(&path, &self.root, self.model.as_deref()).await?;
            let result = wire.initialize(info).await;
            wire.close().await?;
            result
        }
        .await;
        match result {
            Ok(info) => {
                *self.saved.lock().await = Some(path);
                info
            }
            Err(e) => RuntimeInfo::error(e, path.to_string_lossy().into()),
        }
    }
    pub async fn snapshots(&self) -> Vec<TaskSnapshot> {
        let map = self.snapshots.read().await;
        let values: Vec<_> = map.values().cloned().collect();
        drop(map);
        let mut out = Vec::new();
        for snapshot in values {
            out.push(snapshot.read().await.clone());
        }
        out.sort_by(|a, b| a.task_id.cmp(&b.task_id));
        out
    }
    pub async fn start(&self, id: String, explicit: Option<String>) -> Result<TaskSnapshot> {
        self.start_with(
            id,
            explicit,
            LaunchOptions {
                root: self.root.clone(),
                model: self.model.clone(),
                ..Default::default()
            },
        )
        .await
    }
    pub async fn start_with(
        &self,
        id: String,
        explicit: Option<String>,
        options: LaunchOptions,
    ) -> Result<TaskSnapshot> {
        if id.is_empty()
            || id.len() > 80
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(HostError::new(
                "invalid_task",
                "Task id must contain 1-80 letters, digits, - or _",
            ));
        }
        let path = self.resolve(explicit.as_deref()).await?;
        let (tx, rx) = mpsc::channel(16);
        let mut map = self.tasks.write().await;
        if map.contains_key(&id) {
            return Err(HostError::new(
                "task_exists",
                "Use restart for an existing task",
            ));
        }
        if map.len() >= MAX_TASKS {
            return Err(HostError::new(
                "task_limit",
                "Maximum 32 in-memory tasks reached",
            ));
        }
        let snapshot = Arc::new(RwLock::new(TaskSnapshot {
            task_id: id.clone(),
            run_id: options
                .run_id
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
            seq: 0,
            status: "starting".into(),
            events: VecDeque::new(),
            text: String::new(),
            truncated: false,
            runtime: RuntimeInfo {
                executable: path.to_string_lossy().into(),
                version: None,
                status: "starting".into(),
                protocol: None,
                capabilities: Value::Null,
                error: None,
            },
            error: None,
            pending_ui: Vec::new(),
            tools: Vec::new(),
            usage: UsageSummary::default(),
        }));
        map.insert(
            id.clone(),
            Task {
                snapshot: snapshot.clone(),
                tx,
            },
        );
        drop(map);
        self.snapshots.write().await.insert(id, snapshot.clone());
        let initial = snapshot.read().await.clone();
        tokio::spawn(run(path, options, snapshot, rx, self.broker.clone()));
        Ok(initial)
    }
    pub async fn restart(&self, id: &str) -> Result<TaskSnapshot> {
        let snapshot = self.snapshot(id).await?;
        self.stop(id).await?;
        self.tasks.write().await.remove(id);
        self.snapshots.write().await.remove(id);
        self.start(id.into(), Some(snapshot.runtime.executable))
            .await
    }
    pub async fn snapshot(&self, id: &str) -> Result<TaskSnapshot> {
        let map = self.tasks.read().await;
        let task = map
            .get(id)
            .ok_or_else(|| HostError::new("task_not_found", id))?;
        Ok(task.snapshot.read().await.clone())
    }
    pub async fn forget_stopped(&self, id: &str) -> Result<()> {
        let mut tasks = self.tasks.write().await;
        if let Some(task) = tasks.get(id) {
            if !task.tx.is_closed() {
                return Err(HostError::new("task_busy", "Process is still active"));
            }
        }
        tasks.remove(id);
        self.snapshots.write().await.remove(id);
        Ok(())
    }
    pub async fn query(&self, id: &str, typ: &'static str, payload: Value) -> Result<Value> {
        if !matches!(typ, "get_state" | "get_messages_page" | "set_model" | "get_session_stats") {
            return Err(HostError::new("forbidden_command", typ));
        }
        let (reply, done) = oneshot::channel();
        self.sender(id)
            .await?
            .try_send(Action::Query {
                typ,
                payload,
                reply,
            })
            .map_err(|_| HostError::new("task_busy", "Command queue unavailable"))?;
        timeout(DEADLINE * 2, done)
            .await
            .map_err(|_| HostError::new("timeout", "Query timed out"))?
            .map_err(|_| HostError::new("process_exited", "Task stopped"))?
    }
    pub async fn observer_info(&self) -> Result<ObserverInfo> {
        self.broker
            .observer_info(self.snapshots.clone())
            .await
            .map_err(|e| HostError::new("observer_unavailable", e))
    }
    async fn sender(&self, id: &str) -> Result<mpsc::Sender<Action>> {
        self.tasks
            .read()
            .await
            .get(id)
            .map(|t| t.tx.clone())
            .ok_or_else(|| HostError::new("task_not_found", id))
    }
    pub async fn stop(&self, id: &str) -> Result<()> {
        let tx = self.sender(id).await?;
        if tx.is_closed() {
            return Ok(());
        }
        let (reply, done) = oneshot::channel();
        tx.try_send(Action::Stop(reply))
            .map_err(|_| HostError::new("task_busy", "Task control queue full"))?;
        timeout(DEADLINE * 4, done)
            .await
            .map_err(|_| HostError::new("timeout", "Stop timed out"))?
            .map_err(|_| HostError::new("process_exited", "Task stopped"))?
    }
    pub async fn request(&self, id: &str, typ: &'static str, payload: Value) -> Result<()> {
        if !matches!(typ, "prompt" | "abort" | "extension_ui_response") {
            return Err(HostError::new("forbidden_command", typ));
        }
        let tx = self.sender(id).await?;
        let (reply, done) = oneshot::channel();
        tx.try_send(Action::Request {
            typ,
            payload,
            reply,
        })
        .map_err(|_| HostError::new("task_busy", "Task unavailable or control queue full"))?;
        timeout(DEADLINE * 2, done)
            .await
            .map_err(|_| HostError::new("timeout", "Command response timed out"))?
            .map_err(|_| HostError::new("process_exited", "Task process stopped"))?
    }
    pub async fn shutdown(&self) -> Result<()> {
        let ids: Vec<_> = self.tasks.read().await.keys().cloned().collect();
        let mut failures = Vec::new();
        for id in ids {
            if let Err(e) = self.stop(&id).await {
                failures.push(e.message);
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(HostError::new(
                "process_cleanup_failed",
                failures.join("; "),
            ))
        }
    }
}

struct Wire {
    child: Child,
    stdin: ChildStdin,
    rx: mpsc::Receiver<Result<Value>>,
    readers: Vec<tokio::task::JoinHandle<()>>,
    max_frame: usize,
    serial: u64,
}
impl Wire {
    async fn spawn(path: &Path, root: &Path, model: Option<&str>) -> Result<Self> {
        Self::spawn_with(
            path,
            &LaunchOptions {
                root: root.into(),
                model: model.map(str::to_owned),
                ..Default::default()
            },
        )
        .await
    }
    async fn spawn_with(path: &Path, options: &LaunchOptions) -> Result<Self> {
        let root = &options.root;
        if !root.is_dir() {
            return Err(HostError::new(
                "invalid_workspace",
                "Host workspace directory does not exist",
            ));
        }
        let mut command = runtime::command(path);
        command.args(["--mode", "rpc"]);
        command.arg("--cwd").arg(root);
        for path in &options.additional {
            command.arg("--add-dir").arg(path);
        }
        if let Some(dir) = &options.session_dir {
            command.arg("--session-dir").arg(dir);
        }
        if let Some(file) = &options.session_file {
            if !file.is_file() {
                return Err(HostError::new(
                    "session_missing",
                    "Bound session file is missing",
                ));
            }
            command.arg("--session").arg(file);
        }
        if let Some(model) = &options.model {
            command.arg("--model").arg(model);
        }
        let mut child = command
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| HostError::new("process_exited", e))?;
        let stdin = child.stdin.take().unwrap();
        let mut out = child.stdout.take().unwrap();
        let mut err = child.stderr.take().unwrap();
        let (tx, rx) = mpsc::channel(8);
        let err_tx = tx.clone();
        let stdout = tokio::spawn(async move {
            let mut decoder = JsonlDecoder::new(MAX_FRAME);
            let mut b = [0u8; 8192];
            loop {
                match out.read(&mut b).await {
                    Ok(0) => {
                        let e = decoder
                            .finish()
                            .err()
                            .map(|e| HostError::new("protocol_error", e))
                            .unwrap_or_else(|| {
                                HostError::new("process_exited", "OMP stdout closed")
                            });
                        let _ = tx.send(Err(e)).await;
                        break;
                    }
                    Ok(n) => match decoder.push(&b[..n]) {
                        Ok(values) => {
                            for v in values {
                                if v["type"] == "ready" {
                                    if let (Some(line), Some(logical)) = (
                                        v["maxFrameBytes"].as_u64(),
                                        v["maxReassembledFrameBytes"].as_u64(),
                                    ) {
                                        if let Err(e) =
                                            decoder.limits(line as usize, logical as usize)
                                        {
                                            let _ = tx
                                                .send(Err(HostError::new("protocol_error", e)))
                                                .await;
                                            return;
                                        }
                                    }
                                }
                                if tx.send(Ok(v)).await.is_err() {
                                    return;
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Err(HostError::new("protocol_error", e))).await;
                            break;
                        }
                    },
                    Err(e) => {
                        let _ = tx.send(Err(HostError::new("process_exited", e))).await;
                        break;
                    }
                }
            }
        });
        let stderr = tokio::spawn(async move {
            let mut b = [0u8; 4096];
            let mut tail = String::new();
            loop {
                match err.read(&mut b).await {
                    Ok(0) => break,
                    Ok(n) => {
                        tail.push_str(&String::from_utf8_lossy(&b[..n]));
                        if tail.contains("No models available.") {
                            let _=err_tx.send(Err(HostError::new("model_required","OMP has no configured model; configure OMP before loading a session"))).await;
                        }
                        if tail.len() > 128 {
                            tail = tail
                                .chars()
                                .rev()
                                .take(128)
                                .collect::<String>()
                                .chars()
                                .rev()
                                .collect();
                        }
                        // Drain even when the UI cannot keep up; do not retain unbounded diagnostics.
                        if err_tx
                            .try_send(Ok(json!({"type":"stderr","bytes":n})))
                            .is_err()
                        {
                            continue;
                        }
                    }
                    Err(e) => {
                        let _ = err_tx.try_send(Err(HostError::new("stderr_error", e)));
                        break;
                    }
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            rx,
            readers: vec![stdout, stderr],
            max_frame: MAX_FRAME,
            serial: 0,
        })
    }
    async fn send(&mut self, typ: &str, payload: Value) -> Result<String> {
        self.serial += 1;
        let id = format!("req-{}", self.serial);
        let bytes = encode_request(&id, typ, payload, self.max_frame)
            .map_err(|e| HostError::new("invalid_request", e))?;
        timeout(DEADLINE, self.stdin.write_all(&bytes))
            .await
            .map_err(|_| HostError::new("timeout", "OMP stdin blocked"))?
            .map_err(|e| HostError::new("process_exited", e))?;
        Ok(id)
    }
    async fn next(&mut self) -> Result<Value> {
        timeout(DEADLINE, self.rx.recv())
            .await
            .map_err(|_| HostError::new("handshake_failed", "OMP startup timed out"))?
            .ok_or_else(|| HostError::new("process_exited", "OMP stream closed"))?
    }
    async fn query(&mut self, typ: &str, payload: Value) -> Result<Value> {
        let id = self.send(typ, payload).await?;
        timeout(DEADLINE, async {
            loop {
                let v = self.next().await?;
                if v["type"] == "response" {
                    if v["id"] != id || v["command"] != typ {
                        return Err(HostError::new(
                            "protocol_error",
                            "Unexpected startup response identity",
                        ));
                    }
                    if v["success"] != true {
                        return Err(HostError::new(
                            "capability_query_failed",
                            v["error"].as_str().unwrap_or("OMP query failed"),
                        ));
                    }
                    return Ok(v["data"].clone());
                }
            }
        })
        .await
        .map_err(|_| HostError::new("handshake_failed", "Startup query timed out"))?
    }
    async fn initialize(&mut self, mut info: RuntimeInfo) -> Result<RuntimeInfo> {
        let ready = timeout(DEADLINE, async {
            loop {
                let v = self.next().await?;
                if v["type"] == "stderr" || v["type"] == "available_commands_update" {
                    continue;
                }
                return Ok::<_, HostError>(v);
            }
        })
        .await
        .map_err(|_| HostError::new("handshake_failed", "Ready timed out"))??;
        if ready["type"] != "ready"
            || !ready["supportedProtocolVersions"]
                .as_array()
                .is_some_and(|a| a.contains(&json!(2)))
        {
            return Err(HostError::new(
                "handshake_failed",
                "OMP must advertise protocol v2",
            ));
        }
        let physical = ready["maxFrameBytes"]
            .as_u64()
            .filter(|n| *n > 0)
            .ok_or_else(|| HostError::new("handshake_failed", "Missing frame limit"))?;
        if ready["maxReassembledFrameBytes"]
            .as_u64()
            .filter(|n| *n > 0)
            .is_none()
        {
            return Err(HostError::new(
                "handshake_failed",
                "Missing reassembly limit",
            ));
        }
        self.max_frame = (physical as usize).min(MAX_FRAME);
        let negotiated = self
            .query("negotiate_protocol", json!({"protocolVersion":2}))
            .await
            .map_err(|e| HostError::new("handshake_failed", e.message))?;
        if negotiated["protocolVersion"] != 2 {
            return Err(HostError::new(
                "handshake_failed",
                "Negotiation did not confirm v2",
            ));
        }
        let state = self.query("get_state", json!({})).await?;
        if !state.is_object() {
            return Err(HostError::new(
                "capability_query_failed",
                "Invalid get_state response",
            ));
        }
        let models = self.query("get_available_models", json!({})).await?;
        let commands = self.query("get_available_commands", json!({})).await?;
        if !models["models"].is_array() || !commands["commands"].is_array() {
            return Err(HostError::new(
                "capability_query_failed",
                "Invalid model/command response",
            ));
        }
        info.status = if models["models"].as_array().unwrap().is_empty() {
            "partially_available"
        } else {
            "ready"
        }
        .into();
        info.protocol = Some(2);
        info.capabilities = json!({"state":state,"models":models["models"],"commands":commands["commands"],"maxFrameBytes":self.max_frame,"maxReassembledFrameBytes":ready["maxReassembledFrameBytes"].as_u64().unwrap().min(MAX_LOGICAL as u64)});
        Ok(info)
    }
    async fn close(&mut self) -> Result<()> {
        let result = runtime::reap(&mut self.child).await;
        for reader in &self.readers {
            reader.abort();
        }
        result
    }
}
impl Drop for Wire {
    fn drop(&mut self) {
        for r in &self.readers {
            r.abort();
        }
        #[cfg(unix)]
        if let Some(pid) = self.child.id() {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
    }
}

struct Pending {
    typ: &'static str,
    deadline: Instant,
    reply: Option<oneshot::Sender<Result<()>>>,
}
async fn run(
    path: PathBuf,
    options: LaunchOptions,
    snapshot: Arc<RwLock<TaskSnapshot>>,
    mut actions: mpsc::Receiver<Action>,
    broker: EventBroker,
) {
    let startup = async {
        let info = runtime::probe(&path).await?;
        let mut wire = Wire::spawn_with(&path, &options).await?;
        match wire.initialize(info).await {
            Ok(info) => {
                if let Some(expected) = &options.expected_session {
                    if info.capabilities["state"]["sessionId"] != *expected {
                        wire.close().await?;
                        return Err(HostError::new(
                            "session_mismatch",
                            "OMP resumed a different session",
                        ));
                    }
                }
                Ok((wire, info))
            }
            Err(e) => {
                wire.close().await?;
                Err(e)
            }
        }
    };
    let (mut wire, info) = match startup.await {
        Ok(v) => v,
        Err(e) => {
            let mut s = snapshot.write().await;
            s.runtime = RuntimeInfo::error(e.clone(), path.to_string_lossy().into());
            let event = s.fail(e);
            broker.publish(event);
            return;
        }
    };
    {
        let mut s = snapshot.write().await;
        s.runtime = info;
        if let Some(state) = s.runtime.capabilities.get("state").cloned() {
            update_usage(&mut s, "get_state", &state);
        }
        let event = s.event("process_started", json!({"pid":wire.child.id()}));
        broker.publish(event);
        let event = s.status("ready");
        broker.publish(event);
    }
    let mut pending: HashMap<String, Pending> = HashMap::new();
    let mut queries: HashMap<String, (&'static str, Instant, oneshot::Sender<Result<Value>>)> =
        HashMap::new();
    let mut active_prompt: Option<String> = None;
    let mut aborting = false;
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    let mut stop_reply = None;
    loop {
        tokio::select! {
            action=actions.recv()=>match action {
                Some(Action::Query{typ,payload,reply})=>{
                    let status=snapshot.read().await.status.clone();
                    if !matches!(status.as_str(),"ready"|"idle"|"interrupted") {let _=reply.send(Err(HostError::new("session_busy","Task is not idle")));continue;}
                    match wire.send(typ,payload).await {Ok(id)=>{queries.insert(id,(typ,Instant::now()+DEADLINE,reply));},Err(e)=>{let _=reply.send(Err(e));}}
                },
                Some(Action::Stop(reply))=>{stop_reply=Some(reply);break;},
                None=>break,
                Some(Action::Request{typ,payload,reply})=>{
                    if typ=="extension_ui_response" {
                        let mut s=snapshot.write().await;
                        let id=payload["id"].as_str().unwrap_or_default();
                        let Some(pos)=s.pending_ui.iter().position(|v|v["id"]==id) else {let _=reply.send(Err(HostError::new("invalid_ui_request","UI request expired or unknown")));continue;};
                        let request=&s.pending_ui[pos];
                        let valid=valid_ui_response(request,&payload);
                        if !valid {let _=reply.send(Err(HostError::new("invalid_ui_response","Invalid response for this UI request")));continue;}
                        let mut body=json!({"type":"extension_ui_response","id":id});
                        for key in ["cancelled","confirmed","value"] {if let Some(v)=payload.get(key){body[key]=v.clone();}}
                        let mut bytes=serde_json::to_vec(&body).unwrap();bytes.push(b'\n');
                        if bytes.len()>wire.max_frame {let _=reply.send(Err(HostError::new("invalid_request","UI response too large")));continue;}
                        let result=timeout(DEADLINE,wire.stdin.write_all(&bytes)).await.map_err(|_|HostError::new("timeout","UI response write timed out")).and_then(|r|r.map_err(|e|HostError::new("process_exited",e)));
                        if result.is_ok(){s.pending_ui.remove(pos);let event=s.event("ui_response",json!({"id":id}));broker.publish(event);}
                        let _=reply.send(result);continue;
                    }
                    let status=snapshot.read().await.status.clone();
                    if typ=="prompt" && !matches!(status.as_str(),"ready"|"idle"|"interrupted") {let _=reply.send(Err(HostError::new("task_busy","Task is not idle")));continue;}
                    if typ=="prompt" && payload["message"].as_str().is_none_or(|s|s.trim().is_empty()) {let _=reply.send(Err(HostError::new("invalid_request","Message is empty")));continue;}
                    if typ=="abort" && status!="running" {let _=reply.send(Err(HostError::new("task_busy","Task is not running")));continue;}
                    if pending.len()>=16 {let _=reply.send(Err(HostError::new("task_busy","Too many pending commands")));continue;}
                    match wire.send(typ,payload.clone()).await {
                        Ok(id)=>{
                            if typ=="prompt" {active_prompt=Some(id.clone());aborting=false;let mut s=snapshot.write().await;s.text.clear();let event=s.status("running");broker.publish(event);let event=s.event("user_message",json!({"text":payload["message"]}));broker.publish(event);}
                            if typ=="abort"{aborting=true;}
                            pending.insert(id,Pending{typ,deadline:Instant::now()+DEADLINE,reply:Some(reply)});
                        },
                        Err(e)=>{let _=reply.send(Err(e.clone()));let event=snapshot.write().await.fail(e);broker.publish(event);break;}
                    }
                }
            },
            value=wire.rx.recv()=>{
                let v=match value {Some(Ok(v))=>v,Some(Err(e))=>{let event=snapshot.write().await.fail(e);broker.publish(event);break;},None=>{let event=snapshot.write().await.fail(HostError::new("process_exited","OMP stream ended"));broker.publish(event);break;}};
                let typ=v["type"].as_str().unwrap_or("unknown");
                if typ=="stderr" { let event=snapshot.write().await.event("stderr",v.clone());broker.publish(event);continue; }
                if typ=="response" {
                    let id=v["id"].as_str().unwrap_or_default();
                    if let Some((command,_,reply))=queries.remove(id) {
                        let result=if v["command"]!=command {Err(HostError::new("protocol_error","Query command mismatch"))}else if v["success"]==true {Ok(v["data"].clone())}else{Err(HostError::new(v["code"].as_str().unwrap_or("request_failed"),v["error"].as_str().unwrap_or("Query failed")))};
                        if let Ok(data)=&result {let mut s=snapshot.write().await;if command=="get_state" {s.runtime.capabilities["state"]=data.clone();} update_usage(&mut s,command,data);}
                        let _=reply.send(result);continue;
                    }
                    if let Some(p)=pending.get_mut(id) {
                        if v["command"]!=p.typ {let event=snapshot.write().await.fail(HostError::new("protocol_error","Response command mismatch"));broker.publish(event);break;}
                        let result=if v["success"]==true {Ok(())}else{Err(HostError::new("request_failed",v["error"].as_str().unwrap_or("OMP rejected request")))};
                        if let Some(reply)=p.reply.take(){let _=reply.send(result.clone());}
                        if let Err(e)=result {let event=snapshot.write().await.fail(e);broker.publish(event);if active_prompt.as_deref()==Some(id){active_prompt=None;}}
                        if p.typ!="prompt" || v["success"]!=true || v["data"]["agentInvoked"]==false {
                            if p.typ=="prompt" && v["success"]==true {let event=snapshot.write().await.status("idle");broker.publish(event);active_prompt=None;}
                            pending.remove(id);
                        } else {p.deadline=Instant::now()+Duration::from_secs(86400);}
                    } else {let event=snapshot.write().await.event("protocol_warning",json!({"message":"Uncorrelated response","id":id}));broker.publish(event);}
                }
                if typ=="prompt_result" && v["agentInvoked"]==false && active_prompt.as_deref()==v["id"].as_str() {
                    if let Some(id)=active_prompt.take(){if let Some(mut p)=pending.remove(&id){if let Some(reply)=p.reply.take(){let _=reply.send(Ok(()));}}}
                    let event=snapshot.write().await.status("idle");broker.publish(event);
                }
                if typ=="agent_end" && v["isTerminal"]!=false {
                    // Keep ACK correlation until it arrives even if agent_end precedes it.
                    if let Some(id)=active_prompt.take(){if pending.get(&id).is_some_and(|p|p.reply.is_none()){pending.remove(&id);}}
                    let mut s=snapshot.write().await;s.pending_ui.clear();let event=s.status(if aborting{"interrupted"}else{"idle"});broker.publish(event);aborting=false;
                }
                if typ=="message_update" && v["assistantMessageEvent"]["type"]=="text_delta" {
                    if let Some(delta)=v["assistantMessageEvent"]["delta"].as_str(){let mut s=snapshot.write().await;s.text.push_str(delta);if s.text.len()>EVENT_BYTES {let mut cut=s.text.len()-EVENT_BYTES;while !s.text.is_char_boundary(cut){cut+=1;}s.text.drain(..cut);s.truncated=true;}}
                }
                if typ=="extension_ui_request" {
                    let mut s=snapshot.write().await;
                    if v["method"]=="cancel" {s.pending_ui.retain(|p|p["id"]!=v["id"]);}
                    else if matches!(v["method"].as_str(),Some("confirm"|"select"|"input"|"editor")) {
                        if s.pending_ui.len()>=16 {let event=s.fail(HostError::new("ui_limit","Too many UI requests"));broker.publish(event);break;}
                        s.pending_ui.push(v.clone());
                    }
                }
                let mut s=snapshot.write().await;
                let event=s.event(typ,v.clone());
                update_tool(&mut s,typ,&v);
                broker.publish(event);
            },
            _=tick.tick()=>{
                let expired:Vec<_>=queries.iter().filter(|(_,(_,deadline,_))|*deadline<=Instant::now()).map(|(id,_)|id.clone()).collect();
                for id in expired {if let Some((_,_,reply))=queries.remove(&id){let _=reply.send(Err(HostError::new("timeout","Query response timed out")));}}
                if let Some(id)=pending.iter().find(|(_,p)|p.reply.is_some()&&p.deadline<=Instant::now()).map(|(id,_)|id.clone()) {
                    let e=HostError::new("timeout",format!("OMP did not respond to {id}"));let event=snapshot.write().await.fail(e);broker.publish(event);break;
                }
                match wire.child.try_wait(){Ok(Some(status))=>{let event=snapshot.write().await.fail(HostError::new("process_exited",status));broker.publish(event);break;},Err(e)=>{let event=snapshot.write().await.fail(HostError::new("process_exited",e));broker.publish(event);break;},_=>{}}
            }
        }
    }
    let closed = wire.close().await;
    {
        let mut s = snapshot.write().await;
        s.pending_ui.clear();
        if let Err(e) = &closed {
            let event = s.fail(e.clone());
            broker.publish(event);
        } else if s.status != "failed" {
            let event = s.status("stopped");
            broker.publish(event);
        }
    }
    for (_, mut p) in pending {
        if let Some(reply) = p.reply.take() {
            let _ = reply.send(Err(HostError::new("process_exited", "OMP process stopped")));
        }
    }
    if let Some(reply) = stop_reply {
        let _ = reply.send(closed);
    }
}

fn valid_ui_response(request: &Value, payload: &Value) -> bool {
    payload["cancelled"] == true
        || match request["method"].as_str() {
            Some("confirm") => payload["confirmed"].is_boolean(),
            Some("select") => request["options"]
                .as_array()
                .is_some_and(|a| a.contains(&payload["value"])),
            Some("input" | "editor") => payload["value"].is_string(),
            _ => false,
        }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> TaskSnapshot {
        TaskSnapshot { task_id: "task".into(), run_id: "run".into(), seq: 0, status: "idle".into(), events: VecDeque::new(), text: String::new(), truncated: false, runtime: RuntimeInfo { executable: String::new(), version: None, status: "idle".into(), protocol: Some(2), capabilities: Value::Null, error: None }, error: None, pending_ui: Vec::new(), tools: Vec::new(), usage: UsageSummary::default() }
    }
    #[test]
    fn ui_responses_validate_method_and_choice() {
        let confirm = json!({"method":"confirm"});
        assert!(valid_ui_response(&confirm, &json!({"confirmed":false})));
        assert!(!valid_ui_response(&confirm, &json!({"confirmed":"true"})));
        let select = json!({"method":"select","options":["yes","no"]});
        assert!(valid_ui_response(&select, &json!({"value":"no"})));
        assert!(!valid_ui_response(&select, &json!({"value":"unknown"})));
        assert!(valid_ui_response(&select, &json!({"cancelled":true})));
        for method in ["input", "editor"] {
            assert!(valid_ui_response(
                &json!({"method":method}),
                &json!({"value":""})
            ));
            assert!(!valid_ui_response(
                &json!({"method":method}),
                &json!({"value":null})
            ));
        }
    }
    #[test]
    fn tool_activity_and_usage_keep_omp_semantics() {
        let mut state = snapshot();
        let start = json!({"toolCallId":"call","toolName":"read","args":{"path":"a"}});
        state.event("tool_execution_start", start.clone()); update_tool(&mut state, "tool_execution_start", &start);
        let end = json!({"toolCallId":"call","toolName":"read","result":{"content":[{"type":"text","text":"ok"}]},"isError":false});
        state.event("tool_execution_end", end.clone()); update_tool(&mut state, "tool_execution_end", &end);
        update_usage(&mut state, "get_state", &json!({"contextUsage":{"tokens":120,"contextWindow":200,"percent":60.0}}));
        update_usage(&mut state, "get_session_stats", &json!({"tokens":{"input":10,"output":20,"reasoning":3,"cacheRead":4,"cacheWrite":5,"total":42},"cost":0.12}));
        assert_eq!(state.tools[0].status, "succeeded"); assert_eq!(state.tools[0].args["path"], "a");
        assert_eq!(state.usage.context_tokens, Some(120)); assert_eq!(state.usage.total_tokens, Some(42)); assert_eq!(state.usage.cost, Some(0.12));
    }
}
