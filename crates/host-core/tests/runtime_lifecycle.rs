#![cfg(unix)]
use host_core::{Workbench, writer_lock::WriterLease};
use serde_json::json;
use std::path::PathBuf;

async fn setup() -> (Workbench, String, PathBuf) {
    let root = std::env::temp_dir().join(format!("lifecycle-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("project")).unwrap();
    std::fs::create_dir_all(root.join("extra dir")).unwrap();
    let w = Workbench::open(root.join("data")).await.unwrap();
    let executable =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/approval-runtime.py");
    w.store
        .set_setting("executable", executable.to_string_lossy().into_owned())
        .await
        .unwrap();
    let p = w
        .store
        .register_project(
            "P",
            vec![root.join("project"), root.join("extra dir")],
            true,
        )
        .await
        .unwrap();
    let t = w.store.create_task(&p.id, "T").await.unwrap();
    (w, t.id, root)
}

#[tokio::test]
async fn focus_reuses_pid_manual_stop_and_stale_actions() {
    let (w, id, _) = setup().await;
    let worker = w.clone();
    let result = tokio::spawn(async move {
        worker.runtime_focus(Some(id.clone())).await.unwrap();
        let first = worker.runtime_tasks().await.unwrap().remove(0);
        assert!(first.pid.is_some());
        assert_eq!(first.owner, "desktop");
        worker.runtime_focus(None).await.unwrap();
        worker.runtime_focus(Some(id.clone())).await.unwrap();
        let next = worker.runtime_tasks().await.unwrap().remove(0);
        assert_eq!(first.pid, next.pid);
        assert_eq!(first.run_id, next.run_id);
        assert_eq!(
            worker
                .runtime_action(&id, "stop", Some("stale".into()))
                .await
                .unwrap_err()
                .code,
            "stale_runtime"
        );
        worker
            .runtime_action(&id, "stop", first.run_id)
            .await
            .unwrap();
        worker.runtime_focus(None).await.unwrap();
        worker.runtime_focus(Some(id.clone())).await.unwrap();
        let stopped = worker.runtime_tasks().await.unwrap().remove(0);
        assert!(stopped.pid.is_none());
        assert!(stopped.auto_start_suppressed);
        worker.continue_task(&id).await.unwrap();
        assert!(worker.runtime_tasks().await.unwrap()[0].pid.is_some());
    })
    .await;
    w.shutdown().await.unwrap();
    result.unwrap();
}

#[tokio::test]
async fn reclamation_preserves_foreground_keepalive_busy_and_pty() {
    let (w, id, root) = setup().await;
    let worker = w.clone();
    let result = tokio::spawn(async move {
        worker.runtime_focus(Some(id.clone())).await.unwrap();
        let terminal = worker.terminal_create(&id, 0).await.unwrap();
        assert!(worker.runtime_release_idle(true).await.unwrap().is_empty());
        worker.runtime_focus(None).await.unwrap();
        worker.runtime_keep_alive(&id, true).await.unwrap();
        assert!(worker.runtime_release_idle(true).await.unwrap().is_empty());
        worker.runtime_keep_alive(&id, false).await.unwrap();
        for value in [
            json!({"isStreaming":true}),
            json!({"isCompacting":true}),
            json!({"queuedMessageCount":1}),
            json!({"queuedMessageCount":null}),
        ] {
            std::fs::write(root.join("project/state.json"), value.to_string()).unwrap();
            assert!(worker.runtime_release_idle(true).await.unwrap().is_empty());
        }
        std::fs::write(root.join("project/state.json"), "{}").unwrap();
        assert_eq!(
            worker.runtime_release_idle(true).await.unwrap(),
            vec![id.clone()]
        );
        assert!(
            !worker
                .terminal_snapshot(&id, &terminal.id, 0)
                .await
                .unwrap()
                .exited
        );
    })
    .await;
    w.shutdown().await.unwrap();
    result.unwrap();
}

#[tokio::test]
async fn idle_pool_keeps_three_and_skips_unsafe_oldest() {
    let (w, id, root) = setup().await;
    let worker = w.clone();
    let result = tokio::spawn(async move {
        let project = worker.store.task(&id).await.unwrap().project_id;
        let mut ids = vec![id.clone()];
        for _ in 0..3 {
            ids.push(worker.store.create_task(&project, "Background").await.unwrap().id);
        }
        for id in &ids {
            worker.runtime_focus(Some(id.clone())).await.unwrap();
        }
        worker.runtime_focus(None).await.unwrap();
        // All sessions share this fixture cwd; only the oldest loses its persisted file.
        let oldest = worker.store.task(&id).await.unwrap();
        let file = oldest.session_file.unwrap();
        std::fs::rename(&file, root.join("retained-session.jsonl")).unwrap();
        let released = worker.runtime_release_idle(false).await.unwrap();
        assert_eq!(released.len(), 1);
        assert_ne!(released[0], id);
        assert_eq!(worker.runtime_tasks().await.unwrap().iter().filter(|r| r.pid.is_some()).count(), 3);
        assert!(worker.runtime_release_idle(false).await.unwrap().is_empty());
    }).await;
    w.shutdown().await.unwrap();
    result.unwrap();
}

#[tokio::test]
async fn complete_command_handoff_and_external_ownership() {
    let (w, id, root) = setup().await;
    let worker = w.clone();
    let result = tokio::spawn(async move {
        worker
            .set_approval_mode(&id, Some("always-ask".into()))
            .await
            .unwrap();
        worker.runtime_focus(Some(id.clone())).await.unwrap();
        let command = worker.runtime_command(&id).await.unwrap();
        assert!(
            command
                .arguments
                .windows(2)
                .any(|a| a == ["--approval-mode", "always-ask"])
        );
        assert!(
            command
                .arguments
                .windows(2)
                .any(|a| a[0] == "--add-dir" && a[1].ends_with("extra dir"))
        );
        assert!(command.arguments.iter().any(|a| a == "--resume"));
        assert!(!command.arguments.iter().any(|a| a == "rpc"));
        assert!(worker.runtime_tasks().await.unwrap()[0].pid.is_some());
        let mut changed = command.arguments.clone();
        changed.push("--yolo".into());
        assert_eq!(
            worker
                .handoff(&id, &command.executable, &changed)
                .await
                .unwrap_err()
                .code,
            "stale_command"
        );
        std::fs::write(root.join("project/state.json"), "{\"isCompacting\":true}").unwrap();
        assert_eq!(
            worker
                .handoff(&id, &command.executable, &command.arguments)
                .await
                .unwrap_err()
                .code,
            "task_busy"
        );
        std::fs::write(root.join("project/state.json"), "{}").unwrap();
        let environment = worker
            .handoff(&id, &command.executable, &command.arguments)
            .await
            .unwrap();
        assert!(environment.contains_key("HOME"));
        assert!(worker.runtime_tasks().await.unwrap()[0].pid.is_none());
        assert_eq!(
            worker.continue_task(&id).await.unwrap_err().code,
            "session_owned"
        );
        let mut lease = WriterLease::acquire(&root.join("data"), &id, "terminal").unwrap();
        lease.set_pid(std::process::id()).unwrap();
        assert_eq!(worker.runtime_tasks().await.unwrap()[0].owner, "terminal");
        assert_eq!(worker.restart(&id).await.unwrap_err().code,"session_owned");
        assert_eq!(worker.update_task(&id,"T",false,true).await.unwrap_err().code,"session_owned");
        assert_eq!(worker.select_model(&id,"provider","model").await.unwrap_err().code,"session_owned");
        assert_eq!(
            worker
                .set_approval_mode(&id, Some("yolo".into()))
                .await
                .unwrap_err()
                .code,
            "session_owned"
        );
        worker.shutdown().await.unwrap();
        assert!(
            host_core::writer_lock::owner(&root.join("data"), &id)
                .unwrap()
                .is_some()
        );
        drop(lease);
    })
    .await;
    w.shutdown().await.unwrap();
    result.unwrap();
}

#[tokio::test]
#[ignore = "requires explicit OMP_EXECUTABLE; starts real OMP without making model calls"]
async fn real_open_task_loads_system_prompt_and_reuses_pid() {
    let (w, id, _) = setup().await;
    w.store
        .set_setting("executable", std::env::var("OMP_EXECUTABLE").unwrap())
        .await
        .unwrap();
    let worker = w.clone();
    let result=tokio::spawn(async move{
  worker.runtime_focus(Some(id.clone())).await.unwrap();
  let first=worker.runtime_tasks().await.unwrap().remove(0);
  let s=worker.runtime.snapshot(&id).await.unwrap();
  let prompt=&s.runtime.capabilities["state"]["systemPrompt"];
  assert!(prompt.as_str().is_some_and(|s|!s.is_empty())||prompt.as_array().is_some_and(|a|!a.is_empty()),"OMP did not expose system prompt");
  worker.runtime_focus(None).await.unwrap();worker.runtime_focus(Some(id.clone())).await.unwrap();
  assert_eq!(first.pid,worker.runtime_tasks().await.unwrap()[0].pid);
  worker.runtime_action(&id,"stop",first.run_id).await.unwrap();
  assert!(worker.runtime_tasks().await.unwrap()[0].pid.is_none());
  println!("PASS real OMP: open task loads system prompt; switching reuses PID; explicit stop reaps process");
 }).await;
    w.shutdown().await.unwrap();
    result.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires OMP_EXECUTABLE and OMP_DESKTOP_HELPER; real interactive terminal, no model calls"]
async fn real_terminal_exec_keeps_ownership_and_survives_desktop_shutdown() {
    let (w, id, root) = setup().await;
    w.store
        .set_setting("executable", std::env::var("OMP_EXECUTABLE").unwrap())
        .await
        .unwrap();
    let dir = root.join("data/sessions").join(&id);
    std::fs::create_dir_all(&dir).unwrap();
    let session_id = uuid::Uuid::new_v4().to_string();
    let header = json!({"type":"session","version":3,"id":session_id,"cwd":root.join("project"),"additionalDirectories":[root.join("extra dir")],"timestamp":"2026-09-13T00:00:00.000Z"});
    std::fs::write(dir.join("seed.jsonl"), format!("{header}\n")).unwrap();
    w.recover_session(&id, true).await.unwrap();
    w.start_handoff_server().await.unwrap();
    let terminal = host_core::terminal::TerminalService::default();
    let worker = w.clone();
    let external = terminal.clone();
    let result=tokio::spawn(async move{
  worker.runtime_focus(Some(id.clone())).await.unwrap();
  let desktop_pid=worker.runtime_tasks().await.unwrap()[0].pid.unwrap();
  let command=worker.runtime_command(&id).await.unwrap();
  let mut args=vec![std::env::var("OMP_DESKTOP_HELPER").unwrap(),"session".into(),"open".into(),"--data".into(),root.join("data").to_string_lossy().into_owned(),"--task".into(),id.clone(),"--".into(),command.executable];args.extend(command.arguments);
  let line=format!("exec {}\r",args.iter().map(|v|format!("'{}'",v.replace('\'',"'\\''"))).collect::<Vec<_>>().join(" "));
  let t=external.create("external",root.join("project").canonicalize().unwrap()).await.unwrap();
  external.write("external",&t.id,line).await.unwrap();
  tokio::time::timeout(std::time::Duration::from_secs(15),async{
   loop {if worker.runtime_tasks().await.unwrap()[0].owner=="terminal"{break;}tokio::time::sleep(std::time::Duration::from_millis(100)).await;}
  }).await.expect("terminal did not acquire the session");
  tokio::time::sleep(std::time::Duration::from_secs(2)).await;
  let info=worker.runtime_tasks().await.unwrap().remove(0);assert_eq!(info.owner,"terminal");assert_ne!(info.pid,Some(desktop_pid));
  assert_eq!(unsafe{libc::kill(desktop_pid as i32,0)},-1);
  assert!(!external.snapshot("external",&t.id,0).await.unwrap().exited);
  assert_eq!(worker.continue_task(&id).await.unwrap_err().code,"session_owned");
  assert!(worker.history(&id,None).await.is_ok());
  worker.shutdown().await.unwrap();
  assert!(!external.snapshot("external",&t.id,0).await.unwrap().exited);
  assert!(host_core::writer_lock::owner(&root.join("data"),&id).unwrap().is_some());
  external.close("external",&t.id).await.unwrap();
  assert!(host_core::writer_lock::owner(&root.join("data"),&id).unwrap().is_none());
  worker.continue_task(&id).await.unwrap();
  assert_eq!(worker.store.task(&id).await.unwrap().session_id.as_deref(),Some(session_id.as_str()));
  println!("PASS real TTY: exact-session OMP exec, exclusive writer, desktop shutdown preserves terminal, terminal exit releases ownership, desktop resumes same session");
 }).await;
    terminal.close_all().await.unwrap();
    w.shutdown().await.unwrap();
    result.unwrap();
}

#[tokio::test]
async fn session_export_refuses_streaming_compaction_queued_and_external_writers() {
    let (w, id, root) = setup().await;
    let worker = w.clone();
    let result = tokio::spawn(async move {
        worker.runtime_focus(Some(id.clone())).await.unwrap();
        for state in [json!({"isStreaming":true}), json!({"isCompacting":true}), json!({"queuedMessageCount":1}), json!({"queuedMessageCount":null})] {
            std::fs::write(root.join("project/state.json"), state.to_string()).unwrap();
            assert_eq!(worker.export_session(&id).await.unwrap_err().code, "task_busy");
        }
        let run_id = worker.runtime.snapshot(&id).await.unwrap().run_id;
        worker.runtime_action(&id, "stop", Some(run_id)).await.unwrap();
        let _lease = WriterLease::acquire(&root.join("data"), &id, "terminal").unwrap();
        assert!(worker.export_session(&id).await.is_err());
    }).await;
    w.shutdown().await.unwrap();
    result.unwrap();
}
