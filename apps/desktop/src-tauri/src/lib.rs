pub mod workbench;
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    workbench::run();
}
