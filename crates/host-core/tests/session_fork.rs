use host_core::{Workbench, HostError};
use serde_json::{Value, json};
use std::path::PathBuf;

async fn fixture() -> (Workbench, String, PathBuf, Vec<u8>) {
    let root = std::env::temp_dir().join(format!("omp-fork-{}",uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("project")).unwrap();
    let w = Workbench::open(root.join("data")).await.unwrap();
    let p = w.store.register_project("P",vec![root.join("project")],true).await.unwrap();
    let t = w.store.create_task(&p.id,"Fork test").await.unwrap();
    w.store.set_setting(&format!("task_approval:{}",t.id),"write".into()).await.unwrap();
    let source = root.join("source.jsonl");
    let entries = vec![
        json!({"type":"session","version":3,"id":"source-session","cwd":root.join("project"),"timestamp":"2026-09-13T00:00:00Z"}),
        json!({"type":"message","id":"u1","parentId":null,"timestamp":"2026-09-13T00:00:01Z","message":{"role":"user","content":"Remember FORK_KEEP","timestamp":1000}}),
        json!({"type":"message","id":"a1","parentId":"u1","timestamp":"2026-09-13T00:00:02Z","message":{"role":"assistant","content":[{"type":"text","text":"SAVED"}],"timestamp":1100,"completedAt":2000,"stopReason":"stop","usage":{"input":10,"output":1,"totalTokens":11,"cost":{"total":0.01}}}}),
        json!({"type":"message","id":"u2","parentId":"a1","timestamp":"2026-09-13T00:00:03Z","message":{"role":"user","content":"FORK_EXCLUDE","timestamp":3000}}),
        json!({"type":"message","id":"a2","parentId":"u2","timestamp":"2026-09-13T00:00:04Z","message":{"role":"assistant","content":[{"type":"text","text":"SECOND"}],"timestamp":3100,"completedAt":4000,"stopReason":"stop"}}),
    ];
    let raw = entries.iter().map(|e|format!("{e}\n")).collect::<String>().into_bytes();
    std::fs::write(&source,&raw).unwrap();
    w.store.bind(&t.id,"source-session",source.clone()).await.unwrap();
    (w,t.id,source,raw)
}
#[tokio::test]
async fn fork_copies_only_selected_history_and_is_independent_after_reopen() {
    let (w,id,source,raw)=fixture().await;
    let fork=w.fork_task(&id,1100.0).await.unwrap();
    assert_ne!(fork.id,id);assert_ne!(fork.session_id.as_deref(),Some("source-session"));
    assert_eq!(fork.approval_mode.as_deref(),Some("write"));
    assert_eq!(w.store.validate_task_roots(&id).await.unwrap(),w.store.validate_task_roots(&fork.id).await.unwrap());
    let history=w.history(&fork.id,None).await.unwrap();
    assert_eq!(history["totalMessages"],2);assert!(history.to_string().contains("FORK_KEEP"));assert!(!history.to_string().contains("FORK_EXCLUDE"));
    assert_eq!(std::fs::read(&source).unwrap(),raw);
    let path=fork.session_file.unwrap();
    let header:Value=serde_json::from_str(std::fs::read_to_string(&path).unwrap().lines().next().unwrap()).unwrap();
    assert_eq!(header["parentSession"],json!(source.canonicalize().unwrap()));
    let data=path.parent().unwrap().parent().unwrap().parent().unwrap().to_path_buf();
    w.shutdown().await.unwrap();
    let reopened=Workbench::open(data).await.unwrap();
    assert_eq!(reopened.history(&fork.id,None).await.unwrap()["totalMessages"],2);
    assert_eq!(reopened.history(&id,None).await.unwrap()["totalMessages"],4);
    println!("PASS fork boundary, distinct identity, source unchanged, roots/approval inherited, persisted reopen");
}
#[tokio::test]
async fn invalid_and_nonfinal_boundaries_never_create_tasks() {
    let (w,id,source,_)=fixture().await;
    for timestamp in [999.0,1000.0,f64::NAN] {assert_eq!(w.fork_task(&id,timestamp).await.unwrap_err().code,"session_fork_failed");}
    let mut entries:Vec<Value>=std::fs::read_to_string(&source).unwrap().lines().map(|s|serde_json::from_str(s).unwrap()).collect();
    entries[2]["message"]["content"]=json!([{"type":"toolCall","id":"c1","name":"read","arguments":{}}]);
    std::fs::write(&source,entries.iter().map(|e|format!("{e}\n")).collect::<String>()).unwrap();
    assert_eq!(w.fork_task(&id,1100.0).await.unwrap_err().code,"session_fork_failed");
    assert_eq!(w.store.tasks().await.unwrap().len(),1);
}
#[tokio::test]
#[ignore="requires system OMP; one short real model request"]
async fn real_omp_loads_fork_and_keeps_source_binding() {
    let (w,id,source,raw)=fixture().await;
    let executable=std::env::var("OMP_EXECUTABLE").unwrap();
    w.store.set_setting("executable",executable).await.unwrap();
    let fork=w.fork_task(&id,1100.0).await.unwrap();
    let result:Result<(),HostError>=async {
        let resumed=w.continue_task(&fork.id).await?;
        assert_eq!(resumed.runtime.capabilities["state"]["sessionId"],json!(fork.session_id));
        let history=w.history(&fork.id,None).await?;
        assert!(history.to_string().contains("FORK_KEEP"));assert!(!history.to_string().contains("FORK_EXCLUDE"));
        assert_eq!(std::fs::read(&source).unwrap(),raw);
        assert_eq!(w.store.task(&id).await?.session_id.as_deref(),Some("source-session"));
        w.request(&fork.id,host_core::RpcRequest::Prompt,json!({"message":"What exact marker were you asked to remember? Reply only with the marker, no tools."})).await?;
        tokio::time::timeout(std::time::Duration::from_secs(90),async {
            loop {
                let state=w.runtime.snapshot(&fork.id).await.unwrap();
                assert_ne!(state.status,"failed","{:?}",state.error);
                if state.status=="idle" {
                    assert!(state.text.contains("FORK_KEEP"),"fork lost its context");
                    assert!(!state.text.contains("FORK_EXCLUDE"));
                    assert!(state.turn_started_at.is_some());
                    assert!(state.turn_completed_at>=state.turn_started_at);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }).await.expect("real fork continuation timed out");
        assert_eq!(std::fs::read(&source).unwrap(),raw);
        println!("PASS real OMP: resumed fork, model recalls inherited marker, later history excluded, source unchanged, live timing settled");
        Ok(())
    }.await;
    w.shutdown().await.unwrap();result.unwrap();
}

#[tokio::test]
async fn fork_keeps_active_ancestry_and_resolves_image_bytes_without_rewriting_prose() {
    let (w,id,source,_)=fixture().await;
    let mut entries:Vec<Value>=std::fs::read_to_string(&source).unwrap().lines().map(|s|serde_json::from_str(s).unwrap()).collect();
    let binary_hash="a".repeat(64);let url_hash="b".repeat(64);
    let reference=format!("blob:sha256:{binary_hash}");
    let blob_dir=source.parent().unwrap().join("blobs");std::fs::create_dir(&blob_dir).unwrap();
    std::fs::write(blob_dir.join(&binary_hash),b"test image bytes").unwrap();
    std::fs::write(blob_dir.join(&url_hash),b"data:image/png;base64,aGVsbG8=").unwrap();
    entries[1]["message"]["content"]=json!([
        {"type":"text","text":reference,"textSignature":"preserve"},
        {"type":"image","mimeType":"image/png","data":reference},
        {"type":"image_url","image_url":format!("blob:sha256:{url_hash}")}
    ]);
    entries[2]["message"]["images"]=json!([{"data":reference}]);
    entries.insert(2,json!({"type":"message","id":"abandoned","parentId":"u1","message":{"role":"assistant","timestamp":1050,"content":"ABANDONED"}}));
    std::fs::write(&source,entries.iter().map(|e|format!("{e}\n")).collect::<String>()).unwrap();
    assert!(w.fork_task(&id,1050.0).await.is_err());
    let fork=w.fork_task(&id,1100.0).await.unwrap();
    let copied:Vec<Value>=std::fs::read_to_string(fork.session_file.unwrap()).unwrap().lines().map(|s|serde_json::from_str(s).unwrap()).collect();
    assert_eq!(copied[1]["message"]["content"][0]["text"],reference);
    assert_eq!(copied[1]["message"]["content"][1]["data"],"dGVzdCBpbWFnZSBieXRlcw==");
    assert_eq!(copied[1]["message"]["content"][2]["image_url"],"data:image/png;base64,aGVsbG8=");
    assert_eq!(copied[2]["message"]["images"][0]["data"],"dGVzdCBpbWFnZSBieXRlcw==");
    assert!(!json!(copied).to_string().contains("ABANDONED"));
    entries[1]["message"]["content"][1]["data"]=json!(format!("blob:sha256:{}","c".repeat(64)));
    std::fs::write(&source,entries.iter().map(|e|format!("{e}\n")).collect::<String>()).unwrap();
    assert!(w.fork_task(&id,1100.0).await.is_err());
    assert_eq!(w.store.tasks().await.unwrap().len(),2);
}
