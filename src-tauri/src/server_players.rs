use std::{collections::HashMap, fs, io::Read, path::Path};

use fastnbt::Value;
use flate2::read::GzDecoder;
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::server_files::{local_server_root, resolve_existing};
use crate::server_profiles::ServerProfileStore;

const MAX_PLAYER_FILE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStatistic {
    pub category: String,
    pub key: String,
    pub value: i64,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerData {
    pub has_saved_data: bool,
    pub has_saved_statistics: bool,
    pub gamemode: Option<String>,
    pub level: Option<i64>,
    pub health: Option<f64>,
    pub hunger: Option<i64>,
    pub position: Option<[f64; 3]>,
    pub dimension: Option<String>,
    pub statistics: Vec<PlayerStatistic>,
    pub data_warning: Option<String>,
    pub stats_warning: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerLocation {
    pub uuid: Uuid,
    pub position: Option<[f64; 3]>,
    pub dimension: Option<String>,
}

pub(crate) fn decode_player_location(uuid: Uuid, bytes: Option<Vec<u8>>) -> PlayerLocation {
    let data = decode_player_data(bytes, None);
    PlayerLocation { uuid, position: data.position, dimension: data.dimension }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerRosterFiles {
    pub cache: String,
    pub whitelist: String,
    pub ops: String,
    pub bans: String,
    pub banned_ips: String,
    pub uuids: Vec<String>,
}

pub(crate) fn player_list_paths(world: &str) -> [String; 4] {
    [
        format!("{world}/playerdata"),
        format!("{world}/players/data"),
        format!("{world}/stats"),
        format!("{world}/players/stats"),
    ]
}

pub(crate) fn world_name(properties: Option<&[u8]>) -> Result<String, String> {
    let setting = properties
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .and_then(|text| {
            text.lines()
                .filter_map(|line| line.trim().split_once('='))
                .find(|(key, _)| key.trim() == "level-name")
        })
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or("world");
    if setting.len() > 128
        || matches!(setting, "." | "..")
        || setting.contains(['/', '\\'])
        || setting.chars().any(char::is_control)
    {
        return Err("The configured world folder name is not safe to open.".into());
    }
    Ok(setting.to_owned())
}

pub(crate) fn player_data_paths(world: &str, uuid: Uuid) -> [String; 2] {
    [
        format!("{world}/playerdata/{uuid}.dat"),
        format!("{world}/players/data/{uuid}.dat"),
    ]
}

pub(crate) fn player_stats_paths(world: &str, uuid: Uuid) -> [String; 2] {
    [
        format!("{world}/stats/{uuid}.json"),
        format!("{world}/players/stats/{uuid}.json"),
    ]
}

pub(crate) fn max_player_file_bytes() -> u64 {
    MAX_PLAYER_FILE_BYTES
}

pub(crate) fn decode_player_data(
    player_file: Option<Vec<u8>>,
    stats_file: Option<Vec<u8>>,
) -> PlayerData {
    let mut data = PlayerData::default();
    data.has_saved_data = player_file.is_some();
    data.has_saved_statistics = stats_file.is_some();
    if let Some(bytes) = player_file {
        let mut decoded = Vec::new();
        let parsed = GzDecoder::new(bytes.as_slice())
            .take(MAX_PLAYER_FILE_BYTES + 1)
            .read_to_end(&mut decoded)
            .map_err(|error| error.to_string())
            .and_then(|_| {
                if decoded.len() as u64 > MAX_PLAYER_FILE_BYTES {
                    Err("The player data is too large.".into())
                } else {
                    fastnbt::from_bytes::<HashMap<String, Value>>(&decoded)
                        .map_err(|error| error.to_string())
                }
            });
        match parsed {
            Ok(root) => {
                data.gamemode = root
                    .get("playerGameType")
                    .and_then(Value::as_i64)
                    .map(|value| {
                        match value {
                            0 => "Survival",
                            1 => "Creative",
                            2 => "Adventure",
                            3 => "Spectator",
                            _ => "Unknown",
                        }
                        .to_owned()
                    });
                data.level = root.get("XpLevel").and_then(Value::as_i64);
                data.health = root.get("Health").and_then(Value::as_f64);
                data.hunger = root.get("foodLevel").and_then(Value::as_i64);
                data.position = match root.get("Pos") {
                    Some(Value::List(values)) if values.len() >= 3 => values[0]
                        .as_f64()
                        .zip(values[1].as_f64())
                        .zip(values[2].as_f64())
                        .map(|((x, y), z)| [x, y, z]),
                    _ => None,
                };
                data.dimension = match root.get("Dimension") {
                    Some(Value::String(value)) => Some(value.clone()),
                    _ => None,
                };
            }
            Err(error) => {
                data.data_warning = Some(format!("Could not read saved player data: {error}"))
            }
        }
    }
    if let Some(bytes) = stats_file {
        let parsed = serde_json::from_slice::<serde_json::Value>(&bytes);
        match parsed {
            Ok(json) => {
                if let Some(categories) = json.get("stats").and_then(serde_json::Value::as_object) {
                    for (category, entries) in categories {
                        if let Some(entries) = entries.as_object() {
                            for (key, value) in entries {
                                if let Some(value) = value.as_i64() {
                                    data.statistics.push(PlayerStatistic {
                                        category: category.clone(),
                                        key: key.clone(),
                                        value,
                                    });
                                }
                            }
                        }
                    }
                    data.statistics.sort_by(|left, right| {
                        left.category
                            .cmp(&right.category)
                            .then_with(|| left.key.cmp(&right.key))
                    });
                }
            }
            Err(error) => {
                data.stats_warning = Some(format!("Could not read player statistics: {error}"))
            }
        }
    }
    data
}

fn optional_local_file(root: &Path, relative: &str) -> Result<Option<Vec<u8>>, String> {
    let candidate = root.join(relative);
    match fs::symlink_metadata(&candidate) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Err("A player data path is not a regular file.".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Could not inspect player data: {error}")),
    }
    let path = resolve_existing(root, relative)?;
    let size = fs::metadata(&path)
        .map_err(|error| format!("Could not inspect player data: {error}"))?
        .len();
    if size > MAX_PLAYER_FILE_BYTES {
        return Err("A player data file is too large to display (over 8 MB).".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|error| format!("Could not open player data: {error}"))?
        .take(MAX_PLAYER_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read player data: {error}"))?;
    if bytes.len() as u64 > MAX_PLAYER_FILE_BYTES {
        return Err("A player data file is too large to display (over 8 MB).".into());
    }
    Ok(Some(bytes))
}

fn optional_local_player_list(root: &Path, relative: &str) -> Result<Vec<String>, String> {
    let candidate = root.join(relative);
    match fs::symlink_metadata(&candidate) {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => return Err("A player list path is not a folder.".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("Could not inspect player files: {error}")),
    }
    let path = resolve_existing(root, relative)?;
    let mut uuids = Vec::new();
    for entry in
        fs::read_dir(path).map_err(|error| format!("Could not list player files: {error}"))?
    {
        let entry = entry.map_err(|error| format!("Could not list player files: {error}"))?;
        if !entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            continue;
        }
        let filename = entry.file_name().to_string_lossy().into_owned();
        let stem = filename
            .strip_suffix(".dat")
            .or_else(|| filename.strip_suffix(".json"));
        if let Some(Ok(uuid)) = stem.map(Uuid::parse_str) {
            uuids.push(uuid.to_string());
        }
    }
    Ok(uuids)
}

#[tauri::command]
pub fn read_local_player_usercache(
    profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
) -> Result<String, String> {
    let root = local_server_root(profile_id, &profiles)?;
    optional_local_file(&root, "usercache.json")?
        .map(|bytes| String::from_utf8(bytes).map_err(|error| error.to_string()))
        .transpose()
        .map(|value| value.unwrap_or_default())
}

#[tauri::command]
pub fn read_local_player_roster(
    profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
) -> Result<PlayerRosterFiles, String> {
    let root = local_server_root(profile_id, &profiles)?;
    let properties = optional_local_file(&root, "server.properties")?;
    let world = world_name(properties.as_deref())?;
    let read_text = |name| -> Result<String, String> {
        optional_local_file(&root, name)?
            .map(|bytes| String::from_utf8(bytes).map_err(|error| error.to_string()))
            .transpose()
            .map(|value| value.unwrap_or_default())
    };
    let mut uuids = Vec::new();
    for path in player_list_paths(&world) {
        uuids.extend(optional_local_player_list(&root, &path)?);
    }
    uuids.sort();
    uuids.dedup();
    Ok(PlayerRosterFiles {
        cache: read_text("usercache.json")?,
        whitelist: read_text("whitelist.json")?,
        ops: read_text("ops.json")?,
        bans: read_text("banned-players.json")?,
        banned_ips: read_text("banned-ips.json")?,
        uuids,
    })
}

#[tauri::command]
pub fn read_local_player_data(
    profile_id: Uuid,
    uuid: Uuid,
    profiles: State<'_, ServerProfileStore>,
) -> Result<PlayerData, String> {
    let root = local_server_root(profile_id, &profiles)?;
    let properties = optional_local_file(&root, "server.properties")?;
    let world = world_name(properties.as_deref())?;
    let mut player_file = None;
    for path in player_data_paths(&world, uuid) {
        player_file = optional_local_file(&root, &path)?;
        if player_file.is_some() {
            break;
        }
    }
    let mut stats_file = None;
    for path in player_stats_paths(&world, uuid) {
        stats_file = optional_local_file(&root, &path)?;
        if stats_file.is_some() {
            break;
        }
    }
    Ok(decode_player_data(player_file, stats_file))
}

#[tauri::command]
pub fn read_local_player_locations(
    profile_id: Uuid,
    uuids: Vec<Uuid>,
    profiles: State<'_, ServerProfileStore>,
) -> Result<Vec<PlayerLocation>, String> {
    if uuids.len() > 1000 { return Err("Too many player locations were requested.".into()); }
    let root = local_server_root(profile_id, &profiles)?;
    let properties = optional_local_file(&root, "server.properties")?;
    let world = world_name(properties.as_deref())?;
    let mut locations = Vec::with_capacity(uuids.len());
    for uuid in uuids {
        let mut player_file = None;
        for path in player_data_paths(&world, uuid) {
            player_file = optional_local_file(&root, &path)?;
            if player_file.is_some() { break; }
        }
        locations.push(decode_player_location(uuid, player_file));
    }
    Ok(locations)
}
