use host_core::store::{Project, TaskRecord};
use host_core::{HostError, ObserverInfo, RuntimeInfo, TaskSnapshot, Workbench};
use serde_json::Value;
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;
type Result<T> = std::result::Result<T, HostError>;

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
) -> Result<TaskRecord> {
    w.store.create_task(&project_id, &title).await
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
) -> Result<TaskSnapshot> {
    w.request(&task_id, "prompt", serde_json::json!({"message":message}))
        .await
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
async fn task_git_status(w: State<'_, Workbench>, task_id: String, root_index: usize) -> Result<host_core::git::GitStatus> {
    w.git_status(&task_id, root_index).await
}
#[tauri::command]
async fn task_git_diff(w: State<'_, Workbench>, task_id: String, root_index: usize, path: String, staged: bool, untracked: bool) -> Result<host_core::git::GitDiff> {
    w.git_diff(&task_id, root_index, &path, staged, untracked).await
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

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            let workbench = tauri::async_runtime::block_on(Workbench::open(directory))?;
            app.manage(workbench);
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
            abort_task,
            respond_ui,
            task_history,
            task_usage,
            task_git_status,
            task_git_diff,
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
