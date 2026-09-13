use crate::{
    HostError, Workbench, git,
    runtime::Result,
    store::{TaskRecord, TaskRoot},
};
use std::{collections::HashSet, path::PathBuf};

fn missing(message: impl std::fmt::Display) -> HostError {
    HostError::new("worktree_missing", message)
}

pub(crate) async fn verify(root: &TaskRoot) -> Result<()> {
    let path = root
        .worktree_path
        .as_ref()
        .ok_or_else(|| missing("Missing worktree registration"))?;
    let repo = root
        .git_top_level
        .as_ref()
        .ok_or_else(|| missing("Missing source repository"))?;
    let branch = root
        .branch
        .as_deref()
        .ok_or_else(|| missing("Missing task branch"))?;
    let relative = root
        .relative_path
        .as_ref()
        .ok_or_else(|| missing("Missing relative directory"))?;
    if !root.created_by_client
        || relative.is_absolute()
        || relative
            .components()
            .any(|p| matches!(p, std::path::Component::ParentDir))
    {
        return Err(missing("Invalid worktree ownership"));
    }
    let canonical = path.canonicalize().map_err(missing)?;
    if canonical != *path
        || root.execution_path != path.join(relative)
        || root.execution_path.canonicalize().map_err(missing)? != root.execution_path
        || !root.execution_path.is_dir()
    {
        return Err(missing(
            "执行目录已移动或被符号链接替换；恢复登记的目录后重新复核",
        ));
    }
    // HEAD may advance through commits. Baseline describes creation, not the current HEAD.
    if !git::worktree_matches(repo, path, branch).await? {
        return Err(missing(
            "Git worktree 登记或任务分支不匹配；恢复原路径及分支后重新复核",
        ));
    }
    Ok(())
}

impl Workbench {
    pub async fn create_task(&self, project: &str, title: &str, mode: &str) -> Result<TaskRecord> {
        self.create_task_with_model(project,title,mode,None).await
    }
    pub async fn create_task_with_model(&self, project: &str,title: &str,mode: &str,model: Option<String>) -> Result<TaskRecord> {
        if !matches!(mode, "shared" | "isolated") {
            return Err(HostError::new(
                "invalid_workspace_mode",
                "Unknown workspace mode",
            ));
        }
        let _guard = self.gate.lock().await;
        // Fail obvious non-Git/unborn cases before inserting a task.
        if mode == "isolated" {
            let project = self
                .store
                .projects()
                .await?
                .into_iter()
                .find(|p| p.id == project)
                .ok_or_else(|| HostError::new("project_missing", "Unknown project"))?;
            if !project.trusted {
                return Err(HostError::new(
                    "project_untrusted",
                    "Trust project before isolation",
                ));
            }
            for root in project.roots {
                let (repo, _) = git::repository_info(&root).await?.ok_or_else(|| {
                    HostError::new(
                        "worktree_unavailable",
                        "隔离任务要求所有执行目录均为 Git 仓库；可选择共享任务",
                    )
                })?;
                git::head(&repo).await?;
            }
        }
        let task = self.store.create_task_with_model(project, title, model).await?;
        if mode == "isolated" {
            self.isolate_locked(&task.id).await?;
        }
        Ok(task)
    }

    pub async fn task_roots(&self, id: &str) -> Result<Vec<TaskRoot>> {
        self.store.task(id).await?;
        let mut roots = self.store.task_roots(id).await?;
        for root in &mut roots {
            if root.mode == "isolated" && root.status == "ready" && verify(root).await.is_err() {
                root.status = "worktree_missing".into();
            }
        }
        Ok(roots)
    }

    async fn require_stopped(&self, id: &str) -> Result<()> {
        if let Ok(run) = self.runtime.snapshot(id).await {
            if !matches!(run.status.as_str(), "stopped" | "failed") {
                return Err(HostError::new("task_busy", "先停止任务进程再操作工作区"));
            }
        }
        Ok(())
    }

    pub async fn isolate_task(&self, id: &str) -> Result<Vec<TaskRoot>> {
        let _guard = self.gate.lock().await;
        self.isolate_locked(id).await
    }

    async fn isolate_locked(&self, id: &str) -> Result<Vec<TaskRoot>> {
        self.require_stopped(id).await?;
        let task = self.store.task(id).await?;
        if task.session_id.is_some() {
            return Err(HostError::new(
                "worktree_recovery_required",
                "已有会话不能更换执行目录；请创建新的隔离任务",
            ));
        }
        self.store.validate_task_roots(id).await?;
        let mut roots = self.store.task_roots(id).await?;
        if roots.iter().any(|r| r.mode != "shared") {
            return Ok(roots);
        }
        let base = self
            .data
            .canonicalize()
            .map_err(missing)?
            .join("worktrees")
            .join(id);
        let mut groups: Vec<(PathBuf, PathBuf, String, String, bool)> = vec![];
        // Resolve every repository and baseline before any worktree is created.
        for root in &mut roots {
            let (repo, relative) = git::repository_info(&root.execution_path)
                .await?
                .ok_or_else(|| HostError::new("worktree_unavailable", "所有执行根须为 Git 仓库"))?;
            let index = if let Some(i) = groups.iter().position(|g| g.0 == repo) {
                i
            } else {
                let i = groups.len();
                let baseline = git::head(&repo).await?;
                let dirty = git::worktree_dirty(&repo).await?;
                groups.push((
                    repo.clone(),
                    base.join(format!("repo-{i}")),
                    format!("cool-pi/{id}-{i}"),
                    baseline,
                    dirty,
                ));
                i
            };
            let (_, path, branch, baseline, dirty) = &groups[index];
            root.git_top_level = Some(repo);
            root.execution_path = path.join(&relative);
            root.relative_path = Some(relative);
            root.worktree_path = Some(path.clone());
            root.baseline_commit = Some(baseline.clone());
            root.branch = Some(branch.clone());
            root.mode = "isolated".into();
            root.status = "creating".into();
            root.created_by_client = true;
            root.source_dirty = *dirty;
            if path.exists() {
                return Err(HostError::new(
                    "worktree_create_failed",
                    "目标路径已存在，保留现场",
                ));
            }
        }
        std::fs::create_dir_all(&base).map_err(missing)?;
        if base.canonicalize().map_err(missing)? != base {
            return Err(missing("受控目录不得经符号链接重定向"));
        }
        // Persist intent before Git writes, so a Host crash never loses ownership records.
        self.store.update_task_roots(roots.clone()).await?;
        let result: Result<()> = async {
            for (repo, path, branch, baseline, _) in &groups {
                git::worktree_add(repo, path, branch, baseline).await?;
                for root in roots
                    .iter_mut()
                    .filter(|r| r.worktree_path.as_ref() == Some(path))
                {
                    verify(root).await?;
                    root.status = "ready".into();
                }
                self.store.update_task_roots(roots.clone()).await?;
            }
            Ok(())
        }
        .await;
        if let Err(error) = result {
            let mut recovery_errors = Vec::new();
            for root in &mut roots {
                if root.status != "ready" {
                    root.status = "failed".into();
                }
            }
            // Only clean, fully verified resources from this operation are sent to Trash.
            let mut seen = HashSet::new();
            for root in roots.clone() {
                let path = root.worktree_path.clone().unwrap();
                if !seen.insert(path.clone()) || !path.exists() {
                    continue;
                }
                match self.trash_root(&root).await {
                    Ok(()) => {
                        for r in roots
                            .iter_mut()
                            .filter(|r| r.worktree_path.as_ref() == Some(&path))
                        {
                            r.status = "trashed".into();
                        }
                    }
                    Err(e) => recovery_errors.push(e.message),
                }
            }
            self.store.update_task_roots(roots).await?;
            return Err(HostError::new(
                "worktree_create_failed",
                format!(
                    "隔离任务 {id} 创建失败：{}。任务与资源登记已保留；{}",
                    error.message,
                    recovery_errors.join("；")
                ),
            ));
        }
        Ok(roots)
    }

    async fn trash_root(&self, root: &TaskRoot) -> Result<()> {
        verify(root).await?;
        let path = root.worktree_path.clone().unwrap();
        let base = self
            .data
            .canonicalize()
            .map_err(missing)?
            .join("worktrees")
            .join(&root.task_id);
        if !path.starts_with(&base) || path == base {
            return Err(missing("Worktree is not client-owned"));
        }
        if git::worktree_dirty(&path).await? {
            return Err(HostError::new(
                "worktree_dirty",
                "工作区含暂存、未暂存、未跟踪或忽略文件，保留资源；请先处理后再清理",
            ));
        }
        let trash_path = path.clone();
        tokio::task::spawn_blocking(move || {
            trash::delete(trash_path).map_err(|e| HostError::new("worktree_cleanup_failed", e))
        })
        .await
        .map_err(missing)??;
        // The path is already in the system Trash; remove only Git's stale
        // registration, which otherwise keeps the branch locked.
        git::worktree_remove(root.git_top_level.as_ref().unwrap(), &path).await
    }

    pub async fn cleanup_worktrees(&self, id: &str) -> Result<Vec<TaskRoot>> {
        let _guard = self.gate.lock().await;
        self.require_stopped(id).await?;
        let mut roots = self.store.task_roots(id).await?;
        let mut seen = HashSet::new();
        // Check every worktree before moving any of them.
        for root in roots
            .iter()
            .filter(|r| r.mode == "isolated" && r.status != "trashed")
        {
            verify(root).await?;
            let owner = id.to_owned();
            let worktree = root.worktree_path.clone().unwrap();
            let referenced = self.store.access(move |c| {
                let mut statement = c.prepare("SELECT execution_path FROM task_roots WHERE task_id != ?1 AND status != 'trashed'").map_err(|e| HostError::new("storage_error", e))?;
                let paths = statement.query_map([owner], |r| r.get::<_,String>(0)).map_err(|e| HostError::new("storage_error", e))?;
                for path in paths {
                    if PathBuf::from(path.map_err(|e| HostError::new("storage_error", e))?).starts_with(&worktree) { return Ok(true); }
                }
                Ok(false)
            }).await?;
            if referenced { return Err(HostError::new("worktree_in_use", "其他会话（包括 Fork）仍在使用此工作目录，不能清理")); }
            if git::worktree_dirty(root.worktree_path.as_ref().unwrap()).await? {
                return Err(HostError::new(
                    "worktree_dirty",
                    "工作区存在改动，清理已取消",
                ));
            }
        }
        for root in roots
            .clone()
            .into_iter()
            .filter(|r| r.mode == "isolated" && r.status != "trashed")
        {
            let path = root.worktree_path.clone().unwrap();
            if !seen.insert(path.clone()) {
                continue;
            }
            self.trash_root(&root).await?;
            for r in roots
                .iter_mut()
                .filter(|r| r.worktree_path.as_ref() == Some(&path))
            {
                r.status = "trashed".into();
            }
            self.store.update_task_roots(roots.clone()).await?;
        }
        Ok(roots)
    }

    pub async fn recover_worktrees(&self, id: &str) -> Result<Vec<TaskRoot>> {
        let _guard = self.gate.lock().await;
        self.require_stopped(id).await?;
        let mut roots = self.store.task_roots(id).await?;
        for root in &mut roots {
            if root.mode == "isolated" {
                verify(root).await?;
                root.status = "ready".into();
            }
        }
        self.store.update_task_roots(roots.clone()).await?;
        Ok(roots)
    }
}
