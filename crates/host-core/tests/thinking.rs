#![cfg(unix)]
use host_core::Workbench;
use serde_json::json;
use std::path::PathBuf;

#[tokio::test]
async fn thinking_selection_launch_restore_and_model_change() {
    let root = std::env::temp_dir().join(format!("thinking-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("project")).unwrap();
    let w = Workbench::open(root.join("data")).await.unwrap();
    let executable = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/thinking-runtime.py");
    w.store.set_setting("executable", executable.display().to_string()).await.unwrap();
    let p = w.store.register_project("P", vec![root.join("project")], true).await.unwrap();
    let t = w.store.create_task(&p.id, "T").await.unwrap();
    let worker = w.clone();
    let result = tokio::spawn(async move {
        let w = worker;
        w.select_model(&t.id, "test", "wide").await.unwrap();
        w.select_thinking(&t.id, Some("high".into())).await.unwrap();
        for bad in ["max", "invented", "off"] {
            assert_eq!(w.select_thinking(&t.id, Some(bad.into())).await.unwrap_err().code, "model_thinking_unavailable");
            assert_eq!(w.store.task(&t.id).await.unwrap().thinking.as_deref(), Some("high"));
        }
        let first = w.continue_task(&t.id).await.unwrap();
        assert_eq!(first.runtime.capabilities["state"]["thinkingLevel"], "high");
        let args = first.runtime.capabilities["state"]["fixtureArgs"].as_array().unwrap();
        assert_eq!(args.iter().filter(|v| **v == "--thinking").count(), 1);
        assert_eq!(w.select_thinking(&t.id, None).await.unwrap_err().code, "task_running");
        assert!(w.select_model(&t.id, "test", "absent").await.is_err());
        assert_eq!(w.store.task(&t.id).await.unwrap().thinking.as_deref(), Some("high"));
        let command = w.runtime_command(&t.id).await.unwrap();
        assert!(command.arguments.windows(2).any(|w| w == ["--thinking", "high"]));
        w.stop(&t.id).await.unwrap();
        w.store.set_setting(&format!("project_model:{}", p.id), "test/narrow".into()).await.unwrap();
        let second = w.continue_task(&t.id).await.unwrap();
        assert_eq!(second.runtime.capabilities["state"]["thinkingLevel"], "high");
        assert_eq!(second.runtime.capabilities["state"]["model"]["id"], "wide");
        assert_eq!(first.runtime.capabilities["state"]["sessionId"], second.runtime.capabilities["state"]["sessionId"]);
        w.select_model(&t.id, "test", "narrow").await.unwrap();
        assert_eq!(w.store.task(&t.id).await.unwrap().thinking, None);
        w.stop(&t.id).await.unwrap();
        w.select_model(&t.id, "test", "unknown").await.unwrap();
        assert_eq!(w.select_thinking(&t.id, Some("low".into())).await.unwrap_err().code, "model_thinking_unknown");
        w.select_model(&t.id, "test", "fixed").await.unwrap();
        assert_eq!(w.select_thinking(&t.id, Some("low".into())).await.unwrap_err().code, "model_thinking_unavailable");
        w.select_model(&t.id, "test", "wide").await.unwrap();
        w.select_thinking(&t.id, None).await.unwrap();
        let default = w.continue_task(&t.id).await.unwrap();
        assert!(!default.runtime.capabilities["state"]["fixtureArgs"].as_array().unwrap().contains(&json!("--thinking")));
        w.stop(&t.id).await.unwrap();
        // Stale persisted capability must block startup without altering the selection.
        w.store.set_task_thinking(&t.id, Some("max".into())).await.unwrap();
        assert_eq!(w.continue_task(&t.id).await.unwrap_err().code, "model_thinking_unavailable");
        assert_eq!(w.store.task_thinking(&t.id).await.unwrap().as_deref(), Some("max"));
        w.shutdown().await.unwrap();
        let reopened = host_core::store::Store::open(root.join("data/desktop.sqlite")).await.unwrap();
        assert_eq!(reopened.task(&t.id).await.unwrap().thinking.as_deref(), Some("max"));
    }).await;
    w.shutdown().await.unwrap();
    result.unwrap();
}

#[tokio::test]
#[ignore = "requires explicit OMP_EXECUTABLE; starts real OMP without sending a prompt"]
async fn live_omp_thinking_launch_without_provider_request() {
    use host_core::{LaunchOptions, TaskManager};
    let executable = std::env::var("OMP_EXECUTABLE").expect("set OMP_EXECUTABLE explicitly");
    let root = std::env::temp_dir().join(format!("thinking-live-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let available = host_core::model_config::discover(std::path::Path::new(&executable), &root).await.unwrap();
    let supported: Vec<_> = available.models.iter().filter(|m| m["capability"]["thinking"]["support"] == "supported" && m["capability"]["reasoning"] != false).collect();
    let a = supported.first().expect("a configurable model");
    let b = supported.iter().find(|m| m["capability"]["thinking"]["levels"] != a["capability"]["thinking"]["levels"]).expect("two distinct capability sets");
    for (index, model) in [*a, *b].into_iter().enumerate() {
        for explicit in [true, false] {
            let levels = model["capability"]["thinking"]["levels"].as_array().unwrap();
            let level = explicit.then(|| levels.iter().find(|v| **v == "medium").unwrap_or(&levels[0]).as_str().unwrap().to_owned());
            let runtime = TaskManager::new(root.clone());
            let id = format!("live-{index}-{explicit}");
            let session_dir = root.join(&id);
            std::fs::create_dir_all(&session_dir).unwrap();
            runtime.start_with(id.clone(), Some(executable.clone()), LaunchOptions {
                root: root.clone(), model: Some(format!("{}/{}", model["provider"].as_str().unwrap(), model["id"].as_str().unwrap())),
                thinking: level.clone(), session_dir: Some(session_dir), ..Default::default()
            }).await.unwrap();
            let result = tokio::time::timeout(std::time::Duration::from_secs(40), async {
                loop {
                    let snapshot = runtime.snapshot(&id).await.unwrap();
                    if snapshot.status == "failed" { panic!("real OMP startup failed: {:?}", snapshot.error); }
                    if snapshot.status == "ready" {
                        let runtime_model = snapshot.runtime.capabilities["models"].as_array().unwrap().iter()
                            .find(|m| m["provider"] == model["provider"] && m["id"] == model["id"]).unwrap();
                        assert_eq!(runtime_model["capability"]["thinking"]["support"], "supported");
                        assert_eq!(runtime_model["capability"]["thinking"]["levels"], model["capability"]["thinking"]["levels"]);
                        if let Some(level) = &level { assert_eq!(snapshot.runtime.capabilities["state"]["thinkingLevel"], *level); }
                        println!("model={} explicit={} thinking={}", model["id"], explicit, snapshot.runtime.capabilities["state"]["thinkingLevel"]);
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }).await;
            runtime.shutdown().await.unwrap();
            result.unwrap();
        }
    }
}
