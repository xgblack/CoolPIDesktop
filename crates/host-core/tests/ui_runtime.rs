#![cfg(unix)]
use host_core::TaskManager;
use serde_json::{Value, json};
use std::{path::PathBuf, time::Duration};

#[tokio::test]
async fn ui_response_validates_correlates_and_expires() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let manager = TaskManager::new(root.clone());
    let worker = manager.clone();
    let outcome = tokio::spawn(async move {
        worker
            .start(
                "approval".into(),
                Some(
                    root.join("tests/fixtures/ui-runtime.py")
                        .to_string_lossy()
                        .into(),
                ),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while worker.snapshot("approval").await.unwrap().status == "starting" {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        worker
            .request("approval", "prompt", json!({"message":"confirm"}))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while worker
                .snapshot("approval")
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
        let invalid = worker
            .request(
                "approval",
                "extension_ui_response",
                json!({"id":"approval-1","confirmed":"yes"}),
            )
            .await
            .unwrap_err();
        assert_eq!(invalid.code, "invalid_ui_response");
        assert_eq!(
            worker.snapshot("approval").await.unwrap().pending_ui.len(),
            1
        );
        worker
            .request(
                "approval",
                "extension_ui_response",
                json!({"id":"approval-1","confirmed":false}),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while worker.snapshot("approval").await.unwrap().status != "idle" {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let snapshot = worker.snapshot("approval").await.unwrap();
        let sent: Value = serde_json::from_str(&snapshot.text).unwrap();
        assert_eq!(
            sent,
            json!({"type":"extension_ui_response","id":"approval-1","confirmed":false})
        );
        assert!(snapshot.pending_ui.is_empty());
        let expired = worker
            .request(
                "approval",
                "extension_ui_response",
                json!({"id":"approval-1","confirmed":true}),
            )
            .await
            .unwrap_err();
        assert_eq!(expired.code, "invalid_ui_request");
    })
    .await;
    manager.shutdown().await.unwrap();
    outcome.unwrap();
}
