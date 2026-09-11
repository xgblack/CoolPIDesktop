use crate::runtime::{HostError, Result};
use serde::{Deserialize, Serialize};
use std::{path::{Component, Path, PathBuf}, process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command, time::timeout};

const OUTPUT_LIMIT: usize = 1024 * 1024;
const GIT_TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitChange {
    pub path: String,
    pub original_path: Option<String>,
    pub index_status: String,
    pub worktree_status: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub root_index: usize,
    pub available: bool,
    pub branch: Option<String>,
    pub changes: Vec<GitChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDiff {
    pub root_index: usize,
    pub path: String,
    pub staged: bool,
    pub text: String,
    pub binary: bool,
}

fn safe_relative(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if value.is_empty() || path.is_absolute() || path.components().any(|part| matches!(part, Component::ParentDir | Component::RootDir | Component::Prefix(_))) {
        return Err(HostError::new("invalid_git_path", "Git path must be a non-empty relative path"));
    }
    Ok(path.to_path_buf())
}

async fn git(root: &Path, args: &[&str], allow_nonzero: bool) -> Result<Vec<u8>> {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .kill_on_drop(true)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .args(["-c", "core.pager=cat", "-c", "pager.diff=false", "-c", "color.ui=false", "-c", "diff.external="])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|e| HostError::new("git_unavailable", e))?;
    let stdout = child.stdout.take().ok_or_else(|| HostError::new("git_unavailable", "Git stdout unavailable"))?;
    let stderr = child.stderr.take().ok_or_else(|| HostError::new("git_unavailable", "Git stderr unavailable"))?;
    let read = async {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut limited_stdout = stdout.take((OUTPUT_LIMIT + 1) as u64);
        let mut limited_stderr = stderr.take(16385);
        let (outcome, error) = tokio::join!(limited_stdout.read_to_end(&mut out), limited_stderr.read_to_end(&mut err));
        outcome.map_err(|e| HostError::new("git_failed", e))?;
        error.map_err(|e| HostError::new("git_failed", e))?;
        if out.len() > OUTPUT_LIMIT {
            return Err(HostError::new("git_output_too_large", "Git output exceeds the 1 MiB display limit"));
        }
        let status = child.wait().await.map_err(|e| HostError::new("git_failed", e))?;
        if !status.success() && !(allow_nonzero && status.code() == Some(1) && err.is_empty()) {
            let message = String::from_utf8_lossy(&err).trim().to_owned();
            return Err(HostError::new("git_failed", if message.is_empty() { format!("Git exited with {status}") } else { message }));
        }
        Ok(out)
    };
    timeout(GIT_TIMEOUT, read).await.map_err(|_| HostError::new("git_timeout", "Git command timed out"))?
}

fn status_code(value: u8) -> String { if value == b' ' { String::new() } else { (value as char).to_string() } }

fn kind(index: &str, worktree: &str) -> String {
    if index == "U" || worktree == "U" || (index == "A" && worktree == "A") || (index == "D" && worktree == "D") { "conflicted".into() }
    else if index == "R" || worktree == "R" { "renamed".into() }
    else if index == "?" || worktree == "?" { "untracked".into() }
    else if index == "D" || worktree == "D" { "deleted".into() }
    else { "modified".into() }
}

pub async fn status(root: &Path, root_index: usize) -> Result<GitStatus> {
    match git(root, &["rev-parse", "--is-inside-work-tree"], false).await {
        Ok(value) if value == b"true\n" => {}
        Ok(_) => return Ok(GitStatus { root_index, available: false, branch: None, changes: Vec::new() }),
        Err(error) if error.code == "git_failed" && error.message.contains("not a git repository") =>
            return Ok(GitStatus { root_index, available: false, branch: None, changes: Vec::new() }),
        Err(error) => return Err(error),
    }
    // symbolic-ref also works before the first commit; detached HEAD exits 1.
    let branch = git(root, &["symbolic-ref", "--quiet", "--short", "HEAD"], true).await?;
    let branch = Some(String::from_utf8_lossy(&branch).trim().to_owned()).filter(|value| !value.is_empty());
    // Porcelain paths are always relative to the repository top-level, even
    // when `root` is a trusted subdirectory. Scope the query and then return
    // paths relative to the selected task root so the caller cannot discover
    // changes outside it or feed an unusable repository-relative path to diff.
    let prefix = String::from_utf8(git(root, &["rev-parse", "--show-prefix"], false).await?)
        .map_err(|_| HostError::new("git_failed", "Git path is not UTF-8"))?;
    let prefix = prefix.trim_end_matches(['\r', '\n']);
    let raw = git(root, &["status", "--porcelain=v1", "-z", "--untracked-files=all", "--", "."], false).await?;
    let fields: Vec<&[u8]> = raw.split(|byte| *byte == 0).filter(|field| !field.is_empty()).collect();
    let mut changes = Vec::new();
    let mut index = 0;
    while index < fields.len() {
        let field = fields[index];
        if field.len() < 4 { return Err(HostError::new("git_failed", "Invalid porcelain status output")); }
        let index_status = status_code(field[0]);
        let worktree_status = status_code(field[1]);
        let path = String::from_utf8(field[3..].to_vec()).map_err(|_| HostError::new("git_failed", "Git path is not UTF-8"))?;
        let path = path.strip_prefix(prefix).ok_or_else(|| HostError::new("git_failed", "Git returned a path outside the selected root"))?.to_owned();
        let renamed = matches!(index_status.as_str(), "R" | "C") || matches!(worktree_status.as_str(), "R" | "C");
        let original_path = if renamed {
            index += 1;
            Some(String::from_utf8(fields.get(index).ok_or_else(|| HostError::new("git_failed", "Rename status lacks original path"))?.to_vec()).map_err(|_| HostError::new("git_failed", "Git path is not UTF-8"))?.strip_prefix(prefix).ok_or_else(|| HostError::new("git_failed", "Git returned a path outside the selected root"))?.to_owned())
        } else { None };
        changes.push(GitChange { kind: kind(&index_status, &worktree_status), path, original_path, index_status, worktree_status });
        index += 1;
    }
    Ok(GitStatus { root_index, available: true, branch, changes })
}

pub async fn diff(root: &Path, root_index: usize, path: &str, staged: bool, untracked: bool) -> Result<GitDiff> {
    let relative = safe_relative(path)?;
    let absolute = root.join(&relative);
    let canonical_root = root.canonicalize().map_err(|e| HostError::new("directory_missing", e))?;
    if absolute.exists() && !absolute.canonicalize().map_err(|e| HostError::new("invalid_git_path", e))?.starts_with(&canonical_root) {
        return Err(HostError::new("invalid_git_path", "Git path leaves the selected root"));
    }
    let printable = relative.to_string_lossy().into_owned();
    let args = if untracked {
        vec!["diff", "--no-index", "--no-ext-diff", "--no-textconv", "--binary", "--", "/dev/null", absolute.to_str().ok_or_else(|| HostError::new("invalid_git_path", "Git path is not UTF-8"))?]
    } else if staged {
        vec!["--literal-pathspecs", "diff", "--relative", "--cached", "--no-ext-diff", "--no-textconv", "--binary", "--", &printable]
    } else {
        vec!["--literal-pathspecs", "diff", "--relative", "--no-ext-diff", "--no-textconv", "--binary", "--", &printable]
    };
    let output = git(root, &args, untracked).await?;
    let text = String::from_utf8(output).map_err(|_| HostError::new("git_failed", "Git diff is not UTF-8"))?;
    let binary = text.contains("Binary files ") || text.contains("GIT binary patch");
    Ok(GitDiff { root_index, path: printable, staged, text, binary })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_allows_relative_git_paths() {
        for path in ["../secret", "/tmp/secret", "", "a/../../b"] { assert!(safe_relative(path).is_err(), "{path}"); }
        assert_eq!(safe_relative("src/main.rs").unwrap(), PathBuf::from("src/main.rs"));
    }
}
