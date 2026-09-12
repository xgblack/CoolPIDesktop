//! Real system OMP acceptance; isolated workspace and sessions, no configuration writes.
use host_core::trajectory_history::read;
use host_core::{LaunchOptions, TaskManager};
use serde_json::json;
use std::time::Duration;
#[tokio::test]
#[ignore = "requires explicit OMP_EXECUTABLE; makes model calls using the user's configured default"]
async fn real_trajectory_stream_tools_persistence_and_cancel() {
    let executable = std::env::var("OMP_EXECUTABLE").expect("explicit system OMP required");
    let root =
        std::env::temp_dir().join(format!("cool-pi-trajectory-live-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("sessions")).unwrap();
    std::fs::write(root.join("trajectory-input.txt"), "TRAJECTORY_READ_OK").unwrap();
    let manager = TaskManager::new(root.clone());
    let worker = manager.clone();
    let outcome=tokio::spawn(async move {
  worker.start_with("trajectory".into(),Some(executable),LaunchOptions{root:root.clone(),session_dir:Some(root.join("sessions")),..Default::default()}).await.unwrap();
  tokio::time::timeout(Duration::from_secs(45),async{loop{let s=worker.snapshot("trajectory").await.unwrap();assert_ne!(s.status,"failed","OMP startup failed");if s.status=="ready"{break;}tokio::time::sleep(Duration::from_millis(30)).await;}}).await.unwrap();
  let state=worker.query("trajectory","get_state",json!({})).await.unwrap();
  let session=state["sessionId"].as_str().unwrap().to_owned();let file=std::path::PathBuf::from(state["sessionFile"].as_str().unwrap());
  worker.request("trajectory","prompt",json!({"message":"Use your read tool to read trajectory-input.txt in the current directory. Do not edit any files. Reply with the file contents only."})).await.unwrap();
  let mut saw_running=false;
  tokio::time::timeout(Duration::from_secs(120),async{loop{let s=worker.snapshot("trajectory").await.unwrap();assert_ne!(s.status,"failed","OMP execution failed");saw_running|=s.trajectory.iter().any(|r|r.status=="running");assert!(s.trajectory.iter().filter(|r|r.status=="running").all(|r|r.duration_ms.is_none()));if s.status=="idle"{break;}tokio::time::sleep(Duration::from_millis(20)).await;}}).await.unwrap();
  let final_state=worker.snapshot("trajectory").await.unwrap();assert!(saw_running,"no live trajectory was observed");assert!(final_state.text.contains("TRAJECTORY_READ_OK"));
  assert!(final_state.trajectory.iter().any(|r|r.kind=="tool"&&r.status=="succeeded"),"model did not execute read tool");
  let h=read(&file,&session).unwrap();assert!(h.records.iter().any(|r|r.kind=="tool"&&r.status=="succeeded"));assert!(h.records.iter().any(|r|r.kind=="assistant"&&r.usage.is_some()));
  for live in final_state.trajectory.iter().filter(|r|r.kind=="assistant") {assert!(h.records.iter().any(|r|r.aliases.iter().any(|a|live.aliases.contains(a))),"persisted/live identities do not reconcile");}
  println!("PASS real OMP: streaming trajectory, real read tool, persisted tool result, usage and stable identities");
  let before=final_state.seq;
  worker.request("trajectory","prompt",json!({"message":"Do not use tools. Write a detailed 2000 word explanation of database indexing."})).await.unwrap();
  tokio::time::timeout(Duration::from_secs(120),async{loop{let s=worker.snapshot("trajectory").await.unwrap();if s.seq>before&&s.trajectory.iter().any(|r|r.kind=="assistant"&&r.status=="running"&&!r.content.to_string().is_empty()){break;}assert_ne!(s.status,"failed");tokio::time::sleep(Duration::from_millis(20)).await;}}).await.unwrap();
  worker.request("trajectory","abort",json!({})).await.unwrap();
  tokio::time::timeout(Duration::from_secs(30),async{loop{let s=worker.snapshot("trajectory").await.unwrap();if s.status=="interrupted"{assert!(s.trajectory.iter().all(|r|r.status!="running"));break;}tokio::time::sleep(Duration::from_millis(30)).await;}}).await.unwrap();
  println!("PASS real OMP: cancel settles trajectory without dangling running records");
 }).await;
    manager.shutdown().await.unwrap();
    outcome.unwrap();
}
