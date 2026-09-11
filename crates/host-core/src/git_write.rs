use crate::{
    HostError, Workbench, files,
    git::{self, git},
    isolation,
    runtime::Result,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommitPreview {
    pub branch: String,
    pub root: PathBuf,
    pub head: String,
    pub tree: String,
    pub paths: Vec<String>,
}
fn denied(message: &str) -> HostError {
    HostError::new("git_write_denied", message)
}
async fn text(root: &Path, args: &[&str]) -> Result<String> {
    String::from_utf8(git(root, args, false).await?)
        .map(|s| s.trim_end_matches('\n').to_owned())
        .map_err(|_| denied("Git 返回非 UTF-8 数据"))
}
// Validate ancestors by directory descriptor, including deleted tracked files.
fn regular_path(root: &Path, path: &str) -> Result<()> {
    let parts = files::components(path)?;
    if parts.is_empty() || parts.iter().any(|p| p.eq_ignore_ascii_case(".git")) {
        return Err(denied("不允许访问 Git 元数据"));
    }
    let dir = files::absolute_dir(root)?;
    let parent = parts[..parts.len() - 1].join("/");
    let parent = files::relative_open(&dir, &parent, true)?;
    match files::open_at(
        &parent,
        std::ffi::OsStr::new(parts.last().unwrap()),
        libc::O_RDONLY,
    ) {
        Ok(file) if file.metadata().map_err(files::io_error)?.is_file() => Ok(()),
        Err(e) if e.code == "file_missing" => Ok(()),
        Err(e) => Err(e),
        _ => Err(denied("仅支持普通文件；目录、符号链接和子模块不可写")),
    }
}
async fn preview(root: &Path, index: usize) -> Result<CommitPreview> {
    for state in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ] {
        let path = text(
            root,
            &["rev-parse", "--path-format=absolute", "--git-path", state],
        )
        .await?;
        if Path::new(&path).try_exists().map_err(files::io_error)? {
            return Err(denied(
                "仓库正在合并、变基或拣选提交，请先在终端完成或中止该操作",
            ));
        }
    }
    let status = git::status(root, index).await?;
    let branch = status
        .branch
        .ok_or_else(|| denied("分支缺失或 HEAD 已分离"))?;
    if status.changes.iter().any(|c| c.kind == "conflicted") {
        return Err(denied("请先解决冲突"));
    }
    let prefix = text(root, &["rev-parse", "--show-prefix"]).await?;
    // --name-only without a pathspec deliberately examines the entire index.
    let raw = git(
        root,
        &[
            "diff",
            "--cached",
            "--name-only",
            "--no-renames",
            "-z",
            "--no-ext-diff",
            "--no-textconv",
        ],
        false,
    )
    .await?;
    let mut paths = Vec::new();
    for value in raw.split(|b| *b == 0).filter(|v| !v.is_empty()) {
        let full = std::str::from_utf8(value).map_err(|_| denied("文件名不是 UTF-8"))?;
        let path = full
            .strip_prefix(&prefix)
            .ok_or_else(|| denied("同一 worktree 有其他目录的暂存改动，请先取消那些暂存"))?;
        regular_path(root, path)?;
        paths.push(path.to_owned());
    }
    if paths.is_empty() {
        return Err(HostError::new("git_empty_index", "没有暂存改动"));
    }
    Ok(CommitPreview {
        branch,
        root: root.into(),
        head: text(root, &["rev-parse", "HEAD"]).await?,
        tree: text(root, &["write-tree"]).await?,
        paths,
    })
}
impl Workbench {
    async fn writable_git_root(&self, id: &str, index: usize) -> Result<PathBuf> {
        let task = self.store.task(id).await?;
        if task.archived {
            return Err(denied("已归档任务不可写"));
        }
        if let Ok(run) = self.runtime.snapshot(id).await {
            if !matches!(run.status.as_str(), "stopped" | "failed") {
                return Err(denied("请先停止任务再进行 Git 写操作"));
            }
        }
        self.store.validate_task_roots(id).await?;
        let roots = self.store.task_roots(id).await?;
        let root = roots
            .get(index)
            .ok_or_else(|| denied("目录不属于当前任务"))?;
        if root.mode != "isolated" || !root.created_by_client {
            return Err(denied("Git 写操作仅支持客户端创建的隔离 worktree"));
        }
        isolation::verify(root).await?;
        Ok(root.execution_path.clone())
    }
    pub async fn git_change(
        &self,
        id: &str,
        index: usize,
        path: &str,
        action: &str,
        confirmed: bool,
    ) -> Result<()> {
        let _guard = self.gate.lock().await;
        let root = self.writable_git_root(id, index).await?;
        regular_path(&root, path)?;
        let status = git::status(&root, index).await?;
        let change = status
            .changes
            .iter()
            .find(|c| c.path == path)
            .ok_or_else(|| denied("文件状态已变化，请刷新"))?;
        if change.kind == "conflicted" {
            return Err(denied("请先解决冲突"));
        }
        let mut paths = vec![path];
        if let Some(old) = &change.original_path {
            regular_path(&root, old)?;
            paths.push(old);
        }
        let mut args = match action {
            "stage" => vec!["add", "--all", "--"],
            "unstage" => vec!["restore", "--staged", "--"],
            "discard" => {
                if !confirmed
                    || change.worktree_status != "M"
                    || change.original_path.is_some()
                    || change.kind == "untracked"
                {
                    return Err(denied("撤销须确认，且仅支持普通已跟踪文件的未暂存修改"));
                }
                // Verify a regular index entry before moving the current file to Trash.
                let entry = git(&root, &["ls-files", "--stage", "--", path], false).await?;
                if !entry.starts_with(b"100644 ") && !entry.starts_with(b"100755 ") {
                    return Err(denied("暂存版本不是普通文件"));
                }
                let target = root.join(path);
                tokio::task::spawn_blocking(move || trash::delete(&target))
                    .await
                    .map_err(|_| denied("回收站操作中断，未执行恢复"))?
                    .map_err(|_| denied("无法移入回收站，未执行恢复"))?;
                vec!["restore", "--worktree", "--"]
            }
            _ => return Err(denied("未知 Git 写操作")),
        };
        args.extend(paths);
        git(&root, &args, false).await?;
        Ok(())
    }
    pub async fn git_commit_preview(&self, id: &str, index: usize) -> Result<CommitPreview> {
        let _guard = self.gate.lock().await;
        let root = self.writable_git_root(id, index).await?;
        preview(&root, index).await
    }
    pub async fn git_commit(
        &self,
        id: &str,
        index: usize,
        message: &str,
        expected: CommitPreview,
    ) -> Result<String> {
        let _guard = self.gate.lock().await;
        if message.trim().is_empty() || message.len() > 16384 || message.contains('\0') {
            return Err(denied("提交信息不能为空或超过 16 KiB"));
        }
        let root = self.writable_git_root(id, index).await?;
        if preview(&root, index).await? != expected {
            return Err(HostError::new(
                "git_stale_preview",
                "暂存内容或分支已变化，请重新确认",
            ));
        }
        // Commit exactly the confirmed index tree. A concurrent index update must
        // never silently become part of this commit; update-ref checks the old HEAD.
        let commit = text(
            &root,
            &[
                "commit-tree",
                &expected.tree,
                "-p",
                &expected.head,
                "-m",
                message,
            ],
        )
        .await?;
        let reference = format!("refs/heads/{}", expected.branch);
        git(
            &root,
            &[
                "update-ref",
                "-m",
                "desktop commit",
                &reference,
                &commit,
                &expected.head,
            ],
            false,
        )
        .await?;
        Ok(commit)
    }
}
