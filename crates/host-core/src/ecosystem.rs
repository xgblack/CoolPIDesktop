use crate::runtime::{self, HostError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::Path, process::Stdio, time::Duration};
use tokio::io::AsyncReadExt;

const STDOUT_LIMIT: u64 = 1024 * 1024;
const STDERR_LIMIT: u64 = 64 * 1024;
const QUERY_TIMEOUT: Duration = Duration::from_secs(45);
const MUTATION_TIMEOUT: Duration = Duration::from_secs(90);
const NETWORK_MUTATION_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginOverview {
    pub plugins: Value,
    pub diagnostics: Value,
}

fn valid_plugin_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value.is_ascii()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | '@' | ':'))
        && !value.contains("..")
        && !value.starts_with('/')
}

async fn run_output(
    executable: &Path,
    cwd: &Path,
    args: &[String],
    deadline: Duration,
) -> Result<Vec<u8>> {
    let mut child = runtime::command(executable)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| HostError::new("ecosystem_command_failed", e))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| HostError::new("ecosystem_command_failed", "Missing command stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| HostError::new("ecosystem_command_failed", "Missing command stderr"))?;
    let read = async {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut bounded_out = stdout.take(STDOUT_LIMIT + 1);
        let mut bounded_err = stderr.take(STDERR_LIMIT + 1);
        let (out_result, err_result) = tokio::join!(
            bounded_out.read_to_end(&mut out),
            bounded_err.read_to_end(&mut err)
        );
        out_result.map_err(|e| HostError::new("ecosystem_command_failed", e))?;
        err_result.map_err(|e| HostError::new("ecosystem_command_failed", e))?;
        if out.len() as u64 > STDOUT_LIMIT || err.len() as u64 > STDERR_LIMIT {
            return Err(HostError::new(
                "ecosystem_output_too_large",
                "OMP ecosystem command exceeded its output limit",
            ));
        }
        let status = child
            .wait()
            .await
            .map_err(|e| HostError::new("ecosystem_command_failed", e))?;
        if !status.success() {
            return Err(HostError::new(
                "ecosystem_command_failed",
                format!("OMP ecosystem command exited with {status}"),
            ));
        }
        Ok(out)
    };
    let result = tokio::time::timeout(deadline, read).await;
    match result {
        Ok(value) => value,
        Err(_) => {
            runtime::reap(&mut child).await?;
            Err(HostError::new(
                "ecosystem_timeout",
                "OMP ecosystem command timed out",
            ))
        }
    }
}

async fn run_json(
    executable: &Path,
    cwd: &Path,
    args: &[String],
    deadline: Duration,
) -> Result<Value> {
    let out = run_output(executable, cwd, args, deadline).await?;
    serde_json::from_slice(&out)
        .map_err(|_| HostError::new("ecosystem_response_invalid", "OMP returned invalid JSON"))
}

pub async fn usage(executable: &Path, cwd: &Path) -> Result<Value> {
    run_json(
        executable,
        cwd,
        &["usage".into(), "--json".into(), "--redact".into()],
        QUERY_TIMEOUT,
    )
    .await
}

pub async fn logout_provider(executable: &Path, cwd: &Path, provider_id: &str) -> Result<()> {
    if !valid_plugin_id(provider_id) || provider_id.contains(['/', '@', ':']) {
        return Err(HostError::new(
            "invalid_provider",
            "Provider id contains unsupported characters",
        ));
    }
    run_output(
        executable,
        cwd,
        &["auth-broker".into(), "logout".into(), provider_id.into()],
        MUTATION_TIMEOUT,
    )
    .await?;
    Ok(())
}

pub async fn plugin_overview(executable: &Path, cwd: &Path) -> Result<PluginOverview> {
    let plugins = run_json(
        executable,
        cwd,
        &["plugin".into(), "list".into(), "--json".into()],
        QUERY_TIMEOUT,
    )
    .await?;
    let diagnostics = run_json(
        executable,
        cwd,
        &["plugin".into(), "doctor".into(), "--json".into()],
        QUERY_TIMEOUT,
    )
    .await?;
    Ok(PluginOverview {
        plugins,
        diagnostics,
    })
}

pub async fn set_plugin_enabled(
    executable: &Path,
    cwd: &Path,
    plugin_id: &str,
    enabled: bool,
    scope: &str,
) -> Result<Value> {
    let args = plugin_toggle_args(plugin_id, enabled, scope)?;
    run_json(executable, cwd, &args, MUTATION_TIMEOUT).await
}

pub async fn mutate_plugin(
    executable: &Path,
    cwd: &Path,
    action: &str,
    plugin_id: &str,
    scope: &str,
) -> Result<PluginOverview> {
    let args = plugin_mutation_args(action, plugin_id, scope)?;
    // Install/upgrade output is intentionally discarded: it is human-oriented and
    // may include local paths. The refreshed structured overview is authoritative.
    run_output(executable, cwd, &args, NETWORK_MUTATION_TIMEOUT).await?;
    plugin_overview(executable, cwd).await
}

fn plugin_toggle_args(plugin_id: &str, enabled: bool, scope: &str) -> Result<Vec<String>> {
    if !valid_plugin_id(plugin_id) {
        return Err(HostError::new(
            "invalid_plugin",
            "Plugin id contains unsupported characters",
        ));
    }
    if !matches!(scope, "user" | "project") {
        return Err(HostError::new(
            "invalid_plugin_scope",
            "Plugin scope must be user or project",
        ));
    }
    Ok(vec![
        "plugin".into(),
        (if enabled { "enable" } else { "disable" }).into(),
        plugin_id.into(),
        "--scope".into(),
        scope.into(),
        "--json".into(),
    ])
}

fn plugin_mutation_args(action: &str, plugin_id: &str, scope: &str) -> Result<Vec<String>> {
    if !matches!(action, "install" | "uninstall" | "upgrade") {
        return Err(HostError::new(
            "invalid_plugin_action",
            "Plugin action must be install, uninstall, or upgrade",
        ));
    }
    if !valid_plugin_id(plugin_id)
        || plugin_id.starts_with('@') && plugin_id.matches('@').count() < 2
        || !plugin_id
            .rsplit_once('@')
            .is_some_and(|(name, marketplace)| !name.is_empty() && !marketplace.is_empty())
    {
        return Err(HostError::new(
            "invalid_plugin",
            "Only an installed marketplace plugin id such as name@marketplace is allowed",
        ));
    }
    if !matches!(scope, "user" | "project") {
        return Err(HostError::new(
            "invalid_plugin_scope",
            "Plugin scope must be user or project",
        ));
    }
    Ok(vec![
        "plugin".into(),
        action.into(),
        plugin_id.into(),
        "--scope".into(),
        scope.into(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_ids_are_data_not_argv() {
        for valid in [
            "alpha",
            "@scope/name",
            "name@marketplace",
            "github:user/repo",
        ] {
            assert!(valid_plugin_id(valid), "{valid}");
        }
        for invalid in [
            "",
            "../plugin",
            "/tmp/plugin",
            "name --force",
            "name\n--fix",
        ] {
            assert!(!valid_plugin_id(invalid), "{invalid}");
        }
        assert_eq!(
            plugin_toggle_args("@scope/name", false, "project").unwrap(),
            [
                "plugin",
                "disable",
                "@scope/name",
                "--scope",
                "project",
                "--json"
            ]
        );
        assert_eq!(
            plugin_toggle_args("name --force", true, "user")
                .unwrap_err()
                .code,
            "invalid_plugin"
        );
    }

    #[test]
    fn marketplace_mutations_are_narrow_and_structured() {
        assert_eq!(
            plugin_mutation_args("install", "review@official", "project").unwrap(),
            ["plugin", "install", "review@official", "--scope", "project"]
        );
        assert_eq!(
            plugin_mutation_args("upgrade", "@scope/review@official", "user").unwrap(),
            [
                "plugin",
                "upgrade",
                "@scope/review@official",
                "--scope",
                "user"
            ]
        );
        for invalid in [
            "review",
            "https://example.com/plugin",
            "github:user/repo",
            "../review@official",
            "review@official --force",
        ] {
            assert!(plugin_mutation_args("install", invalid, "user").is_err());
        }
        assert!(plugin_mutation_args("remove", "review@official", "user").is_err());
    }

    #[test]
    fn provider_logout_rejects_argument_injection() {
        for valid in ["anthropic", "openai-codex", "google_gemini"] {
            assert!(valid_plugin_id(valid) && !valid.contains(['/', '@', ':']));
        }
        for invalid in ["", "../anthropic", "anthropic --all", "name@provider"] {
            assert!(!valid_plugin_id(invalid) || invalid.contains(['/', '@', ':']));
        }
    }
}
