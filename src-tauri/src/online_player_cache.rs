use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::server_profiles::ServerProfileStore;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnlinePlayerCache {
    names: Vec<String>,
    response: String,
    updated_at: u64,
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
        .join("online-players")
        .join(format!("{profile_id}.json")))
}

#[tauri::command]
pub fn read_online_player_cache(
    app: AppHandle,
    profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
) -> Result<Option<OnlinePlayerCache>, String> {
    let path = cache_path(&app, profile_id, &profiles)?;
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| format!("Could not read the online player cache: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Could not read the online player cache: {error}")),
    }
}

#[tauri::command]
pub fn write_online_player_cache(
    app: AppHandle,
    profile_id: Uuid,
    names: Vec<String>,
    response: String,
    profiles: State<'_, ServerProfileStore>,
) -> Result<OnlinePlayerCache, String> {
    if names.len() > 1_000
        || response.len() > 32_768
        || names.iter().any(|name| {
            !(3..=16).contains(&name.len())
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
    {
        return Err("The server returned an invalid player list.".into());
    }
    let path = cache_path(&app, profile_id, &profiles)?;
    fs::create_dir_all(
        path.parent()
            .ok_or("The online player cache path is invalid.")?,
    )
    .map_err(|error| format!("Could not create the online player cache folder: {error}"))?;
    let cache = OnlinePlayerCache {
        names,
        response,
        updated_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_secs(),
    };
    fs::write(
        path,
        serde_json::to_vec(&cache).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("Could not save the online player cache: {error}"))?;
    Ok(cache)
}
