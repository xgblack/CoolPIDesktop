use base64::{Engine, engine::general_purpose::STANDARD};
use host_core::{
    Workbench,
    terminal::{TerminalService, TerminalSnapshot},
};
use std::{path::PathBuf, time::Duration};
fn dir() -> PathBuf {
    let p = std::env::temp_dir().join(format!("pty-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&p).unwrap();
    p.canonicalize().unwrap()
}
async fn until(s: &TerminalService, task: &str, id: &str, needle: &str) -> TerminalSnapshot {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let out = s.snapshot(task, id, 0).await.unwrap();
            let bytes = STANDARD.decode(&out.output).unwrap();
            if String::from_utf8_lossy(&bytes).contains(needle) {
                return out;
            }
            tokio::time::sleep(Duration::from_millis(20)).await
        }
    })
    .await
    .expect("PTY output deadline")
}
#[tokio::test]
async fn independent_io_resize_unicode_and_close_while_waiting() {
    let a = dir();
    let b = dir();
    let s = TerminalService::default();
    let ta = s.create("a", a.clone()).await.unwrap();
    let tb = s.create("b", b.clone()).await.unwrap();
    s.write(
        "a",
        &ta.id,
        "printf '中文甲' > result; pwd; printf '\\nREADY_A\\n'\r".into(),
    )
    .await
    .unwrap();
    s.write(
        "b",
        &tb.id,
        "printf '中文乙' > result; pwd; printf '\\nREADY_B\\n'\r".into(),
    )
    .await
    .unwrap();
    until(&s, "a", &ta.id, "READY_A\r\n").await;
    until(&s, "b", &tb.id, "READY_B\r\n").await;
    assert_eq!(std::fs::read_to_string(a.join("result")).unwrap(), "中文甲");
    assert_eq!(std::fs::read_to_string(b.join("result")).unwrap(), "中文乙");
    assert_eq!(
        s.write("b", &ta.id, "oops".into()).await.unwrap_err().code,
        "terminal_denied"
    );
    s.resize("a", &ta.id, 97, 31).await.unwrap();
    s.write("a", &ta.id, "stty size\r".into()).await.unwrap();
    until(&s, "a", &ta.id, "31 97").await;
    s.write("a", &ta.id, "sleep 60\r".into()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    tokio::time::timeout(Duration::from_secs(4), s.close("a", &ta.id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        s.snapshot("a", &ta.id, 0).await.unwrap_err().code,
        "terminal_missing"
    );
    s.write("b", &tb.id, "exit 7\r".into()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            let v = s.snapshot("b", &tb.id, 0).await.unwrap();
            if v.exited {
                assert_eq!(v.exit_code, Some(7));
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await
        }
    })
    .await
    .unwrap();
    s.close_all().await.unwrap();
}
#[tokio::test]
async fn bounded_output_and_incremental_cursor() {
    let s = TerminalService::default();
    let t = s.create("task", dir()).await.unwrap();
    s.write(
        "task",
        &t.id,
        "head -c 700000 /dev/zero | tr '\\0' x; printf '\\nEND_MARKER\\n'\r".into(),
    )
    .await
    .unwrap();
    let out = until(&s, "task", &t.id, "END_MARKER\r\n").await;
    assert!(out.start > 0);
    assert!(STANDARD.decode(out.output).unwrap().len() <= 512 * 1024);
    let delta = s.snapshot("task", &t.id, out.end).await.unwrap();
    assert_eq!(delta.start, out.end);
    assert!(s.write("task", &t.id, "x".repeat(65537)).await.is_err());
    assert!(s.resize("task", &t.id, 0, 1).await.is_err());
    s.close_all().await.unwrap();
}
#[tokio::test]
async fn host_stop_without_omp_and_missing_root_reclaim_terminal() {
    let root = dir();
    let w = Workbench::open(root.join("data")).await.unwrap();
    let p = w
        .store
        .register_project("P", vec![root.clone()], true)
        .await
        .unwrap();
    let task = w.create_task(&p.id, "task", "shared").await.unwrap();
    let t = w.terminal_create(&task.id, 0).await.unwrap();
    let _ = w.stop(&task.id).await;
    assert_eq!(
        w.terminal_snapshot(&task.id, &t.id, 0)
            .await
            .unwrap_err()
            .code,
        "terminal_missing"
    );
    let t = w.terminal_create(&task.id, 0).await.unwrap();
    let relocated = root.with_extension("moved");
    std::fs::rename(&root, &relocated).unwrap();
    assert!(w.terminal_snapshot(&task.id, &t.id, 0).await.is_err());
    w.shutdown().await.unwrap();
}
