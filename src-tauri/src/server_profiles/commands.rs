use tauri::State;
use uuid::Uuid;

use super::{ServerEnvironment, ServerProfile, ServerProfileStore, ServerTarget};

#[tauri::command]
pub fn get_local_install_root(store: State<'_, ServerProfileStore>) -> Result<Option<String>, String> {
    store.local_root()
}

#[tauri::command]
pub fn get_auto_accept_eula(store: State<'_, ServerProfileStore>) -> Result<bool, String> {
    store.auto_accept_eula()
}

#[tauri::command]
pub fn set_auto_accept_eula(
    enabled: bool,
    store: State<'_, ServerProfileStore>,
) -> Result<bool, String> {
    store.set_auto_accept_eula(enabled)
}

#[tauri::command]
pub fn set_local_install_root(
    root_directory: String,
    store: State<'_, ServerProfileStore>,
) -> Result<String, String> {
    store.set_local_root(root_directory)
}

#[tauri::command]
pub fn list_server_environments(
    store: State<'_, ServerProfileStore>,
) -> Result<Vec<ServerEnvironment>, String> {
    store.list_environments()
}

#[tauri::command]
pub fn create_server_environment(
    name: String,
    root_directory: String,
    host: String,
    port: u16,
    username: String,
    identity_file: Option<String>,
    store: State<'_, ServerProfileStore>,
) -> Result<ServerEnvironment, String> {
    store.create_environment(name, root_directory, host, port, username, identity_file)
}

#[tauri::command]
pub fn update_server_environment(
    id: Uuid,
    name: String,
    root_directory: String,
    host: String,
    port: u16,
    username: String,
    identity_file: Option<String>,
    store: State<'_, ServerProfileStore>,
) -> Result<ServerEnvironment, String> {
    store.update_environment(id, name, root_directory, host, port, username, identity_file)
}

#[tauri::command]
pub fn delete_server_environment(
    id: Uuid,
    store: State<'_, ServerProfileStore>,
) -> Result<(), String> {
    store.delete_environment(id)
}

#[tauri::command]
pub fn list_server_profiles(
    store: State<'_, ServerProfileStore>,
) -> Result<Vec<ServerProfile>, String> {
    store.list()
}

#[tauri::command]
pub fn create_server_profile(
    name: String,
    target: ServerTarget,
    store: State<'_, ServerProfileStore>,
) -> Result<ServerProfile, String> {
    store.create(name, target)
}

#[tauri::command]
pub fn update_server_profile(
    id: Uuid,
    name: String,
    target: ServerTarget,
    store: State<'_, ServerProfileStore>,
) -> Result<ServerProfile, String> {
    store.update(id, name, target)
}

#[tauri::command]
pub fn delete_server_profile(id: Uuid, store: State<'_, ServerProfileStore>) -> Result<(), String> {
    store.delete(id)
}
