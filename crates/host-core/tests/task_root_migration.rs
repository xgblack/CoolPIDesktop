use host_core::store::Store;
use rusqlite::{Connection, params};

const V1_SCHEMA: &str = r#"CREATE TABLE projects(id TEXT PRIMARY KEY,name TEXT NOT NULL,roots TEXT NOT NULL,original_roots TEXT NOT NULL,trusted INTEGER NOT NULL,archived INTEGER NOT NULL DEFAULT 0);
                    CREATE TABLE project_roots(path TEXT PRIMARY KEY,project_id TEXT NOT NULL REFERENCES projects(id),ordinal INTEGER NOT NULL);
                    CREATE TABLE tasks(id TEXT PRIMARY KEY,project_id TEXT NOT NULL REFERENCES projects(id),title TEXT NOT NULL,roots TEXT NOT NULL,original_roots TEXT NOT NULL,trusted INTEGER NOT NULL,pinned INTEGER NOT NULL DEFAULT 0,archived INTEGER NOT NULL DEFAULT 0,model TEXT,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL);
                    CREATE TABLE session_bindings(task_id TEXT PRIMARY KEY REFERENCES tasks(id),session_id TEXT NOT NULL UNIQUE,session_file TEXT NOT NULL UNIQUE,omp_version TEXT);
                    CREATE TABLE task_runs(id TEXT PRIMARY KEY,task_id TEXT NOT NULL REFERENCES tasks(id),state TEXT NOT NULL,error_code TEXT,started_at INTEGER NOT NULL,ended_at INTEGER);
                    CREATE UNIQUE INDEX one_active_run ON task_runs(task_id) WHERE ended_at IS NULL;
                    CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);
                    PRAGMA user_version=1;"#;

fn fixture() -> (std::path::PathBuf, Vec<std::path::PathBuf>) {
    let dir = std::env::temp_dir().join(format!("omp-migration-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(dir.join("primary")).unwrap();
    std::fs::create_dir_all(dir.join("additional")).unwrap();
    let dir = dir.canonicalize().unwrap();
    let roots = vec![dir.join("primary"), dir.join("additional")];
    let db = dir.join("v1.sqlite");
    let c = Connection::open(&db).unwrap();
    c.execute_batch(V1_SCHEMA).unwrap();
    let json = serde_json::to_string(&roots).unwrap();
    c.execute(
        "INSERT INTO projects VALUES('p','Legacy',?1,?1,1,0)",
        [&json],
    )
    .unwrap();
    for (i, root) in roots.iter().enumerate() {
        c.execute(
            "INSERT INTO project_roots VALUES(?1,'p',?2)",
            params![root.to_str().unwrap(), i as i64],
        )
        .unwrap();
    }
    c.execute(
        "INSERT INTO tasks VALUES('t','p','Legacy task',?1,?1,1,1,0,'legacy-model',10,20)",
        [&json],
    )
    .unwrap();
    c.execute(
        "INSERT INTO tasks VALUES('archived','p','Archived task',?1,?1,1,0,1,NULL,30,40)",
        [&json],
    )
    .unwrap();
    c.execute(
        "INSERT INTO session_bindings VALUES('t','session-legacy',?1,'13.9.3')",
        [dir.join("session.jsonl").to_str().unwrap()],
    )
    .unwrap();
    (db, roots)
}

#[tokio::test]
async fn v1_backfills_all_roots_without_rewriting_tasks_or_session_bindings() {
    let (db, expected) = fixture();
    let c = Connection::open(&db).unwrap();
    let before: String = c.query_row("SELECT roots || original_roots || session_id || session_file || omp_version FROM tasks JOIN session_bindings ON id=task_id WHERE id='t'", [], |r| r.get(0)).unwrap();
    for _ in 0..2 {
        let store = Store::open(db.clone()).await.unwrap();
        for task in ["t", "archived"] {
            let roots = store.task_roots(task).await.unwrap();
            assert_eq!(roots.len(), 2);
            for (i, root) in roots.iter().enumerate() {
                assert_eq!(root.task_id, task);
                assert_eq!(root.root_index, i);
                assert_eq!(root.original_root, expected[i]);
                assert_eq!(root.execution_path, expected[i]);
                assert_eq!(root.mode, "shared");
                assert_eq!(root.status, "ready");
                assert!(!root.created_by_client && !root.source_dirty);
                assert!(
                    root.worktree_path.is_none()
                        && root.branch.is_none()
                        && root.baseline_commit.is_none()
                );
            }
        }
        assert_eq!(store.validate_task_roots("t").await.unwrap(), expected);
        let task = store.task("t").await.unwrap();
        assert_eq!(task.title, "Legacy task");
        assert!(task.pinned);
        assert_eq!(task.model.as_deref(), Some("legacy-model"));
        assert_eq!(task.session_id.as_deref(), Some("session-legacy"));
        assert!(store.task("archived").await.unwrap().archived);
        let after: String = c.query_row("SELECT roots || original_roots || session_id || session_file || omp_version FROM tasks JOIN session_bindings ON id=task_id WHERE id='t'", [], |r| r.get(0)).unwrap();
        assert_eq!(after, before);
        assert_eq!(
            c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            4
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM task_roots", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            4
        );
    }
}

#[tokio::test]
async fn malformed_legacy_roots_roll_back_migration_without_partial_backfill() {
    let (db, _) = fixture();
    let c = Connection::open(&db).unwrap();
    c.execute(
        "UPDATE tasks SET roots='broken json' WHERE id='archived'",
        [],
    )
    .unwrap();
    assert!(Store::open(db.clone()).await.is_err());
    assert_eq!(
        c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='task_roots'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    let original: String = c
        .query_row("SELECT roots FROM tasks WHERE id='t'", [], |r| r.get(0))
        .unwrap();
    c.execute("UPDATE tasks SET roots=?1 WHERE id='archived'", [&original])
        .unwrap();
    let store = Store::open(db).await.unwrap();
    assert_eq!(store.task_roots("t").await.unwrap().len(), 2);
    assert_eq!(store.task_roots("archived").await.unwrap().len(), 2);
}

const V2_ROOTS_SCHEMA: &str = r#"CREATE TABLE IF NOT EXISTS task_roots(task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,root_index INTEGER NOT NULL,original_root TEXT NOT NULL,execution_path TEXT NOT NULL,git_top_level TEXT,relative_path TEXT,mode TEXT NOT NULL,baseline_commit TEXT,branch TEXT,worktree_path TEXT,status TEXT NOT NULL,created_by_client INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(task_id,root_index));"#;

#[tokio::test]
async fn v2_upgrade_preserves_existing_isolated_mapping_and_defaults_source_dirty() {
    let (db, roots) = fixture();
    let c = Connection::open(&db).unwrap();
    c.execute_batch(V2_ROOTS_SCHEMA).unwrap();
    c.execute_batch("PRAGMA user_version=2;").unwrap();
    for (i, root) in roots.iter().enumerate() {
        c.execute("INSERT INTO task_roots VALUES('t',?1,?2,?3,?4,'one','isolated','baseline','cool-pi/existing',?5,'worktree_missing',1)", params![i as i64, root.to_str().unwrap(), root.join("execution").to_str().unwrap(), roots[0].to_str().unwrap(), root.join("worktree").to_str().unwrap()]).unwrap();
    }
    let before: Vec<String> = c.prepare("SELECT json_array(task_id,root_index,original_root,execution_path,git_top_level,relative_path,mode,baseline_commit,branch,worktree_path,status,created_by_client) FROM task_roots ORDER BY root_index").unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_,_>>().unwrap();
    let store = Store::open(db).await.unwrap();
    let mapped = store.task_roots("t").await.unwrap();
    assert_eq!(mapped.len(), 2);
    assert!(mapped.iter().all(|r| !r.source_dirty
        && r.mode == "isolated"
        && r.status == "worktree_missing"
        && r.created_by_client));
    let after: Vec<String> = c.prepare("SELECT json_array(task_id,root_index,original_root,execution_path,git_top_level,relative_path,mode,baseline_commit,branch,worktree_path,status,created_by_client) FROM task_roots ORDER BY root_index").unwrap().query_map([], |r| r.get(0)).unwrap().collect::<Result<_,_>>().unwrap();
    assert_eq!(after, before);
    assert_eq!(
        c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        4
    );
}
