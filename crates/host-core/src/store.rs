use crate::{
    runtime::{HostError, Result},
    workspace,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub roots: Vec<PathBuf>,
    pub archived: bool,
    pub trusted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub roots: Vec<PathBuf>,
    pub pinned: bool,
    pub archived: bool,
    pub session_id: Option<String>,
    pub session_file: Option<PathBuf>,
    pub model: Option<String>,
    pub last_run: Option<TaskRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRun {
    pub id: String,
    pub task_id: String,
    pub state: String,
    pub error_code: Option<String>,
}

#[derive(Clone)]
pub struct Store {
    connection: Arc<Mutex<Connection>>,
}

fn db(e: impl std::fmt::Display) -> HostError {
    HostError::new("database_error", e)
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
fn name(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 512 {
        return Err(HostError::new(
            "invalid_name",
            "Name must contain 1 to 512 bytes",
        ));
    }
    Ok(value.into())
}
fn paths(value: String) -> rusqlite::Result<Vec<PathBuf>> {
    serde_json::from_str(&value).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}
fn session_path(file: &std::path::Path, require_file: bool) -> Result<PathBuf> {
    if !file.is_absolute() {
        return Err(HostError::new(
            "session_invalid",
            "Session path must be absolute",
        ));
    }
    let parent = file
        .parent()
        .ok_or_else(|| HostError::new("session_invalid", "Session path has no parent"))?
        .canonicalize()
        .map_err(|e| HostError::new("session_missing", e))?;
    let name = file
        .file_name()
        .ok_or_else(|| HostError::new("session_invalid", "Session path has no file name"))?;
    let resolved = parent.join(name);
    if std::fs::symlink_metadata(&resolved).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(HostError::new(
            "session_invalid",
            "Session file may not be a symlink",
        ));
    }
    if require_file && !resolved.is_file() {
        return Err(HostError::new(
            "session_invalid",
            "Session must be an existing file with an identity",
        ));
    }
    Ok(resolved)
}
fn task_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRecord> {
    Ok(TaskRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        title: row.get(2)?,
        roots: paths(row.get(3)?)?,
        pinned: row.get(4)?,
        archived: row.get(5)?,
        session_id: row.get(6)?,
        session_file: row.get::<_, Option<String>>(7)?.map(PathBuf::from),
        model: row.get(8)?,
        last_run: row
            .get::<_, Option<String>>(9)?
            .map(|id| -> rusqlite::Result<TaskRun> {
                Ok(TaskRun {
                    id,
                    task_id: row.get(0)?,
                    state: row.get(10)?,
                    error_code: row.get(11)?,
                })
            })
            .transpose()?,
    })
}
const TASK_QUERY: &str = "SELECT t.id,t.project_id,t.title,t.roots,t.pinned,t.archived,b.session_id,b.session_file,t.model,r.id,r.state,r.error_code FROM tasks t LEFT JOIN session_bindings b ON b.task_id=t.id LEFT JOIN task_runs r ON r.id=(SELECT id FROM task_runs WHERE task_id=t.id ORDER BY started_at DESC,rowid DESC LIMIT 1)";

impl Store {
    pub async fn open(path: PathBuf) -> Result<Self> {
        tokio::task::spawn_blocking(move || {
            if let Some(parent) = path.parent() { std::fs::create_dir_all(parent).map_err(db)?; }
            let mut connection = Connection::open(path).map_err(db)?;
            connection.busy_timeout(std::time::Duration::from_secs(5)).map_err(db)?;
            connection.execute_batch("PRAGMA foreign_keys=ON;").map_err(db)?;
            let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(db)?;
            if version > 1 { return Err(HostError::new("database_version_unsupported", format!("Unsupported schema {version}"))); }
            if version == 0 {
                let tx = connection.transaction().map_err(db)?;
                tx.execute_batch("CREATE TABLE projects(id TEXT PRIMARY KEY,name TEXT NOT NULL,roots TEXT NOT NULL,original_roots TEXT NOT NULL,trusted INTEGER NOT NULL,archived INTEGER NOT NULL DEFAULT 0);
                    CREATE TABLE project_roots(path TEXT PRIMARY KEY,project_id TEXT NOT NULL REFERENCES projects(id),ordinal INTEGER NOT NULL);
                    CREATE TABLE tasks(id TEXT PRIMARY KEY,project_id TEXT NOT NULL REFERENCES projects(id),title TEXT NOT NULL,roots TEXT NOT NULL,original_roots TEXT NOT NULL,trusted INTEGER NOT NULL,pinned INTEGER NOT NULL DEFAULT 0,archived INTEGER NOT NULL DEFAULT 0,model TEXT,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL);
                    CREATE TABLE session_bindings(task_id TEXT PRIMARY KEY REFERENCES tasks(id),session_id TEXT NOT NULL UNIQUE,session_file TEXT NOT NULL UNIQUE,omp_version TEXT);
                    CREATE TABLE task_runs(id TEXT PRIMARY KEY,task_id TEXT NOT NULL REFERENCES tasks(id),state TEXT NOT NULL,error_code TEXT,started_at INTEGER NOT NULL,ended_at INTEGER);
                    CREATE UNIQUE INDEX one_active_run ON task_runs(task_id) WHERE ended_at IS NULL;
                    CREATE TABLE settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);
                    PRAGMA user_version=1;").map_err(db)?;
                tx.commit().map_err(db)?;
            }
            connection.execute("UPDATE task_runs SET state='interrupted',error_code='host_interrupted',ended_at=?1 WHERE ended_at IS NULL", [now()]).map_err(db)?;
            Ok(Self { connection: Arc::new(Mutex::new(connection)) })
        }).await.map_err(db)?
    }

    async fn access<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let connection = self.connection.clone();
        tokio::task::spawn_blocking(move || {
            let mut c = connection.lock().map_err(db)?;
            f(&mut c)
        })
        .await
        .map_err(db)?
    }

    pub async fn projects(&self) -> Result<Vec<Project>> {
        self.access(|c| {
            c.prepare("SELECT id,name,roots,archived,trusted FROM projects ORDER BY rowid")
                .map_err(db)?
                .query_map([], |r| {
                    Ok(Project {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        roots: paths(r.get(2)?)?,
                        archived: r.get(3)?,
                        trusted: r.get(4)?,
                    })
                })
                .map_err(db)?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(db)
        })
        .await
    }

    pub async fn register_project(
        &self,
        title: &str,
        roots: Vec<PathBuf>,
        trusted: bool,
    ) -> Result<Project> {
        let title = name(title)?;
        self.access(move |c| {
            let canonical = workspace::validate_roots(&roots)?;
            let p = Project {
                id: uuid::Uuid::new_v4().to_string(),
                name: title,
                roots: canonical,
                archived: false,
                trusted,
            };
            let tx = c.transaction().map_err(db)?;
            tx.execute(
                "INSERT INTO projects(id,name,roots,original_roots,trusted) VALUES(?1,?2,?3,?4,?5)",
                params![
                    p.id,
                    p.name,
                    serde_json::to_string(&p.roots).map_err(db)?,
                    serde_json::to_string(&roots).map_err(db)?,
                    trusted
                ],
            )
            .map_err(db)?;
            for (ordinal, root) in p.roots.iter().enumerate() {
                let duplicate = tx
                    .query_row(
                        "SELECT 1 FROM project_roots WHERE path=?1",
                        [root.to_string_lossy().as_ref()],
                        |_| Ok(()),
                    )
                    .optional()
                    .map_err(db)?
                    .is_some();
                if duplicate {
                    return Err(HostError::new(
                        "directory_duplicate",
                        "Directory already registered",
                    ));
                }
                tx.execute(
                    "INSERT INTO project_roots VALUES(?1,?2,?3)",
                    params![root.to_string_lossy(), p.id, ordinal as i64],
                )
                .map_err(db)?;
            }
            tx.commit().map_err(db)?;
            Ok(p)
        })
        .await
    }

    pub async fn update_project(&self, id: &str, title: &str, archived: bool) -> Result<()> {
        let (id, title) = (id.to_owned(), name(title)?);
        self.access(move |c| {
            if archived && c.query_row("SELECT EXISTS(SELECT 1 FROM task_runs r JOIN tasks t ON t.id=r.task_id WHERE t.project_id=?1 AND r.ended_at IS NULL)",[&id],|r| r.get::<_,bool>(0)).map_err(db)? { return Err(HostError::new("task_busy", "Stop active project tasks before archiving")); }
            if c.execute("UPDATE projects SET name=?2,archived=?3 WHERE id=?1",params![id,title,archived]).map_err(db)? == 0 { return Err(HostError::new("project_missing", "Unknown project")); }
            Ok(())
        }).await
    }

    pub async fn trust_project(&self, id: &str) -> Result<()> {
        let id = id.to_owned();
        self.access(move |c| {
            let (roots, original): (String, String) = c
                .query_row(
                    "SELECT roots,original_roots FROM projects WHERE id=?1",
                    [&id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(db)?
                .ok_or_else(|| HostError::new("project_missing", "Unknown project"))?;
            workspace::verify_snapshot(
                &paths(original).map_err(db)?,
                &paths(roots.clone()).map_err(db)?,
                true,
            )?;
            let tx = c.transaction().map_err(db)?;
            tx.execute("UPDATE projects SET trusted=1 WHERE id=?1", [&id])
                .map_err(db)?;
            tx.execute(
                "UPDATE tasks SET trusted=1 WHERE project_id=?1 AND roots=?2",
                params![id, roots],
            )
            .map_err(db)?;
            tx.commit().map_err(db)
        })
        .await
    }

    pub async fn tasks(&self) -> Result<Vec<TaskRecord>> {
        self.access(|c| {
            c.prepare(&format!(
                "{TASK_QUERY} ORDER BY t.pinned DESC,t.created_at DESC,t.rowid DESC"
            ))
            .map_err(db)?
            .query_map([], task_row)
            .map_err(db)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db)
        })
        .await
    }

    pub async fn task(&self, id: &str) -> Result<TaskRecord> {
        let id = id.to_owned();
        self.access(move |c| {
            c.query_row(&format!("{TASK_QUERY} WHERE t.id=?1"), [id], task_row)
                .optional()
                .map_err(db)?
                .ok_or_else(|| HostError::new("task_missing", "Unknown task"))
        })
        .await
    }

    pub async fn create_task(&self, project_id: &str, title: &str) -> Result<TaskRecord> {
        let (project_id, title) = (project_id.to_owned(), name(title)?);
        self.access(move |c| {
            let (roots,original,trusted,archived):(String,String,bool,bool)=c.query_row("SELECT roots,original_roots,trusted,archived FROM projects WHERE id=?1",[&project_id],|r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(db)?.ok_or_else(|| HostError::new("project_missing","Unknown project"))?;
            if archived { return Err(HostError::new("project_archived","Cannot add task to archived project")); }
            let id=uuid::Uuid::new_v4().to_string();
            c.execute("INSERT INTO tasks(id,project_id,title,roots,original_roots,trusted,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?7)",params![id,project_id,title,roots,original,trusted,now()]).map_err(db)?;
            c.query_row(&format!("{TASK_QUERY} WHERE t.id=?1"),[id],task_row).map_err(db)
        }).await
    }

    pub async fn validate_task_roots(&self, id: &str) -> Result<Vec<PathBuf>> {
        let id = id.to_owned();
        self.access(move |c| {
            let (roots,original,trusted,archived):(String,String,bool,bool)=c.query_row("SELECT t.roots,t.original_roots,t.trusted,(t.archived OR p.archived) FROM tasks t JOIN projects p ON p.id=t.project_id WHERE t.id=?1",[id],|r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(db)?;
            if archived { return Err(HostError::new("task_archived","Unarchive task and project before continuing")); }
            let roots=paths(roots).map_err(db)?;
            workspace::verify_snapshot(&paths(original).map_err(db)?, &roots, trusted)?;
            Ok(roots)
        }).await
    }

    pub async fn update_task(
        &self,
        id: &str,
        title: &str,
        pinned: bool,
        archived: bool,
    ) -> Result<()> {
        let (id, title) = (id.to_owned(), name(title)?);
        self.access(move |c| {
            if archived
                && c.query_row(
                    "SELECT EXISTS(SELECT 1 FROM task_runs WHERE task_id=?1 AND ended_at IS NULL)",
                    [&id],
                    |r| r.get::<_, bool>(0),
                )
                .map_err(db)?
            {
                return Err(HostError::new("task_busy", "Stop task before archiving"));
            }
            if c.execute(
                "UPDATE tasks SET title=?2,pinned=?3,archived=?4,updated_at=?5 WHERE id=?1",
                params![id, title, pinned, archived, now()],
            )
            .map_err(db)?
                == 0
            {
                return Err(HostError::new("task_missing", "Unknown task"));
            }
            Ok(())
        })
        .await
    }

    /// Replace a stopped task's directory snapshot after the original directory was moved,
    /// deleted, or redirected. The picker is the only caller allowed to supply these paths.
    pub async fn relocate_task(
        &self,
        id: &str,
        roots: Vec<PathBuf>,
        trusted: bool,
    ) -> Result<TaskRecord> {
        let id = id.to_owned();
        self.access(move |c| {
            if !trusted {
                return Err(HostError::new(
                    "project_untrusted",
                    "Confirm trust for the relocated directory before starting OMP",
                ));
            }
            let original = serde_json::to_string(&roots).map_err(db)?;
            let roots = workspace::validate_roots(&roots)?;
            if c.query_row(
                "SELECT EXISTS(SELECT 1 FROM task_runs WHERE task_id=?1 AND ended_at IS NULL)",
                [&id],
                |r| r.get::<_, bool>(0),
            )
            .map_err(db)?
            {
                return Err(HostError::new(
                    "task_busy",
                    "Stop task before relocating its directory",
                ));
            }
            let json = serde_json::to_string(&roots).map_err(db)?;
            if c.execute(
                "UPDATE tasks SET roots=?2,original_roots=?4,trusted=1,updated_at=?3 WHERE id=?1",
                params![id, json, now(), original],
            )
            .map_err(db)?
                == 0
            {
                return Err(HostError::new("task_missing", "Unknown task"));
            }
            c.query_row(&format!("{TASK_QUERY} WHERE t.id=?1"), [&id], task_row)
                .map_err(db)
        })
        .await
    }

    pub async fn bind(&self, id: &str, session_id: &str, session_file: PathBuf) -> Result<()> {
        let (id, session_id) = (id.to_owned(), session_id.to_owned());
        self.access(move |c| {
            let path = session_path(&session_file, true)?;
            if session_id.is_empty() {
                return Err(HostError::new(
                    "session_invalid",
                    "Session must have an identity",
                ));
            }
            let existing: Option<(String, String)> = c
                .query_row(
                    "SELECT session_id,session_file FROM session_bindings WHERE task_id=?1",
                    [&id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(db)?;
            let file = path.to_string_lossy().into_owned();
            if let Some((old_id, old_file)) = existing {
                if old_id != session_id || old_file != file {
                    return Err(HostError::new(
                        "session_mismatch",
                        "Cannot replace a task's bound session",
                    ));
                }
                return Ok(());
            }
            c.execute(
                "INSERT INTO session_bindings(task_id,session_id,session_file) VALUES(?1,?2,?3)",
                params![id, session_id, file],
            )
            .map_err(db)?;
            Ok(())
        })
        .await
    }

    pub async fn reserve_session(&self, id: &str, session_id: &str, file: PathBuf) -> Result<()> {
        let (id, session_id) = (id.to_owned(), session_id.to_owned());
        self.access(move |c| {
            if session_id.is_empty() {
                return Err(HostError::new(
                    "session_invalid",
                    "Invalid reserved session",
                ));
            }
            // OMP reports the session path before its first prompt creates the file. Normalize the
            // existing directory now so a later canonical path cannot look like a new session.
            let file = session_path(&file, false)?;
            c.execute(
                "INSERT INTO session_bindings(task_id,session_id,session_file) VALUES(?1,?2,?3)",
                params![id, session_id, file.to_string_lossy()],
            )
            .map_err(db)?;
            Ok(())
        })
        .await
    }

    pub async fn set_model(&self, id: &str, model: Option<String>) -> Result<()> {
        let id = id.to_owned();
        self.access(move |c| {
            if c.execute(
                "UPDATE tasks SET model=?2,updated_at=?3 WHERE id=?1",
                params![id, model, now()],
            )
            .map_err(db)?
                == 0
            {
                return Err(HostError::new("task_missing", "Unknown task"));
            }
            Ok(())
        })
        .await
    }
    pub async fn session_version(&self, id: &str, version: Option<String>) -> Result<()> {
        let id = id.to_owned();
        self.access(move |c| {
            c.execute(
                "UPDATE session_bindings SET omp_version=?2 WHERE task_id=?1",
                params![id, version],
            )
            .map_err(db)?;
            Ok(())
        })
        .await
    }
    pub async fn set_executable(&self, path: String) -> Result<()> {
        self.set_setting("executable", path).await
    }
    pub async fn executable(&self) -> Result<Option<String>> {
        self.setting("executable").await
    }
    pub async fn set_setting(&self, key: &str, value: String) -> Result<()> {
        let key = key.to_owned();
        self.access(move |c| {c.execute("INSERT INTO settings VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value]).map_err(db)?;Ok(())}).await
    }
    pub async fn setting(&self, key: &str) -> Result<Option<String>> {
        let key = key.to_owned();
        self.access(move |c| {
            c.query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .map_err(db)
        })
        .await
    }
    pub async fn begin_run(&self, task_id: &str, run_id: &str) -> Result<()> {
        let (task_id, run_id) = (task_id.to_owned(), run_id.to_owned());
        self.access(move |c| {
            c.execute(
                "INSERT INTO task_runs(id,task_id,state,started_at) VALUES(?1,?2,'starting',?3)",
                params![run_id, task_id, now()],
            )
            .map_err(db)?;
            Ok(())
        })
        .await
    }
    pub async fn end_run(
        &self,
        run_id: &str,
        state: &str,
        error_code: Option<String>,
    ) -> Result<()> {
        let (run_id, state) = (run_id.to_owned(), state.to_owned());
        self.access(move |c| {
            if c.execute(
                "UPDATE task_runs SET state=?2,error_code=?3,ended_at=?4 WHERE id=?1",
                params![run_id, state, error_code, now()],
            )
            .map_err(db)?
                == 0
            {
                return Err(HostError::new("run_missing", "Unknown run"));
            }
            Ok(())
        })
        .await
    }
    pub async fn runs(&self, task_id: &str) -> Result<Vec<TaskRun>> {
        let task_id = task_id.to_owned();
        self.access(move |c|c.prepare("SELECT id,task_id,state,error_code FROM task_runs WHERE task_id=?1 ORDER BY started_at DESC,rowid DESC").map_err(db)?.query_map([task_id],|r|Ok(TaskRun{id:r.get(0)?,task_id:r.get(1)?,state:r.get(2)?,error_code:r.get(3)?})).map_err(db)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("cool-pi-store-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        path
    }
    #[tokio::test]
    async fn persists_and_recovers_interrupted_run() {
        let root = dir();
        let path = root.join("db.sqlite");
        let store = Store::open(path.clone()).await.unwrap();
        let project = store
            .register_project("Project", vec![root.clone()], true)
            .await
            .unwrap();
        let task = store.create_task(&project.id, "Task").await.unwrap();
        store
            .update_task(&task.id, "Renamed", true, false)
            .await
            .unwrap();
        store.set_executable("/tmp/omp".into()).await.unwrap();
        store.begin_run(&task.id, "first").await.unwrap();
        assert!(store.begin_run(&task.id, "second").await.is_err());
        assert_eq!(
            store
                .update_task(&task.id, "Task", false, true)
                .await
                .unwrap_err()
                .code,
            "task_busy"
        );
        drop(store);
        let reopened = Store::open(path).await.unwrap();
        assert_eq!(reopened.task(&task.id).await.unwrap().title, "Renamed");
        assert_eq!(
            reopened.runs(&task.id).await.unwrap()[0].state,
            "interrupted"
        );
        assert_eq!(
            reopened.executable().await.unwrap(),
            Some("/tmp/omp".into())
        );
        assert_eq!(
            reopened.validate_task_roots(&task.id).await.unwrap(),
            vec![root.canonicalize().unwrap()]
        );
    }
    #[tokio::test]
    async fn duplicate_root_rolls_back_project_and_untrusted_task_is_blocked() {
        let root = dir();
        let store = Store::open(root.join("db")).await.unwrap();
        let p = store
            .register_project("One", vec![root.clone()], false)
            .await
            .unwrap();
        assert_eq!(
            store
                .register_project("Two", vec![root], true)
                .await
                .unwrap_err()
                .code,
            "directory_duplicate"
        );
        assert_eq!(store.projects().await.unwrap().len(), 1);
        let task = store.create_task(&p.id, "Task").await.unwrap();
        assert_eq!(
            store.validate_task_roots(&task.id).await.unwrap_err().code,
            "project_untrusted"
        );
    }
    #[tokio::test]
    async fn unsupported_and_broken_schema_are_not_recreated() {
        let root = dir();
        let path = root.join("future");
        let c = Connection::open(&path).unwrap();
        c.execute_batch("PRAGMA user_version=99;").unwrap();
        drop(c);
        assert!(matches!(Store::open(path).await,Err(e) if e.code=="database_version_unsupported"));
        let path = root.join("broken");
        let c = Connection::open(&path).unwrap();
        c.execute_batch("CREATE TABLE tasks(existing TEXT);")
            .unwrap();
        drop(c);
        assert!(Store::open(path.clone()).await.is_err());
        let c = Connection::open(path).unwrap();
        let version: i64 = c
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 0);
        assert_eq!(
            c.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name='projects'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn session_binding_is_unique_and_immutable() {
        let root = dir();
        let store = Store::open(root.join("db")).await.unwrap();
        let p = store
            .register_project("P", vec![root.clone()], true)
            .await
            .unwrap();
        let a = store.create_task(&p.id, "A").await.unwrap();
        let b = store.create_task(&p.id, "B").await.unwrap();
        let session = root.join("session");
        std::fs::write(&session, "test fixture").unwrap();
        store
            .bind(&a.id, "session-a", session.clone())
            .await
            .unwrap();
        assert!(
            store
                .bind(&b.id, "session-a", session.clone())
                .await
                .is_err()
        );
        assert_eq!(
            store
                .bind(&a.id, "session-b", session)
                .await
                .unwrap_err()
                .code,
            "session_mismatch"
        );
        assert!(
            store
                .bind(&b.id, "missing", root.join("missing"))
                .await
                .is_err()
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn reservation_normalizes_parent_before_omp_creates_file() {
        let root = dir();
        let actual = root.join("actual");
        std::fs::create_dir(&actual).unwrap();
        let alias = root.join("alias");
        std::os::unix::fs::symlink(&actual, &alias).unwrap();
        let store = Store::open(root.join("db")).await.unwrap();
        let p = store.register_project("P", vec![root], true).await.unwrap();
        let t = store.create_task(&p.id, "T").await.unwrap();
        store
            .reserve_session(&t.id, "reserved", alias.join("s.jsonl"))
            .await
            .unwrap();
        std::fs::write(actual.join("s.jsonl"), "fixture").unwrap();
        store
            .bind(&t.id, "reserved", actual.join("s.jsonl"))
            .await
            .unwrap();
        assert_eq!(
            store.task(&t.id).await.unwrap().session_file,
            Some(actual.canonicalize().unwrap().join("s.jsonl"))
        );
    }

    #[tokio::test]
    async fn read_only_write_reports_error_without_changing_data() {
        let root = dir();
        let store = Store::open(root.join("db")).await.unwrap();
        store.set_executable("original".into()).await.unwrap();
        store
            .access(|c| c.execute_batch("PRAGMA query_only=ON").map_err(db))
            .await
            .unwrap();
        assert_eq!(
            store
                .set_executable("changed".into())
                .await
                .unwrap_err()
                .code,
            "database_error"
        );
        assert_eq!(
            store.executable().await.unwrap().as_deref(),
            Some("original")
        );
    }

    #[tokio::test]
    async fn explicit_trust_enables_existing_task_and_archived_project_blocks_it() {
        let root = dir();
        let store = Store::open(root.join("db")).await.unwrap();
        let project = store
            .register_project("P", vec![root], false)
            .await
            .unwrap();
        let task = store.create_task(&project.id, "T").await.unwrap();
        store.trust_project(&project.id).await.unwrap();
        store.validate_task_roots(&task.id).await.unwrap();
        store
            .update_project(&project.id, "Renamed", true)
            .await
            .unwrap();
        assert_eq!(
            store.validate_task_roots(&task.id).await.unwrap_err().code,
            "task_archived"
        );
        assert_eq!(
            store
                .create_task(&project.id, "New")
                .await
                .unwrap_err()
                .code,
            "project_archived"
        );
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn redirected_directory_requires_fresh_trust() {
        let root = dir();
        let original = root.join("original");
        let moved = root.join("moved");
        let other = root.join("other");
        std::fs::create_dir(&original).unwrap();
        std::fs::create_dir(&other).unwrap();
        let store = Store::open(root.join("db")).await.unwrap();
        let p = store
            .register_project("P", vec![original.clone()], true)
            .await
            .unwrap();
        let t = store.create_task(&p.id, "T").await.unwrap();
        std::fs::rename(&original, &moved).unwrap();
        std::os::unix::fs::symlink(&other, &original).unwrap();
        assert_eq!(
            store.validate_task_roots(&t.id).await.unwrap_err().code,
            "directory_changed"
        );
    }

    #[tokio::test]
    async fn relocating_a_stopped_task_replaces_its_snapshot_only_with_new_trust() {
        let root = dir();
        let first = root.join("first");
        let second = root.join("second");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        let store = Store::open(root.join("db")).await.unwrap();
        let project = store
            .register_project("P", vec![first], true)
            .await
            .unwrap();
        let task = store.create_task(&project.id, "T").await.unwrap();
        assert_eq!(
            store
                .relocate_task(&task.id, vec![second.clone()], false)
                .await
                .unwrap_err()
                .code,
            "project_untrusted"
        );
        let moved = store
            .relocate_task(&task.id, vec![second.clone()], true)
            .await
            .unwrap();
        assert_eq!(moved.roots, vec![second.canonicalize().unwrap()]);
        assert_eq!(
            store.validate_task_roots(&task.id).await.unwrap(),
            moved.roots
        );
        store.begin_run(&task.id, "active").await.unwrap();
        assert_eq!(
            store
                .relocate_task(&task.id, vec![root], true)
                .await
                .unwrap_err()
                .code,
            "task_busy"
        );
    }
}
