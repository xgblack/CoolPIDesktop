use crate::{HostError, Workbench, files, runtime::Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

const OUTPUT_LIMIT: usize = 512 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSnapshot {
    pub id: String,
    pub task_id: String,
    pub output: String,
    pub start: u64,
    pub end: u64,
    pub exited: bool,
    pub exit_code: Option<u32>,
    pub error: Option<HostError>,
}
#[derive(Default)]
struct Output {
    bytes: VecDeque<u8>,
    end: u64,
    exited: bool,
    code: Option<u32>,
    error: Option<HostError>,
}
struct Terminal {
    task: String,
    root: PathBuf,
    output: Arc<Mutex<Output>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    pid: i32,
    closed: AtomicBool,
}
fn failure(e: impl std::fmt::Display) -> HostError {
    HostError::new("terminal_failed", e)
}
fn lock<T>(m: &Mutex<T>) -> Result<std::sync::MutexGuard<'_, T>> {
    m.lock().map_err(|_| failure("终端状态锁不可用"))
}
impl Terminal {
    fn terminate(&self) -> Result<()> {
        if self.closed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let foreground = lock(&self.master)?.process_group_leader();
        // PTY child is a session leader. Kill the shell group and current foreground job.
        for group in [foreground, Some(self.pid)].into_iter().flatten() {
            if group > 1 && unsafe { libc::kill(-group, libc::SIGKILL) } < 0 {
                let e = std::io::Error::last_os_error();
                if e.raw_os_error() != Some(libc::ESRCH) {
                    self.closed.store(false, Ordering::SeqCst);
                    return Err(failure(e));
                }
            }
        }
        if !lock(&self.output)?.exited {
            if let Err(e) = lock(&self.killer)?.kill() {
                if e.raw_os_error() != Some(libc::ESRCH) {
                    return Err(failure(e));
                }
            }
        }
        Ok(())
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        if let Err(e) = self.terminate() {
            eprintln!("PTY cleanup: {e}")
        }
    }
}
#[derive(Clone, Default)]
pub struct TerminalService {
    inner: Arc<Mutex<HashMap<String, Arc<Terminal>>>>,
}
impl TerminalService {
    fn owned(&self, task: &str, id: &str) -> Result<Arc<Terminal>> {
        let all = lock(&self.inner)?;
        let t = all
            .get(id)
            .ok_or_else(|| HostError::new("terminal_missing", "终端不存在或已关闭"))?;
        if t.task != task {
            return Err(HostError::new("terminal_denied", "终端不属于当前任务"));
        }
        Ok(t.clone())
    }
    pub async fn create(&self, task: &str, root: PathBuf) -> Result<TerminalSnapshot> {
        {
            let all = lock(&self.inner)?;
            if all.len() >= 32 || all.values().filter(|t| t.task == task).count() >= 4 {
                return Err(HostError::new("terminal_limit", "终端数量达到上限"));
            }
        }
        let task = task.to_owned();
        let term = tokio::task::spawn_blocking(move || -> Result<Terminal> {
            files::absolute_dir(&root)?;
            let pair = NativePtySystem::default()
                .openpty(PtySize {
                    rows: 24,
                    cols: 80,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(failure)?;
            let mut reader = pair.master.try_clone_reader().map_err(failure)?;
            let writer = pair.master.take_writer().map_err(failure)?;
            let fd = pair
                .master
                .as_raw_fd()
                .ok_or_else(|| failure("PTY 不支持文件描述符"))?;
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                return Err(failure(std::io::Error::last_os_error()));
            }
            let mut command = CommandBuilder::new("/bin/zsh");
            command.args(["-f", "-i"]);
            command.cwd(&root);
            command.env("TERM", "xterm-256color");
            let mut child = pair.slave.spawn_command(command).map_err(failure)?;
            let pid = child
                .process_id()
                .ok_or_else(|| failure("PTY 没有进程 ID"))? as i32;
            let killer = child.clone_killer();
            drop(pair.slave);
            let output = Arc::new(Mutex::new(Output::default()));
            let out = output.clone();
            thread::spawn(move || {
                let mut buf = [0; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            let Ok(mut o) = out.lock() else { break };
                            o.end += n as u64;
                            o.bytes.extend(&buf[..n]);
                            while o.bytes.len() > OUTPUT_LIMIT {
                                o.bytes.pop_front();
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10))
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(e) => {
                            if e.raw_os_error() != Some(libc::EIO) {
                                if let Ok(mut o) = out.lock() {
                                    o.error = Some(failure(e));
                                }
                            }
                            break;
                        }
                    }
                }
            });
            let out = output.clone();
            thread::spawn(move || {
                let result = child.wait();
                if let Ok(mut o) = out.lock() {
                    o.exited = true;
                    match result {
                        Ok(s) => o.code = Some(s.exit_code()),
                        Err(e) => o.error = Some(failure(e)),
                    }
                }
            });
            Ok(Terminal {
                task,
                root,
                output,
                master: Mutex::new(pair.master),
                writer: Mutex::new(writer),
                killer: Mutex::new(killer),
                pid,
                closed: AtomicBool::new(false),
            })
        })
        .await
        .map_err(failure)??;
        let id = uuid::Uuid::new_v4().to_string();
        let task = term.task.clone();
        lock(&self.inner)?.insert(id.clone(), Arc::new(term));
        self.snapshot(&task, &id, 0).await
    }
    pub async fn snapshot(&self, task: &str, id: &str, after: u64) -> Result<TerminalSnapshot> {
        let t = self.owned(task, id)?;
        if files::absolute_dir(&t.root).is_err() {
            self.close(task, id).await?;
            return Err(HostError::new(
                "worktree_missing",
                "终端目录已不存在，终端已关闭",
            ));
        }
        let o = lock(&t.output)?;
        let base = o.end - o.bytes.len() as u64;
        let start = after.max(base).min(o.end);
        let bytes: Vec<u8> = o
            .bytes
            .iter()
            .skip((start - base) as usize)
            .copied()
            .collect();
        Ok(TerminalSnapshot {
            id: id.into(),
            task_id: task.into(),
            output: STANDARD.encode(bytes),
            start,
            end: o.end,
            exited: o.exited,
            exit_code: o.code,
            error: o.error.clone(),
        })
    }
    pub async fn write(&self, task: &str, id: &str, input: String) -> Result<()> {
        if input.len() > 64 * 1024 {
            return Err(HostError::new("terminal_input_limit", "输入超过 64 KiB"));
        }
        let t = self.owned(task, id)?;
        tokio::task::spawn_blocking(move || {
            if t.closed.load(Ordering::SeqCst) || lock(&t.output)?.exited {
                return Err(HostError::new("terminal_exited", "终端已退出"));
            }
            let mut writer = lock(&t.writer)?;
            let mut bytes = input.as_bytes();
            let until = std::time::Instant::now() + Duration::from_secs(2);
            while !bytes.is_empty() {
                match writer.write(bytes) {
                    Ok(0) => return Err(failure("终端不可写")),
                    Ok(n) => bytes = &bytes[n..],
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < until =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => return Err(HostError::new("terminal_write_failed", e)),
                }
            }
            Ok(())
        })
        .await
        .map_err(failure)?
    }
    pub async fn resize(&self, task: &str, id: &str, cols: u16, rows: u16) -> Result<()> {
        if cols < 2 || rows < 1 || cols > 500 || rows > 200 {
            return Err(HostError::new("terminal_size_invalid", "终端尺寸无效"));
        }
        let t = self.owned(task, id)?;
        lock(&t.master)?
            .resize(PtySize {
                cols,
                rows,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(failure)
    }
    pub async fn close(&self, task: &str, id: &str) -> Result<()> {
        let t = self.owned(task, id)?;
        t.terminate()?;
        let until = tokio::time::Instant::now() + Duration::from_secs(3);
        while !lock(&t.output)?.exited {
            if tokio::time::Instant::now() > until {
                return Err(failure("终端退出超时，保留资源以便重试"));
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        lock(&self.inner)?.remove(id);
        Ok(())
    }
    pub async fn close_task(&self, task: &str) -> Result<()> {
        let ids: Vec<_> = lock(&self.inner)?
            .iter()
            .filter(|(_, t)| t.task == task)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            self.close(task, &id).await?
        }
        Ok(())
    }
    pub async fn close_all(&self) -> Result<()> {
        let tasks: Vec<_> = lock(&self.inner)?
            .values()
            .map(|t| t.task.clone())
            .collect();
        let mut error = None;
        for task in tasks {
            if let Err(e) = self.close_task(&task).await {
                error = Some(e)
            }
        }
        error.map_or(Ok(()), Err)
    }
}
impl Workbench {
    pub async fn terminal_create(&self, task: &str, index: usize) -> Result<TerminalSnapshot> {
        let _guard = self.gate.lock().await;
        if self.store.task(task).await?.archived {
            return Err(HostError::new("task_archived", "已归档任务不能启动终端"));
        }
        let roots = self.store.validate_task_roots(task).await?;
        let root = roots
            .get(index)
            .ok_or_else(|| HostError::new("invalid_file_root", "目录不属于当前任务"))?
            .clone();
        self.terminals.create(task, root).await
    }
    async fn terminal_validate(&self, task: &str) -> Result<()> {
        if let Err(e) = self.store.validate_task_roots(task).await {
            self.terminals.close_task(task).await?;
            return Err(e);
        }
        Ok(())
    }
    pub async fn terminal_snapshot(
        &self,
        task: &str,
        id: &str,
        after: u64,
    ) -> Result<TerminalSnapshot> {
        self.terminal_validate(task).await?;
        self.terminals.snapshot(task, id, after).await
    }
    pub async fn terminal_write(&self, task: &str, id: &str, input: String) -> Result<()> {
        self.terminal_validate(task).await?;
        self.terminals.write(task, id, input).await
    }
    pub async fn terminal_resize(&self, task: &str, id: &str, cols: u16, rows: u16) -> Result<()> {
        self.terminal_validate(task).await?;
        self.terminals.resize(task, id, cols, rows).await
    }
    pub async fn terminal_close(&self, task: &str, id: &str) -> Result<()> {
        self.terminals.close(task, id).await
    }
}
