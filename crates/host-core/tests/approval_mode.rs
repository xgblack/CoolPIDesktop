#![cfg(unix)]
use host_core::Workbench;
use serde_json::json;
use std::{path::PathBuf, time::Duration};

async fn setup() -> (Workbench, String, PathBuf) {
    let root = std::env::temp_dir().join(format!("approval-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("project")).unwrap();
    let w = Workbench::open(root.join("data")).await.unwrap();
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/approval-runtime.py");
    w.store
        .set_setting("executable", fixture.display().to_string())
        .await
        .unwrap();
    let project = w
        .store
        .register_project("Project", vec![root.join("project")], true)
        .await
        .unwrap();
    let task = w.store.create_task(&project.id, "Task").await.unwrap();
    (w, task.id, root)
}

#[tokio::test]
async fn mode_restart_preserves_session_terminal_and_other_task() {
    let (w, id, root) = setup().await;
    let worker = w.clone();
    let result = tokio::spawn(async move {
        let w = worker;
        assert_eq!(w.set_approval_mode(&id, Some("bad".into())).await.unwrap_err().code, "invalid_approval_mode");
        w.set_approval_mode(&id, Some("always-ask".into())).await.unwrap();
        assert!(w.runtime.snapshots().await.is_empty());
        let before = w.continue_task(&id).await.unwrap();
        let terminal = w.terminal_create(&id, 0).await.unwrap();
        let record = w.store.task(&id).await.unwrap();
        let other = w.store.create_task(&record.project_id, "Other").await.unwrap();
        let other_run = w.continue_task(&other.id).await.unwrap();
        for mode in [Some("write"), Some("yolo"), None] {
            let changed = w.set_approval_mode(&id, mode.map(str::to_owned)).await.unwrap();
            let after = w.runtime.snapshot(&id).await.unwrap();
            assert_ne!(before.run_id, after.run_id);
            assert_eq!(changed.session_id, record.session_id);
            assert_eq!(changed.session_file, record.session_file);
            assert_eq!(after.runtime.capabilities["state"]["fixtureApproval"], json!(mode));
            assert_eq!(w.runtime.snapshot(&other.id).await.unwrap().run_id, other_run.run_id);
            assert!(!w.terminal_snapshot(&id, &terminal.id, 0).await.unwrap().exited);
        }
        for state in [json!({"isStreaming":true}), json!({"isCompacting":true}), json!({"queuedMessageCount":1}), json!({"queuedMessageCount":null})] {
            std::fs::write(root.join("project/state.json"), state.to_string()).unwrap();
            assert_eq!(w.set_approval_mode(&id, Some("write".into())).await.unwrap_err().code, "task_busy");
            assert_eq!(w.store.task(&id).await.unwrap().approval_mode, None);
        }
        std::fs::write(root.join("project/state.json"), "{}").unwrap();
        std::fs::write(root.join("project/fail-start"), "").unwrap();
        assert_eq!(w.set_approval_mode(&id, Some("always-ask".into())).await.unwrap_err().code, "approval_switch_failed");
        assert!(matches!(w.runtime.snapshot(&id).await.unwrap().status.as_str(), "stopped" | "failed"));
        assert_eq!(w.store.task(&id).await.unwrap().approval_mode.as_deref(), Some("always-ask"));
        assert!(!w.terminal_snapshot(&id, &terminal.id, 0).await.unwrap().exited);
        w.shutdown().await.unwrap();
        let reopened = host_core::store::Store::open(root.join("data/desktop.sqlite")).await.unwrap();
        assert_eq!(reopened.task(&id).await.unwrap().approval_mode.as_deref(), Some("always-ask"));
        println!("PASS task-only restart, exact session, terminal/other task isolation, busy/unknown state rejection, fail-closed policy retention");
    }).await;
    w.shutdown().await.unwrap();
    result.unwrap();
}

#[tokio::test]
async fn unsaved_session_and_pending_approval_are_not_interrupted() {
    let (w, id, root) = setup().await;
    let worker = w.clone();
    let result = tokio::spawn(async move {
        std::fs::write(root.join("project/unsaved"), "").unwrap();
        let before = worker.continue_task(&id).await.unwrap();
        assert_eq!(
            worker
                .set_approval_mode(&id, Some("write".into()))
                .await
                .unwrap_err()
                .code,
            "session_not_saved"
        );
        worker
            .request(&id, "prompt", json!({"message":"approval"}))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while worker
                .runtime
                .snapshot(&id)
                .await
                .unwrap()
                .pending_ui
                .is_empty()
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            worker
                .set_approval_mode(&id, Some("yolo".into()))
                .await
                .unwrap_err()
                .code,
            "task_busy"
        );
        assert_eq!(
            worker.runtime.snapshot(&id).await.unwrap().run_id,
            before.run_id
        );
        assert_eq!(
            worker.runtime.snapshot(&id).await.unwrap().pending_ui.len(),
            1
        );
    })
    .await;
    w.shutdown().await.unwrap();
    result.unwrap();
}
