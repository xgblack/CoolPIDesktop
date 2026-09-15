//! Bounded Agent Profile editing under the fixed user or trusted project roots.
use crate::{HostError, Workbench, runtime::Result};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const MAX_PROFILE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfile {
    pub scope: String,
    pub project_id: Option<String>,
    pub root_index: Option<usize>,
    pub name: String,
    pub content: String,
    pub revision: String,
}

fn failure(code: &str, value: impl std::fmt::Display) -> HostError {
    HostError::new(code, value)
}

fn valid_name(name: &str) -> Result<&str> {
    if name.is_empty()
        || name.len() > 80
        || !name
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'-' | b'_'))
    {
        return Err(failure(
            "agent_profile_name_invalid",
            "Profile 名称只能包含字母、数字、连字符和下划线",
        ));
    }
    Ok(name)
}

fn revision(metadata: &fs::Metadata) -> String {
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    format!("{}:{modified}", metadata.len())
}

fn regular_metadata(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(Some(metadata)),
        Ok(_) => Err(failure(
            "agent_profile_path_invalid",
            "Profile 必须是普通文件，不能是符号链接",
        )),
        Err(value) if value.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(value) => Err(failure("agent_profile_io", value)),
    }
}

fn checked_directory(root: &Path) -> Result<PathBuf> {
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_type().is_dir() => {}
        Ok(_) => {
            return Err(failure(
                "agent_profile_path_invalid",
                "Agent Profile 目录不能是符号链接",
            ));
        }
        Err(value) if value.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(root).map_err(|value| failure("agent_profile_io", value))?;
        }
        Err(value) => return Err(failure("agent_profile_io", value)),
    }
    root.canonicalize()
        .map_err(|value| failure("agent_profile_io", value))
}

fn read_profile(
    directory: &Path,
    scope: &str,
    project_id: Option<String>,
    root_index: Option<usize>,
    name: &str,
) -> Result<AgentProfile> {
    valid_name(name)?;
    let path = directory.join(format!("{name}.md"));
    let metadata = regular_metadata(&path)?.ok_or_else(|| {
        failure(
            "agent_profile_missing",
            format!("Agent Profile {name} 不存在"),
        )
    })?;
    if metadata.len() > MAX_PROFILE_BYTES {
        return Err(failure(
            "agent_profile_too_large",
            "Agent Profile 超过 1 MiB",
        ));
    }
    let mut content = String::new();
    fs::File::open(&path)
        .and_then(|file| {
            file.take(MAX_PROFILE_BYTES + 1)
                .read_to_string(&mut content)
        })
        .map_err(|value| failure("agent_profile_io", value))?;
    Ok(AgentProfile {
        scope: scope.into(),
        project_id,
        root_index,
        name: name.into(),
        content,
        revision: revision(&metadata),
    })
}

fn save_profile(directory: &Path, name: &str, content: &str, expected: &str) -> Result<String> {
    valid_name(name)?;
    if content.len() as u64 > MAX_PROFILE_BYTES {
        return Err(failure(
            "agent_profile_too_large",
            "Agent Profile 超过 1 MiB",
        ));
    }
    let target = directory.join(format!("{name}.md"));
    let current = regular_metadata(&target)?;
    let current_revision = current.as_ref().map(revision).unwrap_or_default();
    if current_revision != expected {
        return Err(failure(
            "agent_profile_conflict",
            "Agent Profile 已在别处修改，请重新加载后合并",
        ));
    }
    let temporary = directory.join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4()));
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options
        .open(&temporary)
        .map_err(|value| failure("agent_profile_io", value))?;
    let result = (|| -> Result<String> {
        output
            .write_all(content.as_bytes())
            .and_then(|_| output.sync_all())
            .map_err(|value| failure("agent_profile_io", value))?;
        let latest = regular_metadata(&target)?;
        if latest.as_ref().map(revision).unwrap_or_default() != expected {
            return Err(failure(
                "agent_profile_conflict",
                "Agent Profile 在保存期间发生变化",
            ));
        }
        fs::rename(&temporary, &target).map_err(|value| failure("agent_profile_io", value))?;
        if let Ok(parent) = fs::File::open(directory) {
            let _ = parent.sync_all();
        }
        regular_metadata(&target)?
            .as_ref()
            .map(revision)
            .ok_or_else(|| failure("agent_profile_io", "保存后的 Profile 不存在"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

impl Workbench {
    async fn profile_directory(
        &self,
        scope: &str,
        project_id: Option<&str>,
        root_index: Option<usize>,
    ) -> Result<PathBuf> {
        match scope {
            "user" => {
                let agent = std::env::var_os("PI_CODING_AGENT_DIR")
                    .map(PathBuf::from)
                    .or_else(|| {
                        std::env::var_os("HOME")
                            .map(|value| PathBuf::from(value).join(".omp/agent"))
                    })
                    .ok_or_else(|| {
                        failure("agent_profile_root_missing", "无法定位 OMP 用户目录")
                    })?;
                checked_directory(&agent.join("agents"))
            }
            "project" => {
                let project_id = project_id.ok_or_else(|| {
                    failure(
                        "agent_profile_project_missing",
                        "项目 Profile 缺少 projectId",
                    )
                })?;
                let project = self
                    .store
                    .projects()
                    .await?
                    .into_iter()
                    .find(|value| value.id == project_id)
                    .ok_or_else(|| failure("project_missing", "项目不存在"))?;
                if !project.trusted {
                    return Err(failure(
                        "workspace_untrusted",
                        "未经信任的项目不能管理 Agent Profile",
                    ));
                }
                let root = project
                    .roots
                    .get(root_index.unwrap_or(0))
                    .ok_or_else(|| failure("invalid_file_root", "项目目录不存在"))?
                    .canonicalize()
                    .map_err(|value| failure("agent_profile_io", value))?;
                let omp = root.join(".omp");
                if fs::symlink_metadata(&omp).is_ok_and(|value| value.file_type().is_symlink()) {
                    return Err(failure("agent_profile_path_invalid", ".omp 不能是符号链接"));
                }
                let directory = checked_directory(&omp.join("agents"))?;
                if !directory.starts_with(&root) {
                    return Err(failure(
                        "agent_profile_path_invalid",
                        "Agent Profile 目录超出项目根目录",
                    ));
                }
                Ok(directory)
            }
            _ => Err(failure(
                "agent_profile_scope_invalid",
                "Profile scope 只能是 user 或 project",
            )),
        }
    }

    pub async fn agent_profiles(
        &self,
        scope: &str,
        project_id: Option<&str>,
        root_index: Option<usize>,
    ) -> Result<Vec<AgentProfile>> {
        let directory = self
            .profile_directory(scope, project_id, root_index)
            .await?;
        let mut profiles = Vec::new();
        for entry in fs::read_dir(&directory).map_err(|value| failure("agent_profile_io", value))? {
            let entry = entry.map_err(|value| failure("agent_profile_io", value))?;
            let path = entry.path();
            if path.extension().is_some_and(|value| value == "md") {
                if let Some(name) = path.file_stem().and_then(|value| value.to_str()) {
                    if valid_name(name).is_ok() {
                        profiles.push(read_profile(
                            &directory,
                            scope,
                            project_id.map(str::to_owned),
                            root_index,
                            name,
                        )?);
                    }
                }
            }
        }
        profiles.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(profiles)
    }

    pub async fn save_agent_profile(
        &self,
        scope: &str,
        project_id: Option<&str>,
        root_index: Option<usize>,
        name: &str,
        content: &str,
        revision: &str,
    ) -> Result<AgentProfile> {
        let directory = self
            .profile_directory(scope, project_id, root_index)
            .await?;
        save_profile(&directory, name, content, revision)?;
        read_profile(
            &directory,
            scope,
            project_id.map(str::to_owned),
            root_index,
            name,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn preserves_unknown_frontmatter_and_rejects_stale_revisions() {
        let root = std::env::temp_dir().join(format!("profiles-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let workbench = Workbench::open(root.join("data")).await.unwrap();
        let project = workbench
            .store
            .register_project("P", vec![root.clone()], true)
            .await
            .unwrap();
        let raw = "---\nname: reviewer\nunknown-field: keep\n---\nReview carefully.\n";
        let created = workbench
            .save_agent_profile("project", Some(&project.id), Some(0), "reviewer", raw, "")
            .await
            .unwrap();
        assert_eq!(created.content, raw);
        let updated = workbench
            .save_agent_profile(
                "project",
                Some(&project.id),
                Some(0),
                "reviewer",
                &format!("{}More.\n", created.content),
                &created.revision,
            )
            .await
            .unwrap();
        assert!(updated.content.contains("unknown-field: keep"));
        assert_eq!(
            workbench
                .save_agent_profile(
                    "project",
                    Some(&project.id),
                    Some(0),
                    "reviewer",
                    "stale",
                    &created.revision,
                )
                .await
                .unwrap_err()
                .code,
            "agent_profile_conflict"
        );
    }

    #[tokio::test]
    async fn rejects_symlinked_profile_directory() {
        #[cfg(unix)]
        {
            let root = std::env::temp_dir().join(format!("profiles-link-{}", uuid::Uuid::new_v4()));
            let outside = root.join("outside");
            fs::create_dir_all(&outside).unwrap();
            fs::create_dir_all(root.join("project/.omp")).unwrap();
            std::os::unix::fs::symlink(&outside, root.join("project/.omp/agents")).unwrap();
            let workbench = Workbench::open(root.join("data")).await.unwrap();
            let project = workbench
                .store
                .register_project("P", vec![root.join("project")], true)
                .await
                .unwrap();
            assert_eq!(
                workbench
                    .agent_profiles("project", Some(&project.id), Some(0))
                    .await
                    .unwrap_err()
                    .code,
                "agent_profile_path_invalid"
            );
        }
    }
}
