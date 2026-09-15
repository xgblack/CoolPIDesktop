use host_core::Workbench;
use serde_json::json;
use std::time::Duration;
async fn idle(w: &Workbench, id: &str) {
    tokio::time::timeout(Duration::from_secs(90), async {
        loop {
            let s = w.runtime.snapshot(id).await.unwrap();
            assert_ne!(s.status, "failed", "{:?}", s.error);
            if s.status == "idle" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
#[ignore = "explicit executable and model; paid real recovery"]
async fn database_reopen_preserves_task_and_session() {
    let root = std::env::temp_dir().join(format!("omp-workbench-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("project")).unwrap();
    let w = Workbench::open(root.join("data")).await.unwrap();
    let scenario = w.clone();
    let result=tokio::spawn(async move {
  let w=scenario;
  let info=w.detect(Some(std::env::var("OMP_EXECUTABLE").unwrap())).await;assert_eq!(info.status,"ready");
  let project=w.store.register_project("Project",vec![root.join("project")],true).await.unwrap();
  std::fs::create_dir_all(root.join("second")).unwrap();
  std::fs::create_dir_all(root.join("additional")).unwrap();
  let other_project=w.store.register_project("Second",vec![root.join("second"),root.join("additional")],true).await.unwrap();
  let other=w.store.create_task(&other_project.id,"Isolated").await.unwrap();
  w.store.set_model(&other.id,Some(std::env::var("OMP_TEST_MODEL").unwrap())).await.unwrap();
  let task=w.store.create_task(&project.id,"Persistent task").await.unwrap();
  w.store.set_model(&task.id,Some(std::env::var("OMP_TEST_MODEL").unwrap())).await.unwrap();
  let (a,b)=tokio::join!(w.continue_task(&task.id),w.continue_task(&task.id));
  assert_eq!(a.unwrap().run_id,b.unwrap().run_id);
  let other_snapshot=w.continue_task(&other.id).await.unwrap();
  let pid=other_snapshot.events.iter().find(|e|e.event_type=="process_started").unwrap().payload["pid"].as_i64().unwrap();
  let output=std::process::Command::new("ps").args(["-p",&pid.to_string(),"-o","command="]).output().unwrap();
  let command=String::from_utf8(output.stdout).unwrap();
  assert!(command.contains("--add-dir") && command.contains("additional") && command.contains("second"));
  let root_marker=format!("ROOT{}",uuid::Uuid::new_v4().simple());
  let extra_marker=format!("EXTRA{}",uuid::Uuid::new_v4().simple());
  std::fs::write(root.join("second/marker.txt"),&root_marker).unwrap();
  std::fs::write(root.join("additional/marker.txt"),&extra_marker).unwrap();
  w.request(&other.id,host_core::RpcRequest::Prompt,json!({"message":format!("Use tools to read marker.txt in your current working directory and {}. Return the exact contents of both files. Do not change files.",root.join("additional/marker.txt").display())})).await.unwrap();
  idle(&w,&other.id).await;
  let proof=w.runtime.snapshot(&other.id).await.unwrap();
  assert!(proof.text.contains(&root_marker)&&proof.text.contains(&extra_marker),"Workspace tool read failed: {}",proof.text);
  assert!(proof.tools.iter().any(|tool| tool.status=="succeeded"),"OMP did not emit a completed tool activity: {:?}",proof.tools);
  let usage=w.refresh_usage(&other.id).await.unwrap().usage;
  assert!(usage.context_tokens.is_some(),"OMP did not report context usage: {:?}",usage);
  assert!(usage.total_tokens.is_some()||usage.input_tokens.is_some()||usage.output_tokens.is_some(),"OMP did not report cumulative session usage: {:?}",usage);
  let original_model=w.runtime.query(&task.id,host_core::RpcQuery::GetState,json!({})).await.unwrap()["model"].clone();
  assert!(w.select_model(&task.id,"not-configured","not-a-model").await.is_err());
  assert_eq!(w.runtime.query(&task.id,host_core::RpcQuery::GetState,json!({})).await.unwrap()["model"],original_model);
  w.select_model(&task.id,original_model["provider"].as_str().unwrap(),original_model["id"].as_str().unwrap()).await.unwrap();
  assert_eq!(w.runtime.snapshot(&task.id).await.unwrap().runtime.capabilities["state"]["model"],original_model);
  let marker=format!("P{}",uuid::Uuid::new_v4().simple());
  w.request(&task.id,host_core::RpcRequest::Prompt,json!({"message":format!("Remember {marker}. Do not use tools. Reply SAVED.")})).await.unwrap();idle(&w,&task.id).await;
  let bound=w.store.task(&task.id).await.unwrap();assert!(bound.session_id.is_some());
  assert_ne!(bound.session_id,w.store.task(&other.id).await.unwrap().session_id);
  assert!(!w.history(&other.id,None).await.unwrap().to_string().contains(&marker));
  #[cfg(unix)] {unsafe {libc::kill(pid as i32,libc::SIGKILL);}}
  tokio::time::timeout(Duration::from_secs(5),async{while w.runtime.snapshot(&other.id).await.unwrap().status!="failed"{tokio::time::sleep(Duration::from_millis(30)).await;}}).await.unwrap();
  assert_eq!(w.records().await.unwrap().iter().find(|t|t.id==other.id).unwrap().last_run.as_ref().unwrap().state,"failed");
  assert_eq!(w.runtime.snapshot(&task.id).await.unwrap().status,"idle");
  let page=w.runtime.query(&task.id,host_core::RpcQuery::GetMessagesPage,json!({"limit":1})).await.unwrap();
  assert_eq!(page["messages"].as_array().unwrap().len(),1);
  let cursor=page["nextCursor"].as_str().unwrap();
  let next=w.runtime.query(&task.id,host_core::RpcQuery::GetMessagesPage,json!({"limit":1,"cursor":cursor})).await.unwrap();
  assert_ne!(page["messages"],next["messages"]);
  w.shutdown().await.unwrap();
  let reopened=Workbench::open(root.join("data")).await.unwrap();
  assert!(reopened.runtime.snapshots().await.is_empty());
  assert_eq!(reopened.store.task(&task.id).await.unwrap().title,"Persistent task");
  let outcome=async {
   let resumed=reopened.continue_task(&task.id).await.unwrap();
   assert_eq!(resumed.runtime.capabilities["state"]["sessionId"],bound.session_id.unwrap());
   assert!(reopened.history(&task.id,None).await.unwrap().to_string().contains(&marker));
   reopened.request(&task.id,host_core::RpcRequest::Prompt,json!({"message":"What marker did I give you? Reply only the marker, no tools."})).await.unwrap();idle(&reopened,&task.id).await;
   assert!(reopened.runtime.snapshot(&task.id).await.unwrap().text.contains(&marker));
   assert_eq!(reopened.runtime.query(&task.id,host_core::RpcQuery::GetMessagesPage,json!({"cursor":cursor,"limit":1})).await.unwrap_err().code,"stale_cursor");
   let before=reopened.runtime.snapshot(&task.id).await.unwrap();
   let restarted=reopened.restart(&task.id).await.unwrap();
   assert_ne!(before.run_id,restarted.run_id);
   assert_eq!(before.runtime.capabilities["state"]["sessionId"],restarted.runtime.capabilities["state"]["sessionId"]);
  };
  outcome.await;reopened.shutdown().await.unwrap();
  println!("PASS database reopen, stable task/session identity, no autostart, single writer and real context recovery");
 }).await;
    w.shutdown().await.unwrap();
    result.unwrap();
}
