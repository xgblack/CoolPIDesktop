use host_core::{handshake,probe,resolve_executable,RuntimeInfo};
#[tauri::command]
fn health()->&'static str{"ok"}
#[tauri::command]
fn detect_omp(explicit:Option<String>)->Result<RuntimeInfo,String>{let p=resolve_executable(explicit.as_deref().map(std::path::Path::new)).map_err(|e|e.to_string())?;probe(&p).map_err(|e|e.to_string())}
#[tauri::command]
fn start_omp(explicit:Option<String>)->Result<RuntimeInfo,String>{let p=resolve_executable(explicit.as_deref().map(std::path::Path::new)).map_err(|e|e.to_string())?;handshake(&p,None).map_err(|e|e.to_string())}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(){tauri::Builder::default().invoke_handler(tauri::generate_handler![health,detect_omp,start_omp]).run(tauri::generate_context!()).expect("error while running application");}
