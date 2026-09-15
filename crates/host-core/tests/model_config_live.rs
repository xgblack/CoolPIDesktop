use host_core::model_config::{self, Edit, Provider};
use serde_json::json;
#[tokio::test]
#[ignore = "uses explicitly selected system OMP; set PI_CODING_AGENT_DIR to an empty isolated directory"]
async fn isolated_first_use_loads_saved_config() {
    let path = model_config::config_path().unwrap();
    assert!(!path.exists(), "requires empty isolated directory");
    let config = model_config::load(&path).unwrap();
    let edit = Edit {
        revision: config.revision,
        original_id: None,
        provider: Provider {
            id: "desktop-fixture".into(),
            base_url: Some("http://127.0.0.1:1/v1".into()),
            api: Some("openai-completions".into()),
            auth: Some("none".into()),
            auth_header: None,
            credential_configured: false,
            models: vec![json!({"id":"fixture-model","contextWindow":8192,"maxTokens":1024})],
        },
        credential_action: "keep".into(),
        credential: None,
        deleted: false,
    };
    model_config::save(&path, &edit).unwrap();
    let exe = std::path::PathBuf::from(std::env::var("OMP_EXECUTABLE").unwrap());
    let result = model_config::discover(&exe, &path.parent().unwrap().join("probe"))
        .await
        .unwrap();
    assert!(
        result
            .models
            .iter()
            .any(|m| m["provider"] == "desktop-fixture" && m["id"] == "fixture-model")
    );
    println!("PASS isolated models.yml saved and discovered by OMP");
}
#[tokio::test]
#[ignore = "paid connection to explicitly authorized model; no configuration writes"]
async fn configured_connection() {
    let exe = std::path::PathBuf::from(std::env::var("OMP_EXECUTABLE").unwrap());
    let model = std::env::var("OMP_TEST_MODEL").unwrap();
    let (provider, id) = model.split_once('/').unwrap();
    let root = std::env::temp_dir().join(format!("omp-model-connect-{}", uuid::Uuid::new_v4()));
    let result = model_config::connect(&exe, &root, provider, id)
        .await
        .unwrap();
    assert_eq!(result.stage, "connected");
    println!("PASS authorized OMP model connection completed");
}
#[tokio::test]
#[ignore = "downloads public catalog for the explicitly installed OMP version"]
async fn versioned_catalog_and_idle_task_reload() {
    let exe = std::path::PathBuf::from(std::env::var("OMP_EXECUTABLE").unwrap());
    let version = host_core::probe(&exe).await.unwrap().version.unwrap();
    let root = std::env::temp_dir().join(format!("omp-model-apply-{}", uuid::Uuid::new_v4()));
    let catalog = model_config::catalog(&root.join("cache"), &version)
        .await
        .unwrap();
    assert_eq!(catalog.version, version);
    assert!(catalog.models.iter().any(|m| m["id"] == "qwen3.7-flash"));
    assert!(
        model_config::catalog(&root.join("cache"), &version)
            .await
            .unwrap()
            .cached
    );
    std::fs::create_dir_all(root.join("project")).unwrap();
    let w = host_core::Workbench::open(root.join("host")).await.unwrap();
    w.store
        .set_setting("executable", exe.to_string_lossy().into())
        .await
        .unwrap();
    let p = w
        .store
        .register_project("test", vec![root.join("project")], true)
        .await
        .unwrap();
    let task = w.store.create_task(&p.id, "model reload").await.unwrap();
    w.store
        .set_model(&task.id, Some(std::env::var("OMP_TEST_MODEL").unwrap()))
        .await
        .unwrap();
    let before = w.continue_task(&task.id).await.unwrap();
    // OMP reports an unmaterialized session before the first reply; reloading must retain it.
    let pending = w.apply_model_config().await.unwrap();
    assert_eq!(pending["skipped"], json!([task.id]));
    assert_eq!(
        w.runtime.snapshot(&task.id).await.unwrap().run_id,
        before.run_id
    );
    w.request(
        &task.id,
        host_core::RpcRequest::Prompt,
        json!({"message":"Reply only OK. No tools."}),
    )
    .await
    .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(90), async {
        loop {
            let snap = w.runtime.snapshot(&task.id).await.unwrap();
            if snap.status == "idle" {
                break;
            }
            assert_ne!(snap.status, "failed");
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let bound = w.store.task(&task.id).await.unwrap().session_id;
    let result = w.apply_model_config().await.unwrap();
    assert_eq!(w.store.task(&task.id).await.unwrap().session_id, bound);
    assert!(
        w.runtime
            .snapshot(&task.id)
            .await
            .unwrap()
            .runtime
            .capabilities["models"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == "qwen3.7-flash")
    );
    assert!(
        !w.history(&task.id, None).await.unwrap()["messages"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    w.shutdown().await.unwrap();
    assert_eq!(result["errors"], json!([]));
    assert_eq!(result["applied"], json!([task.id]));
    let after = w.runtime.snapshot(&task.id).await.unwrap();
    assert_ne!(before.run_id, after.run_id);
    println!("PASS exact-version catalog cache and idle-task model reload");
}

#[tokio::test]
#[ignore = "requires explicit system OMP; discovery and startup only, no prompt"]
async fn discovered_model_matches_new_task_and_failed_switch_preserves_selection() {
    let exe = std::path::PathBuf::from(std::env::var("OMP_EXECUTABLE").unwrap());
    let root = std::env::temp_dir().join(format!("omp-model-selection-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("project")).unwrap();
    let discovery = model_config::discover(&exe, &root.join("probe"))
        .await
        .unwrap();
    assert!(!discovery.models.is_empty());
    // Last candidate exercises a selection other than the menu's first item.
    let model = discovery.models.last().unwrap();
    let provider = model["provider"].as_str().unwrap().to_owned();
    let id = model["id"].as_str().unwrap().to_owned();
    let selected = format!("{provider}/{id}");
    let w = host_core::Workbench::open(root.join("host")).await.unwrap();
    w.store
        .set_setting("executable", exe.to_string_lossy().into())
        .await
        .unwrap();
    let p = w
        .store
        .register_project("test", vec![root.join("project")], true)
        .await
        .unwrap();
    let task = w
        .create_task_with_model(&p.id, "selected", "shared", Some(selected.clone()))
        .await
        .unwrap();
    let worker = w.clone();
    let scenario = tokio::spawn(async move {
        let snapshot = worker.continue_task(&task.id).await.unwrap();
        let actual = &snapshot.runtime.capabilities["state"]["model"];
        assert_eq!(actual["provider"], provider);
        assert_eq!(actual["id"], id);
        assert_eq!(
            worker.store.task(&task.id).await.unwrap().model.as_deref(),
            Some(selected.as_str())
        );
        assert!(
            worker
                .select_model(&task.id, "not-configured", "not-a-model")
                .await
                .is_err()
        );
        assert_eq!(
            worker.store.task(&task.id).await.unwrap().model.as_deref(),
            Some(selected.as_str())
        );
        let state = worker
            .runtime
            .query(&task.id, host_core::RpcQuery::GetState, json!({}))
            .await
            .unwrap();
        assert_eq!(state["model"]["provider"], provider);
        assert_eq!(state["model"]["id"], id);
    })
    .await;
    w.shutdown().await.unwrap();
    scenario.unwrap();
    println!(
        "PASS discovered selection matches first startup; rejected switch retains stored and actual model; no prompt sent"
    );
}
