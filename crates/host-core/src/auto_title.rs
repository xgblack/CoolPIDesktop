use crate::{HostError, runtime};
use std::{path::Path, time::Duration};
use tokio::time::timeout;

const MAX_INPUT_CHARS: usize = 2000;
const MAX_OUTPUT_BYTES: usize = 16 * 1024;
const TITLE_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_TITLE_CHARS: usize = 80;
const MAX_TITLE_WORDS: usize = 12;
const TITLE_SYSTEM_PROMPT: &str = "Write a roughly five-word title for the next user message. Return only the title inside <title> tags. If the message has no concrete task, return <title/>.";

const FILLER_TOKENS: &[&str] = &[
    "hi",
    "hii",
    "hiii",
    "hiya",
    "hey",
    "heya",
    "hello",
    "helo",
    "hullo",
    "yo",
    "ya",
    "sup",
    "wassup",
    "whatsup",
    "howdy",
    "greetings",
    "hola",
    "ciao",
    "aloha",
    "gm",
    "gn",
    "good",
    "morning",
    "afternoon",
    "evening",
    "night",
    "day",
    "thanks",
    "thank",
    "thx",
    "ty",
    "tysm",
    "cheers",
    "please",
    "pls",
    "plz",
    "ok",
    "okay",
    "okey",
    "k",
    "kk",
    "yep",
    "yes",
    "yeah",
    "yup",
    "nope",
    "no",
    "nah",
    "sure",
    "cool",
    "nice",
    "great",
    "awesome",
    "perfect",
    "lol",
    "lmao",
    "haha",
    "hehe",
    "test",
    "tests",
    "testing",
    "ping",
    "pong",
    "there",
    "you",
    "u",
    "hmm",
    "hmmm",
    "um",
    "uh",
    "so",
    "well",
    "anyway",
    "你好",
    "您好",
    "嗨",
    "在吗",
    "谢谢",
];

fn words(value: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    for character in value.chars() {
        if character.is_alphanumeric() {
            current.extend(character.to_lowercase());
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

pub(crate) fn low_signal(input: &str) -> bool {
    let words = words(input);
    words.is_empty()
        || words.iter().all(|word| {
            word.chars().all(|character| character.is_ascii_digit())
                || FILLER_TOKENS.contains(&word.as_str())
        })
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
    let prompt = format!("<user>\n{input}\n</user>");
    let mut command = runtime::command(executable);
    command
        .args([
            "--print",
            "--no-session",
            "--no-title",
            "--no-tools",
            "--no-extensions",
            "--no-skills",
            "--no-rules",
            "--max-time",
            "15s",
            "--system-prompt",
            TITLE_SYSTEM_PROMPT,
            "--cwd",
        ])
        .arg(root);
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
    let line = line
        .strip_prefix("<title>")
        .and_then(|line| line.strip_suffix("</title>"))
        .unwrap_or(line)
        .trim()
        .trim_matches(|c| matches!(c, '"' | '\''))
        .trim_end_matches(|c| matches!(c, '.' | '!' | '?' | '。' | '！' | '？'))
        .trim();
    if line.eq_ignore_ascii_case("none") || line.eq_ignore_ascii_case("<title/>") {
        return None;
    }
    let line: String = line
        .chars()
        .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
        .collect();
    let word_count = words(&line).len();
    (!line.is_empty()
        && line.chars().count() <= MAX_TITLE_CHARS
        && (1..=MAX_TITLE_WORDS).contains(&word_count))
    .then_some(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_low_signal_input() {
        assert!(low_signal("hello"));
        assert!(low_signal("okay, thanks!"));
        assert!(low_signal("12345"));
        assert!(low_signal("..."));
        assert!(low_signal("  "));
        assert!(!low_signal("fix"));
        assert!(!low_signal("修复登录页面的状态同步问题"));
    }

    #[test]
    fn normalizes_model_output() {
        assert_eq!(
            normalize("\nTitle: `修复登录状态`\n说明").as_deref(),
            Some("修复登录状态")
        );
        assert_eq!(
            normalize("<title>Fix login timeout.</title>").as_deref(),
            Some("Fix login timeout")
        );
        assert_eq!(normalize("<title/>").as_deref(), None);
        assert_eq!(normalize("none").as_deref(), None);
        assert_eq!(normalize(&"word ".repeat(13)), None);
        assert_eq!(normalize(&"x".repeat(81)), None);
        assert_eq!(normalize("  \n\n").as_deref(), None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn generation_is_ephemeral_and_has_no_agent_capabilities() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!("omp-auto-title-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let executable = root.join("fake-omp");
        std::fs::write(
            &executable,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$0.args\"\nprintf '<title>Fix login timeout.</title>\\n'\n",
        ).unwrap();
        let mut permissions = std::fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&executable, permissions).unwrap();

        let title = generate(
            &executable,
            &root,
            Some("test/model"),
            "Fix the login timeout",
        )
        .await
        .unwrap();
        assert_eq!(title.as_deref(), Some("Fix login timeout"));
        let arguments = std::fs::read_to_string(executable.with_extension("args")).unwrap();
        for flag in [
            "--print",
            "--no-session",
            "--no-title",
            "--no-tools",
            "--no-extensions",
            "--no-skills",
            "--no-rules",
        ] {
            assert!(
                arguments.lines().any(|argument| argument == flag),
                "missing {flag}"
            );
        }
        assert!(arguments.lines().any(|argument| argument == "test/model"));
        assert!(arguments.contains("<user>\nFix the login timeout\n</user>"));
    }
}
