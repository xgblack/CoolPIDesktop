//! Cooperative per-task writer ownership shared by desktop and terminal OMP.
//! The descriptor remains locked through exec, so helper death cannot free a live writer.
use crate::{HostError, runtime::Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    path::Path,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Owner {
    pub owner: String,
    pub pid: Option<u32>,
    pub started_at: u64,
}
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub struct WriterLease {
    file: File,
    metadata: Owner,
}
fn file(data: &Path, id: &str) -> Result<File> {
    if id.is_empty()
        || id.len() > 80
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(HostError::new("invalid_task", "Invalid task ID"));
    }
    let dir = data.join("runtime-locks");
    std::fs::create_dir_all(&dir).map_err(|e| HostError::new("storage_error", e))?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(dir.join(format!("{id}.lock")))
        .map_err(|e| HostError::new("storage_error", e))?;
    if !file
        .metadata()
        .map_err(|e| HostError::new("storage_error", e))?
        .is_file()
    {
        return Err(HostError::new(
            "storage_error",
            "Writer lock must be a regular file",
        ));
    }
    Ok(file)
}
impl WriterLease {
    pub fn acquire(data: &Path, id: &str, owner: &str) -> Result<Self> {
        let file = file(data, id)?;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            let e = std::io::Error::last_os_error();
            return Err(HostError::new(
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    "session_owned"
                } else {
                    "storage_error"
                },
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    "此会话已有写入进程，等待终端退出后再继续".to_string()
                } else {
                    e.to_string()
                },
            ));
        }
        let mut lease = Self {
            file,
            metadata: Owner {
                owner: owner.into(),
                pid: None,
                started_at: now(),
            },
        };
        lease.save()?;
        Ok(lease)
    }
    fn save(&mut self) -> Result<()> {
        let bytes =
            serde_json::to_vec(&self.metadata).map_err(|e| HostError::new("storage_error", e))?;
        self.file
            .seek(SeekFrom::Start(0))
            .and_then(|_| self.file.write_all(&bytes))
            .and_then(|_| self.file.set_len(bytes.len() as u64))
            .map_err(|e| HostError::new("storage_error", e))
    }
    pub fn set_pid(&mut self, pid: u32) -> Result<()> {
        self.metadata.pid = Some(pid);
        self.save()
    }
    pub fn make_inheritable(&self) -> Result<()> {
        if unsafe { libc::fcntl(self.file.as_raw_fd(), libc::F_SETFD, 0) } < 0 {
            return Err(HostError::new(
                "process_launch",
                std::io::Error::last_os_error(),
            ));
        }
        Ok(())
    }
}
pub fn owner(data: &Path, id: &str) -> Result<Option<Owner>> {
    let mut f = file(data, id)?;
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(None);
    }
    let e = std::io::Error::last_os_error();
    if e.kind() != std::io::ErrorKind::WouldBlock {
        return Err(HostError::new("storage_error", e));
    }
    let mut value = String::new();
    (&mut f)
        .take(4096)
        .read_to_string(&mut value)
        .map_err(|e| HostError::new("storage_error", e))?;
    serde_json::from_str(&value)
        .map(Some)
        .map_err(|_| HostError::new("ownership_unknown", "无法读取会话进程所有权，请刷新后重试"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ownership_is_exclusive_and_released_on_drop() {
        let dir = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        let mut l = WriterLease::acquire(&dir, "task", "terminal").unwrap();
        l.set_pid(42).unwrap();
        assert_eq!(owner(&dir, "task").unwrap().unwrap().pid, Some(42));
        assert!(WriterLease::acquire(&dir, "task", "desktop").is_err());
        drop(l);
        assert!(owner(&dir, "task").unwrap().is_none());
        assert!(WriterLease::acquire(&dir, "../escape", "desktop").is_err());
    }
}
