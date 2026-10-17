mod server_profiles;
mod server_sessions;

use server_profiles::{ServerProfileStore, commands};
use server_sessions::{LocalSessionManager, commands as session_commands};
use tauri::Manager;
use std::path::PathBuf;

pub fn run_session_manager(app_data_dir: PathBuf) -> Result<(), String> {
    server_sessions::run_session_manager(app_data_dir)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let profile_path = app.path().app_data_dir()?.join("server-profiles.json");
            let profile_store =
                ServerProfileStore::open(profile_path).map_err(std::io::Error::other)?;
            app.manage(profile_store);
            let session_manager =
                LocalSessionManager::new(app.path().app_data_dir()?).map_err(std::io::Error::other)?;
            app.manage(session_manager);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_server_profiles,
            commands::create_server_profile,
            commands::update_server_profile,
            commands::delete_server_profile,
            session_commands::local_server_status,
            session_commands::start_local_server,
            session_commands::stop_local_server,
        ])
        .run(tauri::generate_context!())
        .expect("error while running servercrafter");
}
