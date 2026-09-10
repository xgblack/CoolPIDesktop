use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::AsyncReadExt,
    process::{Child, Command},
    time::timeout,
};

pub const MIN_VERSION: &str = "18.1.14";
pub const DEADLINE: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{code}: {message}")]
#[serde(rename_all = "camelCase")]
pub struct HostError {
    pub code: String,
    pub message: String,
    pub suggestion: String,
}
impl HostError {
    pub fn new(code: &str, message: impl ToString) -> Self {
        let suggestion = match code {
            "not_found" | "not_executable" => {
                "选择已安装的 OMP 可执行文件，检查文件路径及执行权限。"
            }
            "version_unsupported" => "使用 OMP 官方安装方式更新系统 OMP 后重新检测。",
            "version_unreadable" => "在终端检查所选 OMP 的 --version 输出及启动环境。",
            "handshake_failed" | "capability_query_failed" => "检查 OMP 版本与配置，重新启动任务。",
            "task_busy" => "等待当前操作完成，或中止运行中的任务。",
            _ => "检查任务诊断；必要时停止并重新启动任务。",
        };
        Self {
            code: code.into(),
            message: message.to_string(),
            suggestion: suggestion.into(),
        }
    }
}
pub type Result<T> = std::result::Result<T, HostError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub executable: String,
    pub version: Option<String>,
    pub status: String,
    pub protocol: Option<u32>,
    pub capabilities: Value,
    pub error: Option<HostError>,
}
impl RuntimeInfo {
    pub fn error(e: HostError, executable: String) -> Self {
        Self {
            executable,
            version: None,
            status: e.code.clone(),
            protocol: None,
            capabilities: Value::Null,
            error: Some(e),
        }
    }
}

fn executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.is_file() && std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}
pub fn resolve_executable(explicit: Option<&Path>, saved: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = explicit.or(saved) {
        if !p.is_absolute() || !executable(p) {
            return Err(HostError::new(
                "not_executable",
                "OMP must be an absolute executable file path",
            ));
        }
        return Ok(p.to_path_buf());
    }
    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        if !dir.is_absolute() {
            continue;
        }
        let p = dir.join("omp");
        if executable(&p) {
            return Ok(p);
        }
    }
    Err(HostError::new(
        "not_found",
        "No omp executable found in PATH",
    ))
}

pub(crate) fn command(path: &Path) -> Command {
    let mut c = Command::new(path);
    c.kill_on_drop(true);
    #[cfg(unix)]
    c.process_group(0);
    c
}
// Kill the owned process group too: OMP can leave descendants holding pipe handles.
pub(crate) async fn reap(child: &mut Child) -> Result<()> {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        let result = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
        if result != 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
            return Err(HostError::new(
                "process_cleanup_failed",
                std::io::Error::last_os_error(),
            ));
        }
    }
    if let Err(e) = child.start_kill() {
        if e.kind() != std::io::ErrorKind::InvalidInput {
            return Err(HostError::new("process_cleanup_failed", e));
        }
    }
    timeout(DEADLINE, child.wait())
        .await
        .map_err(|_| HostError::new("process_cleanup_failed", "Timed out reaping OMP"))?
        .map_err(|e| HostError::new("process_cleanup_failed", e))?;
    Ok(())
}

pub async fn probe(path: &Path) -> Result<RuntimeInfo> {
    let mut child = command(path)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| HostError::new("version_unreadable", e))?;
    let out = child.stdout.take().unwrap();
    let err = child.stderr.take().unwrap();
    let read = async {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut fo = out.take(16385);
        let mut fe = err.take(16385);
        let (a, b) = tokio::join!(fo.read_to_end(&mut stdout), fe.read_to_end(&mut stderr));
        a.map_err(|e| HostError::new("version_unreadable", e))?;
        b.map_err(|e| HostError::new("version_unreadable", e))?;
        if stdout.len() > 16384 || stderr.len() > 16384 {
            return Err(HostError::new(
                "version_unreadable",
                "Version output exceeds 16 KiB",
            ));
        }
        let status = child
            .wait()
            .await
            .map_err(|e| HostError::new("version_unreadable", e))?;
        if !status.success() {
            return Err(HostError::new(
                "version_unreadable",
                format!("--version exited with {status}"),
            ));
        }
        let s = String::from_utf8(stdout)
            .map_err(|_| HostError::new("version_unreadable", "Invalid UTF-8 version"))?;
        parse_version(&s)
    };
    let result = timeout(DEADLINE, read).await.unwrap_or_else(|_| {
        Err(HostError::new(
            "version_unreadable",
            "Version probe timed out",
        ))
    });
    if result.is_err() {
        reap(&mut child).await?;
    }
    let version = result?;
    Ok(RuntimeInfo {
        executable: path.to_string_lossy().into(),
        version: Some(version),
        status: "starting".into(),
        protocol: None,
        capabilities: Value::Null,
        error: None,
    })
}
fn parse_version(s: &str) -> Result<String> {
    let versions: Vec<_> = s
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter(|v| !v.is_empty())
        .filter_map(|v| semver::Version::parse(v).ok())
        .collect();
    if versions.len() != 1 {
        return Err(HostError::new(
            "version_unreadable",
            "Expected exactly one semantic version",
        ));
    }
    if versions[0] < semver::Version::parse(MIN_VERSION).unwrap() {
        return Err(HostError::new(
            "version_unsupported",
            format!("OMP >= {MIN_VERSION} required"),
        ));
    }
    Ok(versions[0].to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn version_policy() {
        assert_eq!(parse_version("omp/18.1.14\n").unwrap(), "18.1.14");
        assert_eq!(
            parse_version("omp v18.1.13").unwrap_err().code,
            "version_unsupported"
        );
        assert!(parse_version("unknown").is_err());
        assert!(parse_version("18.1.14 19.0.0").is_err());
    }
    #[test]
    fn invalid_explicit_never_falls_back() {
        assert_eq!(
            resolve_executable(Some(Path::new("relative")), None)
                .unwrap_err()
                .code,
            "not_executable"
        );
    }
}
