use host_core::Workbench;
use std::{
    path::{Path, PathBuf},
    process::Command,
};
fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
async fn fixture() -> (Workbench, String, String, PathBuf, PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("git-write-{}", uuid::Uuid::new_v4()));
    let repo = base.join("repo");
    std::fs::create_dir_all(repo.join("one")).unwrap();
    std::fs::create_dir_all(repo.join("two")).unwrap();
    git(&repo, &["init"]);
    git(&repo, &["config", "user.name", "Test"]);
    git(&repo, &["config", "user.email", "test@example.invalid"]);
    std::fs::write(repo.join("one/a.txt"), "base\n").unwrap();
    std::fs::write(repo.join("two/b.txt"), "base\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "base"]);
    let w = Workbench::open(base.join("data")).await.unwrap();
    let p = w
        .store
        .register_project("P", vec![repo.join("one"), repo.join("two")], true)
        .await
        .unwrap();
    let a = w.create_task(&p.id, "A", "isolated").await.unwrap();
    let b = w.create_task(&p.id, "B", "isolated").await.unwrap();
    let ar = w.task_roots(&a.id).await.unwrap()[0].execution_path.clone();
    let br = w.task_roots(&b.id).await.unwrap()[0].execution_path.clone();
    (w, a.id, b.id, ar, br, repo)
}
#[tokio::test]
async fn stage_unstage_commit_isolate_tasks_and_preserve_unstaged_content() {
    let (w, a, b, ar, br, repo) = fixture().await;
    let head = git(&repo, &["rev-parse", "HEAD"]);
    std::fs::write(ar.join("a.txt"), "stage\n").unwrap();
    std::fs::write(br.join("a.txt"), "other\n").unwrap();
    w.git_change(&a, 0, "a.txt", "stage", false).await.unwrap();
    w.git_change(&a, 0, "a.txt", "unstage", false)
        .await
        .unwrap();
    assert_eq!(
        w.git_commit_preview(&a, 0).await.unwrap_err().code,
        "git_empty_index"
    );
    w.git_change(&a, 0, "a.txt", "stage", false).await.unwrap();
    let preview = w.git_commit_preview(&a, 0).await.unwrap();
    assert_eq!(preview.paths, vec!["a.txt"]);
    std::fs::write(ar.join("a.txt"), "unstaged\n").unwrap();
    let sha = w.git_commit(&a, 0, "update", preview).await.unwrap();
    assert_ne!(sha, head);
    assert_eq!(git(&ar, &["show", "HEAD:one/a.txt"]), "stage");
    assert_eq!(
        std::fs::read_to_string(ar.join("a.txt")).unwrap(),
        "unstaged\n"
    );
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&br, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        w.git_commit_preview(&b, 0).await.unwrap_err().code,
        "git_empty_index"
    );
}
#[tokio::test]
async fn reject_escape_symlinks_shared_roots_and_outside_staged_files() {
    let (w, a, _, ar, _, repo) = fixture().await;
    for path in ["../two/b.txt", "/tmp/file", ".git/config", "."] {
        assert!(w.git_change(&a, 0, path, "stage", false).await.is_err());
    }
    std::os::unix::fs::symlink(&repo, ar.join("escape")).unwrap();
    assert!(
        w.git_change(&a, 0, "escape/one/a.txt", "stage", false)
            .await
            .is_err()
    );
    std::os::unix::fs::symlink(repo.join("one/a.txt"), ar.join("link")).unwrap();
    assert!(w.git_change(&a, 0, "link", "stage", false).await.is_err());
    let second = w.task_roots(&a).await.unwrap()[1].execution_path.clone();
    std::fs::write(second.join("b.txt"), "outside\n").unwrap();
    w.git_change(&a, 1, "b.txt", "stage", false).await.unwrap();
    assert_eq!(
        w.git_commit_preview(&a, 0).await.unwrap_err().code,
        "git_write_denied"
    );
    let task = w.store.task(&a).await.unwrap();
    let shared = w
        .create_task(&task.project_id, "shared", "shared")
        .await
        .unwrap();
    assert_eq!(
        w.git_change(&shared.id, 0, "a.txt", "stage", false)
            .await
            .unwrap_err()
            .code,
        "git_write_denied"
    );
    assert!(w.git_change(&a, 9, "a.txt", "stage", false).await.is_err());
}
#[tokio::test]
async fn literal_paths_stale_confirmation_and_failed_commit_preserve_index() {
    let (w, a, _, ar, _, _) = fixture().await;
    std::fs::write(ar.join("*.txt"), "literal\n").unwrap();
    std::fs::write(ar.join("a.txt"), "not selected\n").unwrap();
    w.git_change(&a, 0, "*.txt", "stage", false).await.unwrap();
    let old = w.git_commit_preview(&a, 0).await.unwrap();
    assert_eq!(old.paths, vec!["*.txt"]);
    w.git_change(&a, 0, "a.txt", "stage", false).await.unwrap();
    assert_eq!(
        w.git_commit(&a, 0, "update", old).await.unwrap_err().code,
        "git_stale_preview"
    );
    let expected = w.git_commit_preview(&a, 0).await.unwrap();
    // Git's own index lock failure must leave staged contents and HEAD unchanged.
    let index = PathBuf::from(git(
        &ar,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "index.lock",
        ],
    ));
    std::fs::write(&index, "test lock").unwrap();
    assert!(
        w.git_commit(&a, 0, "update", expected.clone())
            .await
            .is_err()
    );
    assert_eq!(git(&ar, &["rev-parse", "HEAD"]), expected.head);
    assert_eq!(
        git(&ar, &["diff", "--cached", "--name-only"])
            .lines()
            .count(),
        2
    );
}
#[tokio::test]
async fn discard_requires_confirmation_and_restores_index_version() {
    let (w, a, _, ar, _, _) = fixture().await;
    std::fs::write(ar.join("a.txt"), "staged\n").unwrap();
    w.git_change(&a, 0, "a.txt", "stage", false).await.unwrap();
    std::fs::write(ar.join("a.txt"), "discard me\n").unwrap();
    assert!(
        w.git_change(&a, 0, "a.txt", "discard", false)
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(ar.join("a.txt")).unwrap(),
        "discard me\n"
    );
    let tree = w.git_commit_preview(&a, 0).await.unwrap().tree;
    w.git_change(&a, 0, "a.txt", "discard", true).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(ar.join("a.txt")).unwrap(),
        "staged\n"
    );
    assert_eq!(w.git_commit_preview(&a, 0).await.unwrap().tree, tree);
    std::fs::write(ar.join("new.txt"), "keep").unwrap();
    assert!(
        w.git_change(&a, 0, "new.txt", "discard", true)
            .await
            .is_err()
    );
    assert!(ar.join("new.txt").exists());
}

#[tokio::test]
async fn merge_state_and_detached_head_block_commit() {
    let (w, a, _, ar, _, _) = fixture().await;
    std::fs::write(ar.join("a.txt"), "changed\n").unwrap();
    w.git_change(&a, 0, "a.txt", "stage", false).await.unwrap();
    let marker = git(
        &ar,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "MERGE_HEAD",
        ],
    );
    std::fs::write(marker, git(&ar, &["rev-parse", "HEAD"])).unwrap();
    assert_eq!(
        w.git_commit_preview(&a, 0).await.unwrap_err().code,
        "git_write_denied"
    );
    git(&ar, &["checkout", "--detach"]);
    assert!(w.git_commit_preview(&a, 0).await.is_err());
}
