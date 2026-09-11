use crate::git::{self, GitDiff, GitStatus};
use crate::runtime::Result;
use crate::store::{Store, TaskRecord};
use crate::terminal::TerminalService;
use crate::{HostError, LaunchOptions, TaskManager, TaskSnapshot};
use serde_json::{Value, json};
use std::{
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
}
// Only identity metadata is inspected here. OMP alone loads messages and resolves blobs.
fn session_header(file: &Path) -> Result<Value> {
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
    pub async fn continue_task(&self, id: &str) -> Result<TaskSnapshot> {
        let _guard = self.gate.lock().await;
        self.continue_locked(id).await
    }
    async fn continue_locked(&self, id: &str) -> Result<TaskSnapshot> {
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
        let run_id = uuid::Uuid::new_v4().to_string();
        let options = LaunchOptions {
            run_id: Some(run_id.clone()),
            root: roots[0].clone(),
            additional: roots[1..].to_vec(),
            model: if task.session_file.is_none() {
                task.model.clone()
            } else {
                None
            },
            session_dir: Some(session_dir),
            session_file: task.session_file.clone(),
            expected_session: task.session_id.clone(),
        };
        let executable = self.store.setting("executable").await?;
        self.store.begin_run(id, &run_id).await?;
        let initial = match self
            .runtime
            .start_with(id.into(), executable, options)
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
                    let state = self.runtime.query(id, "get_state", json!({})).await?;
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
                    return Ok(snapshot);
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
        self.runtime.stop(id).await?;
        self.terminals.close_task(id).await;
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
        if self.runtime.snapshot(id).await.is_ok() {
            self.stop_locked(id).await?;
            self.runtime.forget_stopped(id).await?;
        }
        self.continue_locked(id).await
    }
    pub async fn request(
        &self,
        id: &str,
        typ: &'static str,
        payload: Value,
    ) -> Result<TaskSnapshot> {
        let _guard = self.gate.lock().await;
        if typ == "prompt" {
            let state = self.runtime.query(id, "get_state", json!({})).await?;
            if !state["model"].is_object() {
                return Err(HostError::new(
                    "model_required",
                    "Configure and select an available OMP model",
                ));
            }
        }
        self.runtime.request(id, typ, payload).await?;
        self.runtime.snapshot(id).await
    }
    pub async fn history(&self, id: &str, cursor: Option<String>) -> Result<Value> {
        let mut request = json!({"limit":64});
        if let Some(cursor) = cursor {
            request["cursor"] = Value::String(cursor);
        }
        self.runtime.query(id, "get_messages_page", request).await
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

    pub async fn select_model(&self, id: &str, provider: &str, model_id: &str) -> Result<Value> {
        let _guard = self.gate.lock().await;
        self.runtime
            .query(
                id,
                "set_model",
                json!({"provider":provider,"modelId":model_id}),
            )
            .await?;
        let state = self.runtime.query(id, "get_state", json!({})).await?;
        self.store
            .set_model(id, Some(format!("{provider}/{model_id}")))
            .await?;
        Ok(state)
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
            if let Ok(s) = self.runtime.snapshot(id).await {
                if !matches!(s.status.as_str(), "stopped" | "failed") {
                    return Err(HostError::new("task_busy", "Stop task before archiving"));
                }
            }
        }
        self.store.update_task(id, title, pinned, archived).await?;
        self.store.task(id).await
    }
    pub async fn relocate_task(
        &self,
        id: &str,
        roots: Vec<PathBuf>,
        trusted: bool,
    ) -> Result<TaskRecord> {
        let _guard = self.gate.lock().await;
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
        self.terminals.close_all().await;
        error.map_or(Ok(()), Err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
