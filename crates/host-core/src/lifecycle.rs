use crate::{
    HostError, RpcQuery, RpcRequest, TaskSnapshot, Workbench, runtime::Result, writer_lock,
};
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct Lifecycle {
    pub(crate) focused: Option<String>,
    entries: HashMap<String, Entry>,
}
#[derive(Default)]
struct Entry {
    keep: bool,
    suppressed: bool,
    idle: Option<u64>,
    error: Option<HostError>,
    handoff_until: u64,
    reason: Option<String>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeTaskInfo {
    pub task_id: String,
    pub run_id: Option<String>,
    pub status: String,
    pub owner: String,
    pub pid: Option<u32>,
    pub started_at: Option<u64>,
    pub idle_since: Option<u64>,
    pub keep_alive: bool,
    pub auto_start_suppressed: bool,
    pub executable: Option<String>,
    pub version: Option<String>,
    pub error: Option<HostError>,
    pub reason: Option<String>,
}
#[derive(Serialize)]
pub struct RuntimeCommand {
    pub command: String,
    pub executable: String,
    pub arguments: Vec<String>,
}
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
impl Workbench {
    pub(crate) async fn clear_manual_stop(&self, id: &str) {
        let mut state = self.lifecycle.lock().await;
        let entry = state.entries.entry(id.into()).or_default();
        entry.suppressed = false;
        entry.error = None;
    }
    pub async fn runtime_command(&self, id: &str) -> Result<RuntimeCommand> {
        let _guard = self.gate.lock().await;
        self.runtime_command_locked(id).await
    }
    async fn runtime_command_locked(&self, id: &str) -> Result<RuntimeCommand> {
        let task = self.store.task(id).await?;
        if let Some(level) = &task.thinking {
            self.validate_thinking(&task, level).await?;
        }
        if task.archived {
            return Err(HostError::new("task_archived", "请先恢复归档任务"));
        }
        let roots = self.store.validate_task_roots(id).await?;
        let file = task
            .session_file
            .ok_or_else(|| HostError::new("session_not_saved", "会话尚未落盘"))?;
        if crate::session::session_header(&file)?["id"].as_str() != task.session_id.as_deref() {
            return Err(HostError::new("session_mismatch", "会话标识不匹配"));
        }
        let explicit = self.store.setting("executable").await?;
        let executable = crate::runtime::resolve_executable(
            explicit.as_deref().map(std::path::Path::new),
            None,
        )?
        .to_string_lossy()
        .into_owned();
        let mut arguments = vec!["--cwd".into(), roots[0].to_string_lossy().into_owned()];
        for root in &roots[1..] {
            arguments.extend(["--add-dir".into(), root.to_string_lossy().into_owned()]);
        }
        arguments.extend([
            "--session-dir".into(),
            file.parent().unwrap().to_string_lossy().into_owned(),
            "--resume".into(),
            file.to_string_lossy().into_owned(),
        ]);
        if let Some(mode) = task.approval_mode {
            arguments.extend(["--approval-mode".into(), mode]);
        }
        let snapshot = self.runtime.snapshot(id).await.ok();
        let state = if snapshot.as_ref().is_some_and(|s| {
            matches!(
                s.status.as_str(),
                "ready" | "idle" | "interrupted" | "running"
            )
        }) {
            self.runtime
                .query(id, RpcQuery::GetState, json!({}))
                .await?
        } else {
            snapshot.as_ref().map_or(serde_json::Value::Null, |s| {
                s.runtime.capabilities["state"].clone()
            })
        };
        let model = match (
            state["model"]["provider"].as_str(),
            state["model"]["id"].as_str(),
        ) {
            (Some(provider), Some(model)) => Some(format!("{provider}/{model}")),
            _ => task.model,
        };
        if let Some(model) = model {
            arguments.extend(["--model".into(), model]);
        }
        if let Some(thinking) = task
            .thinking
            .as_deref()
            .or_else(|| state["thinkingLevel"].as_str())
        {
            arguments.extend(["--thinking".into(), thinking.into()]);
        }
        let app = std::env::current_exe().map_err(|e| HostError::new("executable_missing", e))?;
        let mut argv = vec![
            app.to_string_lossy().into_owned(),
            "session".into(),
            "open".into(),
            "--data".into(),
            self.data.to_string_lossy().into_owned(),
            "--task".into(),
            id.into(),
            "--".into(),
            executable.clone(),
        ];
        argv.extend(arguments.clone());
        Ok(RuntimeCommand {
            command: argv.iter().map(|s| quote(s)).collect::<Vec<_>>().join(" "),
            executable,
            arguments,
        })
    }
    pub async fn handoff(
        &self,
        id: &str,
        executable: &str,
        arguments: &[String],
    ) -> Result<std::collections::BTreeMap<String, String>> {
        let _guard = self.gate.lock().await;
        self.check_handoff(id).await?;
        let command = self.runtime_command_locked(id).await?;
        if command.executable != executable || command.arguments != arguments {
            return Err(HostError::new(
                "stale_command",
                "任务配置已变化，请重新复制续接命令",
            ));
        }
        if let Ok(s) = self.runtime.snapshot(id).await {
            if !matches!(s.status.as_str(), "failed" | "stopped") {
                self.safe_idle(id).await?;
            }
        }
        let mut environment = std::collections::BTreeMap::new();
        for (key, value) in std::env::vars_os() {
            environment.insert(
                key.into_string().map_err(|_| {
                    HostError::new("environment_invalid", "OMP 环境变量不是有效 UTF-8")
                })?,
                value.into_string().map_err(|_| {
                    HostError::new("environment_invalid", "OMP 环境变量不是有效 UTF-8")
                })?,
            );
        }
        {
            let mut state = self.lifecycle.lock().await;
            let entry = state.entries.entry(id.into()).or_default();
            entry.suppressed = true;
            entry.reason = Some("已交给外部终端".into());
            entry.handoff_until = writer_lock::now() + 35_000;
        }
        if let Ok(s) = self.runtime.snapshot(id).await {
            if !matches!(s.status.as_str(), "failed" | "stopped") {
                self.stop_omp_only(id).await?;
            }
        }
        Ok(environment)
    }
    pub(crate) async fn check_handoff(&self, id: &str) -> Result<()> {
        self.ensure_desktop_owner(id)?;
        if self
            .lifecycle
            .lock()
            .await
            .entries
            .get(id)
            .is_some_and(|e| e.handoff_until > writer_lock::now())
        {
            return Err(HostError::new(
                "session_owned",
                "会话正在交给终端，请稍后重试",
            ));
        }
        Ok(())
    }
    pub(crate) fn ensure_desktop_owner(&self, id: &str) -> Result<()> {
        if writer_lock::owner(&self.data, id)?.is_some_and(|o| o.owner != "desktop") {
            return Err(HostError::new(
                "session_owned",
                "会话由终端接管，退出终端 OMP 后再继续",
            ));
        }
        Ok(())
    }
    pub async fn runtime_tasks(&self) -> Result<Vec<RuntimeTaskInfo>> {
        let records = self.store.tasks().await?;
        let snapshots = self.runtime.snapshots().await;
        let mut state = self.lifecycle.lock().await;
        let mut out = Vec::new();
        for task in records {
            let snapshot = snapshots.iter().find(|s| s.task_id == task.id);
            let owner = writer_lock::owner(&self.data, &task.id)?;
            let entry = state.entries.entry(task.id.clone()).or_default();
            if owner.as_ref().is_some_and(|o| o.owner == "terminal") {
                entry.handoff_until = 0;
            }
            let status = if owner.as_ref().is_some_and(|o| o.owner == "terminal") {
                "external"
            } else {
                snapshot.map_or(
                    if entry.error.is_some() {
                        "failed"
                    } else {
                        "stopped"
                    },
                    |s| s.status.as_str(),
                )
            };
            if matches!(status, "ready" | "idle" | "interrupted") {
                entry.idle.get_or_insert_with(writer_lock::now);
            } else {
                entry.idle = None;
            }
            out.push(RuntimeTaskInfo {
                task_id: task.id,
                run_id: snapshot.map(|s| s.run_id.clone()),
                status: status.into(),
                owner: owner.as_ref().map_or("none", |o| o.owner.as_str()).into(),
                pid: owner.as_ref().and_then(|o| o.pid),
                started_at: owner.map(|o| o.started_at),
                idle_since: entry.idle,
                keep_alive: entry.keep,
                auto_start_suppressed: entry.suppressed,
                executable: snapshot.map(|s| s.runtime.executable.clone()),
                version: snapshot.and_then(|s| s.runtime.version.clone()),
                error: entry
                    .error
                    .clone()
                    .or_else(|| snapshot.and_then(|s| s.error.clone())),
                reason: entry.reason.clone(),
            });
        }
        Ok(out)
    }
    pub async fn runtime_focus(&self, id: Option<String>) -> Result<()> {
        {
            let mut state = self.lifecycle.lock().await;
            if state.focused != id {
                if let Some(previous) = state.focused.clone() {
                    state.entries.entry(previous).or_default().idle = Some(writer_lock::now());
                }
                state.focused = id.clone();
            }
        }
        let Some(id) = id else { return Ok(()) };
        let _guard = self.gate.lock().await;
        {
            let state = self.lifecycle.lock().await;
            if state.focused.as_deref() != Some(&id)
                || state
                    .entries
                    .get(&id)
                    .is_some_and(|e| e.suppressed || e.error.is_some())
            {
                return Ok(());
            }
        }
        if writer_lock::owner(&self.data, &id)?.is_some_and(|o| o.owner != "desktop") {
            return Ok(());
        }
        self.lifecycle
            .lock()
            .await
            .entries
            .entry(id.clone())
            .or_default()
            .reason = Some("进入任务，连接 OMP".into());
        let result = self.continue_locked(&id).await;
        if let Err(error) = &result {
            self.lifecycle
                .lock()
                .await
                .entries
                .entry(id)
                .or_default()
                .error = Some(error.clone());
        }
        result.map(|_| ())
    }
    pub async fn runtime_keep_alive(&self, id: &str, keep: bool) -> Result<()> {
        self.store.task(id).await?;
        self.lifecycle
            .lock()
            .await
            .entries
            .entry(id.into())
            .or_default()
            .keep = keep;
        Ok(())
    }
    pub(crate) async fn safe_idle(&self, id: &str) -> Result<()> {
        let snapshot = self.runtime.snapshot(id).await?;
        if !matches!(snapshot.status.as_str(), "ready" | "idle" | "interrupted")
            || !snapshot.pending_ui.is_empty()
        {
            return Err(HostError::new("task_busy", "任务仍在运行或等待审批"));
        }
        let state = self
            .runtime
            .query(id, RpcQuery::GetState, json!({}))
            .await?;
        let current = self.runtime.snapshot(id).await?;
        if state["isStreaming"] != false
            || state["isCompacting"] != false
            || state["queuedMessageCount"] != 0
            || !current.pending_ui.is_empty()
            || current.tools.iter().any(|t| t.status == "running")
        {
            return Err(HostError::new("task_busy", "任务仍有生成、压缩或排队消息"));
        }
        let task = self.store.task(id).await?;
        let file = task
            .session_file
            .ok_or_else(|| HostError::new("session_not_saved", "会话尚未落盘"))?;
        if crate::session::session_header(&file)?["id"].as_str() != task.session_id.as_deref() {
            return Err(HostError::new("session_mismatch", "会话标识不匹配"));
        }
        Ok(())
    }
    pub(crate) async fn stop_omp_only(&self, id: &str) -> Result<TaskSnapshot> {
        self.runtime.stop(id).await?;
        let s = self.runtime.snapshot(id).await?;
        self.store
            .end_run(
                &s.run_id,
                &s.status,
                s.error.as_ref().map(|e| e.code.clone()),
            )
            .await?;
        Ok(s)
    }
    pub(crate) async fn suppress_autostart(&self, id: &str) {
        self.lifecycle
            .lock()
            .await
            .entries
            .entry(id.into())
            .or_default()
            .suppressed = true;
    }
    pub async fn runtime_action(
        &self,
        id: &str,
        action: &str,
        expected: Option<String>,
    ) -> Result<RuntimeTaskInfo> {
        {
            let _guard = self.gate.lock().await;
            self.check_handoff(id).await?;
            let snapshot = self.runtime.snapshot(id).await.ok();
            if snapshot.as_ref().map(|s| s.run_id.clone()) != expected {
                return Err(HostError::new(
                    "stale_runtime",
                    "运行实例已变化，请刷新后重试",
                ));
            }
            match action {
                "start" | "restart" => {
                    if action == "restart"
                        && snapshot
                            .as_ref()
                            .is_some_and(|s| !matches!(s.status.as_str(), "stopped" | "failed"))
                    {
                        self.stop_omp_only(id).await?;
                    }
                    {
                        let mut state = self.lifecycle.lock().await;
                        let e = state.entries.entry(id.into()).or_default();
                        e.suppressed = false;
                        e.error = None;
                        e.reason = Some(
                            if action == "restart" {
                                "手动重启"
                            } else {
                                "手动启动"
                            }
                            .into(),
                        );
                    }
                    if let Err(error) = self.continue_locked(id).await {
                        self.lifecycle
                            .lock()
                            .await
                            .entries
                            .entry(id.into())
                            .or_default()
                            .error = Some(error.clone());
                        return Err(error);
                    }
                }
                "stop" => {
                    self.suppress_autostart(id).await;
                    self.stop_omp_only(id).await?;
                    self.lifecycle
                        .lock()
                        .await
                        .entries
                        .entry(id.into())
                        .or_default()
                        .reason = Some("手动停止".into());
                }
                "cancel" => {
                    self.runtime
                        .request(id, RpcRequest::Abort, json!({}))
                        .await?;
                }
                _ => return Err(HostError::new("invalid_action", "未知运行操作")),
            }
        }
        self.runtime_tasks()
            .await?
            .into_iter()
            .find(|r| r.task_id == id)
            .ok_or_else(|| HostError::new("task_missing", "任务不存在"))
    }
    pub async fn runtime_release_idle(&self, all: bool) -> Result<Vec<String>> {
        let _guard = self.gate.lock().await;
        let info = self.runtime_tasks().await?;
        let focused = self.lifecycle.lock().await.focused.clone();
        let mut idle: Vec<_> = info
            .into_iter()
            .filter(|i| {
                i.owner == "desktop"
                    && !i.keep_alive
                    && Some(&i.task_id) != focused.as_ref()
                    && i.idle_since.is_some()
            })
            .collect();
        idle.sort_by_key(|i| i.idle_since);
        let mut remaining = idle.len();
        let mut released = Vec::new();
        for i in idle {
            let over_capacity = remaining > 3;
            if !all
                && !over_capacity
                && writer_lock::now().saturating_sub(i.idle_since.unwrap()) < 600_000
            {
                continue;
            }
            match self.safe_idle(&i.task_id).await {
                Ok(()) => {}
                Err(e)
                    if matches!(
                        e.code.as_str(),
                        "task_busy" | "session_not_saved" | "session_missing"
                    ) =>
                {
                    continue;
                }
                Err(e) => return Err(e),
            }
            self.stop_omp_only(&i.task_id).await?;
            self.runtime.forget_stopped(&i.task_id).await?;
            remaining -= 1;
            self.lifecycle
                .lock()
                .await
                .entries
                .entry(i.task_id.clone())
                .or_default()
                .reason = Some(
                if all {
                    "手动释放后台空闲进程"
                } else if over_capacity {
                    "超过后台空闲进程保留上限"
                } else {
                    "后台空闲超过 10 分钟"
                }
                .into(),
            );
            released.push(i.task_id);
        }
        Ok(released)
    }
}
