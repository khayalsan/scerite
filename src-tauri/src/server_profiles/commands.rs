use tauri::State;
use uuid::Uuid;

use super::{ServerEnvironment, ServerProfile, ServerProfileStore, ServerTarget};

#[tauri::command]
pub fn list_server_environments(
    store: State<'_, ServerProfileStore>,
) -> Result<Vec<ServerEnvironment>, String> {
    store.list_environments()
}

#[tauri::command]
pub fn create_server_environment(
    name: String,
    host: String,
    port: u16,
    username: String,
    identity_file: Option<String>,
    store: State<'_, ServerProfileStore>,
) -> Result<ServerEnvironment, String> {
    store.create_environment(name, host, port, username, identity_file)
}

#[tauri::command]
pub fn update_server_environment(
    id: Uuid,
    name: String,
    host: String,
    port: u16,
    username: String,
    identity_file: Option<String>,
    store: State<'_, ServerProfileStore>,
) -> Result<ServerEnvironment, String> {
    store.update_environment(id, name, host, port, username, identity_file)
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
