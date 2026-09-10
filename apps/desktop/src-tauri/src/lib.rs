use host_core::{TaskManager,RuntimeInfo};
use std::path::PathBuf;
use tokio::sync::OnceCell;
static MANAGER: OnceCell<TaskManager> = OnceCell::const_new();
async fn manager()->&'static TaskManager { MANAGER.get_or_init(||async{TaskManager::new(std::env::current_dir().unwrap_or_else(|_|PathBuf::from(".")))}).await }
#[tauri::command]fn health()->&'static str{"ok"}
#[tauri::command]async fn runtime_status(explicit:Option<String>)->Result<RuntimeInfo,String>{Ok(manager().await.runtime_status(explicit).await)}
#[tauri::command]async fn detect_omp(explicit:Option<String>)->Result<RuntimeInfo,String>{runtime_status(explicit).await}
#[tauri::command]async fn start_task(task_id:String,explicit:Option<String>)->Result<serde_json::Value,String>{manager().await.start(task_id,explicit).await.map(|v|serde_json::to_value(v).unwrap()).map_err(|e|e.to_string())}
#[tauri::command]async fn stop_task(task_id:String)->Result<serde_json::Value,String>{manager().await.stop(&task_id).await.map(|_|serde_json::json!({"taskId":task_id,"status":"interrupted"})).map_err(|e|e.to_string())}
#[tauri::command]async fn prompt_task(task_id:String,message:String)->Result<serde_json::Value,String>{manager().await.request(&task_id,"prompt",serde_json::json!({"message":message})).await.map_err(|e|e.to_string())?;task_snapshot(task_id).await}
#[tauri::command]async fn abort_task(task_id:String)->Result<serde_json::Value,String>{manager().await.request(&task_id,"abort",serde_json::json!({})).await.map_err(|e|e.to_string())?;task_snapshot(task_id).await}
#[tauri::command]async fn respond_ui(task_id:String,request_id:String,value:Option<String>,confirmed:Option<bool>,cancelled:bool)->Result<serde_json::Value,String>{manager().await.request(&task_id,"extension_ui_response",serde_json::json!({"id":request_id,"value":value,"confirmed":confirmed,"cancelled":cancelled})).await.map_err(|e|e.to_string())?;task_snapshot(task_id).await}
#[tauri::command]async fn restart_task(task_id:String)->Result<serde_json::Value,String>{manager().await.restart(&task_id).await.map(|v|serde_json::to_value(v).unwrap()).map_err(|e|e.to_string())}
#[tauri::command]async fn list_tasks()->Result<Vec<serde_json::Value>,String>{Ok(manager().await.snapshots().await.into_iter().map(|v|serde_json::to_value(v).unwrap()).collect())}
#[tauri::command]async fn task_snapshot(task_id:String)->Result<serde_json::Value,String>{manager().await.snapshot(&task_id).await.map(|v|serde_json::to_value(v).unwrap()).map_err(|e|e.to_string())}
#[cfg_attr(mobile,tauri::mobile_entry_point)]pub fn run(){tauri::Builder::default().invoke_handler(tauri::generate_handler![health,runtime_status,detect_omp,start_task,stop_task,prompt_task,abort_task,respond_ui,restart_task,list_tasks,task_snapshot]).run(tauri::generate_context!()).expect("error while running application");}
