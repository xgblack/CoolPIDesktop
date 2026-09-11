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

fn git_output(root: &Path, args: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[tokio::test]
async fn clean_worktree_cleanup_removes_registration_preserves_branch_and_is_idempotent() {
    let root = repo();
    let data = root.join("data");
    let w = Workbench::open(data.clone()).await.unwrap();
    let project = w
        .store
        .register_project("P", vec![root.join("one"), root.join("two")], true)
        .await
        .unwrap();
    let task = w
        .create_task(&project.id, "clean", "isolated")
        .await
        .unwrap();
    let other = w
        .create_task(&project.id, "other", "isolated")
        .await
        .unwrap();
    let mapped = w.task_roots(&task.id).await.unwrap();
    let path = mapped[0].worktree_path.as_ref().unwrap();
    let branch = mapped[0].branch.as_ref().unwrap();
    assert_eq!(mapped[1].worktree_path.as_ref(), Some(path));
    let source_head = git_output(&root, &["rev-parse", "HEAD"]);
    let source_status = git_output(&root, &["status", "--porcelain", "-z"]);
    // List the exact, test-owned clean files before exercising the real Trash API.
    assert_eq!(
        git_output(path, &["ls-files"]),
        b"one/base.txt\ntwo/base.txt\n"
    );
    assert_eq!(
        git_output(path, &["status", "--porcelain", "--ignored"]),
        b""
    );
    let cleaned = w.cleanup_worktrees(&task.id).await.unwrap();
    assert_eq!(cleaned.len(), 2);
    assert!(cleaned.iter().all(|r| r.status == "trashed"));
    assert!(!path.exists());
    let registration = format!("worktree {}", path.display());
    let listing = git_output(&root, &["worktree", "list", "--porcelain", "-z"]);
    assert!(
        !listing
            .split(|b| *b == 0)
            .any(|f| f == registration.as_bytes())
    );
    assert_eq!(git_output(&root, &["rev-parse", branch]), source_head);
    assert_eq!(git_output(&root, &["rev-parse", "HEAD"]), source_head);
    assert_eq!(
        git_output(&root, &["status", "--porcelain", "-z"]),
        source_status
    );
    assert_eq!(
        std::fs::read_to_string(root.join("one/base.txt")).unwrap(),
        "base\n"
    );
    assert!(w.store.validate_task_roots(&other.id).await.is_ok());
    assert!(
        w.cleanup_worktrees(&task.id)
            .await
            .unwrap()
            .iter()
            .all(|r| r.status == "trashed")
    );
    drop(w);
    let reopened = Workbench::open(data).await.unwrap();
    assert!(
        reopened
            .task_roots(&task.id)
            .await
            .unwrap()
            .iter()
            .all(|r| r.status == "trashed")
    );
    assert_eq!(
        reopened
            .store
            .validate_task_roots(&task.id)
            .await
            .unwrap_err()
            .code,
        "worktree_missing"
    );
    assert!(reopened.store.validate_task_roots(&other.id).await.is_ok());
}

#[tokio::test]
async fn second_repository_failure_reclaims_only_worktrees_created_by_this_attempt() {
    let first = repo();
    let second = repo();
    let data = first.join("data");
    let w = Workbench::open(data.clone()).await.unwrap();
    let project = w
        .store
        .register_project("P", vec![first.join("one"), second.join("two")], true)
        .await
        .unwrap();
    let other = w
        .create_task(&project.id, "other", "isolated")
        .await
        .unwrap();
    let other_roots = w.task_roots(&other.id).await.unwrap();
    std::fs::write(
        other_roots[0].execution_path.join("keep.txt"),
        "other task\n",
    )
    .unwrap();
    let task = w
        .create_task(&project.id, "failing", "shared")
        .await
        .unwrap();
    // A real Git branch conflict occurs only after repository zero was created.
    let conflict = format!("cool-pi/{}-1", task.id);
    git(&second, &["branch", &conflict]);
    let first_listing = git_output(&first, &["worktree", "list", "--porcelain", "-z"]);
    let second_listing = git_output(&second, &["worktree", "list", "--porcelain", "-z"]);
    let first_head = git_output(&first, &["rev-parse", "HEAD"]);
    let second_head = git_output(&second, &["rev-parse", "HEAD"]);
    let error = w.isolate_task(&task.id).await.unwrap_err();
    assert_eq!(error.code, "worktree_create_failed");
    assert!(error.message.contains(&task.id));
    let mapped = w.task_roots(&task.id).await.unwrap();
    assert_eq!(mapped.len(), 2);
    assert_eq!(mapped[0].status, "trashed");
    assert_eq!(mapped[1].status, "failed");
    assert!(
        mapped
            .iter()
            .all(|r| !r.worktree_path.as_ref().unwrap().exists())
    );
    // The first branch proves creation reached Git; cleanup retains commits/branches.
    assert_eq!(
        git_output(&first, &["rev-parse", mapped[0].branch.as_ref().unwrap()]),
        first_head
    );
    assert_eq!(git_output(&second, &["rev-parse", &conflict]), second_head);
    assert_eq!(
        git_output(&first, &["worktree", "list", "--porcelain", "-z"]),
        first_listing
    );
    assert_eq!(
        git_output(&second, &["worktree", "list", "--porcelain", "-z"]),
        second_listing
    );
    assert_eq!(git_output(&first, &["rev-parse", "HEAD"]), first_head);
    assert_eq!(git_output(&second, &["rev-parse", "HEAD"]), second_head);
    assert_eq!(
        std::fs::read_to_string(other_roots[0].execution_path.join("keep.txt")).unwrap(),
        "other task\n"
    );
    assert!(w.store.validate_task_roots(&other.id).await.is_ok());
    drop(w);
    let reopened = Workbench::open(data).await.unwrap();
    let persisted = reopened.task_roots(&task.id).await.unwrap();
    assert_eq!(persisted[0].status, "trashed");
    assert_eq!(persisted[1].status, "failed");
    assert_eq!(
        reopened
            .store
            .validate_task_roots(&task.id)
            .await
            .unwrap_err()
            .code,
        "worktree_missing"
    );
    assert!(reopened.store.validate_task_roots(&other.id).await.is_ok());
}

#[tokio::test]
async fn task_files_reject_escape_and_symlink_and_preview_bounded_files() {
    let root = repo();
    let outside = root.join("outside.txt");
    std::fs::write(&outside, "outside\n").unwrap();
    std::os::unix::fs::symlink(&outside, root.join("one/link.txt")).unwrap();
    std::fs::write(root.join("one/readme.txt"), "hello\n").unwrap();
    let w = Workbench::open(root.join("data")).await.unwrap();
    let project = w
        .store
        .register_project("P", vec![root.join("one")], true)
        .await
        .unwrap();
    let task = w.store.create_task(&project.id, "files").await.unwrap();
    let page = w.list_files(&task.id, 0, "").await.unwrap();
    assert!(
        page.entries
            .iter()
            .any(|e| e.name == "link.txt" && e.kind == "symlink")
    );
    assert_eq!(
        w.preview_file(&task.id, 0, "readme.txt")
            .await
            .unwrap()
            .text
            .as_deref(),
        Some("hello\n")
    );
    assert_eq!(
        w.preview_file(&task.id, 0, "link.txt")
            .await
            .unwrap_err()
            .code,
        "file_path_denied"
    );
    for escaped in ["../outside.txt", "/tmp/nope", "readme.txt/../outside.txt"] {
        assert_eq!(
            w.preview_file(&task.id, 0, escaped).await.unwrap_err().code,
            "file_path_denied"
        );
    }
}

#[tokio::test]
async fn attachments_are_task_owned_and_prompt_payload_is_bounded() {
    let root = repo();
    let source = root.join("upload.txt");
    std::fs::write(&source, "attachment text\n").unwrap();
    let w = Workbench::open(root.join("data")).await.unwrap();
    let project = w
        .store
        .register_project("P", vec![root.join("one")], true)
        .await
        .unwrap();
    let a = w.store.create_task(&project.id, "a").await.unwrap();
    let b = w.store.create_task(&project.id, "b").await.unwrap();
    let imported = w.import_attachment(&a.id, &source).await.unwrap();
    assert_eq!(w.attachments(&a.id).await.unwrap().len(), 1);
    assert!(w.attachments(&b.id).await.unwrap().is_empty());
    assert_eq!(
        w.preview_attachment(&b.id, &imported.id)
            .await
            .unwrap_err()
            .code,
        "attachment_missing"
    );
    let payload = w
        .attachment_prompt(&a.id, "请总结", std::slice::from_ref(&imported.id))
        .await
        .unwrap();
    assert!(
        payload["message"]
            .as_str()
            .unwrap()
            .contains("attachment text")
    );
    assert!(
        w.attachment_prompt(&b.id, "", std::slice::from_ref(&imported.id))
            .await
            .unwrap_err()
            .code
            == "attachment_missing"
    );
}
