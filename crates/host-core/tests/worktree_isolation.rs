use host_core::Workbench;
use std::{path::Path, process::Command};

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(root)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn repo() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("omp-worktree-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("one")).unwrap();
    std::fs::create_dir_all(root.join("two")).unwrap();
    git(&root, &["init"]);
    git(&root, &["config", "user.email", "test@example.invalid"]);
    git(&root, &["config", "user.name", "OMP test"]);
    std::fs::write(root.join("one/base.txt"), "base\n").unwrap();
    std::fs::write(root.join("two/base.txt"), "base\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "base"]);
    root
}

#[tokio::test]
async fn isolated_tasks_have_independent_worktrees_and_reuse_repo_for_subdirectories() {
    let root = repo();
    std::fs::write(root.join("one/base.txt"), "uncommitted source\n").unwrap();
    std::fs::write(root.join("one/staged.txt"), "staged source\n").unwrap();
    git(&root, &["add", "one/staged.txt"]);
    std::fs::write(root.join(".git/info/exclude"), "one/ignored.txt\n").unwrap();
    std::fs::write(root.join("one/ignored.txt"), "ignored source\n").unwrap();
    std::fs::write(root.join("one/untracked.txt"), "untracked source\n").unwrap();
    let workbench = Workbench::open(root.join("data")).await.unwrap();
    let project = workbench
        .store
        .register_project("P", vec![root.join("one"), root.join("two")], true)
        .await
        .unwrap();
    let first = workbench
        .create_task(&project.id, "first", "isolated")
        .await
        .unwrap();
    let second = workbench
        .create_task(&project.id, "second", "isolated")
        .await
        .unwrap();
    let first_roots = workbench.task_roots(&first.id).await.unwrap();
    let second_roots = workbench.task_roots(&second.id).await.unwrap();
    assert_eq!(first_roots.len(), 2);
    assert!(first_roots[0].source_dirty);
    assert_eq!(first_roots[0].mode, "isolated");
    assert_eq!(first_roots[0].worktree_path, first_roots[1].worktree_path);
    assert_ne!(first_roots[0].worktree_path, second_roots[0].worktree_path);
    assert_ne!(first_roots[0].branch, second_roots[0].branch);
    assert_eq!(
        std::fs::read_to_string(first_roots[0].execution_path.join("base.txt")).unwrap(),
        "base\n"
    );
    assert!(!first_roots[0].execution_path.join("staged.txt").exists());
    assert!(!first_roots[0].execution_path.join("ignored.txt").exists());
    assert!(!first_roots[0].execution_path.join("untracked.txt").exists());
    std::fs::write(first_roots[0].execution_path.join("created.txt"), "first\n").unwrap();
    std::fs::write(
        second_roots[1].execution_path.join("created.txt"),
        "second\n",
    )
    .unwrap();
    assert!(!root.join("one/created.txt").exists());
    assert!(!root.join("two/created.txt").exists());
    assert!(first_roots[0].execution_path.join("created.txt").is_file());
    assert!(second_roots[1].execution_path.join("created.txt").is_file());
}

#[tokio::test]
async fn missing_isolated_worktree_is_reported_after_reopen() {
    let root = repo();
    let data = root.join("data");
    let workbench = Workbench::open(data.clone()).await.unwrap();
    let project = workbench
        .store
        .register_project("P", vec![root.join("one")], true)
        .await
        .unwrap();
    let task = workbench
        .create_task(&project.id, "first", "isolated")
        .await
        .unwrap();
    let mapped = workbench.task_roots(&task.id).await.unwrap();
    let path = mapped[0].worktree_path.clone().unwrap();
    std::fs::rename(&path, path.with_extension("moved")).unwrap();
    let reopened = Workbench::open(data).await.unwrap();
    assert_eq!(
        reopened.continue_task(&task.id).await.unwrap_err().code,
        "worktree_missing"
    );
}

#[tokio::test]
async fn non_git_root_cannot_be_marked_isolated() {
    let root = std::env::temp_dir().join(format!("omp-nongit-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("plain")).unwrap();
    let workbench = Workbench::open(root.join("data")).await.unwrap();
    let project = workbench
        .store
        .register_project("P", vec![root.join("plain")], true)
        .await
        .unwrap();
    assert_eq!(
        workbench
            .create_task(&project.id, "first", "isolated")
            .await
            .unwrap_err()
            .code,
        "worktree_unavailable"
    );
}

#[tokio::test]
async fn cleanup_refuses_dirty_worktree_and_preserves_resources() {
    let root = repo();
    let workbench = Workbench::open(root.join("data")).await.unwrap();
    let project = workbench
        .store
        .register_project("P", vec![root.join("one")], true)
        .await
        .unwrap();
    let task = workbench
        .create_task(&project.id, "first", "isolated")
        .await
        .unwrap();
    let mapped = workbench.task_roots(&task.id).await.unwrap();
    let execution = mapped[0].execution_path.clone();
    std::fs::write(execution.join("changed.txt"), "keep\n").unwrap();

    let error = workbench.cleanup_worktrees(&task.id).await.unwrap_err();
    assert_eq!(error.code, "worktree_dirty");
    assert!(execution.join("changed.txt").is_file());
    assert_eq!(
        workbench.task_roots(&task.id).await.unwrap()[0].status,
        "ready"
    );
}
