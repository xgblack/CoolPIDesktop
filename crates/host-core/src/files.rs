//! Task-scoped, bounded reads. Unix directory handles prevent symlink replacement
//! between validation and access; the current desktop target is macOS.
use crate::{HostError, Workbench, runtime::Result};
use serde::{Deserialize, Serialize};
use std::{
    ffi::{CStr, CString, OsString},
    fs::File,
    io::{self, Read},
    os::{
        fd::{AsRawFd, FromRawFd, IntoRawFd},
        unix::ffi::{OsStrExt, OsStringExt},
    },
    path::{Component, Path},
};

pub const PREVIEW_LIMIT: u64 = 256 * 1024;
pub const DIRECTORY_LIMIT: usize = 1000;
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub size: Option<u64>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryPage {
    pub entries: Vec<FileEntry>,
    pub truncated: bool,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePreview {
    pub name: String,
    pub size: u64,
    pub state: String,
    pub text: Option<String>,
}

pub(crate) fn io_error(e: io::Error) -> HostError {
    let (code, message) = match e.raw_os_error() {
        Some(libc::ELOOP | libc::ENOTDIR) => ("file_path_denied", "路径不是普通目录或包含符号链接"),
        Some(libc::ENOENT) => ("file_missing", "文件或目录已不存在，请刷新列表"),
        Some(libc::EACCES | libc::EPERM) => ("file_permission_denied", "没有权限读取此文件或目录"),
        _ => ("file_io_error", "无法访问文件，请检查目录和磁盘状态"),
    };
    HostError::new(code, message)
}
pub(crate) fn components(path: &str) -> Result<Vec<&str>> {
    if path.len() > 4096
        || path.contains(['\0', '\\'])
        || path.starts_with('/')
        || path.split('/').any(|s| s == ".." || s == ".")
    {
        return Err(HostError::new(
            "file_path_denied",
            "只允许任务目录内的相对路径",
        ));
    }
    Ok(path.split('/').filter(|s| !s.is_empty()).collect())
}
pub(crate) fn open_at(parent: &File, name: &std::ffi::OsStr, flags: i32) -> Result<File> {
    let name = CString::new(name.as_bytes())
        .map_err(|_| HostError::new("file_path_denied", "无效路径"))?;
    // O_NONBLOCK prevents a substituted FIFO from hanging before fstat rejects it.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
            0o600,
        )
    };
    if fd < 0 {
        return Err(io_error(io::Error::last_os_error()));
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
pub(crate) fn absolute_dir(path: &Path) -> Result<File> {
    if !path.is_absolute() {
        return Err(HostError::new("file_path_denied", "执行目录必须是绝对路径"));
    }
    let mut dir = File::open("/").map_err(io_error)?;
    for part in path.components() {
        match part {
            Component::RootDir => {}
            Component::Normal(name) => {
                dir = open_at(&dir, name, libc::O_RDONLY | libc::O_DIRECTORY)?
            }
            _ => return Err(HostError::new("file_path_denied", "无效目录")),
        }
    }
    Ok(dir)
}
pub(crate) fn relative_open(root: &File, path: &str, directory: bool) -> Result<File> {
    let parts = components(path)?;
    let mut handle = root.try_clone().map_err(io_error)?;
    for (i, part) in parts.iter().enumerate() {
        let flags = libc::O_RDONLY
            | if directory || i + 1 < parts.len() {
                libc::O_DIRECTORY
            } else {
                0
            };
        handle = open_at(&handle, std::ffi::OsStr::new(part), flags)?;
    }
    let meta = handle.metadata().map_err(io_error)?;
    if (directory && !meta.is_dir()) || (!directory && !meta.is_file()) {
        return Err(HostError::new(
            "file_type_unsupported",
            "仅支持普通文件和目录，不能读取特殊文件",
        ));
    }
    Ok(handle)
}
pub(crate) fn read_bounded(file: &File, limit: u64) -> Result<Vec<u8>> {
    if !file.metadata().map_err(io_error)?.is_file() {
        return Err(HostError::new("file_type_unsupported", "只能读取普通文件"));
    }
    if file.metadata().map_err(io_error)?.len() > limit {
        return Err(HostError::new("file_too_large", "文件超过大小限制"));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 > limit {
        return Err(HostError::new("file_too_large", "文件超过大小限制"));
    }
    Ok(bytes)
}
pub(crate) fn text_content(bytes: &[u8]) -> Option<&str> {
    let text = std::str::from_utf8(bytes).ok()?;
    if text
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        None
    } else {
        Some(text)
    }
}
pub(crate) fn preview(file: &File, name: String) -> Result<FilePreview> {
    let size = file.metadata().map_err(io_error)?.len();
    match read_bounded(file, PREVIEW_LIMIT) {
        Err(e) if e.code == "file_too_large" => Ok(FilePreview {
            name,
            size,
            state: "too_large".into(),
            text: None,
        }),
        Err(e) => Err(e),
        Ok(bytes) => {
            let text = text_content(&bytes).map(str::to_owned);
            Ok(FilePreview {
                name,
                size: bytes.len() as u64,
                state: if text.is_some() { "text" } else { "binary" }.into(),
                text,
            })
        }
    }
}
struct Directory(*mut libc::DIR);
impl Drop for Directory {
    fn drop(&mut self) {
        unsafe {
            libc::closedir(self.0);
        }
    }
}
fn list(root: File, path: &str) -> Result<DirectoryPage> {
    let directory = relative_open(&root, path, true)?;
    let fd = directory.try_clone().map_err(io_error)?.into_raw_fd();
    let ptr = unsafe { libc::fdopendir(fd) };
    if ptr.is_null() {
        let error = io::Error::last_os_error();
        unsafe {
            libc::close(fd);
        }
        return Err(io_error(error));
    }
    let dir = Directory(ptr);
    let mut entries = Vec::new();
    let mut truncated = false;
    loop {
        // readdir returns NULL for either EOF or error; errno distinguishes them.
        #[cfg(target_os = "macos")]
        unsafe {
            *libc::__error() = 0;
        }
        #[cfg(target_os = "linux")]
        unsafe {
            *libc::__errno_location() = 0;
        }
        let entry = unsafe { libc::readdir(dir.0) };
        if entry.is_null() {
            let e = io::Error::last_os_error();
            if e.raw_os_error() != Some(0) {
                return Err(io_error(e));
            }
            break;
        }
        let bytes = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
        if bytes == b"." || bytes == b".." {
            continue;
        }
        if entries.len() == DIRECTORY_LIMIT {
            truncated = true;
            break;
        }
        let os = OsString::from_vec(bytes.to_vec());
        let name = os.to_string_lossy().into_owned();
        let valid = os.to_str().is_some() && components(&name).is_ok();
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        let c = CString::new(bytes).unwrap();
        let ok = unsafe {
            libc::fstatat(
                directory.as_raw_fd(),
                c.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        let (kind, size) = if ok < 0 {
            ("unavailable", None)
        } else {
            let stat = unsafe { stat.assume_init() };
            let kind = if !valid {
                "unsupported"
            } else {
                match stat.st_mode & libc::S_IFMT {
                    libc::S_IFDIR => "directory",
                    libc::S_IFREG => "file",
                    libc::S_IFLNK => "symlink",
                    _ => "special",
                }
            };
            (kind, Some(stat.st_size.max(0) as u64))
        };
        entries.push(FileEntry {
            path: if path.is_empty() {
                name.clone()
            } else {
                format!("{path}/{name}")
            },
            name,
            kind: kind.into(),
            size,
        });
    }
    entries.sort_by(|a, b| (a.kind != "directory", &a.name).cmp(&(b.kind != "directory", &b.name)));
    Ok(DirectoryPage { entries, truncated })
}
impl Workbench {
    pub async fn search_project_files(&self, id: &str, index: usize, query: &str) -> Result<DirectoryPage> {
        if query.len()>256 {return Err(HostError::new("invalid_query","文件查询过长"));}
        let project=self.store.projects().await?.into_iter().find(|p|p.id==id)
            .ok_or_else(||HostError::new("project_missing","项目不存在"))?;
        if !project.trusted {return Err(HostError::new("workspace_untrusted","项目目录未经信任"));}
        let path=project.roots.get(index).ok_or_else(||HostError::new("invalid_file_root","目录不属于当前项目"))?;
        let root=absolute_dir(Path::new(path))?;
        let query=query.to_lowercase();
        tokio::task::spawn_blocking(move||search(root,&query)).await.map_err(|_|HostError::new("file_io_error","文件搜索中断"))?
    }
    pub async fn search_files(&self, id: &str, index: usize, query: &str) -> Result<DirectoryPage> {
        if query.len() > 256 { return Err(HostError::new("invalid_query", "文件查询过长")); }
        let root = self.file_root(id, index).await?;
        let query = query.to_lowercase();
        tokio::task::spawn_blocking(move || search(root, &query)).await
            .map_err(|_| HostError::new("file_io_error", "文件搜索中断"))?
    }
    pub(crate) async fn file_root(&self, id: &str, index: usize) -> Result<File> {
        let roots = self.store.validate_task_roots(id).await?;
        let path = roots
            .get(index)
            .ok_or_else(|| HostError::new("invalid_file_root", "目录不属于当前任务"))?;
        absolute_dir(path)
    }
    pub async fn list_files(&self, id: &str, index: usize, path: &str) -> Result<DirectoryPage> {
        let _guard = self.gate.lock().await;
        components(path)?;
        let root = self.file_root(id, index).await?;
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || list(root, &path))
            .await
            .map_err(|_| HostError::new("file_io_error", "目录读取中断"))?
    }
    pub async fn preview_file(&self, id: &str, index: usize, path: &str) -> Result<FilePreview> {
        let _guard = self.gate.lock().await;
        components(path)?;
        let root = self.file_root(id, index).await?;
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || preview(&relative_open(&root, &path, false)?, path))
            .await
            .map_err(|_| HostError::new("file_io_error", "文件读取中断"))?
    }
}

fn search(root: File, query: &str) -> Result<DirectoryPage> {
    let mut pending = vec![String::new()];
    let mut entries = Vec::new();
    let mut visited = 0;
    let mut truncated = false;
    while let Some(path) = pending.pop() {
        visited += 1;
        if visited > 2000 || entries.len() >= 100 { truncated = true; break; }
        let page = match list(root.try_clone().map_err(io_error)?, &path) {
            Ok(page) => page,
            Err(e) if !path.is_empty() && matches!(e.code.as_str(), "file_permission_denied" | "file_missing" | "file_path_denied") => { truncated = true; continue; },
            Err(e) => return Err(e),
        };
        truncated |= page.truncated;
        for entry in page.entries {
            if entry.kind == "directory" {
                if !matches!(entry.name.as_str(), ".git" | "node_modules" | "target" | "dist" | ".next") && path.matches('/').count() < 32 { pending.push(entry.path); }
            } else if entry.kind == "file" && entry.path.to_lowercase().contains(query) {
                entries.push(entry);
                if entries.len() >= 100 { truncated = true; break; }
            }
        }
    }
    entries.sort_by_key(|e| (!e.name.to_lowercase().starts_with(query), e.path.len(), e.path.clone()));
    Ok(DirectoryPage { entries, truncated })
}
