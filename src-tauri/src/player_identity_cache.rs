use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::server_profiles::ServerProfileStore;

#[derive(Clone, Deserialize, Serialize)]
pub struct PlayerIdentity {
    uuid: Option<Uuid>,
    name: Option<String>,
}

fn cache_path(
    app: &AppHandle,
    profile_id: Uuid,
    profiles: &State<'_, ServerProfileStore>,
) -> Result<PathBuf, String> {
    if !profiles
        .list()?
        .iter()
        .any(|profile| profile.id == profile_id)
    {
        return Err("Server profile was not found.".into());
    }
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("player-identities")
        .join(format!("{profile_id}.json")))
}

#[tauri::command]
pub fn read_player_identity_cache(
    app: AppHandle,
    profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
) -> Result<Vec<PlayerIdentity>, String> {
    let path = cache_path(&app, profile_id, &profiles)?;
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|error| format!("Could not read the player identity cache: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(format!("Could not read the player identity cache: {error}")),
    }
}

#[tauri::command]
pub fn write_player_identity_cache(
    app: AppHandle,
    profile_id: Uuid,
    players: Vec<PlayerIdentity>,
    profiles: State<'_, ServerProfileStore>,
) -> Result<(), String> {
    if players.len() > 100_000
        || players.iter().any(|player| {
            player.uuid.is_none() && player.name.is_none()
                || player.name.as_ref().is_some_and(|name| {
                    !(3..=16).contains(&name.len())
                        || !name
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                })
        })
    {
        return Err("The player identity cache is invalid.".into());
    }
    let path = cache_path(&app, profile_id, &profiles)?;
    fs::create_dir_all(
        path.parent()
            .ok_or("The player identity cache path is invalid.")?,
    )
    .map_err(|error| format!("Could not create the player identity cache folder: {error}"))?;
    fs::write(
        path,
        serde_json::to_vec(&players).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("Could not save the player identity cache: {error}"))
}
