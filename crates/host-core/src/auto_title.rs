use crate::{HostError, runtime};
use std::{path::Path, time::Duration};
use tokio::time::timeout;

const MAX_INPUT_CHARS: usize = 4096;
const MAX_OUTPUT_BYTES: usize = 16 * 1024;
const TITLE_TIMEOUT: Duration = Duration::from_secs(15);

pub(crate) fn low_signal(input: &str) -> bool {
    let value = input.trim().to_ascii_lowercase();
    value.is_empty()
        || value.len() < 4
        || matches!(
            value.as_str(),
            "hi" | "hello" | "hey" | "你好" | "嗨" | "在吗"
        )
}

pub(crate) async fn generate(
    executable: &Path,
    root: &Path,
    model: Option<&str>,
    input: &str,
) -> Result<Option<String>, HostError> {
    if low_signal(input) {
        return Ok(None);
    }
    let input: String = input.chars().take(MAX_INPUT_CHARS).collect();
    let prompt = format!(
        "Generate one concise task title for the following user request. Return only the title, without quotes, markdown, explanation, or punctuation at the end. Keep it under 60 characters.\n\nUser request:\n{input}"
    );
    let mut command = runtime::command(executable);
    command.args(["--print", "--no-title", "--cwd"]).arg(root);
    if let Some(model) = model.filter(|value| !value.trim().is_empty()) {
        command.args(["--model", model]);
    }
    command
        .arg(prompt)
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let output = timeout(TITLE_TIMEOUT, command.output())
        .await
        .map_err(|_| HostError::new("title_timeout", "OMP title generation timed out"))?
        .map_err(|e| HostError::new("title_process_failed", e))?;
    if !output.status.success() {
        return Err(HostError::new(
            "title_process_failed",
            "OMP title generation process failed",
        ));
    }
    if output.stdout.len() > MAX_OUTPUT_BYTES {
        return Err(HostError::new(
            "title_invalid",
            "OMP title output is too large",
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(normalize(&text))
}

fn normalize(output: &str) -> Option<String> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?
        .trim();
    let line = line
        .strip_prefix("Title:")
        .or_else(|| line.strip_prefix("标题："))
        .unwrap_or(line)
        .trim()
        .trim_matches(|c| matches!(c, '`' | '"' | '\''))
        .trim();
    let line: String = line
        .chars()
        .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
        .take(80)
        .collect();
    (!line.is_empty()).then_some(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_low_signal_input() {
        assert!(low_signal("hello"));
        assert!(low_signal("  "));
        assert!(!low_signal("修复登录页面的状态同步问题"));
    }

    #[test]
    fn normalizes_model_output() {
        assert_eq!(
            normalize("\nTitle: `修复登录状态`\n说明").as_deref(),
            Some("修复登录状态")
        );
        assert_eq!(normalize("  \n\n").as_deref(), None);
    }
}
