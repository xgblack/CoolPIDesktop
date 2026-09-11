use host_core::Workbench;
use std::{path::Path, process::Command};

fn command(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(root)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

#[tokio::test]
async fn workbench_exposes_only_read_only_task_scoped_git_data() {
    let root = std::env::temp_dir().join(format!("omp-git-workbench-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    command(&root, &["init"]);
    command(&root, &["config", "user.email", "test@example.invalid"]);
    command(&root, &["config", "user.name", "OMP test"]);
    std::fs::write(root.join("tracked.txt"), "base\n").unwrap();
    command(&root, &["add", "tracked.txt"]);
    command(&root, &["commit", "-m", "base"]);
    std::fs::write(root.join("tracked.txt"), "worktree change\n").unwrap();
    std::fs::write(root.join("staged.txt"), "staged change\n").unwrap();
    command(&root, &["add", "staged.txt"]);
    std::fs::write(root.join("untracked.txt"), "untracked change\n").unwrap();

    let workbench = Workbench::open(root.join("data")).await.unwrap();
    let project = workbench
        .store
        .register_project("Git", vec![root.clone()], true)
        .await
        .unwrap();
    let task = workbench
        .store
        .create_task(&project.id, "Diff")
        .await
        .unwrap();
    let status = workbench.git_status(&task.id, 0).await.unwrap();
    assert!(status.available);
    assert!(
        status
            .changes
            .iter()
            .any(|change| change.path == "tracked.txt" && change.worktree_status == "M")
    );
    assert!(
        status
            .changes
            .iter()
            .any(|change| change.path == "staged.txt" && change.index_status == "A")
    );
    assert!(
        status
            .changes
            .iter()
            .any(|change| change.path == "untracked.txt" && change.kind == "untracked")
    );

    assert!(
        workbench
            .git_diff(&task.id, 0, "tracked.txt", false, false)
            .await
            .unwrap()
            .text
            .contains("worktree change")
    );
    assert!(
        workbench
            .git_diff(&task.id, 0, "staged.txt", true, false)
            .await
            .unwrap()
            .text
            .contains("staged change")
    );
    assert!(
        workbench
            .git_diff(&task.id, 0, "untracked.txt", false, true)
            .await
            .unwrap()
            .text
            .contains("untracked change")
    );
    assert_eq!(
        workbench
            .git_diff(&task.id, 0, "../outside", false, false)
            .await
            .unwrap_err()
            .code,
        "invalid_git_path"
    );
}

#[tokio::test]
async fn git_view_is_scoped_when_a_task_root_is_a_repository_subdirectory() {
    let root = std::env::temp_dir().join(format!("omp-git-subdir-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("task")).unwrap();
    std::fs::create_dir_all(root.join("outside")).unwrap();
    command(&root, &["init"]);
    command(&root, &["config", "user.email", "test@example.invalid"]);
    command(&root, &["config", "user.name", "OMP test"]);
    std::fs::write(root.join("task/inside.txt"), "base\n").unwrap();
    std::fs::write(root.join("outside/private.txt"), "base\n").unwrap();
    command(&root, &["add", "."]);
    command(&root, &["commit", "-m", "base"]);
    std::fs::write(root.join("task/inside.txt"), "changed\n").unwrap();
    std::fs::write(root.join("outside/private.txt"), "private change\n").unwrap();

    let workbench = Workbench::open(root.join("data")).await.unwrap();
    let project = workbench
        .store
        .register_project("Scoped", vec![root.join("task")], true)
        .await
        .unwrap();
    let task = workbench
        .store
        .create_task(&project.id, "Scoped diff")
        .await
        .unwrap();
    let status = workbench.git_status(&task.id, 0).await.unwrap();

    assert_eq!(status.changes.len(), 1);
    assert_eq!(status.changes[0].path, "inside.txt");
    assert!(
        workbench
            .git_diff(&task.id, 0, "inside.txt", false, false)
            .await
            .unwrap()
            .text
            .contains("changed")
    );
    assert!(
        workbench
            .git_diff(&task.id, 0, "../outside/private.txt", false, false)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn unborn_repository_and_literal_paths_and_missing_diff() {
    let root = std::env::temp_dir().join(format!("omp-git-edge-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    command(&root, &["init"]);
    std::fs::write(root.join("literal*.txt"), "literal\n").unwrap();
    std::fs::write(root.join("literal-other.txt"), "other\n").unwrap();
    assert!(host_core::git::status(&root, 0).await.unwrap().available);
    command(&root, &["add", "."]);
    let diff = host_core::git::diff(&root, 0, "literal*.txt", true, false)
        .await
        .unwrap();
    assert!(diff.text.contains("+literal"));
    assert!(!diff.text.contains("+other"));
    assert!(
        host_core::git::diff(&root, 0, "missing.txt", false, true)
            .await
            .is_err()
    );
}
