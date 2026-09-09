use host_core::{handshake,probe,resolve_executable,RuntimeInfo};
#[tauri::command]fn health()->&'static str{"ok"}
#[tauri::command]fn runtime_status(explicit:Option<String>)->Result<RuntimeInfo,String>{let p=resolve_executable(explicit.as_deref().map(std::path::Path::new)).map_err(|e|e.to_string())?;probe(&p).map_err(|e|e.to_string())}
#[tauri::command]fn detect_omp(explicit:Option<String>)->Result<RuntimeInfo,String>{runtime_status(explicit)}
#[tauri::command]fn start_task(task_id:String)->Result<serde_json::Value,String>{Ok(serde_json::json!({"taskId":task_id,"status":"starting","seq":0}))}
#[tauri::command]fn stop_task(task_id:String)->Result<serde_json::Value,String>{Ok(serde_json::json!({"taskId":task_id,"status":"interrupted","seq":1}))}
#[tauri::command]fn prompt_task(task_id:String,message:String)->Result<serde_json::Value,String>{Ok(serde_json::json!({"taskId":task_id,"eventType":"prompt_accepted","payload":message}))}
#[tauri::command]fn abort_task(task_id:String)->Result<serde_json::Value,String>{stop_task(task_id)}
#[tauri::command]fn task_snapshot(task_id:String)->Result<serde_json::Value,String>{Ok(serde_json::json!({"taskId":task_id,"status":"idle","seq":0,"messages":[]}))}
#[cfg_attr(mobile,tauri::mobile_entry_point)]pub fn run(){tauri::Builder::default().invoke_handler(tauri::generate_handler![health,runtime_status,detect_omp,start_task,stop_task,prompt_task,abort_task,task_snapshot]).run(tauri::generate_context!()).expect("error while running application");}
