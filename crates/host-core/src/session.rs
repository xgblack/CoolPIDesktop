use crate::git::{self, GitDiff, GitStatus};
use crate::runtime::Result;
use crate::store::{Store, TaskRecord};
use crate::terminal::TerminalService;
use crate::{HostError, LaunchOptions, TaskManager, TaskSnapshot};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;

/// Product task coordinator. Runtime remains the only writer to OMP stdin.
#[derive(Clone)]
pub struct Workbench {
    pub store: Store,
    pub runtime: TaskManager,
    pub(crate) data: PathBuf,
    pub(crate) gate: Arc<Mutex<()>>,
    pub(crate) terminals: TerminalService,
    pub(crate) lifecycle: Arc<Mutex<crate::lifecycle::Lifecycle>>,
    auto_titles: Arc<Mutex<HashSet<String>>>,
}
// Only identity metadata is inspected here. OMP alone loads messages and resolves blobs.
pub(crate) fn session_header(file: &Path) -> Result<Value> {
    let meta = std::fs::symlink_metadata(file).map_err(|e| HostError::new("session_missing", e))?;
    if !meta.file_type().is_file() {
        return Err(HostError::new(
            "session_invalid",
            "Session must be a regular file, not a symlink",
        ));
    }
    let reader = BufReader::new(
        std::fs::File::open(file).map_err(|e| HostError::new("session_missing", e))?,
    )
    .take(16384);
    let mut lines = BufReader::new(reader).lines();
    let parse = |line: Option<std::io::Result<String>>| -> Result<Value> {
        serde_json::from_str(
            &line
                .ok_or_else(|| HostError::new("session_invalid", "Missing session header"))?
                .map_err(|e| HostError::new("session_invalid", e))?,
        )
        .map_err(|e| HostError::new("session_invalid", e))
    };
    let first = parse(lines.next())?;
    let header = if first["type"] == "title"
        && first["v"] == 1
        && first["title"].is_string()
        && first["pad"].is_string()
        && first["updatedAt"].is_string()
    {
        parse(lines.next())?
    } else {
        first
    };
    if header["type"] != "session"
        || header["id"].as_str().is_none_or(str::is_empty)
        || !header["cwd"].is_string()
    {
        return Err(HostError::new(
            "session_invalid",
            "Invalid session identity header",
        ));
    }
    if header
        .get("version")
        .is_some_and(|v| !matches!(v.as_u64(), Some(1..=3)))
    {
        return Err(HostError::new(
            "session_incompatible",
            "Unsupported session format",
        ));
    }
    Ok(header)
}
impl Workbench {
    pub async fn open(data: PathBuf) -> Result<Self> {
        let store = Store::open(data.join("desktop.sqlite")).await?;
        Ok(Self {
            runtime: TaskManager::new(data.clone()),
            store,
            data,
            gate: Default::default(),
            terminals: Default::default(),
            lifecycle: Default::default(),
            auto_titles: Default::default(),
        })
    }
    pub async fn detect(&self, explicit: Option<String>) -> crate::RuntimeInfo {
        let chosen = match explicit {
            Some(p) => Some(p),
            None => match self.store.setting("executable").await {
                Ok(v) => v,
                Err(e) => return crate::RuntimeInfo::error(e, String::new()),
            },
        };
        let info = self.runtime.runtime_status(chosen).await;
        if matches!(
            info.status.as_str(),
            "ready" | "partially_available" | "model_required"
        ) {
            if let Err(e) = self
                .store
                .set_setting("executable", info.executable.clone())
                .await
            {
                return crate::RuntimeInfo::error(e, info.executable);
            }
        }
        info
    }
    pub async fn title_prompt_settings(&self) -> Result<crate::TitlePromptSettings> {
        Ok(crate::auto_title::prompt_settings(
            self.store
                .setting(crate::auto_title::TITLE_PROMPT_SETTING)
                .await?,
        ))
    }
    pub async fn save_title_prompt(
        &self,
        prompt: Option<String>,
    ) -> Result<crate::TitlePromptSettings> {
        match prompt {
            Some(prompt) => {
                let prompt = prompt.trim();
                if prompt.is_empty() {
                    return Err(HostError::new(
                        "title_prompt_invalid",
                        "标题提示词不能为空；需要恢复官方提示词时请使用“恢复默认”",
                    ));
                }
                if prompt.chars().count() > crate::auto_title::MAX_TITLE_PROMPT_CHARS {
                    return Err(HostError::new(
                        "title_prompt_invalid",
                        "标题提示词不能超过 16000 个字符",
                    ));
                }
                if prompt.contains('\0') {
                    return Err(HostError::new(
                        "title_prompt_invalid",
                        "标题提示词不能包含空字符",
                    ));
                }
                self.store
                    .set_setting(
                        crate::auto_title::TITLE_PROMPT_SETTING,
                        prompt.to_owned(),
                    )
                    .await?;
            }
            None => {
                self.store
                    .remove_setting(crate::auto_title::TITLE_PROMPT_SETTING)
                    .await?
            }
        }
        self.title_prompt_settings().await
    }
    pub async fn continue_task(&self, id: &str) -> Result<TaskSnapshot> {
        let _guard = self.gate.lock().await;
        self.check_handoff(id).await?;
        self.clear_manual_stop(id).await;
        self.continue_locked(id).await
    }
    pub(crate) async fn continue_locked(&self, id: &str) -> Result<TaskSnapshot> {
        self.check_handoff(id).await?;
        if let Ok(snapshot) = self.runtime.snapshot(id).await {
            if matches!(
                snapshot.status.as_str(),
                "ready" | "idle" | "running" | "interrupted"
            ) {
                return Ok(snapshot);
            }
            self.stop_locked(id).await?;
            self.runtime.forget_stopped(id).await?;
        }
        let task = self.store.task(id).await?;
        if task.archived {
            return Err(HostError::new(
                "task_archived",
                "Restore archived task before continuing",
            ));
        }
        let roots = self.store.validate_task_roots(id).await?;
        if let Some(file) = &task.session_file {
            let file = file.clone();
            let header = tokio::task::spawn_blocking(move || session_header(&file))
                .await
                .map_err(|e| HostError::new("session_invalid", e))??;
            if header["id"].as_str() != task.session_id.as_deref() {
                return Err(HostError::new(
                    "session_mismatch",
                    "Bound session identity changed",
                ));
            }
        }
        let session_dir = self.data.join("sessions").join(id);
        tokio::fs::create_dir_all(&session_dir)
            .await
            .map_err(|e| HostError::new("storage_error", e))?;
        // Never guess which orphaned file is the right conversation.
        if task.session_file.is_none() {
            let mut entries = tokio::fs::read_dir(&session_dir)
                .await
                .map_err(|e| HostError::new("storage_error", e))?;
            if entries
                .next_entry()
                .await
                .map_err(|e| HostError::new("storage_error", e))?
                .is_some()
            {
                return Err(HostError::new(
                    "session_recovery_required",
                    "Unbound session files exist; explicit recovery is required",
                ));
            }
        }
        if let Some(level) = &task.thinking { self.validate_thinking(&task, level).await?; }
        let run_id = uuid::Uuid::new_v4().to_string();
        let options = LaunchOptions {
            approval_mode: task.approval_mode.clone(),
            run_id: Some(run_id.clone()),
            root: roots[0].clone(),
            additional: roots[1..].to_vec(),
            model: if task.session_file.is_none() || task.thinking.is_some() {
                task.model.clone()
            } else {
                None
            },
            thinking: task.thinking.clone(),
            session_dir: Some(session_dir),
            session_file: task.session_file.clone(),
            expected_session: task.session_id.clone(),
        };
        let executable = self.store.setting("executable").await?;
        let lease = crate::writer_lock::WriterLease::acquire(&self.data, id, "desktop")?;
        self.store.begin_run(id, &run_id).await?;
        let initial = match self
            .runtime
            .start_with_lease(id.into(), executable, options, Some(lease))
            .await
        {
            Ok(v) => v,
            Err(e) => {
                self.store
                    .end_run(&run_id, "failed", Some(e.code.clone()))
                    .await?;
                return Err(e);
            }
        };
        let result = tokio::time::timeout(Duration::from_secs(40), async {
            loop {
                let snapshot = self.runtime.snapshot(id).await?;
                if snapshot.status == "failed" {
                    return Err(snapshot.error.unwrap_or_else(|| {
                        HostError::new("startup_failed", "OMP startup failed")
                    }));
                }
                if snapshot.status == "ready" {
                    let mut state = self.runtime.query(id, "get_state", json!({})).await?;
                    if let Some(selected) = &task.model {
                        let (provider, model_id) = selected.split_once('/').ok_or_else(|| {
                            HostError::new("model_unavailable", "Invalid model selection")
                        })?;
                        if state["model"]["provider"] != provider
                            || state["model"]["id"] != model_id
                        {
                            self.runtime
                                .query(
                                    id,
                                    "set_model",
                                    json!({"provider":provider,"modelId":model_id}),
                                )
                                .await?;
                            state = self.runtime.query(id, "get_state", json!({})).await?;
                            if state["model"]["provider"] != provider
                                || state["model"]["id"] != model_id
                            {
                                return Err(HostError::new(
                                    "model_unavailable",
                                    "OMP did not select the requested model",
                                ));
                            }
                        }
                    }
                    let session_id = state["sessionId"].as_str().ok_or_else(|| {
                        HostError::new("session_missing", "OMP did not report sessionId")
                    })?;
                    let file = state["sessionFile"].as_str().ok_or_else(|| {
                        HostError::new("session_missing", "OMP did not report sessionFile")
                    })?;
                    let file = PathBuf::from(file);
                    let parent = file
                        .parent()
                        .ok_or_else(|| {
                            HostError::new("session_invalid", "Session path has no parent")
                        })?
                        .canonicalize()
                        .map_err(|e| HostError::new("session_invalid", e))?;
                    let expected = self
                        .data
                        .join("sessions")
                        .join(id)
                        .canonicalize()
                        .map_err(|e| HostError::new("session_invalid", e))?;
                    if parent != expected {
                        return Err(HostError::new(
                            "session_invalid",
                            "OMP session is outside this task's directory",
                        ));
                    }
                    if task.session_id.is_none() {
                        self.store.reserve_session(id, session_id, file).await?;
                    } else {
                        self.store.bind(id, session_id, file).await?;
                    }
                    self.store
                        .session_version(id, snapshot.runtime.version.clone())
                        .await?;
                    self.store
                        .set_model(
                            id,
                            state["model"]["provider"]
                                .as_str()
                                .zip(state["model"]["id"].as_str())
                                .map(|(p, m)| format!("{p}/{m}")),
                        )
                        .await?;
                    if task.session_id.is_none() {
                        if let Some(model) = &task.model {
                            self.store.set_setting(&format!("project_model:{}", task.project_id), model.clone()).await?;
                        }
                    }
                    return self.runtime.snapshot(id).await;
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
        })
        .await
        .unwrap_or_else(|_| Err(HostError::new("timeout", "Session startup timed out")));
        if let Err(e) = &result {
            self.runtime.stop(id).await?;
            self.store
                .end_run(&initial.run_id, "failed", Some(e.code.clone()))
                .await?;
        }
        result
    }
    pub async fn stop(&self, id: &str) -> Result<TaskSnapshot> {
        let _guard = self.gate.lock().await;
        self.check_handoff(id).await?;
        self.suppress_autostart(id).await;
        self.stop_locked(id).await
    }
    pub async fn records(&self) -> Result<Vec<TaskRecord>> {
        let _guard = self.gate.lock().await;
        for snapshot in self.runtime.snapshots().await {
            if matches!(snapshot.status.as_str(), "failed" | "stopped") {
                self.store
                    .end_run(
                        &snapshot.run_id,
                        &snapshot.status,
                        snapshot.error.map(|e| e.code),
                    )
                    .await?;
            }
        }
        self.store.tasks().await
    }
    async fn stop_locked(&self, id: &str) -> Result<TaskSnapshot> {
        self.terminals.close_task(id).await?;
        self.runtime.stop(id).await?;
        let snapshot = self.runtime.snapshot(id).await?;
        self.store
            .end_run(
                &snapshot.run_id,
                &snapshot.status,
                snapshot.error.as_ref().map(|e| e.code.clone()),
            )
            .await?;
        Ok(snapshot)
    }
    /// Explicit settings action: reload idle processes, never interrupt an active turn.
    pub async fn apply_model_config(&self) -> Result<Value> {
        let _guard = self.gate.lock().await;
        let mut applied = Vec::new();
        let mut skipped = Vec::new();
        let mut errors = Vec::new();
        for snapshot in self.runtime.snapshots().await {
            if !matches!(snapshot.status.as_str(), "ready" | "idle" | "interrupted") {
                if snapshot.status == "running" || snapshot.status == "starting" {
                    skipped.push(snapshot.task_id);
                }
                continue;
            }
            let id = snapshot.task_id;
            let record = self.store.task(&id).await?;
            if record
                .session_file
                .as_ref()
                .is_none_or(|file| !file.is_file())
            {
                skipped.push(id);
                continue;
            }
            // State can advance while the UI is reading; check OMP before stopping.
            let state = self.runtime.query(&id, "get_state", json!({})).await?;
            if state["isStreaming"] == true || state["isCompacting"] == true {
                skipped.push(id);
                continue;
            }
            let result = async {
                self.stop_locked(&id).await?;
                self.runtime.forget_stopped(&id).await?;
                self.continue_locked(&id).await?;
                Result::Ok(())
            }
            .await;
            match result {
                Ok(()) => applied.push(id),
                Err(error) => errors.push(json!({"taskId":id,"error":error})),
            }
        }
        Ok(json!({"applied":applied,"skipped":skipped,"errors":errors}))
    }
    pub async fn restart(&self, id: &str) -> Result<TaskSnapshot> {
        let _guard = self.gate.lock().await;
        self.check_handoff(id).await?;
        self.clear_manual_stop(id).await;
        if self.runtime.snapshot(id).await.is_ok() {
            self.stop_locked(id).await?;
            self.runtime.forget_stopped(id).await?;
        }
        self.continue_locked(id).await
    }
    /// Change only this task's OMP process. Workbench terminals remain alive.
    pub async fn set_approval_mode(&self, id: &str, mode: Option<String>) -> Result<TaskRecord> {
        crate::task::validate_approval_mode(mode.as_deref())?;
        let _guard = self.gate.lock().await;
        self.check_handoff(id).await?;
        let task = self.store.task(id).await?;
        if task.archived {
            return Err(HostError::new("task_archived", "Restore task before changing approval mode"));
        }
        if task.approval_mode == mode { return Ok(task); }
        let snapshot = self.runtime.snapshot(id).await.ok();
        let alive = snapshot.as_ref().is_some_and(|s| !matches!(s.status.as_str(), "stopped" | "failed"));
        if alive {
            let snapshot = snapshot.as_ref().unwrap();
            if !matches!(snapshot.status.as_str(), "ready" | "idle" | "interrupted") || !snapshot.pending_ui.is_empty() {
                return Err(HostError::new("task_busy", "等待当前生成或审批结束后再切换审批模式"));
            }
            let state = self.runtime.query(id, "get_state", json!({})).await?;
            // Unknown state must not be interpreted as idle at this permission boundary.
            let current = self.runtime.snapshot(id).await?;
            if state["isStreaming"] != false || state["isCompacting"] != false || state["queuedMessageCount"] != 0
                || !current.pending_ui.is_empty() || current.tools.iter().any(|tool| tool.status == "running") {
                return Err(HostError::new("task_busy", "任务仍在生成、压缩或有排队消息，无法切换审批模式"));
            }
            if task.session_file.as_ref().is_none_or(|file| !file.is_file()) {
                return Err(HostError::new("session_not_saved", "会话尚未落盘，请在首次回复完成后切换审批模式"));
            }
            self.store.validate_task_roots(id).await?;
            let header = session_header(task.session_file.as_ref().unwrap())?;
            if header["id"].as_str() != task.session_id.as_deref() {
                return Err(HostError::new("session_mismatch", "Bound session identity changed"));
            }
            self.runtime.stop(id).await?;
            self.store.end_run(&snapshot.run_id, "stopped", None).await?;
            self.runtime.forget_stopped(id).await?;
        }
        // Persist the requested policy before starting. On failure, the old process
        // stays stopped and retries use the new policy, never a silent rollback.
        self.store.set_setting(&format!("task_approval:{id}"), mode.unwrap_or_default()).await?;
        if alive {
            self.continue_locked(id).await.map_err(|e| HostError::new(
                "approval_switch_failed", format!("审批模式已保存，但 OMP 重启失败（{}）。修复后继续任务，新模式将在启动时应用。", e.code)
            ))?;
        }
        self.store.task(id).await
    }
    pub async fn request(
        &self,
        id: &str,
        typ: &'static str,
        payload: Value,
    ) -> Result<TaskSnapshot> {
        let _guard = self.gate.lock().await;
        if typ == "prompt" {
            self.check_handoff(id).await?;
            let state = self.runtime.query(id, "get_state", json!({})).await?;
            if !state["model"].is_object() {
                return Err(HostError::new(
                    "model_required",
                    "Configure and select an available OMP model",
                ));
            }
        }
        self.runtime.request(id, typ, payload.clone()).await?;
        if typ == "prompt" {
            if let Some(message) = payload["message"].as_str() {
                self.schedule_auto_title(id, message.to_owned()).await;
            }
        }
        self.runtime.snapshot(id).await
    }

    async fn schedule_auto_title(&self, id: &str, message: String) {
        let task = match self.store.task(id).await {
            Ok(task) if task.title_source == "initial" => task,
            _ => return,
        };
        if crate::auto_title::low_signal(&message) { return; }
        let mut pending = self.auto_titles.lock().await;
        if !pending.insert(id.to_owned()) { return; }
        let workbench = self.clone();
        let id = id.to_owned();
        tokio::spawn(async move {
            if let Err(error) = workbench.generate_auto_title(&id, &task.roots, message).await {
                eprintln!("auto title unavailable for task {} ({})", id, error.code);
            }
            workbench.auto_titles.lock().await.remove(&id);
        });
    }

    async fn generate_auto_title(&self, id: &str, roots: &[PathBuf], message: String) -> Result<()> {
        let task = self.store.task(id).await?;
        if task.title_source != "initial" || task.session_id.is_none() { return Ok(()); }
        let snapshot = self.runtime.snapshot(id).await?;
        if matches!(snapshot.status.as_str(), "failed" | "stopped") { return Ok(()); }
        let runtime_session = snapshot.runtime.capabilities["state"]["sessionId"].as_str();
        if runtime_session != task.session_id.as_deref() { return Err(HostError::new("session_mismatch", "OMP session changed before title update")); }
        let Some(root) = roots.first() else { return Ok(()); };
        let model = snapshot.runtime.capabilities["state"]["model"].as_object().and_then(|model| Some(format!("{}/{}", model.get("provider")?.as_str()?, model.get("id")?.as_str()?)));
        let title_prompt = self.title_prompt_settings().await?.prompt;
        let Some(title) = crate::auto_title::generate(Path::new(&snapshot.runtime.executable), root, model.as_deref(), &message, &title_prompt).await? else { return Ok(()); };

        let _guard = self.gate.lock().await;
        let latest = self.store.task(id).await?;
        if latest.title_source != "initial" { return Ok(()); }
        let current = self.runtime.snapshot(id).await?;
        if matches!(current.status.as_str(), "failed" | "stopped") { return Ok(()); }
        if current.runtime.capabilities["state"]["sessionId"].as_str() != latest.session_id.as_deref() {
            return Err(HostError::new("session_mismatch", "OMP session changed before title update"));
        }
        self.runtime.request(id, "set_session_name", json!({"name": title})).await?;
        if self.store.update_auto_title(id, &title).await? {
            self.runtime.publish_event(id, "task_title_update", json!({"title":title,"source":"auto"})).await?;
        }
        Ok(())
    }
    pub async fn suggest_task_title(&self, id: &str) -> Result<String> {
        let task = self.store.task(id).await?;
        if task.archived { return Err(HostError::new("task_archived", "请先恢复归档任务，再生成标题")); }
        let roots = self.store.validate_task_roots(id).await?;
        let file = task.session_file.clone().ok_or_else(|| HostError::new("title_context_missing", "当前任务还没有可用于生成标题的会话内容"))?;
        let session_id = task.session_id.clone().ok_or_else(|| HostError::new("title_context_missing", "当前任务还没有可用于生成标题的会话内容"))?;
        let context = tokio::task::spawn_blocking(move || {
            let history = crate::trajectory_history::read(&file, &session_id)?;
            crate::auto_title::conversation_context(&history).ok_or_else(|| HostError::new("title_context_missing", "当前会话没有足够的有效对话内容来生成标题"))
        }).await.map_err(|e| HostError::new("session_invalid", e))??;
        let live = self.runtime.snapshot(id).await.ok().filter(|snapshot| matches!(snapshot.status.as_str(), "starting" | "ready" | "idle" | "running" | "interrupted"));
        let (executable, model) = if let Some(snapshot) = live {
            let model = snapshot.runtime.capabilities["state"]["model"].as_object().and_then(|model| Some(format!("{}/{}", model.get("provider")?.as_str()?, model.get("id")?.as_str()?))).or(task.model.clone());
            (PathBuf::from(snapshot.runtime.executable), model)
        } else {
            let saved = self.store.executable().await?;
            (crate::resolve_executable(None, saved.as_deref().map(Path::new))?, task.model.clone())
        };
        let root = roots
            .first()
            .ok_or_else(|| HostError::new("invalid_workspace", "任务没有工作目录"))?;
        let title_prompt = self.title_prompt_settings().await?.prompt;
        crate::auto_title::generate_context(&executable, root, model.as_deref(), &context, &title_prompt).await?
            .ok_or_else(|| HostError::new("title_unavailable", "OMP 未能从当前会话生成有效标题，请重试或手工输入"))
    }
    pub async fn history(&self, id: &str, cursor: Option<String>) -> Result<Value> {
        // A stopped task has no in-memory runtime, but its OMP session remains the
        // source of truth. Only use RPC while that task is actually alive.
        let task = self.store.task(id).await?;
        let runtime_alive = self.runtime.snapshot(id).await.map_or(false, |snapshot| {
            matches!(snapshot.status.as_str(), "starting" | "ready" | "idle" | "running" | "interrupted")
        });
        if !runtime_alive {
            return self.stored_history(&task, cursor).await;
        }
        let mut request = json!({"limit":64});
        if let Some(cursor) = cursor {
            request["cursor"] = Value::String(cursor);
        }
        self.runtime.query(id, "get_messages_page", request).await
    }

    async fn stored_history(&self, task: &TaskRecord, cursor: Option<String>) -> Result<Value> {
        let Some(file) = task.session_file.clone() else {
            return Ok(json!({"messages": [], "totalMessages": 0, "nextCursor": null}));
        };
        let expected = task.session_id.clone();
        let offset = cursor
            .as_deref()
            .map(|value| {
                value.parse::<usize>().map_err(|_| {
                    HostError::new("stale_cursor", "Invalid persisted history cursor")
                })
            })
            .transpose()?;
        tokio::task::spawn_blocking(move || {
            let header = session_header(&file)?;
            if header["id"].as_str() != expected.as_deref() {
                return Err(HostError::new(
                    "session_mismatch",
                    "Bound session identity changed",
                ));
            }
            let reader = BufReader::new(
                std::fs::File::open(&file).map_err(|e| HostError::new("session_missing", e))?,
            );
            let mut messages = Vec::new();
            for line in reader.lines().skip(1) {
                let line = line.map_err(|e| HostError::new("session_invalid", e))?;
                let entry: Value = serde_json::from_str(&line)
                    .map_err(|e| HostError::new("session_invalid", e))?;
                if entry["type"] == "message" {
                    if let Some(message) = entry.get("message") {
                        messages.push(message.clone());
                    }
                }
            }
            let start = offset.unwrap_or(0).min(messages.len());
            let end = (start + 64).min(messages.len());
            let next = (end < messages.len()).then(|| end.to_string());
            Ok(json!({
                "messages": messages[start..end].to_vec(),
                "totalMessages": messages.len(),
                "nextCursor": next,
            }))
        })
        .await
        .map_err(|e| HostError::new("session_invalid", e))?
    }
    /// Refreshes only usage fields OMP actually reports. It never estimates a missing value.
    pub async fn refresh_usage(&self, id: &str) -> Result<TaskSnapshot> {
        let _guard = self.gate.lock().await;
        self.runtime.query(id, "get_state", json!({})).await?;
        self.runtime
            .query(id, "get_session_stats", json!({}))
            .await?;
        self.runtime.snapshot(id).await
    }
    async fn git_root(&self, id: &str, root_index: usize) -> Result<PathBuf> {
        let roots = self.store.validate_task_roots(id).await?;
        roots.get(root_index).cloned().ok_or_else(|| {
            HostError::new(
                "invalid_git_root",
                "Selected directory is not part of this task",
            )
        })
    }
    pub async fn git_status(&self, id: &str, root_index: usize) -> Result<GitStatus> {
        let _guard = self.gate.lock().await;
        let root = self.git_root(id, root_index).await?;
        git::status(&root, root_index).await
    }
    pub async fn git_diff(
        &self,
        id: &str,
        root_index: usize,
        path: &str,
        staged: bool,
        untracked: bool,
    ) -> Result<GitDiff> {
        let _guard = self.gate.lock().await;
        let root = self.git_root(id, root_index).await?;
        git::diff(&root, root_index, path, staged, untracked).await
    }

    /// Thinking is a launch option: an existing process must be stopped first.
    pub async fn select_thinking(&self, id: &str, level: Option<String>) -> Result<()> {
        let _guard = self.gate.lock().await;
        self.check_handoff(id).await?;
        let task = self.store.task(id).await?;
        if task.archived { return Err(HostError::new("task_archived", "请先恢复归档任务")); }
        if self.runtime.snapshot(id).await.is_ok_and(|s| matches!(s.status.as_str(), "starting" | "ready" | "idle" | "running" | "interrupted")) {
            return Err(HostError::new("task_running", "请先停止任务进程，再修改推理强度；下次启动生效"));
        }
        if let Some(value) = &level { self.validate_thinking(&task, value).await?; }
        self.store.set_task_thinking(id, level).await
    }

    pub(crate) async fn validate_thinking(&self, task: &TaskRecord, level: &str) -> Result<()> {
        let roots = self.store.validate_task_roots(&task.id).await?;
        let executable = self.store.executable().await?;
        let path = crate::resolve_executable(None, executable.as_deref().map(Path::new))?;
        crate::probe(&path).await?;
        let available = crate::model_config::discover(&path, &roots[0]).await?;
        let model = available.models.iter().find(|m| task.model.as_deref() == Some(format!("{}/{}", m["provider"].as_str().unwrap_or(""), m["id"].as_str().unwrap_or("")).as_str()))
            .ok_or_else(|| HostError::new("model_unavailable", "请先选择可用模型"))?;
        crate::model_config::validate_thinking(model, level)
    }

    pub async fn select_model(&self, id: &str, provider: &str, model_id: &str) -> Result<Value> {
        let _guard = self.gate.lock().await;
        self.check_handoff(id).await?;
        let runtime_alive = self.runtime.snapshot(id).await.map_or(false, |snapshot| {
            matches!(snapshot.status.as_str(), "starting" | "ready" | "idle" | "running" | "interrupted")
        });
        let target = if runtime_alive {
            let snapshot = self.runtime.snapshot(id).await?;
            snapshot.runtime.capabilities["models"].as_array().into_iter().flatten().find(|m| m["provider"] == provider && m["id"] == model_id).cloned()
        } else {
            let roots = self.store.validate_task_roots(id).await?;
            let executable = self.store.executable().await?;
            let path = crate::resolve_executable(None, executable.as_deref().map(Path::new))?;
            crate::probe(&path).await?;
            crate::model_config::discover(&path, &roots[0]).await?.models.into_iter().find(|m| m["provider"] == provider && m["id"] == model_id)
        }.ok_or_else(|| HostError::new("model_unavailable", "所选模型已不可用"))?;
        let clear_thinking = self.store.task_thinking(id).await?.is_some_and(|level| crate::model_config::validate_thinking(&target, &level).is_err());
        if runtime_alive {
            self.runtime
                .query(
                    id,
                    "set_model",
                    json!({"provider":provider,"modelId":model_id}),
                )
                .await?;
            let state = self.runtime.query(id, "get_state", json!({})).await?;
            if state["model"]["provider"] != provider || state["model"]["id"] != model_id {
                return Err(HostError::new(
                    "model_unavailable",
                    "OMP model selection mismatch",
                ));
            }
            self.store.save_model_selection_with_thinking(id, format!("{provider}/{model_id}"), clear_thinking).await?;
            return Ok(state);
        }
        self.store.save_model_selection_with_thinking(id, format!("{provider}/{model_id}"), clear_thinking).await?;
        Ok(json!({"model":{"provider":provider,"id":model_id}}))
    }
    pub async fn recover_session(&self, id: &str, confirmed: bool) -> Result<String> {
        let _guard = self.gate.lock().await;
        let task = self.store.task(id).await?;
        if task.session_id.is_some() {
            return Err(HostError::new(
                "session_mismatch",
                "Task already has a bound session",
            ));
        }
        let roots = self.store.validate_task_roots(id).await?;
        let dir = self.data.join("sessions").join(id);
        let candidate = tokio::task::spawn_blocking(move || -> Result<(String, PathBuf)> {
            let mut files = Vec::new();
            for (index, entry) in std::fs::read_dir(&dir)
                .map_err(|e| HostError::new("session_missing", e))?
                .take(65)
                .enumerate()
            {
                if index == 64 {
                    return Err(HostError::new(
                        "session_recovery_required",
                        "Too many entries for bounded recovery",
                    ));
                }
                let entry = entry.map_err(|e| HostError::new("session_invalid", e))?;
                if entry.path().extension().is_some_and(|v| v == "jsonl") {
                    files.push(entry.path());
                }
            }
            if files.len() != 1 {
                return Err(HostError::new(
                    "session_recovery_required",
                    "Recovery requires exactly one session candidate; no file was selected",
                ));
            }
            let file = files.pop().unwrap();
            let header = session_header(&file)?;
            let cwd = PathBuf::from(header["cwd"].as_str().unwrap());
            if cwd
                .canonicalize()
                .map_err(|e| HostError::new("directory_missing", e))?
                != roots[0]
            {
                return Err(HostError::new(
                    "session_mismatch",
                    "Candidate belongs to another workspace",
                ));
            }
            let additional: Vec<PathBuf> = serde_json::from_value(
                header
                    .get("additionalDirectories")
                    .cloned()
                    .unwrap_or(json!([])),
            )
            .map_err(|e| HostError::new("session_invalid", e))?;
            let mut candidate_roots = vec![cwd];
            candidate_roots.extend(additional);
            if crate::workspace::validate_roots(&candidate_roots)? != roots {
                return Err(HostError::new(
                    "session_mismatch",
                    "Candidate additional directories do not match this task",
                ));
            }
            Ok((header["id"].as_str().unwrap().into(), file))
        })
        .await
        .map_err(|e| HostError::new("session_invalid", e))??;
        if confirmed {
            self.store.bind(id, &candidate.0, candidate.1).await?;
        }
        Ok(candidate.0)
    }
    pub async fn update_task(
        &self,
        id: &str,
        title: &str,
        pinned: bool,
        archived: bool,
    ) -> Result<TaskRecord> {
        let _guard = self.gate.lock().await;
        if archived {
            self.check_handoff(id).await?;
            if let Ok(s) = self.runtime.snapshot(id).await {
                if !matches!(s.status.as_str(), "stopped" | "failed") {
                    return Err(HostError::new("task_busy", "Stop task before archiving"));
                }
            }
        }
        let title_changed = self.store.task(id).await?.title != title.trim();
        self.store.update_task(id, title, pinned, archived).await?;
        if title_changed {
            if let Ok(snapshot) = self.runtime.snapshot(id).await {
                if !matches!(snapshot.status.as_str(), "stopped" | "failed") {
                    // SQLite remains authoritative for the product list; synchronize the
                    // live OMP session when its RPC process is available.
                    let _ = self
                        .runtime
                        .request(id, "set_session_name", json!({"name": title.trim()}))
                        .await;
                }
            }
        }
        self.store.task(id).await
    }
    pub async fn relocate_task(
        &self,
        id: &str,
        roots: Vec<PathBuf>,
        trusted: bool,
    ) -> Result<TaskRecord> {
        let _guard = self.gate.lock().await;
        self.check_handoff(id).await?;
        if let Ok(snapshot) = self.runtime.snapshot(id).await {
            if !matches!(snapshot.status.as_str(), "stopped" | "failed") {
                return Err(HostError::new(
                    "task_busy",
                    "Stop task before relocating its directory",
                ));
            }
            self.runtime.forget_stopped(id).await?;
        }
        self.store.relocate_task(id, roots, trusted).await
    }
    pub async fn shutdown(&self) -> Result<()> {
        let _guard = self.gate.lock().await;
        let mut error = None;
        for task in self.runtime.snapshots().await {
            if let Err(e) = self.stop_locked(&task.task_id).await {
                error = Some(e);
            }
        }
        if let Err(e) = self.terminals.close_all().await {
            error = Some(e);
        }
        error.map_or(Ok(()), Err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[tokio::test]
    async fn suggested_title_reads_bound_history_without_mutating_task() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("omp-title-suggestion-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let executable = root.join("fake-omp");
        std::fs::write(&executable, "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$0.args\"\nprintf '<title>Review session export</title>\\n'\n").unwrap();
        let mut permissions = std::fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&executable, permissions).unwrap();
        let w = Workbench::open(root.join("data")).await.unwrap();
        w.store.set_executable(executable.to_string_lossy().into_owned()).await.unwrap();
        let project = w.store.register_project("P", vec![root.clone()], true).await.unwrap();
        let task = w.store.create_task_with_model(&project.id, "Original", Some("test/model".into())).await.unwrap();
        w.save_title_prompt(Some("Custom session title prompt".into())).await.unwrap();
        let file = root.join("session.jsonl");
        let header = json!({"type":"session","version":3,"id":"session","cwd":root});
        let user = json!({"type":"message","id":"u","parentId":null,"message":{"role":"user","content":"评估会话导出逻辑"}});
        let assistant = json!({"type":"message","id":"a","parentId":"u","message":{"role":"assistant","content":[{"type":"text","text":"我已经检查了持久化实现"}]}});
        std::fs::write(&file, format!("{header}\n{user}\n{assistant}\n")).unwrap();
        w.store.bind(&task.id, "session", file).await.unwrap();
        assert_eq!(w.suggest_task_title(&task.id).await.unwrap(), "Review session export");
        let unchanged = w.store.task(&task.id).await.unwrap();
        assert_eq!(unchanged.title, "Original");
        assert_eq!(unchanged.title_source, "user");
        assert!(w.runtime.snapshots().await.is_empty());
        let arguments = std::fs::read_to_string(executable.with_extension("args")).unwrap();
        assert!(arguments.lines().any(|argument| argument == "Custom session title prompt"));
        assert!(arguments.lines().any(|argument| argument == "test/model"));
        assert!(arguments.contains("<chat>\n<user>\n评估会话导出逻辑\n</user>"));
        assert!(arguments.contains("<assistant>\n我已经检查了持久化实现\n</assistant>\n</chat>"));
    }
    #[tokio::test]
    async fn title_suggestion_requires_matching_session_history() {
        let root = std::env::temp_dir().join(format!("omp-title-context-errors-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let w = Workbench::open(root.join("data")).await.unwrap();
        let project = w.store.register_project("P", vec![root.clone()], true).await.unwrap();
        let missing = w.store.create_task(&project.id, "Missing").await.unwrap();
        assert_eq!(w.suggest_task_title(&missing.id).await.unwrap_err().code, "title_context_missing");
        let mismatch = w.store.create_task(&project.id, "Mismatch").await.unwrap();
        let file = root.join("mismatch.jsonl");
        std::fs::write(&file, format!("{}\n{}\n", json!({"type":"session","version":3,"id":"actual","cwd":root}), json!({"type":"message","id":"u","message":{"role":"user","content":"meaningful request"}}))).unwrap();
        w.store.bind(&mismatch.id, "expected", file).await.unwrap();
        assert_eq!(w.suggest_task_title(&mismatch.id).await.unwrap_err().code, "session_mismatch");
    }
    #[tokio::test]
    async fn title_prompt_settings_save_and_restore_official_default() {
        let root = std::env::temp_dir().join(format!("omp-title-prompt-settings-{}", uuid::Uuid::new_v4()));
        let w = Workbench::open(root.join("data")).await.unwrap();
        let initial = w.title_prompt_settings().await.unwrap();
        assert!(initial.is_default);
        assert_eq!(initial.prompt, crate::auto_title::DEFAULT_TITLE_SYSTEM_PROMPT);

        let custom = w.save_title_prompt(Some("  Generate a concise title.  ".into())).await.unwrap();
        assert!(!custom.is_default);
        assert_eq!(custom.prompt, "Generate a concise title.");
        assert_eq!(
            w.save_title_prompt(Some("   ".into()))
                .await
                .unwrap_err()
                .code,
            "title_prompt_invalid"
        );
        assert_eq!(
            w.save_title_prompt(Some("invalid\0prompt".into()))
                .await
                .unwrap_err()
                .code,
            "title_prompt_invalid"
        );

        let restored = w.save_title_prompt(None).await.unwrap();
        assert!(restored.is_default);
        assert_eq!(restored.prompt, crate::auto_title::DEFAULT_TITLE_SYSTEM_PROMPT);
    }
    #[tokio::test]
    async fn recovery_requires_unique_valid_header_and_explicit_confirmation() {
        let root = std::env::temp_dir().join(format!("omp-recovery-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let w = Workbench::open(root.join("data")).await.unwrap();
        let project = w
            .store
            .register_project("P", vec![root.clone()], true)
            .await
            .unwrap();
        let task = w.store.create_task(&project.id, "T").await.unwrap();
        let dir = root.join("data/sessions").join(&task.id);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("candidate.jsonl");
        std::fs::write(&file, "not a session\n").unwrap();
        assert_eq!(
            w.recover_session(&task.id, false).await.unwrap_err().code,
            "session_invalid"
        );
        let header = json!({"type":"session","version":3,"id":"candidate","cwd":root});
        let title = json!({"type":"title","v":1,"title":"T","updatedAt":"now","pad":""});
        std::fs::write(&file, format!("{title}\n{header}\n")).unwrap();
        assert_eq!(
            w.recover_session(&task.id, false).await.unwrap(),
            "candidate"
        );
        assert!(w.store.task(&task.id).await.unwrap().session_id.is_none());
        let other = dir.join("other.jsonl");
        std::fs::write(&other, format!("{header}\n")).unwrap();
        assert_eq!(
            w.recover_session(&task.id, true).await.unwrap_err().code,
            "session_recovery_required"
        );
        std::fs::rename(other, root.join("retained-other.jsonl")).unwrap();
        assert_eq!(
            w.recover_session(&task.id, true).await.unwrap(),
            "candidate"
        );
        assert_eq!(
            w.store.task(&task.id).await.unwrap().session_id.as_deref(),
            Some("candidate")
        );
        assert!(w.runtime.snapshots().await.is_empty());
    }
    #[tokio::test]
    async fn stopped_task_reads_bound_session_without_runtime() {
        let root = std::env::temp_dir().join(format!("omp-offline-history-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let w = Workbench::open(root.join("data")).await.unwrap();
        let project = w.store.register_project("P", vec![root.clone()], true).await.unwrap();
        let task = w.store.create_task(&project.id, "T").await.unwrap();
        let dir = root.join("data/sessions").join(&task.id);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("session.jsonl");
        let header = json!({"type":"session","version":3,"id":"offline","cwd":root});
        let user = json!({"type":"message","id":"u","parentId":null,"timestamp":"now","message":{"role":"user","content":"hello"}});
        let assistant = json!({"type":"message","id":"a","parentId":"u","timestamp":"now","message":{"role":"assistant","content":"world"}});
        std::fs::write(&file, format!("{header}\n{user}\n{assistant}\n")).unwrap();
        w.store.bind(&task.id, "offline", file).await.unwrap();
        let page = w.history(&task.id, None).await.unwrap();
        assert_eq!(page["totalMessages"], 2);
        assert_eq!(page["messages"][0]["content"], "hello");
        assert_eq!(page["messages"][1]["content"], "world");
        assert!(w.runtime.snapshots().await.is_empty());
    }
    #[test]
    fn rejects_unknown_session_format_and_file_symlink() {
        let root = std::env::temp_dir().join(format!("omp-header-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("session.jsonl");
        std::fs::write(
            &file,
            "{\"type\":\"session\",\"version\":99,\"id\":\"future\",\"cwd\":\"/tmp\"}\n",
        )
        .unwrap();
        assert_eq!(
            session_header(&file).unwrap_err().code,
            "session_incompatible"
        );
        #[cfg(unix)]
        {
            let link = root.join("link");
            std::os::unix::fs::symlink(file, &link).unwrap();
            assert_eq!(session_header(&link).unwrap_err().code, "session_invalid");
        }
    }
}
