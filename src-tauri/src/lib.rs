mod remote_ssh;
mod online_player_cache;
mod player_identity_cache;
mod eula;
mod java_runtime;
mod server_files;
mod server_logs;
mod server_players;
mod server_profiles;
mod server_sessions;
mod vanilla_install;
mod software_install;
mod modrinth;
mod modpacks;
mod server_overview;
mod server_port;
mod world_management;
mod velocity_download;
mod terrain;
mod display_cache;

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
            modrinth::preview_modrinth_mod_removal,
            modrinth::remove_modrinth_mod,
            modpacks::preview_modrinth_modpack,
            modpacks::install_modrinth_modpack,
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
            world_management::read_local_world_settings,
            world_management::read_auto_backup_settings,
            world_management::save_auto_backup_settings,
            world_management::delete_world_zip_backup,
            world_management::list_local_world_backups,
            world_management::restore_local_world_backup,
            world_management::backup_local_world,
            world_management::auto_backup_local_world,
            world_management::download_local_world,
            world_management::upload_local_world,
            world_management::import_local_world,
            world_management::regenerate_local_world,
            server_files::read_local_server_file,
            server_files::read_local_server_log_start,
            server_logs::read_local_server_log,
            server_players::read_local_player_data,
            server_players::read_local_player_locations,
            terrain::read_local_terrain_tiles,
            display_cache::read_cached_terrain_tiles,
            display_cache::write_cached_terrain_tiles,
            display_cache::read_cached_map_snapshot,
            display_cache::write_cached_map_snapshot,
            display_cache::read_cached_logs,
            display_cache::write_cached_logs,
            display_cache::read_cached_server_icon,
            display_cache::write_cached_server_icon,
            display_cache::display_cache_size,
            display_cache::clear_display_cache,
            server_players::read_local_player_roster,
            server_players::read_local_player_usercache,
            server_files::write_local_server_file,
            server_files::upload_local_server_icon,
            server_files::read_local_server_icon,
            remote_ssh::test_remote_connection,
            remote_ssh::test_remote_connection_draft,
            remote_ssh::discord_bridge_status,
            remote_ssh::enable_discord_bridge,
            remote_ssh::disable_discord_bridge,
            remote_ssh::remote_server_status,
            remote_ssh::start_remote_server,
            remote_ssh::stop_remote_server,
            remote_ssh::remote_server_console,
            remote_ssh::send_remote_console_input,
            remote_ssh::list_remote_server_files,
            remote_ssh::list_remote_worlds,
            remote_ssh::read_remote_world_settings,
            remote_ssh::list_remote_world_backups,
            remote_ssh::restore_remote_world_backup,
            remote_ssh::backup_remote_world,
            remote_ssh::auto_backup_remote_world,
            remote_ssh::download_remote_world,
            remote_ssh::upload_remote_world,
            remote_ssh::import_remote_world,
            remote_ssh::velocity_on_demand_status,
            remote_ssh::set_velocity_idle_minutes,
            remote_ssh::velocity_proxy_usage,
            remote_ssh::enable_velocity_on_demand,
            remote_ssh::disable_velocity_on_demand,
            remote_ssh::regenerate_remote_world,
            remote_ssh::read_remote_server_file,
            remote_ssh::read_remote_server_log_start,
            remote_ssh::read_remote_server_log,
            remote_ssh::read_remote_player_data,
            remote_ssh::read_remote_player_locations,
            remote_ssh::read_remote_terrain_tiles,
            remote_ssh::read_remote_player_roster,
            remote_ssh::read_remote_player_usercache,
            player_identity_cache::read_player_identity_cache,
            player_identity_cache::write_player_identity_cache,
            online_player_cache::read_online_player_cache,
            online_player_cache::write_online_player_cache,
            remote_ssh::detect_remote_fabric_version,
            remote_ssh::write_remote_server_file,
            remote_ssh::upload_remote_server_icon,
            remote_ssh::read_remote_server_icon,
        ])
        .run(tauri::generate_context!())
        .expect("error while running scerite");
}
