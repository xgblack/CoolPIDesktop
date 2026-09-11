use crate::{HostError, Workbench, files, runtime::Result};
use portable_pty::{Child, CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};

const OUTPUT_LIMIT: usize = 512 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSnapshot {
    pub id: String,
    pub task_id: String,
    pub output: String,
    pub exited: bool,
    pub exit_code: Option<i32>,
}

struct Terminal {
    task_id: String,
    output: Arc<Mutex<Vec<u8>>>,
    exited: Arc<Mutex<Option<i32>>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
}

#[derive(Clone, Default)]
pub struct TerminalService {
    inner: Arc<Mutex<HashMap<String, Terminal>>>,
}

impl TerminalService {
    pub async fn create(&self, task_id: &str, root: PathBuf) -> Result<TerminalSnapshot> {
        tokio::task::spawn_blocking({
            let root = root.clone();
            move || files::absolute_dir(&root)
        })
        .await
        .map_err(|_| HostError::new("terminal_failed", "终端启动中断"))??;
        let id = uuid::Uuid::new_v4().to_string();
        let task = task_id.to_owned();
        let term = tokio::task::spawn_blocking(move || -> Result<Terminal> {
            let pty = NativePtySystem::default();
            let pair = pty
                .openpty(PtySize {
                    rows: 24,
                    cols: 80,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|e| HostError::new("terminal_failed", e.to_string()))?;
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
            let mut cmd = CommandBuilder::new(shell);
            cmd.cwd(root);
            cmd.env("TERM", "xterm-256color");
            let child = pair
                .slave
                .spawn_command(cmd)
                .map_err(|e| HostError::new("terminal_failed", e.to_string()))?;
            drop(pair.slave);
            let mut reader = pair
                .master
                .try_clone_reader()
                .map_err(|e| HostError::new("terminal_failed", e.to_string()))?;
            let writer = pair
                .master
                .take_writer()
                .map_err(|e| HostError::new("terminal_failed", e.to_string()))?;
            let output = Arc::new(Mutex::new(Vec::new()));
            let exited = Arc::new(Mutex::new(None));
            let out = output.clone();
            thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Ok(mut v) = out.lock() {
                                v.extend_from_slice(&buf[..n]);
                                if v.len() > OUTPUT_LIMIT {
                                    let drop_n = v.len() - OUTPUT_LIMIT;
                                    v.drain(..drop_n);
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
            let child = Arc::new(Mutex::new(child));
            let done = exited.clone();
            let waited = child.clone();
            thread::spawn(move || {
                let code = waited
                    .lock()
                    .ok()
                    .and_then(|mut c| c.wait().ok())
                    .map(|s| s.exit_code() as i32);
                if let Ok(mut d) = done.lock() {
                    *d = code;
                }
            });
            Ok(Terminal {
                task_id: task,
                output,
                exited,
                writer: Arc::new(Mutex::new(writer)),
                master: Arc::new(Mutex::new(pair.master)),
                child,
            })
        })
        .await
        .map_err(|_| HostError::new("terminal_failed", "终端启动中断"))??;
        let snapshot = snapshot(&id, &term);
        self.inner
            .lock()
            .map_err(|_| HostError::new("terminal_failed", "终端状态不可用"))?
            .insert(id, term);
        Ok(snapshot)
    }
    pub async fn snapshot(&self, task_id: &str, id: &str) -> Result<TerminalSnapshot> {
        let g = self
            .inner
            .lock()
            .map_err(|_| HostError::new("terminal_failed", "终端状态不可用"))?;
        let t = g
            .get(id)
            .ok_or_else(|| HostError::new("terminal_missing", "终端不存在或已关闭"))?;
        if t.task_id != task_id {
            return Err(HostError::new("terminal_denied", "终端不属于当前任务"));
        }
        Ok(snapshot(id, t))
    }
    pub async fn write(&self, task_id: &str, id: &str, input: String) -> Result<()> {
        let g = self
            .inner
            .lock()
            .map_err(|_| HostError::new("terminal_failed", "终端状态不可用"))?;
        let t = g
            .get(id)
            .ok_or_else(|| HostError::new("terminal_missing", "终端不存在或已关闭"))?;
        if t.task_id != task_id {
            return Err(HostError::new("terminal_denied", "终端不属于当前任务"));
        }
        if input.len() > 64 * 1024 {
            return Err(HostError::new(
                "terminal_input_limit",
                "输入超过 64 KiB 限制",
            ));
        }
        t.writer
            .lock()
            .map_err(|_| HostError::new("terminal_failed", "终端不可写"))?
            .write_all(input.as_bytes())
            .map_err(|e| HostError::new("terminal_write_failed", e.to_string()))
    }
    pub async fn resize(&self, task_id: &str, id: &str, cols: u16, rows: u16) -> Result<()> {
        let g = self
            .inner
            .lock()
            .map_err(|_| HostError::new("terminal_failed", "终端状态不可用"))?;
        let t = g
            .get(id)
            .ok_or_else(|| HostError::new("terminal_missing", "终端不存在或已关闭"))?;
        if t.task_id != task_id {
            return Err(HostError::new("terminal_denied", "终端不属于当前任务"));
        }
        if cols == 0 || rows == 0 || cols > 500 || rows > 200 {
            return Err(HostError::new("terminal_size_invalid", "终端尺寸无效"));
        }
        t.master
            .lock()
            .map_err(|_| HostError::new("terminal_failed", "终端不可调整大小"))?
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| HostError::new("terminal_resize_failed", e.to_string()))
    }
    pub async fn close(&self, task_id: &str, id: &str) -> Result<()> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| HostError::new("terminal_failed", "终端状态不可用"))?;
        let t = g
            .get(id)
            .ok_or_else(|| HostError::new("terminal_missing", "终端不存在或已关闭"))?;
        if t.task_id != task_id {
            return Err(HostError::new("terminal_denied", "终端不属于当前任务"));
        }
        let t = g.remove(id).expect("terminal checked");
        let _ = t
            .child
            .lock()
            .map_err(|_| HostError::new("terminal_failed", "终端不可关闭"))?
            .kill();
        Ok(())
    }
    pub async fn close_all(&self) {
        if let Ok(mut all) = self.inner.lock() {
            for (_, terminal) in all.drain() {
                if let Ok(mut child) = terminal.child.lock() {
                    let _ = child.kill();
                }
            }
        }
    }
    pub async fn close_task(&self, task_id: &str) {
        let ids = self
            .inner
            .lock()
            .ok()
            .map(|all| {
                all.iter()
                    .filter(|(_, t)| t.task_id == task_id)
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for id in ids {
            let _ = self.close(task_id, &id).await;
        }
    }
}
fn snapshot(id: &str, t: &Terminal) -> TerminalSnapshot {
    let output = t
        .output
        .lock()
        .map(|v| String::from_utf8_lossy(&v).into_owned())
        .unwrap_or_default();
    let code = t.exited.lock().ok().and_then(|v| *v);
    TerminalSnapshot {
        id: id.into(),
        task_id: t.task_id.clone(),
        output,
        exited: code.is_some(),
        exit_code: code,
    }
}

impl Workbench {
    pub async fn terminal_create(
        &self,
        task_id: &str,
        root_index: usize,
    ) -> Result<TerminalSnapshot> {
        let roots = self.store.validate_task_roots(task_id).await?;
        let root = roots
            .get(root_index)
            .ok_or_else(|| HostError::new("invalid_file_root", "目录不属于当前任务"))?
            .clone();
        self.terminals.create(task_id, root).await
    }
    pub async fn terminal_snapshot(&self, task_id: &str, id: &str) -> Result<TerminalSnapshot> {
        self.terminals.snapshot(task_id, id).await
    }
    pub async fn terminal_write(&self, task_id: &str, id: &str, input: String) -> Result<()> {
        self.terminals.write(task_id, id, input).await
    }
    pub async fn terminal_resize(
        &self,
        task_id: &str,
        id: &str,
        cols: u16,
        rows: u16,
    ) -> Result<()> {
        self.terminals.resize(task_id, id, cols, rows).await
    }
    pub async fn terminal_close(&self, task_id: &str, id: &str) -> Result<()> {
        self.terminals.close(task_id, id).await
    }
}
