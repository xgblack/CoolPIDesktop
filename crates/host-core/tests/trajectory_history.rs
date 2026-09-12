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

#[test]
fn real_omp_message_shape_and_multiple_tools() {
    let (p, id) = fixture(&format!(
        "{}\n{}\n{}\n{}\n",
        json!({"type":"message","id":"u","parentId":null,"timestamp":"2026-09-12T00:00:00Z","message":{"role":"user","timestamp":1000,"content":"hi"}}),
        json!({"type":"message","id":"a","parentId":"u","message":{"role":"assistant","timestamp":2000,"completedAt":2050,"duration":45,"ttft":5,"usage":{"input":12,"output":7,"cacheRead":3,"cost":{"total":0.01}},"content":[{"type":"toolCall","id":"one","name":"read","arguments":{"path":"one"}},{"type":"toolCall","id":"two","name":"read","arguments":{"path":"two"}}]}}),
        json!({"type":"message","id":"t1","parentId":"a","message":{"role":"toolResult","toolCallId":"one","timestamp":3000,"isError":false,"content":[{"type":"text","text":"first"}]}}),
        json!({"type":"message","id":"t2","parentId":"t1","message":{"role":"toolResult","toolCallId":"two","timestamp":4000,"isError":true,"content":[{"type":"text","text":"failure"}]}})
    ));
    let h = read(&p, &id).unwrap();
    assert_eq!(h.records.len(), 4);
    assert_eq!(h.records[0].turn, Some(1));
    assert_eq!(h.records[1].step, Some(1));
    assert_eq!(h.records[1].duration_ms, Some(45.));
    assert_eq!(h.records[1].started_at, Some(2000.));
    assert_eq!(h.records[1].ttft_ms, Some(5.));
    assert_eq!(h.records[1].usage.as_ref().unwrap().input_tokens, Some(12));
    assert_eq!(h.records[2].parent_id.as_deref(), Some("a"));
    assert_eq!(h.records[3].parent_id.as_deref(), Some("a"));
    assert_eq!(h.records[2].input.as_ref().unwrap()["path"], "one");
    assert_eq!(h.records[3].status, "failed");
    assert_eq!(h.records[2].duration_ms, None);
}
#[test]
fn stable_pages_survive_append_and_invalid_cursor_is_visible() {
    use host_core::trajectory_history::page;
    let body=(0..125).map(|i|json!({"type":"message","id":format!("m{i}"),"message":{"role":"user","timestamp":i,"content":i.to_string()}}).to_string()+"\n").collect::<String>();
    let (p, id) = fixture(&body);
    let tail = page(read(&p, &id).unwrap(), None, false).unwrap();
    assert_eq!(tail.records.len(), 50);
    assert_eq!(tail.records[0].id, "m75");
    let older = page(read(&p, &id).unwrap(), tail.next_cursor.as_deref(), false).unwrap();
    assert_eq!(older.records[0].id, "m25");
    assert_eq!(older.records.last().unwrap().id, "m74");
    assert_eq!(
        page(read(&p, &id).unwrap(), Some("missing"), false)
            .unwrap_err()
            .code,
        "stale_cursor"
    );
    let next = page(read(&p, &id).unwrap(), Some("m100"), true).unwrap();
    assert_eq!(next.records.len(), 25);
}
#[test]
fn branch_projection_does_not_mix_abandoned_replies() {
    let (p, id) = fixture(&format!(
        "{}\n{}\n{}\n",
        json!({"type":"message","id":"u","parentId":null,"message":{"role":"user","content":"root"}}),
        json!({"type":"message","id":"old","parentId":"u","message":{"role":"assistant","content":"old branch"}}),
        json!({"type":"message","id":"new","parentId":"u","message":{"role":"assistant","content":"current"}})
    ));
    assert_eq!(
        read(&p, &id)
            .unwrap()
            .records
            .iter()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        vec!["u", "new"]
    );
}
#[test]
fn inline_images_use_owned_resource_api_and_are_not_sent_in_pages() {
    use host_core::trajectory_history::page;
    let(p,id)=fixture(&json!({"type":"message","id":"img","message":{"role":"user","content":[{"type":"image","mimeType":"image/png","data":"cG5n"}]}}).to_string());
    assert_eq!(image(&p, &id, "img", "inline:0").unwrap()["data"], "cG5n");
    let p = page(read(&p, &id).unwrap(), None, false).unwrap();
    assert_ne!(p.records[0].content[0]["data"], "cG5n");
}

#[test]
fn large_inline_image_is_not_lost_to_text_truncation() {
    use base64::Engine;
    let data = base64::engine::general_purpose::STANDARD.encode(vec![0u8; 1024 * 1024]);
    let(p,id)=fixture(&json!({"type":"message","id":"large","message":{"role":"user","content":[{"type":"image","mimeType":"image/png","data":data}]}}).to_string());
    let h = read(&p, &id).unwrap();
    assert_eq!(h.records[0].images.len(), 1);
    assert!(h.records[0].content.to_string().len() < 200);
    assert_eq!(
        image(&p, &id, "large", "inline:0").unwrap()["data"]
            .as_str()
            .unwrap()
            .len(),
        data.len()
    );
}
