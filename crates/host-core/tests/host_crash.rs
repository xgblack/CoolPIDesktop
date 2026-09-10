#![cfg(unix)]
use host_core::Workbench;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

#[tokio::test]
#[ignore = "helper invoked only by host_kill_recovers_metadata"]
async fn crash_child() {
    let Ok(root) = std::env::var("OMP_CRASH_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let w = Workbench::open(root.join("data")).await.unwrap();
    w.detect(Some(std::env::var("OMP_EXECUTABLE").unwrap()))
        .await;
    let p = w
        .store
        .register_project("Crash", vec![root.clone()], true)
        .await
        .unwrap();
    let t = w.store.create_task(&p.id, "Interrupted").await.unwrap();
    w.store
        .set_model(&t.id, Some(std::env::var("OMP_TEST_MODEL").unwrap()))
        .await
        .unwrap();
    let s = w.continue_task(&t.id).await.unwrap();
    w.request(
        &t.id,
        "prompt",
        json!({"message":"Remember CRASH-RESTORE-47. Reply SAVED; no tools."}),
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(90), async {
        while w.runtime.snapshot(&t.id).await.unwrap().status != "idle" {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let pid = s
        .events
        .iter()
        .find(|e| e.event_type == "process_started")
        .unwrap()
        .payload["pid"]
        .as_i64()
        .unwrap();
    let binding = w.store.task(&t.id).await.unwrap();
    std::fs::write(
        root.join("ready.json"),
        json!({"taskId":t.id,"pid":pid,"sessionId":binding.session_id}).to_string(),
    )
    .unwrap();
    std::future::pending::<()>().await;
}

#[tokio::test]
#[ignore = "real OMP and paid model; kills only this test's helper Host"]
async fn host_kill_recovers_metadata() {
    let root = std::env::temp_dir().join(format!("omp-host-crash-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "crash_child", "--nocapture"])
        .env("OMP_CRASH_TEST_ROOT", &root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let outcome = tokio::time::timeout(Duration::from_secs(100), async {
        loop {
            if root.join("ready.json").exists() {
                break;
            }
            if let Some(status) = child.try_wait().unwrap() {
                panic!("helper exited: {status}");
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;
    if outcome.is_err() {
        child.kill().unwrap();
        child.wait().unwrap();
        panic!("helper did not become ready");
    }
    let info: Value =
        serde_json::from_slice(&std::fs::read(root.join("ready.json")).unwrap()).unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    let pid = info["pid"].as_i64().unwrap() as i32;
    let exited = tokio::time::timeout(Duration::from_secs(10), async {
        while unsafe { libc::kill(pid, 0) } == 0 {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;
    if exited.is_err() {
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    assert!(exited.is_ok(), "OMP survived Host pipe closure");
    let w = Workbench::open(root.join("data")).await.unwrap();
    let id = info["taskId"].as_str().unwrap();
    assert!(w.runtime.snapshots().await.is_empty());
    let task = w.store.task(id).await.unwrap();
    assert_eq!(task.last_run.unwrap().state, "interrupted");
    let result = w.continue_task(id).await;
    if let Ok(s) = &result {
        assert_eq!(
            s.runtime.capabilities["state"]["sessionId"],
            info["sessionId"]
        );
    }
    let history = w.history(id, None).await;
    w.shutdown().await.unwrap();
    result.unwrap();
    let history = history.unwrap();
    assert_eq!(
        history["messages"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|m| m["role"] == "user")
            .count(),
        1
    );
    println!(
        "PASS Host SIGKILL, OMP EOF exit, interrupted metadata, no autostart or replay, exact session recovery"
    );
}
