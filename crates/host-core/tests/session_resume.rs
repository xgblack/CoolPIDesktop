//! Explicit opt-in real OMP session recovery test.
use host_core::{LaunchOptions, TaskManager, TaskSnapshot};
use serde_json::json;
use std::time::Duration;
async fn ready(m: &TaskManager, id: &str, status: &str) -> TaskSnapshot {
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            let s = m.snapshot(id).await.unwrap();
            assert_ne!(s.status, "failed", "{:?}", s.error);
            if s.status == status {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap()
}
#[tokio::test]
#[ignore = "requires explicit OMP executable/model; paid model calls"]
async fn resume_exact_session_and_history() {
    let path = std::env::var("OMP_EXECUTABLE").unwrap();
    let model = std::env::var("OMP_TEST_MODEL").unwrap();
    let root = std::env::temp_dir().join(format!("omp-resume-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("sessions")).unwrap();
    let m = TaskManager::new(root.clone());
    let worker = m.clone();
    let result=tokio::spawn(async move {
        let options=LaunchOptions{root:root.clone(),model:Some(model),session_dir:Some(root.join("sessions")),..Default::default()};
        worker.start_with("first".into(),Some(path.clone()),options.clone()).await.unwrap();
        let initial=ready(&worker,"first","ready").await;
        let state=worker.query("first",host_core::RpcQuery::GetState,json!({})).await.unwrap();
        let id=state["sessionId"].as_str().unwrap().to_owned();
        let file=state["sessionFile"].as_str().unwrap().to_owned();
        let marker=format!("R{}",uuid::Uuid::new_v4().simple());
        worker.request("first",host_core::RpcRequest::Prompt,json!({"message":format!("Remember this marker: {marker}. Do not use tools. Reply only SAVED.")})).await.unwrap();
        ready(&worker,"first","idle").await;
        worker.stop("first").await.unwrap();
        worker.start_with("resumed".into(),Some(path),LaunchOptions{session_file:Some(file.into()),expected_session:Some(id.clone()),..options}).await.unwrap();
        let resumed=ready(&worker,"resumed","ready").await;
        assert_ne!(initial.run_id,resumed.run_id);
        assert_eq!(resumed.runtime.capabilities["state"]["sessionId"],id);
        let page=worker.query("resumed",host_core::RpcQuery::GetMessagesPage,json!({"limit":2})).await.unwrap();
        assert!(page.to_string().contains(&marker));
        worker.request("resumed",host_core::RpcRequest::Prompt,json!({"message":"What marker did I ask you to remember? Reply with only that marker. Do not use tools."})).await.unwrap();
        let final_state=ready(&worker,"resumed","idle").await;
        assert!(final_state.text.contains(&marker),"Model did not recover marker");
        println!("PASS exact sessionId/path recovery, paged history, recalled random marker without prompt replay");
    }).await;
    m.shutdown().await.unwrap();
    result.unwrap();
}
