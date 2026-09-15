use crate::{HostError, RpcQuery, Workbench, files, session::session_header};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    io::{Cursor, Write},
    path::{Path, PathBuf},
};
use zip::{ZipWriter, write::SimpleFileOptions};

type Result<T> = std::result::Result<T, HostError>;
const SESSION_LIMIT: u64 = 64 * 1024 * 1024;
const EXPORT_LIMIT: u64 = 256 * 1024 * 1024;
fn failed(e: impl std::fmt::Display) -> HostError {
    HostError::new("session_export_failed", e)
}

pub(crate) fn read_file(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let parent = path.parent().ok_or_else(|| failed("无效会话资源路径"))?;
    let dir = files::absolute_dir(parent)?;
    let file = files::open_at(
        &dir,
        path.file_name().ok_or_else(|| failed("无效文件名"))?,
        libc::O_RDONLY,
    )?;
    files::read_bounded(&file, limit)
}

fn add_reference(value: Option<&str>, refs: &mut BTreeSet<String>) -> Result<()> {
    if let Some(hash) = value.and_then(|s| s.strip_prefix("blob:sha256:")) {
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err(failed("会话包含无效 blob 引用"));
        }
        refs.insert(hash.into());
    }
    Ok(())
}

fn references(value: &Value, refs: &mut BTreeSet<String>) -> Result<()> {
    match value {
        Value::Array(values) => {
            for value in values {
                references(value, refs)?;
            }
        }
        Value::Object(values) => {
            // OMP persists image blocks, images/frames payloads, image_url, and generation results.
            if value["type"] == "image"
                || value["mimeType"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("image/"))
            {
                add_reference(value["data"].as_str(), refs)?;
            }
            if value["type"] == "image_generation_call" {
                add_reference(value["result"].as_str(), refs)?;
            }
            if let Some(image) = values.get("image_url") {
                add_reference(image.as_str().or_else(|| image["url"].as_str()), refs)?;
            }
            for value in values.values() {
                references(value, refs)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn blob_path(file: &Path, hash: &str) -> Result<PathBuf> {
    for parent in file.ancestors().skip(1) {
        let candidate = parent.join("blobs").join(hash);
        match candidate.try_exists() {
            Ok(true) => return Ok(candidate),
            Ok(false) => {}
            Err(e) => return Err(failed(e)),
        }
    }
    let agent = std::env::var_os("PI_CODING_AGENT_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".omp/agent")));
    if let Some(agent) = agent {
        return Ok(agent.join("blobs").join(hash));
    }
    Err(failed("会话引用的 blob 不存在，无法完整导出"))
}

fn archive(file: &Path, expected: &str) -> Result<Vec<u8>> {
    if session_header(file)?["id"].as_str() != Some(expected) {
        return Err(HostError::new(
            "session_mismatch",
            "会话 ID 与任务绑定不一致",
        ));
    }
    let raw = read_file(file, SESSION_LIMIT)?;
    let mut refs = BTreeSet::new();
    // Parse every entry, including branches and compaction records, never just the visible page.
    let mut header_seen = false;
    for line in raw.split(|b| *b == b'\n').filter(|line| !line.is_empty()) {
        let entry: Value = serde_json::from_slice(line)
            .map_err(|_| failed("会话记录不完整或无效，请在写入结束后重试"))?;
        if entry["type"] == "session" {
            if entry["id"].as_str() != Some(expected) {
                return Err(HostError::new("session_mismatch", "会话身份发生变化"));
            }
            header_seen = true;
        }
        references(&entry, &mut refs)?;
    }
    if !header_seen {
        return Err(failed("会话缺少身份记录"));
    }
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("session.jsonl", options).map_err(failed)?;
    zip.write_all(&raw).map_err(failed)?;
    let mut total = raw.len() as u64;
    for hash in refs {
        let bytes = read_file(&blob_path(file, &hash)?, EXPORT_LIMIT - total)?;
        total += bytes.len() as u64;
        zip.start_file(format!("blobs/{hash}"), options)
            .map_err(failed)?;
        zip.write_all(&bytes).map_err(failed)?;
    }
    zip.start_file("README.txt", options).map_err(failed)?;
    zip.write_all("OMP 会话归档：session.jsonl 为原始持久化记录，blobs/ 为其引用的资源。\n导出当前会话的全部已保存分支和压缩记录；不包含独立子会话、工作区文件或未发送附件。\n这不是自动导入包。恢复时须将 blobs/ 中资源放入目标 OMP 配置的 blob store（默认 ~/.omp/agent/blobs），并准备原工作目录与模型，然后使用 omp --resume /绝对路径/session.jsonl。\n".as_bytes()).map_err(failed)?;
    Ok(zip.finish().map_err(failed)?.into_inner())
}

impl Workbench {
    /// Reads only the task's bound session. No caller-controlled source path or runtime writes.
    pub async fn export_session(&self, id: &str) -> Result<Vec<u8>> {
        let _guard = self.gate.lock().await;
        self.check_handoff(id).await?;
        let task = self.store.task(id).await?;
        if let Ok(snapshot) = self.runtime.snapshot(id).await {
            if !matches!(snapshot.status.as_str(), "stopped" | "failed") {
                if !matches!(snapshot.status.as_str(), "ready" | "idle" | "interrupted")
                    || !snapshot.pending_ui.is_empty()
                {
                    return Err(HostError::new(
                        "task_busy",
                        "请等待生成或审批结束后下载会话",
                    ));
                }
                let state = self
                    .runtime
                    .query(id, RpcQuery::GetState, json!({}))
                    .await?;
                if state["isStreaming"] != false
                    || state["isCompacting"] != false
                    || state["queuedMessageCount"] != 0
                {
                    return Err(HostError::new(
                        "task_busy",
                        "请等待生成、压缩和排队消息处理结束后下载",
                    ));
                }
            }
        }
        let file = task.session_file.ok_or_else(|| {
            HostError::new("session_not_saved", "会话尚未保存，请在首次回复完成后下载")
        })?;
        let expected = task
            .session_id
            .ok_or_else(|| failed("尚未绑定 OMP 会话 ID"))?;
        tokio::task::spawn_blocking(move || archive(&file, &expected))
            .await
            .map_err(failed)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn collects_omp_image_payload_shapes_without_treating_prose_as_a_resource() {
        let hashes: Vec<String> = (b'a'..=b'e')
            .map(|c| char::from(c).to_string().repeat(64))
            .collect();
        let reference = |i: usize| format!("blob:sha256:{}", hashes[i]);
        let value = json!({"content":[{"type":"image","data":reference(0)}, {"type":"text","text":"blob:sha256:not-an-image"}],
            "images":[{"data":reference(1),"mimeType":"image/png"}],
            "frames":[{"data":reference(2),"mimeType":"image/png"}],
            "providerPayload":{"items":[{"image_url":reference(3)},{"type":"image_generation_call","result":reference(4)}]}});
        let mut refs = BTreeSet::new();
        references(&value, &mut refs).unwrap();
        assert_eq!(refs, hashes.into_iter().collect());
    }

    #[tokio::test]
    async fn exports_full_persisted_session_and_only_referenced_blobs_without_starting_omp() {
        let root = std::env::temp_dir()
            .canonicalize()
            .unwrap()
            .join(format!("session-export-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let w = Workbench::open(root.join("data")).await.unwrap();
        let project = w
            .store
            .register_project("P", vec![root.clone()], true)
            .await
            .unwrap();
        let task = w.store.create_task(&project.id, "T").await.unwrap();
        assert_eq!(
            w.export_session(&task.id).await.unwrap_err().code,
            "session_not_saved"
        );
        let dir = root.join("data/sessions").join(&task.id);
        std::fs::create_dir_all(dir.join("blobs")).unwrap();
        let hash = "a".repeat(64);
        let file = dir.join("original.jsonl");
        let mut raw = format!(
            "{}\n{}\n",
            json!({"type":"title","v":1,"title":"中文标题","pad":"","updatedAt":"now"}),
            json!({"type":"session","version":3,"id":"omp-session-id","cwd":root})
        );
        for i in 0..100 {
            raw.push_str(&format!("{}\n", json!({"type":"message","id":i.to_string(),"parentId":null,"message":{"role":"user","content":"完整历史"}})));
        }
        raw.push_str(&format!("{}\n", json!({"type":"message","message":{"content":[{"type":"image","data":format!("blob:sha256:{hash}"),"mimeType":"image/png"}]}})));
        raw.push_str(&format!(
            "{}\n",
            json!({"type":"compaction","summary":"旧分支也保留"})
        ));
        std::fs::write(&file, &raw).unwrap();
        std::fs::write(dir.join("blobs").join(&hash), b"image bytes").unwrap();
        std::fs::write(
            dir.join("blobs").join("b".repeat(64)),
            b"unrelated private bytes",
        )
        .unwrap();
        w.store
            .bind(&task.id, "omp-session-id", file.clone())
            .await
            .unwrap();
        let bytes = w.export_session(&task.id).await.unwrap();
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert_eq!(zip.len(), 3);
        let mut exported = String::new();
        zip.by_name("session.jsonl")
            .unwrap()
            .read_to_string(&mut exported)
            .unwrap();
        assert_eq!(exported, raw);
        let mut blob = Vec::new();
        zip.by_name(&format!("blobs/{hash}"))
            .unwrap()
            .read_to_end(&mut blob)
            .unwrap();
        assert_eq!(blob, b"image bytes");
        assert!(w.runtime.snapshots().await.is_empty());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), raw);

        std::fs::rename(dir.join("blobs").join(&hash), root.join("retained-blob")).unwrap();
        assert!(
            w.export_session(&task.id).await.is_err(),
            "Missing resources must fail instead of exporting an incomplete ZIP"
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("retained-blob"), dir.join("blobs").join(&hash))
                .unwrap();
            assert!(
                w.export_session(&task.id).await.is_err(),
                "Do not follow substituted blob symlinks"
            );
        }
        std::fs::write(&file, raw.replace("omp-session-id", "other-session")).unwrap();
        assert_eq!(
            w.export_session(&task.id).await.unwrap_err().code,
            "session_mismatch"
        );
        std::fs::write(&file, format!("{raw}{{\"partial\":")).unwrap();
        assert_eq!(
            w.export_session(&task.id).await.unwrap_err().code,
            "session_export_failed"
        );
        std::fs::write(
            &file,
            raw.replace(&format!("blob:sha256:{hash}"), "blob:sha256:../../private"),
        )
        .unwrap();
        assert_eq!(
            w.export_session(&task.id).await.unwrap_err().code,
            "session_export_failed"
        );
        std::fs::write(&file, &raw).unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&file)
            .unwrap()
            .set_len(SESSION_LIMIT + 1)
            .unwrap();
        assert_eq!(
            w.export_session(&task.id).await.unwrap_err().code,
            "file_too_large"
        );
    }
}
