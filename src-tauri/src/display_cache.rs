use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::{server_profiles::ServerProfileStore, terrain};

const MAX_CACHE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_SNAPSHOT_BYTES: usize = 256 * 1024;
const MAX_LOG_SNAPSHOT_BYTES: usize = 512 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedTileInput {
    x: i32,
    z: i32,
    timestamp: Option<u32>,
    rgba: Vec<u8>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedTile {
    x: i32,
    z: i32,
    timestamp: Option<u32>,
    status: &'static str,
    resolution: u8,
    rgba: Vec<u8>,
    read_bytes: u32,
}

fn root(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("display-cache")
        .join("v1"))
}

fn profile_dir(
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
    Ok(root(app)?.join(profile_id.to_string()))
}

fn tile_dir(profile: &Path, dimension: &str, resolution: u8) -> Result<PathBuf, String> {
    terrain::validate_request(dimension, resolution, &[])?;
    if dimension.len() > 120 {
        return Err("The map dimension is too long to cache.".into());
    }
    let encoded = dimension
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(profile
        .join("terrain")
        .join(encoded)
        .join(resolution.to_string()))
}

fn tile_path(dir: &Path, x: i32, z: i32) -> PathBuf {
    dir.join(format!("{x}_{z}.rgba"))
}

fn read_bytes(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > limit {
        return None;
    }
    fs::read(path).ok()
}

fn files_under(path: &Path, files: &mut Vec<(PathBuf, u64, SystemTime)>) -> Result<(), String> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("Could not inspect the display cache: {error}")),
    };
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
        if metadata.is_dir() {
            files_under(&entry.path(), files)?;
        } else if metadata.is_file() {
            files.push((
                entry.path(),
                metadata.len(),
                metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            ));
        }
    }
    Ok(())
}

fn usage(root: &Path) -> Result<(u64, Vec<(PathBuf, u64, SystemTime)>), String> {
    let mut files = Vec::new();
    files_under(root, &mut files)?;
    Ok((files.iter().map(|(_, size, _)| size).sum(), files))
}

fn trim(root: &Path) -> Result<(), String> {
    let (mut bytes, mut files) = usage(root)?;
    files.sort_by_key(|(_, _, modified)| *modified);
    for (path, size, _) in files {
        if bytes <= MAX_CACHE_BYTES {
            break;
        }
        if fs::remove_file(path).is_ok() {
            bytes = bytes.saturating_sub(size);
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn read_cached_terrain_tiles(
    app: AppHandle,
    profile_id: Uuid,
    dimension: String,
    resolution: u8,
    requests: Vec<terrain::TerrainRequest>,
    profiles: State<'_, ServerProfileStore>,
) -> Result<Vec<CachedTile>, String> {
    terrain::validate_request(&dimension, resolution, &[])?;
    if requests.len() > 512 {
        return Err("Too many cached map tiles were requested at once.".into());
    }
    let profile = profile_dir(&app, profile_id, &profiles)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut levels = [resolution, 4, 16]
            .into_iter()
            .filter(|level| *level >= resolution)
            .collect::<Vec<_>>();
        levels.dedup();
        let directories = levels
            .iter()
            .map(|level| tile_dir(&profile, &dimension, *level))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(requests
            .into_iter()
            .filter_map(|request| {
                for (level, dir) in levels.iter().zip(&directories) {
                    let expected = *level as usize * *level as usize * 4;
                    let Some(bytes) =
                        read_bytes(&tile_path(dir, request.x, request.z), (expected + 4) as u64)
                    else {
                        continue;
                    };
                    if bytes.len() != expected + 4 {
                        continue;
                    }
                    let timestamp = u32::from_le_bytes(bytes[..4].try_into().ok()?);
                    return Some(CachedTile {
                        x: request.x,
                        z: request.z,
                        timestamp: (timestamp != u32::MAX).then_some(timestamp),
                        status: "tile",
                        resolution: *level,
                        rgba: bytes[4..].to_vec(),
                        read_bytes: 0,
                    });
                }
                None
            })
            .collect())
    })
    .await
    .map_err(|error| format!("Could not read cached map tiles: {error}"))?
}

#[tauri::command]
pub fn write_cached_terrain_tiles(
    app: AppHandle,
    profile_id: Uuid,
    dimension: String,
    resolution: u8,
    tiles: Vec<CachedTileInput>,
    profiles: State<'_, ServerProfileStore>,
) -> Result<(), String> {
    if tiles.len() > terrain::MAX_TILES_PER_REQUEST {
        return Err("Too many map tiles were supplied.".into());
    }
    let dir = tile_dir(
        &profile_dir(&app, profile_id, &profiles)?,
        &dimension,
        resolution,
    )?;
    let expected = resolution as usize * resolution as usize * 4;
    if tiles.iter().any(|tile| tile.rgba.len() != expected) {
        return Err("A map tile has invalid image data.".into());
    }
    fs::create_dir_all(&dir)
        .map_err(|error| format!("Could not create the display cache: {error}"))?;
    for tile in tiles {
        let mut bytes = Vec::with_capacity(expected + 4);
        bytes.extend_from_slice(&tile.timestamp.unwrap_or(u32::MAX).to_le_bytes());
        bytes.extend_from_slice(&tile.rgba);
        fs::write(tile_path(&dir, tile.x, tile.z), bytes)
            .map_err(|error| format!("Could not save a map tile: {error}"))?;
    }
    trim(&root(&app)?)
}

#[tauri::command]
pub fn read_cached_map_snapshot(
    app: AppHandle,
    profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
) -> Result<Option<Value>, String> {
    let path = profile_dir(&app, profile_id, &profiles)?.join("map.json");
    Ok(read_bytes(&path, MAX_SNAPSHOT_BYTES as u64)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok()))
}

#[tauri::command]
pub fn write_cached_map_snapshot(
    app: AppHandle,
    profile_id: Uuid,
    snapshot: Value,
    profiles: State<'_, ServerProfileStore>,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(&snapshot).map_err(|error| error.to_string())?;
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err("The map snapshot is too large to cache.".into());
    }
    let profile = profile_dir(&app, profile_id, &profiles)?;
    fs::create_dir_all(&profile).map_err(|error| error.to_string())?;
    fs::write(profile.join("map.json"), bytes).map_err(|error| error.to_string())?;
    trim(&root(&app)?)
}

#[tauri::command]
pub fn read_cached_logs(
    app: AppHandle,
    profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
) -> Result<Option<Value>, String> {
    let path = profile_dir(&app, profile_id, &profiles)?.join("logs.json");
    Ok(read_bytes(&path, MAX_LOG_SNAPSHOT_BYTES as u64)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok()))
}

#[tauri::command]
pub fn write_cached_logs(
    app: AppHandle,
    profile_id: Uuid,
    snapshot: Value,
    profiles: State<'_, ServerProfileStore>,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(&snapshot).map_err(|error| error.to_string())?;
    if bytes.len() > MAX_LOG_SNAPSHOT_BYTES {
        return Err("The log snapshot is too large to cache.".into());
    }
    let profile = profile_dir(&app, profile_id, &profiles)?;
    fs::create_dir_all(&profile).map_err(|error| error.to_string())?;
    fs::write(profile.join("logs.json"), bytes).map_err(|error| error.to_string())?;
    trim(&root(&app)?)
}

#[tauri::command]
pub fn read_cached_server_icon(
    app: AppHandle,
    profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
) -> Result<Option<Vec<u8>>, String> {
    let path = profile_dir(&app, profile_id, &profiles)?.join("server-icon.png");
    Ok(read_bytes(&path, 128 * 1024).filter(|bytes| bytes.starts_with(b"\x89PNG\r\n\x1a\n")))
}

#[tauri::command]
pub fn write_cached_server_icon(
    app: AppHandle,
    profile_id: Uuid,
    bytes: Option<Vec<u8>>,
    profiles: State<'_, ServerProfileStore>,
) -> Result<(), String> {
    let profile = profile_dir(&app, profile_id, &profiles)?;
    let path = profile.join("server-icon.png");
    if let Some(bytes) = bytes {
        if bytes.len() > 128 * 1024 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err("The server icon is not a valid cacheable PNG.".into());
        }
        fs::create_dir_all(&profile).map_err(|error| error.to_string())?;
        fs::write(path, bytes).map_err(|error| error.to_string())?;
        trim(&root(&app)?)
    } else {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

#[tauri::command]
pub fn display_cache_size(app: AppHandle) -> Result<u64, String> {
    Ok(usage(&root(&app)?)?.0)
}

#[tauri::command]
pub fn clear_display_cache(app: AppHandle) -> Result<(), String> {
    let path = root(&app)?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path)
            .map_err(|error| format!("Could not clear the display cache: {error}")),
        Ok(_) => Err("The display cache folder is not a directory.".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not inspect the display cache: {error}")),
    }
}
