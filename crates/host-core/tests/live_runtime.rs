//! Opt-in paid integration test. Uses the real Host API, never a substitute RPC client.
use futures_util::{SinkExt, StreamExt};
use host_core::{TaskManager, TaskSnapshot};
use serde_json::json;
use std::{path::PathBuf, time::Duration};
use tokio::time::{sleep, timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message};

async fn wait(
    manager: &TaskManager,
    id: &str,
    predicate: impl Fn(&TaskSnapshot) -> bool,
) -> TaskSnapshot {
    timeout(Duration::from_secs(90), async {
        loop {
            let s = manager.snapshot(id).await.unwrap();
            assert_ne!(s.status, "failed", "{}: {:?}", id, s.error);
            if predicate(&s) {
                return s;
            }
            sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("Host state transition timed out")
}
fn pid(s: &TaskSnapshot) -> i32 {
    s.events
        .iter()
        .find(|e| e.event_type == "process_started")
        .unwrap()
        .payload["pid"]
        .as_i64()
        .unwrap() as i32
}
fn isolated(s: &TaskSnapshot) {
    let mut previous = 0;
    for e in &s.events {
        assert_eq!(e.task_id, s.task_id);
        assert_eq!(e.run_id, s.run_id);
        assert!(e.seq > previous);
        previous = e.seq;
    }
    assert!(previous <= s.seq);
}
#[tokio::test]
#[ignore = "requires OMP_EXECUTABLE and OMP_TEST_MODEL; makes paid model calls"]
async fn real_host_lifecycle() {
    let executable = std::env::var("OMP_EXECUTABLE").expect("Explicit OMP_EXECUTABLE required");
    let model = std::env::var("OMP_TEST_MODEL").expect("Explicit OMP_TEST_MODEL required");
    let root = std::env::temp_dir().join(format!("omp-host-live-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let manager = TaskManager::new(PathBuf::from(&root)).with_model(model.clone());
    // Cleanup even when an assertion panics; leave OMP-owned session files untouched.
    let worker = manager.clone();
    let scenario=tokio::spawn(async move {
        let info=worker.runtime_status(Some(executable.clone())).await;
        assert_eq!(info.status,"ready","{:?}",info.error); assert_eq!(info.protocol,Some(2));
        assert_eq!(info.capabilities["state"]["model"]["id"],"qwen3.7-flash");
        println!("PASS runtime: executable/version/v2/state/models/commands; model={model}");
        let observer=worker.observer_info().await.unwrap();
        let (mut observer_client,_)=connect_async(&observer.url).await.unwrap();
        observer_client.send(Message::Text(json!({"token":observer.token}).to_string().into())).await.unwrap();
        let initial=timeout(Duration::from_secs(3),observer_client.next()).await.unwrap().unwrap().unwrap().into_text().unwrap();
        assert!(initial.contains("snapshot"));
        worker.start("alpha".into(),Some(executable.clone())).await.unwrap();
        let event=timeout(Duration::from_secs(15),async {loop {let text=observer_client.next().await.unwrap().unwrap().into_text().unwrap();if text.contains("process_started"){return text;}}}).await.unwrap();
        assert!(event.contains("alpha"));
        worker.start("beta".into(),Some(executable)).await.unwrap();
        let a=wait(&worker,"alpha",|s|s.status=="ready").await;
        let b=wait(&worker,"beta",|s|s.status=="ready").await;
        assert_ne!(pid(&a),pid(&b)); assert_ne!(a.run_id,b.run_id);
        let (ra,rb)=tokio::join!(
            worker.request("alpha",host_core::RpcRequest::Prompt,json!({"message":"Do not use tools. Reply with exactly ALPHA_OK."})),
            worker.request("beta",host_core::RpcRequest::Prompt,json!({"message":"Do not use tools. Reply with exactly BETA_OK."}))
        );ra.unwrap();rb.unwrap();
        let a=wait(&worker,"alpha",|s|s.status=="idle").await;
        let b=wait(&worker,"beta",|s|s.status=="idle").await;
        assert!(a.text.contains("ALPHA_OK"));assert!(!a.text.contains("BETA_OK"));
        assert!(b.text.contains("BETA_OK"));assert!(!b.text.contains("ALPHA_OK"));isolated(&a);isolated(&b);
        println!("PASS observer/concurrent prompts: authenticated snapshot, event envelope, separate PIDs and session text");
        let before=a.seq;
        worker.request("alpha",host_core::RpcRequest::Prompt,json!({"message":"Do not use tools. Write 1000 words explaining software testing."})).await.unwrap();
        wait(&worker,"alpha",|s|s.status=="running"&&s.events.iter().any(|e|e.seq>before&&e.event_type=="message_update")).await;
        worker.request("alpha",host_core::RpcRequest::Abort,json!({})).await.unwrap();
        let stopped=wait(&worker,"alpha",|s|s.status=="interrupted").await;
        assert!(stopped.events.iter().any(|e|e.seq>before&&e.event_type=="agent_end"));
        assert_eq!(worker.snapshot("beta").await.unwrap().status,"idle");
        println!("PASS abort: active stream -> interrupted on terminal event; beta unaffected");
        let old_pid=pid(&stopped);
        let fresh=worker.restart("alpha").await.unwrap();
        assert_ne!(fresh.run_id,stopped.run_id); assert_eq!(fresh.seq,0);assert!(fresh.events.is_empty());
        assert_eq!(unsafe{libc::kill(old_pid,0)},-1,"old process survived restart");
        let restarted=wait(&worker,"alpha",|s|s.status=="ready").await;isolated(&restarted);
        worker.request("alpha",host_core::RpcRequest::Prompt,json!({"message":"Do not use tools. Reply exactly RESTART_OK."})).await.unwrap();
        let restarted=wait(&worker,"alpha",|s|s.status=="idle").await;
        assert!(restarted.text.contains("RESTART_OK"));assert!(!restarted.text.contains("ALPHA_OK"));
        println!("PASS restart: new runId/reset seq, old process reaped, subsequent prompt works");
        worker.request("beta",host_core::RpcRequest::Prompt,json!({"message":"Do not use tools. Reply exactly SURVIVOR_OK."})).await.unwrap();
        assert_eq!(unsafe{libc::kill(pid(&restarted),libc::SIGKILL)},0);
        timeout(Duration::from_secs(10),async{loop{if worker.snapshot("alpha").await.unwrap().status=="failed"{break;}sleep(Duration::from_millis(50)).await;}}).await.unwrap();
        let crash=timeout(Duration::from_secs(10),async {loop {let text=observer_client.next().await.unwrap().unwrap().into_text().unwrap();if text.contains("process_exited")&&text.contains("alpha"){return text;}}}).await.unwrap();
        assert!(crash.contains("error"));
        let survived=wait(&worker,"beta",|s|s.status=="idle").await;
        assert!(survived.text.contains("SURVIVOR_OK"));isolated(&survived);
        println!("PASS crash isolation: alpha killed, beta completes real prompt");
        worker.stop("beta").await.unwrap();
        assert_eq!(unsafe{libc::kill(pid(&b),0)},-1,"stopped process survived");
        assert_eq!(worker.snapshot("beta").await.unwrap().status,"stopped");
        assert!(worker.request("beta",host_core::RpcRequest::Prompt,json!({"message":"must not run"})).await.is_err());
        worker.restart("beta").await.unwrap();
        let alive=wait(&worker,"beta",|s|s.status=="ready").await;
        worker.shutdown().await.unwrap();
        assert_eq!(unsafe{libc::kill(pid(&alive),0)},-1);
        assert_eq!(std::io::Error::last_os_error().raw_os_error(),Some(libc::ESRCH));
        println!("PASS stop and shutdown: stopped snapshot, rejects prompt, active child reaped");
    }).await;
    manager.shutdown().await.unwrap();
    scenario.unwrap();
}
