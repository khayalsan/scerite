use tauri::State;
use uuid::Uuid;

use super::{ServerProfile, ServerProfileStore, ServerTarget};

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
