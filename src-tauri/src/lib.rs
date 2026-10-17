mod server_profiles;

use server_profiles::{ServerProfileStore, commands};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let profile_path = app.path().app_data_dir()?.join("server-profiles.json");
            let profile_store =
                ServerProfileStore::open(profile_path).map_err(std::io::Error::other)?;
            app.manage(profile_store);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_server_profiles,
            commands::create_server_profile,
            commands::update_server_profile,
            commands::delete_server_profile,
        ])
        .run(tauri::generate_context!())
        .expect("error while running servercrafter");
}
