use std::{fs, path::Path, sync::Arc};
use tauri::State;
use uuid::Uuid;

use crate::remote_ssh::{self, RemoteSessionManager};
use crate::server_sessions::{LocalServerState, LocalSessionManager};
use super::model::validate_name;

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
pub async fn create_server_profile(
    name: String,
    mut target: ServerTarget,
    auto_directory: bool,
    store: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<ServerProfile, String> {
    validate_name(&name)?;
    target.validate()?;
    if auto_directory {
        let ServerTarget::Remote { environment_id, directory, .. } = &mut target else {
            return Err("Automatic folders are only available for remote servers.".into());
        };
        let environment = store.environment(*environment_id)?;
        if environment.root_directory.is_empty() {
            return Err("Set this environment's Minecraft root before adding a server.".into());
        }
        *directory = remote_ssh::prepare_remote_server_directory(
            environment,
            name.clone(),
            Arc::clone(remote_sessions.inner()),
        ).await?;
    } else if let ServerTarget::Remote { environment_id, directory, .. } = &mut target {
        if directory.trim().is_empty() {
            return Err("Enter an existing remote server folder or choose automatic folder creation.".into());
        }
        let environment = store.environment(*environment_id)?;
        *directory = remote_ssh::verify_remote_server_directory(
            environment,
            directory.clone(),
            Arc::clone(remote_sessions.inner()),
        ).await?;
    }
    store.create(name, target)
}

fn folder_name(name: &str) -> String {
    let mut slug = String::new();
    for character in name.trim().chars() {
        if character.is_alphanumeric() {
            slug.extend(character.to_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_owned()
}

#[tauri::command]
pub async fn update_server_profile(
    id: Uuid,
    name: String,
    mut target: ServerTarget,
    store: State<'_, ServerProfileStore>,
    local_sessions: State<'_, LocalSessionManager>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<ServerProfile, String> {
    let previous = store.list()?.into_iter().find(|profile| profile.id == id)
        .ok_or("Server profile was not found.")?;
    if previous.name == name.trim() {
        return store.update(id, name, target);
    }
    validate_name(&name)?;
    let slug = folder_name(&name);
    if slug.is_empty() {
        return Err("The server name needs a letter or number for its folder name.".into());
    }
    let old_directory = match &previous.target {
        ServerTarget::Local { directory, .. } | ServerTarget::Remote { directory, .. } => directory.clone(),
    };
    let requested_directory = match &target {
        ServerTarget::Local { directory, .. } | ServerTarget::Remote { directory, .. } => directory,
    };
    if requested_directory != &old_directory || std::mem::discriminant(&target) != std::mem::discriminant(&previous.target) {
        return Err("Rename the server and change its folder in separate saves.".into());
    }
    if let (ServerTarget::Remote { environment_id: old, .. }, ServerTarget::Remote { environment_id: new, .. }) = (&previous.target, &target) {
        if old != new { return Err("Rename the server and change its remote environment in separate saves.".into()); }
    }
    if store.list()?.iter().any(|profile| profile.id != id && match &profile.target {
        ServerTarget::Local { directory, .. } | ServerTarget::Remote { directory, .. } => directory == &old_directory,
    }) {
        return Err("Another server profile uses this folder. Give it its own folder before renaming.".into());
    }

    let new_directory = match &previous.target {
        ServerTarget::Local { directory, .. } => {
            if !matches!(local_sessions.status(id)?.state, LocalServerState::Stopped) {
                return Err("Stop the server before renaming its folder.".into());
            }
            let source = Path::new(directory);
            if !fs::symlink_metadata(source).map_err(|error| format!("Could not inspect the server folder: {error}"))?.file_type().is_dir() {
                return Err("The server folder must be a real directory, not a symbolic link.".into());
            }
            let canonical_source = fs::canonicalize(source).map_err(|error| format!("Could not open the server folder: {error}"))?;
            if store.local_root()?.is_some_and(|root| fs::canonicalize(root).ok().as_deref() == Some(canonical_source.as_path())) {
                return Err("The configured Minecraft root cannot be renamed as a server folder.".into());
            }
            let parent = source.parent().ok_or("The server folder has no parent directory.")?;
            let destination = parent.join(&slug);
            if destination != source {
                match fs::symlink_metadata(&destination) {
                    Ok(_) => return Err("A folder with the new server name already exists.".into()),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
                    Err(error) => return Err(format!("Could not check the new server folder: {error}")),
                }
                fs::rename(&source, &destination).map_err(|error| format!("Could not rename the server folder: {error}"))?;
            }
            let result = destination.to_string_lossy().into_owned();
            if let ServerTarget::Local { directory, .. } = &mut target { *directory = result.clone(); }
            match store.update_after_directory_move(id, name, target) {
                Ok(profile) => return Ok(profile),
                Err(error) => {
                    if destination != source {
                        if fs::symlink_metadata(source).is_ok() {
                            return Err(format!("Could not save the server profile: {error}. The folder is now at {result}; the old path is occupied, so it cannot be moved back."));
                        }
                        fs::rename(&destination, &source).map_err(|rollback| format!("Could not save the server profile: {error}. The folder is now at {result}; moving it back failed: {rollback}"))?;
                    }
                    return Err(error);
                }
            }
        }
        ServerTarget::Remote { environment_id, .. } => {
            let environment = store.environment(*environment_id)?;
            remote_ssh::rename_remote_server_directory(previous.clone(), environment.clone(), slug, Arc::clone(remote_sessions.inner())).await?
        }
    };
    if let ServerTarget::Remote { directory, .. } = &mut target { *directory = new_directory.clone(); }
    match store.update_after_directory_move(id, name, target) {
        Ok(profile) => Ok(profile),
        Err(error) => {
            let environment_id = match previous.target { ServerTarget::Remote { environment_id, .. } => environment_id, _ => unreachable!() };
            let environment = store.environment(environment_id)?;
            remote_ssh::move_remote_server_directory(previous, environment, new_directory.clone(), old_directory, Arc::clone(remote_sessions.inner()))
                .await.map_err(|rollback| format!("Could not save the server profile: {error}. The folder is now at {new_directory}; moving it back failed: {rollback}"))?;
            Err(error)
        }
    }
}

#[tauri::command]
pub fn delete_server_profile(id: Uuid, store: State<'_, ServerProfileStore>) -> Result<(), String> {
    store.delete(id)
}
