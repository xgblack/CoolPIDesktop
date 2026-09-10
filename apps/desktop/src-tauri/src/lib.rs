use host_core::{HostError, ObserverInfo, RuntimeInfo, TaskManager, TaskSnapshot};
use std::path::PathBuf;
use tokio::sync::OnceCell;
static MANAGER: OnceCell<TaskManager> = OnceCell::const_new();
async fn manager() -> &'static TaskManager {
    MANAGER
        .get_or_init(|| async {
            TaskManager::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        })
        .await
}
#[tauri::command]
fn health() -> &'static str {
    "ok"
}
#[tauri::command]
async fn runtime_status(explicit: Option<String>) -> Result<RuntimeInfo, HostError> {
    Ok(manager().await.runtime_status(explicit).await)
}
#[tauri::command]
async fn detect_omp(explicit: Option<String>) -> Result<RuntimeInfo, HostError> {
    runtime_status(explicit).await
}
#[tauri::command]
async fn start_task(task_id: String, explicit: Option<String>) -> Result<TaskSnapshot, HostError> {
    manager().await.start(task_id, explicit).await
}
#[tauri::command]
async fn stop_task(task_id: String) -> Result<TaskSnapshot, HostError> {
    manager().await.stop(&task_id).await?;
    task_snapshot(task_id).await
}
#[tauri::command]
async fn prompt_task(task_id: String, message: String) -> Result<TaskSnapshot, HostError> {
    manager()
        .await
        .request(&task_id, "prompt", serde_json::json!({"message":message}))
        .await?;
    task_snapshot(task_id).await
}
#[tauri::command]
async fn abort_task(task_id: String) -> Result<TaskSnapshot, HostError> {
    manager()
        .await
        .request(&task_id, "abort", serde_json::json!({}))
        .await?;
    task_snapshot(task_id).await
}
#[tauri::command]
async fn respond_ui(
    task_id: String,
    request_id: String,
    value: Option<String>,
    confirmed: Option<bool>,
    cancelled: bool,
) -> Result<TaskSnapshot, HostError> {
    manager().await.request(&task_id,"extension_ui_response",serde_json::json!({"id":request_id,"value":value,"confirmed":confirmed,"cancelled":cancelled})).await?;
    task_snapshot(task_id).await
}
#[tauri::command]
async fn restart_task(task_id: String) -> Result<TaskSnapshot, HostError> {
    manager().await.restart(&task_id).await
}
#[tauri::command]
async fn list_tasks() -> Result<Vec<TaskSnapshot>, HostError> {
    Ok(manager().await.snapshots().await)
}
#[tauri::command]
async fn task_snapshot(task_id: String) -> Result<TaskSnapshot, HostError> {
    manager().await.snapshot(&task_id).await
}
#[tauri::command]
async fn observer_info() -> Result<ObserverInfo, HostError> {
    manager().await.observer_info().await
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            health,
            runtime_status,
            detect_omp,
            start_task,
            stop_task,
            prompt_task,
            abort_task,
            respond_ui,
            restart_task,
            list_tasks,
            task_snapshot,
            observer_info
        ])
        .build(tauri::generate_context!())
        .expect("error while building Tauri application");
    app.run(|_, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            if let Some(manager) = MANAGER.get() {
                if let Err(error) = tauri::async_runtime::block_on(manager.shutdown()) {
                    eprintln!("OMP cleanup failed: {error}");
                }
            }
        }
    });
}
