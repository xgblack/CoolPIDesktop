#![cfg(unix)]
use host_core::{RpcQuery, RpcRequest, TaskManager};
use serde_json::json;
use std::{path::PathBuf, time::Duration};

fn fixture() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/m1-runtime.py")
        .to_string_lossy()
        .into()
}

async fn manager(id: &str) -> TaskManager {
    let manager = TaskManager::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    manager.start(id.into(), Some(fixture())).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while manager.snapshot(id).await.unwrap().status == "starting" {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    manager
}

#[tokio::test]
async fn provider_login_uses_only_the_typed_rpc_commands() {
    let manager = manager("providers").await;
    let options = manager
        .query("providers", RpcQuery::GetLoginProviders, json!({}))
        .await
        .unwrap();
    assert_eq!(options["providers"][0]["id"], "oauth-test");
    assert_eq!(options["providers"][0]["authenticated"], false);

    let login = manager
        .query(
            "providers",
            RpcQuery::LoginProvider,
            json!({"providerId":"oauth-test"}),
        )
        .await
        .unwrap();
    assert_eq!(login["providerId"], "oauth-test");
    let rejected = manager
        .query(
            "providers",
            RpcQuery::LoginProvider,
            json!({"providerId":"unknown"}),
        )
        .await
        .unwrap_err();
    assert_eq!(rejected.code, "unknown_provider");
    manager.shutdown().await.unwrap();
}

#[tokio::test]
async fn typed_controls_project_commands_queues_and_subagents() {
    let manager = manager("controls").await;
    let initial = manager.snapshot("controls").await.unwrap();
    assert_eq!(initial.available_commands[0]["name"], "review");

    manager
        .request("controls", RpcRequest::Prompt, json!({"message":"run"}))
        .await
        .unwrap();
    manager
        .request("controls", RpcRequest::Steer, json!({"message":"guide"}))
        .await
        .unwrap();
    manager
        .query("controls", RpcQuery::GetState, json!({}))
        .await
        .unwrap();
    let queued = manager.snapshot("controls").await.unwrap();
    assert_eq!(queued.authoritative_queued_count, 1);
    assert_eq!(queued.queued_messages[0].message, "guide");

    manager
        .request(
            "controls",
            RpcRequest::FollowUp,
            json!({"message":"desync"}),
        )
        .await
        .unwrap();
    manager
        .query("controls", RpcQuery::GetState, json!({}))
        .await
        .unwrap();
    let desynced = manager.snapshot("controls").await.unwrap();
    assert_eq!(desynced.authoritative_queued_count, 3);
    assert!(
        desynced.queued_messages.is_empty(),
        "unverified queue text must be discarded"
    );

    manager
        .query("controls", RpcQuery::GetSubagents, json!({}))
        .await
        .unwrap();
    assert_eq!(
        manager.snapshot("controls").await.unwrap().subagents[0]["id"],
        "sub-1"
    );
    let messages = manager
        .query(
            "controls",
            RpcQuery::GetSubagentMessages,
            json!({"subagentId":"sub-1","fromByte":0}),
        )
        .await
        .unwrap();
    assert_eq!(messages["nextByte"], 42);

    manager
        .request("controls", RpcRequest::Abort, json!({}))
        .await
        .unwrap();
    manager.shutdown().await.unwrap();
}

#[tokio::test]
async fn compaction_errors_and_extension_ui_have_explicit_states() {
    let manager = manager("states").await;
    manager
        .query("states", RpcQuery::Compact, json!({}))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while manager.snapshot("states").await.unwrap().status != "idle" {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let rejected = manager
        .query("states", RpcQuery::SetThinkingLevel, json!({"level":"max"}))
        .await
        .unwrap_err();
    assert_eq!(rejected.code, "unsupported_thinking_level");

    manager
        .request("states", RpcRequest::Prompt, json!({"message":"ui"}))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while manager
            .snapshot("states")
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
    let snapshot = manager.snapshot("states").await.unwrap();
    assert_eq!(snapshot.status, "pending_ui");
    assert_eq!(
        snapshot.pending_ui[0]["optionDetails"][0]["description"],
        "first"
    );
    assert_eq!(snapshot.extension_ui.statuses["mode"], "testing");
    assert_eq!(
        snapshot.extension_ui.widgets["summary"]["widgetLines"][0],
        "line"
    );
    assert_eq!(snapshot.extension_ui.title.as_deref(), Some("Fixture"));
    assert_eq!(snapshot.extension_ui.editor_text.as_deref(), Some("draft"));
    assert_eq!(
        snapshot.extension_ui.open_url.as_ref().unwrap()["launchUrl"],
        "http://127.0.0.1/link"
    );
    assert!(
        snapshot
            .events
            .iter()
            .any(|event| event.event_type == "extension_ui_request"
                && event.payload["method"] == "notify")
    );

    manager
        .request(
            "states",
            RpcRequest::ExtensionUiResponse,
            json!({"id":"select-1","value":"one"}),
        )
        .await
        .unwrap();
    assert_eq!(manager.snapshot("states").await.unwrap().status, "running");
    manager
        .request("states", RpcRequest::Abort, json!({}))
        .await
        .unwrap();
    manager.shutdown().await.unwrap();
}
