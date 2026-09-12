use host_core::store::{Project, TaskRecord, TaskRoot};
use host_core::{HostError, ObserverInfo, RuntimeInfo, TaskSnapshot, Workbench};
use serde_json::Value;
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;
type Result<T> = std::result::Result<T, HostError>;

#[derive(Default)]
struct SelectedFolders(std::sync::Mutex<std::collections::HashMap<String,std::path::PathBuf>>);

#[tauri::command]
async fn choose_project_folders(app: tauri::AppHandle, selections: State<'_, SelectedFolders>) -> Result<Vec<Value>> {
    let picked = tauri::async_runtime::spawn_blocking(move || app.dialog().file().blocking_pick_folders()).await.map_err(|e|HostError::new("dialog_failed",e))?;
    let mut output=Vec::new();
    let mut selected=selections.0.lock().map_err(|_|HostError::new("dialog_failed","目录选择状态不可用"))?;
    for file in picked.unwrap_or_default() {
        let path=file.into_path().map_err(|e|HostError::new("invalid_workspace",e))?;
        let token=uuid::Uuid::new_v4().to_string();
        output.push(serde_json::json!({"token":token,"path":path}));
        selected.insert(token,path);
    }
    Ok(output)
}

#[tauri::command]
async fn create_local_project(w:State<'_,Workbench>,selections:State<'_,SelectedFolders>,name:String,tokens:Vec<String>,trusted:bool)->Result<Project>{
    let paths={let selected=selections.0.lock().map_err(|_|HostError::new("dialog_failed","目录选择状态不可用"))?;
        tokens.iter().map(|t|selected.get(t).cloned().ok_or_else(||HostError::new("invalid_workspace","请重新选择项目文件夹"))).collect::<Result<Vec<_>>>()?};
    if !trusted { return Err(HostError::new("workspace_untrusted","请确认目录信任")); }
    let project=w.store.register_project(&name,paths,trusted).await?;
    if let Ok(mut selected)=selections.0.lock(){for t in tokens {selected.remove(&t);}}
    Ok(project)
}

#[derive(serde::Deserialize)]
#[serde(rename_all="camelCase")]
enum ProjectFolder { Existing(usize), Selected(String) }

#[tauri::command]
async fn edit_local_project(w:State<'_,Workbench>,selections:State<'_,SelectedFolders>,id:String,name:String,folders:Vec<ProjectFolder>,trusted:bool)->Result<Project>{
    let project=w.store.projects().await?.into_iter().find(|p|p.id==id).ok_or_else(||HostError::new("project_missing","项目不存在"))?;
    let paths={let selected=selections.0.lock().map_err(|_|HostError::new("dialog_failed","目录选择状态不可用"))?;
        folders.iter().map(|folder|match folder {
            ProjectFolder::Existing(index)=>project.roots.get(*index).cloned().ok_or_else(||HostError::new("invalid_workspace","项目目录已改变，请重新打开编辑窗口")),
            ProjectFolder::Selected(token)=>selected.get(token).cloned().ok_or_else(||HostError::new("invalid_workspace","请重新选择项目文件夹")),
        }).collect::<Result<Vec<_>>>()?};
    w.store.edit_project(&id,&name,paths,trusted).await
}

fn external_url(value: &str) -> Result<url::Url> {
    let url = url::Url::parse(value).map_err(|_| HostError::new("invalid_url", "链接格式无效"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(HostError::new(
            "invalid_url",
            "仅允许无内嵌凭据的 HTTP/HTTPS 链接",
        ));
    }
    Ok(url)
}

#[tauri::command]
async fn open_external_link(url: String) -> Result<()> {
    let url = external_url(&url)?;
    // macOS is the current release target. Pass one validated URL argument, never shell code.
    #[cfg(target_os = "macos")]
    {
        let status = tauri::async_runtime::spawn_blocking(move || {
            std::process::Command::new("/usr/bin/open")
                .arg(url.as_str())
                .status()
        })
        .await
        .map_err(|e| HostError::new("open_link_failed", e))?
        .map_err(|e| HostError::new("open_link_failed", e))?;
        if !status.success() {
            return Err(HostError::new("open_link_failed", "系统未能打开链接"));
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = url;
        Err(HostError::new(
            "unsupported_platform",
            "当前仅支持 macOS 外部链接",
        ))
    }
}

#[cfg(test)]
mod link_tests {
    use super::external_url;
    #[test]
    fn restrict_external_urls() {
        for value in [
            "javascript:alert(1)",
            "file:///tmp/a",
            "data:text/html,x",
            "https://user:secret@example.com",
            "https://",
            "--help",
            "ftp://example.com",
        ] {
            assert!(external_url(value).is_err(), "{value}");
        }
        for value in [
            "https://example.com/a?q=hello#world",
            "http://localhost:3000",
        ] {
            assert!(external_url(value).is_ok(), "{value}");
        }
    }
}

#[tauri::command]
async fn runtime_status(w: State<'_, Workbench>, explicit: Option<String>) -> Result<RuntimeInfo> {
    Ok(w.detect(explicit).await)
}
#[tauri::command]
async fn list_projects(w: State<'_, Workbench>) -> Result<Vec<Project>> {
    w.store.projects().await
}
#[tauri::command]
async fn register_project(
    app: tauri::AppHandle,
    w: State<'_, Workbench>,
    name: String,
    trusted: bool,
) -> Result<Option<Project>> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("选择项目目录（第一项为主目录）")
            .blocking_pick_folders()
    })
    .await
    .map_err(|e| HostError::new("dialog_failed", e))?;
    let Some(paths) = picked else { return Ok(None) };
    let paths = paths
        .into_iter()
        .map(|p| {
            p.into_path()
                .map_err(|e| HostError::new("invalid_workspace", e))
        })
        .collect::<Result<Vec<_>>>()?;
    w.store
        .register_project(&name, paths, trusted)
        .await
        .map(Some)
}
#[tauri::command]
async fn update_project(
    w: State<'_, Workbench>,
    id: String,
    name: String,
    archived: bool,
) -> Result<()> {
    w.store.update_project(&id, &name, archived).await
}
#[tauri::command]
async fn task_records(w: State<'_, Workbench>) -> Result<Vec<TaskRecord>> {
    w.records().await
}
#[tauri::command]
async fn create_task(
    w: State<'_, Workbench>,
    project_id: String,
    title: String,
    mode: Option<String>,
    model: Option<String>,
) -> Result<TaskRecord> {
    w.create_task_with_model(
        &project_id,
        &title,
        mode.as_deref().unwrap_or("shared"),
        model,
    )
    .await
}
#[tauri::command]
async fn update_task(
    w: State<'_, Workbench>,
    id: String,
    title: String,
    pinned: bool,
    archived: bool,
) -> Result<TaskRecord> {
    w.update_task(&id, &title, pinned, archived).await
}
#[tauri::command]
async fn relocate_task(
    app: tauri::AppHandle,
    w: State<'_, Workbench>,
    task_id: String,
    trusted: bool,
) -> Result<Option<TaskRecord>> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("重新定位任务目录（第一项为主目录）")
            .blocking_pick_folders()
    })
    .await
    .map_err(|e| HostError::new("dialog_failed", e))?;
    let Some(paths) = picked else { return Ok(None) };
    let paths = paths
        .into_iter()
        .map(|p| {
            p.into_path()
                .map_err(|e| HostError::new("invalid_workspace", e))
        })
        .collect::<Result<Vec<_>>>()?;
    w.relocate_task(&task_id, paths, trusted).await.map(Some)
}
#[tauri::command]
async fn continue_task(w: State<'_, Workbench>, task_id: String) -> Result<TaskSnapshot> {
    w.continue_task(&task_id).await
}
#[tauri::command]
async fn restart_task(w: State<'_, Workbench>, task_id: String) -> Result<TaskSnapshot> {
    w.restart(&task_id).await
}
#[tauri::command]
async fn stop_task(w: State<'_, Workbench>, task_id: String) -> Result<TaskSnapshot> {
    w.stop(&task_id).await
}
#[tauri::command]
async fn list_tasks(w: State<'_, Workbench>) -> Result<Vec<TaskSnapshot>> {
    Ok(w.runtime.snapshots().await)
}
#[tauri::command]
async fn task_snapshot(w: State<'_, Workbench>, task_id: String) -> Result<TaskSnapshot> {
    w.runtime.snapshot(&task_id).await
}
#[tauri::command]
async fn prompt_task(
    w: State<'_, Workbench>,
    task_id: String,
    message: String,
    attachment_ids: Option<Vec<String>>,
    references: Option<Vec<FileReference>>,
) -> Result<TaskSnapshot> {
    let mut message=message;
    let references=references.unwrap_or_default();
    if references.len()>32 {return Err(HostError::new("invalid_request","引用文件过多"));}
    for reference in references {
        w.preview_file(&task_id,reference.root_index,&reference.path).await?;
        let roots=w.store.validate_task_roots(&task_id).await?;
        let path=roots.get(reference.root_index).ok_or_else(||HostError::new("invalid_file_root","目录不属于当前任务"))?.join(&reference.path);
        message.push_str(&format!("\nReferenced workspace file: {}",serde_json::to_string(&path).map_err(|e|HostError::new("invalid_request",e))?));
    }
    w.prompt_with_attachments(&task_id, &message, &attachment_ids.unwrap_or_default())
        .await
}
#[derive(serde::Deserialize)]
#[serde(rename_all="camelCase")]
struct FileReference {root_index:usize,path:String}

#[tauri::command]
async fn search_task_files(w:State<'_,Workbench>,task_id:String,root_index:usize,query:String)->Result<host_core::files::DirectoryPage>{
    if let Some(project)=task_id.strip_prefix("project:"){return w.search_project_files(project,root_index,&query).await;}
    w.search_files(&task_id,root_index,&query).await
}
#[tauri::command]
async fn list_task_files(
    w: State<'_, Workbench>,
    task_id: String,
    root_index: usize,
    path: String,
) -> Result<host_core::files::DirectoryPage> {
    w.list_files(&task_id, root_index, &path).await
}
#[tauri::command]
async fn preview_task_file(
    w: State<'_, Workbench>,
    task_id: String,
    root_index: usize,
    path: String,
) -> Result<host_core::files::FilePreview> {
    w.preview_file(&task_id, root_index, &path).await
}
#[tauri::command]
async fn task_attachments(
    w: State<'_, Workbench>,
    task_id: String,
) -> Result<Vec<host_core::attachments::Attachment>> {
    w.attachments(&task_id).await
}
#[tauri::command]
async fn preview_task_attachment(
    w: State<'_, Workbench>,
    task_id: String,
    resource_id: String,
) -> Result<host_core::files::FilePreview> {
    w.preview_attachment(&task_id, &resource_id).await
}
#[tauri::command]
async fn import_task_attachment(
    app: tauri::AppHandle,
    w: State<'_, Workbench>,
    task_id: String,
) -> Result<Option<host_core::attachments::Attachment>> {
    w.store.validate_task_roots(&task_id).await?;
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("导入任务附件（最大 8 MiB）")
            .blocking_pick_file()
    })
    .await
    .map_err(|e| HostError::new("dialog_failed", e))?;
    let Some(path) = picked else { return Ok(None) };
    let path = path
        .into_path()
        .map_err(|_| HostError::new("attachment_import_failed", "无效文件路径"))?;
    w.import_attachment(&task_id, &path).await.map(Some)
}
#[tauri::command]
async fn terminal_create(
    w: State<'_, Workbench>,
    task_id: String,
    root_index: usize,
) -> Result<host_core::terminal::TerminalSnapshot> {
    w.terminal_create(&task_id, root_index).await
}
#[tauri::command]
async fn terminal_snapshot(
    w: State<'_, Workbench>,
    task_id: String,
    terminal_id: String,
    after: u64,
) -> Result<host_core::terminal::TerminalSnapshot> {
    w.terminal_snapshot(&task_id, &terminal_id, after).await
}
#[tauri::command]
async fn terminal_write(
    w: State<'_, Workbench>,
    task_id: String,
    terminal_id: String,
    input: String,
) -> Result<()> {
    w.terminal_write(&task_id, &terminal_id, input).await
}
#[tauri::command]
async fn terminal_resize(
    w: State<'_, Workbench>,
    task_id: String,
    terminal_id: String,
    cols: u16,
    rows: u16,
) -> Result<()> {
    w.terminal_resize(&task_id, &terminal_id, cols, rows).await
}
#[tauri::command]
async fn terminal_close(
    w: State<'_, Workbench>,
    task_id: String,
    terminal_id: String,
) -> Result<()> {
    w.terminal_close(&task_id, &terminal_id).await
}
#[tauri::command]
async fn abort_task(w: State<'_, Workbench>, task_id: String) -> Result<TaskSnapshot> {
    w.request(&task_id, "abort", serde_json::json!({})).await
}
#[tauri::command]
async fn respond_ui(
    w: State<'_, Workbench>,
    task_id: String,
    request_id: String,
    value: Option<String>,
    confirmed: Option<bool>,
    cancelled: bool,
) -> Result<TaskSnapshot> {
    w.request(&task_id,"extension_ui_response",serde_json::json!({"id":request_id,"value":value,"confirmed":confirmed,"cancelled":cancelled})).await
}
#[tauri::command]
async fn recover_session(
    w: State<'_, Workbench>,
    task_id: String,
    confirmed: bool,
) -> Result<String> {
    w.recover_session(&task_id, confirmed).await
}
#[tauri::command]
async fn task_history(
    w: State<'_, Workbench>,
    task_id: String,
    cursor: Option<String>,
) -> Result<Value> {
    w.history(&task_id, cursor).await
}
#[tauri::command]
async fn task_usage(w: State<'_, Workbench>, task_id: String) -> Result<TaskSnapshot> {
    w.refresh_usage(&task_id).await
}
#[tauri::command]
async fn task_git_status(
    w: State<'_, Workbench>,
    task_id: String,
    root_index: usize,
) -> Result<host_core::git::GitStatus> {
    w.git_status(&task_id, root_index).await
}
#[tauri::command]
async fn task_git_diff(
    w: State<'_, Workbench>,
    task_id: String,
    root_index: usize,
    path: String,
    staged: bool,
    untracked: bool,
) -> Result<host_core::git::GitDiff> {
    w.git_diff(&task_id, root_index, &path, staged, untracked)
        .await
}
#[tauri::command]
async fn task_roots(w: State<'_, Workbench>, task_id: String) -> Result<Vec<TaskRoot>> {
    w.task_roots(&task_id).await
}
#[tauri::command]
async fn isolate_task(w: State<'_, Workbench>, task_id: String) -> Result<Vec<TaskRoot>> {
    w.isolate_task(&task_id).await
}
#[tauri::command]
async fn cleanup_worktrees(w: State<'_, Workbench>, task_id: String) -> Result<Vec<TaskRoot>> {
    w.cleanup_worktrees(&task_id).await
}
#[tauri::command]
async fn recover_worktrees(w: State<'_, Workbench>, task_id: String) -> Result<Vec<TaskRoot>> {
    w.recover_worktrees(&task_id).await
}
#[tauri::command]
async fn select_task_model(
    w: State<'_, Workbench>,
    task_id: String,
    provider: String,
    model_id: String,
) -> Result<Value> {
    w.select_model(&task_id, &provider, &model_id).await
}
#[tauri::command]
async fn observer_info(w: State<'_, Workbench>) -> Result<ObserverInfo> {
    w.runtime.observer_info().await
}

#[tauri::command]
async fn model_config_apply(w: State<'_, Workbench>) -> Result<Value> {
    w.apply_model_config().await
}
async fn model_executable(w: &Workbench) -> Result<std::path::PathBuf> {
    let saved = w.store.setting("executable").await?;
    let path = host_core::resolve_executable(None, saved.as_deref().map(std::path::Path::new))?;
    host_core::probe(&path).await?;
    Ok(path)
}
#[tauri::command]
async fn model_config_load() -> Result<host_core::model_config::Config> {
    host_core::model_config::load(&host_core::model_config::config_path()?)
}
#[tauri::command]
async fn model_config_save(
    edit: host_core::model_config::Edit,
) -> Result<host_core::model_config::Config> {
    host_core::model_config::save(&host_core::model_config::config_path()?, &edit)
}
#[tauri::command]
async fn model_catalog(
    app: tauri::AppHandle,
    w: State<'_, Workbench>,
) -> Result<host_core::model_config::Catalog> {
    let path = model_executable(&w).await?;
    let version = host_core::probe(&path).await?.version.unwrap();
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|_| HostError::new("catalog_cache", "无法定位缓存目录"))?;
    host_core::model_config::catalog(&cache, &version).await
}
#[tauri::command]
async fn model_config_verify(
    app: tauri::AppHandle,
    w: State<'_, Workbench>,
    provider: Option<String>,
    model_id: Option<String>,
) -> Result<host_core::model_config::Verification> {
    let path = model_executable(&w).await?;
    let cwd = app
        .path()
        .app_cache_dir()
        .map_err(|_| HostError::new("config_verify", "无法定位验证目录"))?
        .join("model-probe");
    host_core::model_config::load(&host_core::model_config::config_path()?)?;
    if let (Some(p), Some(id)) = (provider, model_id) {
        host_core::model_config::connect(&path, &cwd, &p, &id).await
    } else {
        host_core::model_config::discover(&path, &cwd).await
    }
}

#[tauri::command]
async fn task_git_change(
    w: State<'_, Workbench>,
    task_id: String,
    root_index: usize,
    path: String,
    action: String,
    confirmed: bool,
) -> Result<()> {
    w.git_change(&task_id, root_index, &path, &action, confirmed)
        .await
}
#[tauri::command]
async fn task_git_commit_preview(
    w: State<'_, Workbench>,
    task_id: String,
    root_index: usize,
) -> Result<host_core::git_write::CommitPreview> {
    w.git_commit_preview(&task_id, root_index).await
}
#[tauri::command]
async fn task_git_commit(
    w: State<'_, Workbench>,
    task_id: String,
    root_index: usize,
    message: String,
    expected: host_core::git_write::CommitPreview,
) -> Result<String> {
    w.git_commit(&task_id, root_index, &message, expected).await
}
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            let workbench = tauri::async_runtime::block_on(Workbench::open(directory))?;
            app.manage(workbench);
            app.manage(SelectedFolders::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            model_config_apply,
            model_config_load,
            model_config_save,
            model_catalog,
            model_config_verify,
            open_external_link,
            runtime_status,
            list_projects,
            register_project,
            choose_project_folders,
            create_local_project,
            edit_local_project,
            update_project,
            task_records,
            create_task,
            update_task,
            relocate_task,
            continue_task,
            restart_task,
            stop_task,
            list_tasks,
            task_snapshot,
            prompt_task,
            list_task_files,
            search_task_files,
            preview_task_file,
            task_attachments,
            preview_task_attachment,
            import_task_attachment,
            terminal_create,
            terminal_snapshot,
            terminal_write,
            terminal_resize,
            terminal_close,
            abort_task,
            respond_ui,
            task_history,
            task_usage,
            task_git_status,
            task_git_diff,
            task_git_change,
            task_git_commit_preview,
            task_git_commit,
            task_roots,
            isolate_task,
            cleanup_worktrees,
            recover_worktrees,
            recover_session,
            select_task_model,
            observer_info
        ])
        .build(tauri::generate_context!())
        .expect("Cannot initialize desktop data store")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                if let Err(e) = tauri::async_runtime::block_on(app.state::<Workbench>().shutdown())
                {
                    eprintln!("OMP cleanup failed: {e}");
                }
            }
        });
}
