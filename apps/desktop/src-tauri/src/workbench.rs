use host_core::store::{Project, TaskRecord};
use host_core::{HostError, ObserverInfo, RuntimeInfo, TaskSnapshot, Workbench};
use serde_json::Value;
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;
type Result<T> = std::result::Result<T, HostError>;

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
