mod remote_ssh;
mod eula;
mod java_runtime;
mod server_files;
mod server_logs;
mod server_profiles;
mod server_sessions;
mod vanilla_install;
mod software_install;
mod modrinth;
mod server_overview;

use server_profiles::{ServerProfileStore, commands};
use server_sessions::{LocalSessionManager, commands as session_commands};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;

pub fn run_session_manager(app_data_dir: PathBuf) -> Result<(), String> {
    server_sessions::run_session_manager(app_data_dir)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let profile_path = app.path().app_data_dir()?.join("server-profiles.json");
            let profile_store =
                ServerProfileStore::open(profile_path).map_err(std::io::Error::other)?;
            app.manage(profile_store);
            let session_manager = LocalSessionManager::new(app.path().app_data_dir()?)
                .map_err(std::io::Error::other)?;
            app.manage(session_manager);
            app.manage(Arc::new(remote_ssh::RemoteSessionManager::default()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            vanilla_install::list_vanilla_versions,
            vanilla_install::install_vanilla_version,
            software_install::install_server_software,
            modrinth::search_modrinth,
            modrinth::list_modrinth_versions,
            modrinth::preview_modrinth_install,
            modrinth::install_modrinth_plan,
            modrinth::remove_modrinth_plugin,
            vanilla_install::use_managed_java,
            commands::get_local_install_root,
            commands::get_auto_accept_eula,
            commands::set_auto_accept_eula,
            commands::set_local_install_root,
            commands::list_server_environments,
            commands::create_server_environment,
            commands::update_server_environment,
            commands::delete_server_environment,
            commands::list_server_profiles,
            commands::create_server_profile,
            commands::update_server_profile,
            server_overview::server_usage,
            commands::delete_server_profile,
            session_commands::local_server_status,
            session_commands::start_local_server,
            session_commands::stop_local_server,
            session_commands::local_server_console,
            session_commands::send_local_console_input,
            server_files::list_local_server_files,
            server_files::list_local_worlds,
            server_files::read_local_server_file,
            server_files::read_local_server_log_start,
            server_logs::read_local_server_log,
            server_files::write_local_server_file,
            remote_ssh::test_remote_connection,
            remote_ssh::test_remote_connection_draft,
            remote_ssh::remote_server_status,
            remote_ssh::start_remote_server,
            remote_ssh::stop_remote_server,
            remote_ssh::remote_server_console,
            remote_ssh::send_remote_console_input,
            remote_ssh::list_remote_server_files,
            remote_ssh::list_remote_worlds,
            remote_ssh::read_remote_server_file,
            remote_ssh::read_remote_server_log_start,
            remote_ssh::read_remote_server_log,
            remote_ssh::detect_remote_fabric_version,
            remote_ssh::write_remote_server_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running scerite");
}
