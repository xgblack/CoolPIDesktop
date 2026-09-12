use host_core::trajectory_history::{image, read};
use serde_json::json;
use std::{fs, path::PathBuf};

fn fixture(body: &str) -> (PathBuf, String) {
    let id = "session-test".to_string();
    let p = std::env::temp_dir().join(format!("cool-pi-trajectory-{}.jsonl", uuid::Uuid::new_v4()));
    let header = json!({"type":"session","version":3,"id":id,"timestamp":"2026-09-10T00:00:00Z","cwd":"/tmp"});
    fs::write(&p, format!("{header}\n{body}")).unwrap();
    (p, id)
}

#[test]
fn projects_messages_and_pairs_tool_result() {
    let (p, id) = fixture(&format!(
        "{}\n{}\n{}\n",
        json!({"type":"message","id":"u","parentId":null,"timestamp":1,"message":{"role":"user","content":"hello"}}),
        json!({"type":"message","id":"a","parentId":"u","timestamp":2,"message":{"role":"assistant","content":[{"type":"text","text":"ok"},{"type":"toolCall","id":"c1","name":"read","arguments":{"path":"x"}}],"usage":{"inputTokens":2}}}),
        json!({"type":"toolResult","toolCallId":"c1","timestamp":3,"result":{"content":[{"type":"text","text":"done"}]},"isError":false}),
    ));
    let h = read(&p, &id).unwrap();
    assert_eq!(h.records.len(), 3);
    assert_eq!(h.records[2].status, "succeeded");
    assert_eq!(h.records[2].tool_call_id.as_deref(), Some("c1"));
    assert_eq!(
        h.records[1].usage.as_ref().and_then(|u| u.input_tokens),
        Some(2)
    );
    fs::remove_file(p).ok();
}

#[test]
fn rejects_wrong_identity_but_allows_half_written_tail() {
    let (p, id) = fixture(&format!(
        "{}\n{{\"type\":\"message\"",
        json!({"type":"message","id":"u","message":{"role":"user","content":"x"}})
    ));
    assert_eq!(read(&p, "other").unwrap_err().code, "session_mismatch");
    let h = read(&p, &id).unwrap();
    assert_eq!(h.records.len(), 1);
    assert!(!h.warnings.is_empty());
    fs::remove_file(p).ok();
}

#[test]
fn resolves_owned_raster_blob_only() {
    let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let (p, id) = fixture(&json!({"type":"message","id":"u","message":{"role":"user","content":[{"type":"image","mimeType":"image/png","data":format!("blob:sha256:{hash}")}]}}).to_string());
    let blobs = p.parent().unwrap().join("blobs");
    fs::create_dir_all(&blobs).unwrap();
    fs::write(blobs.join(hash), b"png").unwrap();
    let got = image(&p, &id, "u", hash).unwrap();
    assert_eq!(got["mime"], "image/png");
    assert!(image(&p, &id, "u", "deadbeef").is_err());
    fs::remove_file(blobs.join(hash)).ok();
    fs::remove_dir(blobs).ok();
    fs::remove_file(p).ok();
}
