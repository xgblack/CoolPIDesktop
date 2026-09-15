use host_core::Workbench;
use serde_json::json;
use std::time::Duration;

async fn turn(w: &Workbench, id: &str, message: &str) -> usize {
    w.request(
        id,
        host_core::RpcRequest::Prompt,
        json!({"message":message}),
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(120), async {
        let mut approvals = 0;
        loop {
            let s = w.runtime.snapshot(id).await.unwrap();
            assert_ne!(s.status, "failed", "{:?}", s.error);
            for ui in &s.pending_ui {
                let response = if ui["method"] == "confirm" {
                    json!({"id":ui["id"],"confirmed":true})
                } else {
                    assert_eq!(ui["method"], "select");
                    let option = ui["options"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|v| {
                            let s = v.as_str().unwrap_or("").to_lowercase();
                            s == "approve"
                                || ((s.contains("allow") || s.contains("approve"))
                                    && s.contains("once"))
                        })
                        .unwrap_or_else(|| panic!("No allow-once option: {}", ui["options"]));
                    json!({"id":ui["id"],"value":option})
                };
                w.request(id, host_core::RpcRequest::ExtensionUiResponse, response)
                    .await
                    .unwrap();
                approvals += 1;
            }
            if s.status == "idle" {
                return approvals;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    })
    .await
    .expect("real approval turn timed out")
}

#[tokio::test]
#[ignore = "requires explicit OMP_EXECUTABLE/OMP_TEST_MODEL; paid tool calls in temporary workspace"]
async fn real_modes_approve_tools_and_resume_session() {
    let root = std::env::temp_dir().join(format!("omp-approval-live-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("project")).unwrap();
    std::fs::write(root.join("project/input.txt"), "APPROVAL_READ_MARKER").unwrap();
    let w = Workbench::open(root.join("data")).await.unwrap();
    let worker = w.clone();
    let result = tokio::spawn(async move {
        let w = worker;
        assert_eq!(w.detect(Some(std::env::var("OMP_EXECUTABLE").unwrap())).await.status, "ready");
        let p = w.store.register_project("Approval verification", vec![root.join("project")], true).await.unwrap();
        let task = w.store.create_task_with_model(&p.id, "Approval verification", Some(std::env::var("OMP_TEST_MODEL").unwrap())).await.unwrap();
        let id = &task.id;
        w.set_approval_mode(id, Some("always-ask".into())).await.unwrap();
        w.continue_task(id).await.unwrap();
        assert_eq!(turn(&w, id, "Use only the read tool to read input.txt. Return the content. Do not use any other tools.").await, 0);
        assert!(w.runtime.snapshot(id).await.unwrap().text.contains("APPROVAL_READ_MARKER"));
        let bound = w.store.task(id).await.unwrap();
        let count = turn(&w, id, "Use only the write tool to create first.txt containing APPROVED_WRITE. Do not use other tools. Then reply done.").await;
        assert!(count > 0, "always-ask write did not ask for approval");
        assert_eq!(std::fs::read_to_string(root.join("project/first.txt")).unwrap().trim(), "APPROVED_WRITE");
        for (mode, prompt, file, expects_approval) in [
            ("write", "Use only the write tool to create second.txt containing WRITE_MODE. Do not use other tools.", "second.txt", false),
            ("write", "Use only the bash tool to run: printf EXEC_MODE > third.txt . Do not use other tools.", "third.txt", true),
            ("yolo", "Use only the bash tool to run: printf YOLO_MODE > fourth.txt . Do not use other tools.", "fourth.txt", false),
        ] {
            w.set_approval_mode(id, Some(mode.into())).await.unwrap();
            assert_eq!(w.store.task(id).await.unwrap().session_id, bound.session_id);
            assert_eq!(w.store.task(id).await.unwrap().session_file, bound.session_file);
            let count = turn(&w, id, prompt).await;
            assert_eq!(count > 0, expects_approval, "approval mismatch for {mode}/{file}");
            assert!(root.join("project").join(file).is_file(), "tool did not create {file}");
        }
        println!("PASS real OMP: read auto, always-ask write prompt, write-mode write auto/exec prompt, yolo exec auto, original session preserved");
    }).await;
    w.shutdown().await.unwrap();
    result.unwrap();
}
