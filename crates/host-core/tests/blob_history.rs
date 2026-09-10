use host_core::Workbench;
use serde_json::json;
use std::{path::PathBuf, process::Command};

#[tokio::test]
#[ignore = "requires real OMP and isolated PI_CODING_AGENT_DIR; no model calls"]
async fn omp_resolves_blob_history() {
    let agent = PathBuf::from(
        std::env::var("PI_CODING_AGENT_DIR").expect("isolated agent directory required"),
    );
    assert!(agent.starts_with(std::env::temp_dir()) || agent.starts_with("/tmp"));
    let root = agent.join("fixture");
    std::fs::create_dir_all(&root).unwrap();
    let blobdir = agent.join("blobs");
    std::fs::create_dir_all(&blobdir).unwrap();
    let image = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/desktop/src-tauri/icons/icon.png");
    let hash = String::from_utf8(
        Command::new("shasum")
            .args(["-a", "256"])
            .arg(&image)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .split_whitespace()
    .next()
    .unwrap()
    .to_owned();
    std::fs::copy(&image, blobdir.join(&hash)).unwrap();
    let encoded = String::from_utf8(
        Command::new("base64")
            .arg("-i")
            .arg(&image)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned();
    let w = Workbench::open(root.join("data")).await.unwrap();
    let info = w
        .detect(Some(std::env::var("OMP_EXECUTABLE").unwrap()))
        .await;
    assert_eq!(info.status, "ready", "{:?}", info.error);
    let p = w
        .store
        .register_project("No models", vec![root.clone()], true)
        .await
        .unwrap();
    let t = w.store.create_task(&p.id, "Blob history").await.unwrap();
    let dir = root.join("data/sessions").join(&t.id);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("fixture.jsonl");
    let id = uuid::Uuid::new_v4().to_string();
    let header =
        json!({"type":"session","version":3,"id":id,"timestamp":"2026-09-10T00:00:00Z","cwd":root});
    let message = json!({"type":"message","id":"user-1","parentId":null,"timestamp":"2026-09-10T00:00:01Z","message":{"role":"user","content":[{"type":"text","text":"Image history fixture"},{"type":"image","mimeType":"image/png","data":format!("blob:sha256:{hash}")}],"timestamp":1}});
    std::fs::write(&file, format!("{header}\n{message}\n")).unwrap();
    w.store.bind(&t.id, &id, file).await.unwrap();
    let start = w.continue_task(&t.id).await;
    if start.is_err() {
        w.shutdown().await.unwrap();
    }
    start.unwrap();
    let page = w.history(&t.id, None).await;
    w.shutdown().await.unwrap();
    let page = page.unwrap();
    assert_eq!(page["messages"][0]["content"][1]["data"], encoded);
    println!("PASS real OMP resolves external PNG blob without a model call");
}

#[tokio::test]
#[ignore = "requires real OMP and isolated empty PI_CODING_AGENT_DIR"]
async fn no_model_still_allows_metadata_management() {
    let root = PathBuf::from(std::env::var("PI_CODING_AGENT_DIR").unwrap()).join("no-model-test");
    let w = Workbench::open(root.join("data")).await.unwrap();
    let info = w
        .detect(Some(std::env::var("OMP_EXECUTABLE").unwrap()))
        .await;
    assert_eq!(info.status, "model_required", "{:?}", info.error);
    let p = w
        .store
        .register_project("No model", vec![root], true)
        .await
        .unwrap();
    let t = w
        .store
        .create_task(&p.id, "Manage without model")
        .await
        .unwrap();
    w.update_task(&t.id, "Saved", true, true).await.unwrap();
    assert_eq!(w.records().await.unwrap()[0].title, "Saved");
    assert!(w.runtime.snapshots().await.is_empty());
    println!(
        "PASS empty OMP configuration reports model_required and preserves project/task management"
    );
}
