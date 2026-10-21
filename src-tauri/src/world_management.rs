use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File},
    io::{self, Read},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use fastnbt::Value;
use flate2::read::GzDecoder;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::{
    server_files::{local_server_root, resolve_existing},
    server_profiles::ServerProfileStore,
    server_sessions::{LocalServerState, LocalSessionManager},
};

pub(crate) const MAX_LEVEL_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ARCHIVE_FILES: usize = 200_000;
const MAX_ARCHIVE_BYTES: u64 = 100 * 1024 * 1024 * 1024;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldRule {
    name: String,
    value: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldSettings {
    actual_seed: Option<String>,
    pub(crate) gamerules: Vec<WorldRule>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldBackup {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub modified: Option<u64>,
}

pub(crate) fn valid_world_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !matches!(name, "." | "..")
        && !name.contains(['/', '\\'])
        && !name.contains([':', '*', '?', '"', '<', '>', '|'])
        && !name.ends_with([' ', '.'])
        && !name.chars().any(char::is_control)
}

pub(crate) fn decode_settings(bytes: &[u8]) -> Result<WorldSettings, String> {
    let mut decoded = Vec::new();
    GzDecoder::new(bytes)
        .take(MAX_LEVEL_BYTES + 1)
        .read_to_end(&mut decoded)
        .map_err(|error| format!("Could not read level.dat: {error}"))?;
    if decoded.len() as u64 > MAX_LEVEL_BYTES {
        return Err("level.dat is too large to inspect.".into());
    }
    let root: HashMap<String, Value> = fastnbt::from_bytes(&decoded)
        .map_err(|error| format!("Could not decode level.dat: {error}"))?;
    let Some(Value::Compound(data)) = root.get("Data") else {
        return Err("level.dat does not contain world data.".into());
    };
    let actual_seed = data
        .get("WorldGenSettings")
        .and_then(|value| match value {
            Value::Compound(settings) => settings.get("seed"),
            _ => None,
        })
        .or_else(|| data.get("RandomSeed"))
        .and_then(Value::as_i64)
        .map(|seed| seed.to_string());
    let mut gamerules = BTreeMap::new();
    if let Some(Value::Compound(rules)) = data.get("game_rules").or_else(|| data.get("GameRules")) {
        for (name, value) in rules {
            let text = match value {
                Value::String(text) => Some(text.clone()),
                Value::Byte(value) => Some((*value != 0).to_string()),
                _ => value.as_i64().map(|number| number.to_string()),
            };
            if let Some(text) = text {
                gamerules.insert(name.clone(), text);
            }
        }
    }
    Ok(WorldSettings {
        actual_seed,
        gamerules: gamerules
            .into_iter()
            .map(|(name, value)| WorldRule { name, value })
            .collect(),
    })
}

pub(crate) fn decode_separate_rules(bytes: &[u8]) -> Result<Vec<WorldRule>, String> {
    let mut decoded = Vec::new();
    GzDecoder::new(bytes)
        .take(MAX_LEVEL_BYTES + 1)
        .read_to_end(&mut decoded)
        .map_err(|error| format!("Could not read game_rules.dat: {error}"))?;
    if decoded.len() as u64 > MAX_LEVEL_BYTES {
        return Err("game_rules.dat is too large to inspect.".into());
    }
    let root: HashMap<String, Value> = fastnbt::from_bytes(&decoded)
        .map_err(|error| format!("Could not decode game_rules.dat: {error}"))?;
    let rules = root.get("data").and_then(|value| match value {
        Value::Compound(data) => data.get("gamerules"),
        _ => None,
    });
    let Some(Value::Compound(rules)) = rules else {
        return Err("game_rules.dat does not contain gamerules.".into());
    };
    let mut result = Vec::new();
    for (name, value) in rules {
        let text = match value {
            Value::String(text) => Some(text.clone()),
            Value::Byte(value) => Some((*value != 0).to_string()),
            _ => value.as_i64().map(|number| number.to_string()),
        };
        if let Some(value) = text { result.push(WorldRule { name: name.clone(), value }); }
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}

pub(crate) fn checked_local_world(root: &Path, name: &str) -> Result<PathBuf, String> {
    if !valid_world_name(name) {
        return Err("Choose a valid world folder.".into());
    }
    if !fs::symlink_metadata(root.join(name)).is_ok_and(|metadata| metadata.file_type().is_dir()) {
        return Err("The selected world is missing or is not a regular folder.".into());
    }
    let world = resolve_existing(root, name)?;
    if !world.is_dir()
        || !fs::symlink_metadata(world.join("level.dat"))
            .is_ok_and(|metadata| metadata.file_type().is_file())
    {
        return Err("The selected folder is not a world.".into());
    }
    Ok(world)
}

#[tauri::command]
pub fn read_local_world_settings(
    profile_id: Uuid,
    world_name: String,
    profiles: State<'_, ServerProfileStore>,
) -> Result<WorldSettings, String> {
    let root = local_server_root(profile_id, &profiles)?;
    let world = checked_local_world(&root, &world_name)?;
    let level = resolve_existing(&root, &format!("{world_name}/level.dat"))?;
    let metadata = fs::metadata(&level).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_LEVEL_BYTES {
        return Err("level.dat is too large to inspect.".into());
    }
    let mut settings = decode_settings(&fs::read(level).map_err(|error| format!("Could not read level.dat: {error}"))?)?;
    let separate = world.join("data/minecraft/game_rules.dat");
    if fs::symlink_metadata(&separate).is_ok_and(|metadata| metadata.file_type().is_file()) {
        settings.gamerules = decode_separate_rules(&fs::read(separate).map_err(|error| format!("Could not read game_rules.dat: {error}"))?)?;
    }
    Ok(settings)
}

fn require_stopped(
    profile_id: Uuid,
    manager: &State<'_, LocalSessionManager>,
) -> Result<(), String> {
    if !matches!(manager.status(profile_id)?.state, LocalServerState::Stopped) {
        return Err("Stop the server before changing or copying a world.".into());
    }
    Ok(())
}

fn safe_archive_path(relative: &Path) -> Result<PathBuf, String> {
    if relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("The ZIP contains an unsafe path.".into());
    }
    Ok(relative.to_path_buf())
}

#[derive(Clone)]
pub(crate) struct ArchiveEntry {
    pub index: usize,
    pub relative: PathBuf,
    pub directory: bool,
}

pub(crate) fn archive_entries(archive: &mut ZipArchive<File>) -> Result<Vec<ArchiveEntry>, String> {
    if archive.len() > MAX_ARCHIVE_FILES {
        return Err("The ZIP contains too many files.".into());
    }
    let mut raw = Vec::new();
    let mut total = 0_u64;
    let mut root_level = false;
    let mut top_level = None::<String>;
    for index in 0..archive.len() {
        let file = archive.by_index(index).map_err(|error| error.to_string())?;
        if file
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err("The ZIP contains a symbolic link.".into());
        }
        let name = file.name();
        if name.contains('\\') || name.starts_with('/') || name.contains(':') {
            return Err("The ZIP contains an unsafe path.".into());
        }
        let path = safe_archive_path(
            &file
                .enclosed_name()
                .ok_or("The ZIP contains an unsafe path.")?,
        )?;
        if path.components().count() > 64 {
            return Err("The ZIP folder nesting is too deep.".into());
        }
        if path.as_os_str().is_empty() {
            continue;
        }
        let first = path
            .components()
            .next()
            .and_then(|part| part.as_os_str().to_str())
            .ok_or("The ZIP contains an invalid folder name.")?;
        if path == Path::new("level.dat") {
            root_level = true;
        }
        if let Some(top) = &top_level {
            if top != first {
                top_level = Some(String::new());
            }
        } else {
            top_level = Some(first.to_owned());
        }
        total = total
            .checked_add(file.size())
            .ok_or("The ZIP is too large.")?;
        if total > MAX_ARCHIVE_BYTES {
            return Err("The ZIP expands beyond 100 GB.".into());
        }
        raw.push((index, path, file.is_dir()));
    }
    let prefix = if root_level {
        None
    } else {
        let top = top_level
            .filter(|top| !top.is_empty())
            .ok_or("The ZIP must contain one world folder.")?;
        let level = format!("{top}/level.dat");
        if !raw
            .iter()
            .any(|(_, path, directory)| !directory && path == Path::new(&level))
        {
            return Err("The ZIP does not contain level.dat in the world folder.".into());
        }
        Some(top)
    };
    Ok(raw
        .into_iter()
        .filter_map(|(index, path, directory)| {
            let relative = match &prefix {
                Some(prefix) => path.strip_prefix(prefix).ok()?.to_path_buf(),
                None => path,
            };
            if relative.as_os_str().is_empty() {
                None
            } else {
                Some(ArchiveEntry {
                    index,
                    relative,
                    directory,
                })
            }
        })
        .collect())
}

pub(crate) fn open_archive(source: &Path) -> Result<(ZipArchive<File>, Vec<ArchiveEntry>), String> {
    let file = File::open(source).map_err(|error| format!("Could not open the ZIP: {error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("Could not open the ZIP: {error}"))?;
    let entries = archive_entries(&mut archive)?;
    Ok((archive, entries))
}

pub(crate) fn backup_destination(
    app: &AppHandle,
    profile_id: Uuid,
    name: &str,
) -> Result<PathBuf, String> {
    if !valid_world_name(name) {
        return Err("Choose a valid world folder.".into());
    }
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("world-backups")
        .join(profile_id.to_string());
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not create the backup folder: {error}"))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs();
    Ok(directory.join(format!("{name}-{timestamp}-{}.zip", Uuid::new_v4())))
}

fn backup_name(name: &str, zip: bool) -> Option<String> {
    let stem = if zip { name.strip_suffix(".zip")? } else { name };
    // UUIDs contain hyphens, so take the fixed-width suffix instead.
    let id = stem.get(stem.len().checked_sub(36)?..)?;
    Uuid::parse_str(id).ok()?;
    let prefix = stem.strip_suffix(id)?.strip_suffix('-')?;
    let world = if zip { prefix.rsplit_once('-')?.0 } else { prefix };
    valid_world_name(world).then(|| world.to_owned())
}

pub(crate) fn preserved_backup_name(name: &str) -> Option<String> {
    backup_name(name, false)
}

fn modified_seconds(metadata: &fs::Metadata) -> Option<u64> {
    metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok().map(|time| time.as_secs())
}

pub(crate) fn app_world_backups(app: &AppHandle, profile_id: Uuid) -> Result<Vec<WorldBackup>, String> {
    let folder = app.path().app_data_dir().map_err(|error| error.to_string())?
        .join("world-backups").join(profile_id.to_string());
    if !folder.exists() { return Ok(Vec::new()); }
    if !fs::symlink_metadata(&folder).is_ok_and(|metadata| metadata.file_type().is_dir()) {
        return Err("The app world backup path is not a regular folder.".into());
    }
    let mut backups = Vec::new();
    for entry in fs::read_dir(folder).map_err(|error| format!("Could not list world backups: {error}"))? {
        let entry = entry.map_err(|error| format!("Could not read a world backup: {error}"))?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
        if !metadata.file_type().is_file() { continue; }
        let filename = entry.file_name().to_string_lossy().into_owned();
        if let Some(name) = backup_name(&filename, true) {
            backups.push(WorldBackup { name, path: entry.path().to_string_lossy().into_owned(),
                kind: "zip".into(), modified: modified_seconds(&metadata) });
        }
    }
    Ok(backups)
}

#[tauri::command]
pub fn list_local_world_backups(
    app: AppHandle, profile_id: Uuid, profiles: State<'_, ServerProfileStore>,
) -> Result<Vec<WorldBackup>, String> {
    let root = local_server_root(profile_id, &profiles)?;
    let mut backups = app_world_backups(&app, profile_id)?;
    let folder = root.join(".scerite-world-backups");
    if folder.exists() {
        if !fs::symlink_metadata(&folder).is_ok_and(|metadata| metadata.file_type().is_dir()) {
            return Err("The preserved world path is not a regular folder.".into());
        }
        for entry in fs::read_dir(folder).map_err(|error| format!("Could not list preserved worlds: {error}"))? {
            let entry = entry.map_err(|error| format!("Could not read a preserved world: {error}"))?;
            let metadata = fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
            if !metadata.file_type().is_dir() || !fs::symlink_metadata(entry.path().join("level.dat"))
                .is_ok_and(|level| level.file_type().is_file()) { continue; }
            let filename = entry.file_name().to_string_lossy().into_owned();
            if let Some(name) = backup_name(&filename, false) {
                backups.push(WorldBackup { name, path: entry.path().to_string_lossy().into_owned(),
                    kind: "preserved".into(), modified: modified_seconds(&metadata) });
            }
        }
    }
    backups.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(backups)
}

fn zip_local_directory(
    zip: &mut ZipWriter<File>,
    directory: &Path,
    relative: &Path,
    count: &mut usize,
    depth: usize,
) -> Result<(), String> {
    if depth > 64 {
        return Err("The world folder nesting is too deep to archive.".into());
    }
    for entry in
        fs::read_dir(directory).map_err(|error| format!("Could not list world files: {error}"))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        if kind.is_symlink() {
            return Err(
                "The world contains a symbolic link. Remove it before making a ZIP.".into(),
            );
        }
        if entry.file_name().to_string_lossy().contains(['\\', ':']) {
            return Err(
                "The world contains a filename that cannot be stored safely in a ZIP.".into(),
            );
        }
        *count += 1;
        if *count > MAX_ARCHIVE_FILES {
            return Err("The world has too many files to archive.".into());
        }
        let child = relative.join(entry.file_name());
        let name = child
            .to_str()
            .ok_or("A world filename cannot be stored in a ZIP.")?
            .replace('\\', "/");
        if kind.is_dir() {
            zip.add_directory(format!("{name}/"), SimpleFileOptions::default())
                .map_err(|error| error.to_string())?;
            zip_local_directory(zip, &entry.path(), &child, count, depth + 1)?;
        } else if kind.is_file() {
            zip.start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .map_err(|error| error.to_string())?;
            io::copy(
                &mut File::open(entry.path()).map_err(|error| error.to_string())?,
                zip,
            )
            .map_err(|error| format!("Could not archive a world file: {error}"))?;
        }
    }
    Ok(())
}

pub(crate) fn export_local_world(
    world: &Path,
    name: &str,
    destination: &Path,
) -> Result<(), String> {
    if destination
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
        .is_some_and(|parent| parent.starts_with(world))
    {
        return Err("Save the ZIP outside the world folder.".into());
    }
    let temporary = destination.with_extension(format!("{}.part", Uuid::new_v4()));
    let result = (|| {
        let output = File::create_new(&temporary)
            .map_err(|error| format!("Could not create ZIP: {error}"))?;
        let mut zip = ZipWriter::new(output);
        zip.add_directory(format!("{name}/"), SimpleFileOptions::default())
            .map_err(|error| error.to_string())?;
        zip_local_directory(&mut zip, world, Path::new(name), &mut 0, 0)?;
        zip.finish()
            .map_err(|error| format!("Could not finish ZIP: {error}"))?;
        fs::rename(&temporary, destination).map_err(|error| format!("Could not save ZIP: {error}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn preserved_world_path(root: &Path, name: &str) -> Result<PathBuf, String> {
    let backups = root.join(".scerite-world-backups");
    if backups.exists()
        && fs::symlink_metadata(&backups)
            .map_err(|error| error.to_string())?
            .file_type()
            .is_symlink()
    {
        return Err("The world backup folder is a symbolic link.".into());
    }
    fs::create_dir_all(&backups)
        .map_err(|error| format!("Could not create the world backup folder: {error}"))?;
    Ok(backups.join(format!("{name}-{}", Uuid::new_v4())))
}

#[tauri::command]
pub async fn backup_local_world(
    app: AppHandle,
    profile_id: Uuid,
    world_name: String,
    profiles: State<'_, ServerProfileStore>,
    manager: State<'_, LocalSessionManager>,
) -> Result<String, String> {
    require_stopped(profile_id, &manager)?;
    let root = local_server_root(profile_id, &profiles)?;
    let world = checked_local_world(&root, &world_name)?;
    let destination = backup_destination(&app, profile_id, &world_name)?;
    tauri::async_runtime::spawn_blocking(move || {
        export_local_world(&world, &world_name, &destination)?;
        Ok(destination.to_string_lossy().into_owned())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn download_local_world(
    profile_id: Uuid,
    world_name: String,
    destination: String,
    profiles: State<'_, ServerProfileStore>,
    manager: State<'_, LocalSessionManager>,
) -> Result<(), String> {
    require_stopped(profile_id, &manager)?;
    let root = local_server_root(profile_id, &profiles)?;
    let world = checked_local_world(&root, &world_name)?;
    tauri::async_runtime::spawn_blocking(move || {
        export_local_world(&world, &world_name, Path::new(&destination))
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn upload_local_world(
    profile_id: Uuid,
    world_name: String,
    source: String,
    profiles: State<'_, ServerProfileStore>,
    manager: State<'_, LocalSessionManager>,
) -> Result<String, String> {
    require_stopped(profile_id, &manager)?;
    let root = local_server_root(profile_id, &profiles)?;
    let world = checked_local_world(&root, &world_name)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (mut archive, entries) = open_archive(Path::new(&source))?;
        let staging = root.join(format!(".scerite-world-upload-{}", Uuid::new_v4()));
        fs::create_dir(&staging)
            .map_err(|error| format!("Could not create upload folder: {error}"))?;
        let result = (|| {
            for entry in entries {
                let destination = staging.join(&entry.relative);
                if entry.directory {
                    fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
                } else {
                    fs::create_dir_all(destination.parent().ok_or("Invalid ZIP path.")?)
                        .map_err(|error| error.to_string())?;
                    io::copy(
                        &mut archive
                            .by_index(entry.index)
                            .map_err(|error| error.to_string())?,
                        &mut File::create_new(destination).map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| format!("Could not extract the world ZIP: {error}"))?;
                }
            }
            if !staging.join("level.dat").is_file() {
                return Err("The ZIP does not contain level.dat.".into());
            }
            let preserved = preserved_world_path(&root, &world_name)?;
            fs::rename(&world, &preserved)
                .map_err(|error| format!("Could not preserve the current world: {error}"))?;
            if let Err(error) = fs::rename(&staging, &world) {
                let recovery = fs::rename(&preserved, &world)
                    .err()
                    .map(|_| format!(" The previous world remains at {}.", preserved.display()))
                    .unwrap_or_default();
                return Err(format!(
                    "Could not install the uploaded world: {error}.{recovery}"
                ));
            }
            Ok(preserved.to_string_lossy().into_owned())
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
        result
    })
    .await
    .map_err(|error| error.to_string())?
}

fn restore_local_worlds(moved: &[(PathBuf, PathBuf)]) -> String {
    let unresolved: Vec<_> = moved
        .iter()
        .rev()
        .filter_map(|(original, backup)| {
            fs::rename(backup, original)
                .err()
                .map(|_| backup.display().to_string())
        })
        .collect();
    if unresolved.is_empty() {
        String::new()
    } else {
        format!(
            " Recovery needs attention: preserved worlds remain at {}.",
            unresolved.join(", ")
        )
    }
}

#[tauri::command]
pub fn regenerate_local_world(
    profile_id: Uuid,
    world_name: String,
    seed: String,
    level_type: String,
    generate_structures: bool,
    profiles: State<'_, ServerProfileStore>,
    manager: State<'_, LocalSessionManager>,
) -> Result<String, String> {
    require_stopped(profile_id, &manager)?;
    let root = local_server_root(profile_id, &profiles)?;
    let world = checked_local_world(&root, &world_name)?;
    let mut worlds = vec![(world_name.clone(), world)];
    for suffix in ["_nether", "_the_end"] {
        let name = format!("{world_name}{suffix}");
        let path = root.join(&name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_dir() => worlds.push((name, path)),
            Ok(_) => {
                return Err(format!(
                    "The companion world path {name} is not a regular folder."
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Could not inspect {name}: {error}")),
        }
    }
    let (properties, updated) =
        generation_properties(&root, &world_name, &seed, &level_type, generate_structures)?;
    let temporary = root.join(format!(".scerite-properties-{}.tmp", Uuid::new_v4()));
    fs::write(&temporary, updated)
        .map_err(|error| format!("Could not stage server.properties: {error}"))?;
    let mut moved = Vec::new();
    for (name, path) in worlds {
        let preserved = match preserved_world_path(&root, &name) {
            Ok(path) => path,
            Err(error) => {
                let recovery = restore_local_worlds(&moved);
                let _ = fs::remove_file(&temporary);
                return Err(format!("{error}{recovery}"));
            }
        };
        if let Err(error) = fs::rename(&path, &preserved) {
            let recovery = restore_local_worlds(&moved);
            let _ = fs::remove_file(&temporary);
            return Err(format!("Could not preserve {name}: {error}.{recovery}"));
        }
        moved.push((path, preserved));
    }
    if let Err(error) = fs::rename(&temporary, &properties) {
        let _ = fs::remove_file(&temporary);
        let recovery = restore_local_worlds(&moved);
        return Err(format!(
            "Could not save server.properties: {error}.{recovery}"
        ));
    }
    Ok(moved[0].1.to_string_lossy().into_owned())
}

pub(crate) fn generation_values(name: &str, seed: &str, level_type: &str) -> Result<(), String> {
    if !valid_world_name(name) {
        return Err("Choose a valid world folder.".into());
    }
    if seed.len() > 128
        || seed.chars().any(char::is_control)
        || level_type.is_empty()
        || level_type.len() > 128
        || level_type.chars().any(char::is_control)
    {
        return Err("Choose a valid seed and world type without line breaks.".into());
    }
    Ok(())
}

pub(crate) fn updated_properties(
    content: &str,
    name: &str,
    seed: &str,
    level_type: &str,
    structures: bool,
) -> Result<String, String> {
    generation_values(name, seed, level_type)?;
    let values = BTreeMap::from([
        ("level-name", name.to_owned()),
        ("level-seed", seed.to_owned()),
        ("level-type", level_type.to_owned()),
        ("generate-structures", structures.to_string()),
    ]);
    let mut seen = std::collections::HashSet::new();
    let newline = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut lines = Vec::new();
    for line in content.lines() {
        let key = line.split_once('=').map(|(key, _)| key.trim());
        if let Some((key, value)) = key.and_then(|key| values.get_key_value(key)) {
            lines.push(format!("{key}={value}"));
            seen.insert(*key);
        } else {
            lines.push(line.to_owned());
        }
    }
    for (key, value) in values {
        if !seen.contains(key) {
            lines.push(format!("{key}={value}"));
        }
    }
    Ok(format!("{}{}", lines.join(newline), newline))
}

fn generation_properties(
    root: &Path,
    name: &str,
    seed: &str,
    level_type: &str,
    structures: bool,
) -> Result<(PathBuf, String), String> {
    if !fs::symlink_metadata(root.join("server.properties"))
        .is_ok_and(|metadata| metadata.file_type().is_file())
    {
        return Err("server.properties is missing or is not a regular file.".into());
    }
    let properties = resolve_existing(root, "server.properties")?;
    if fs::metadata(&properties)
        .map_err(|error| error.to_string())?
        .len()
        > 1024 * 1024
    {
        return Err("server.properties is too large to edit here.".into());
    }
    let content = fs::read_to_string(&properties)
        .map_err(|error| format!("Could not read server.properties: {error}"))?;
    let active = crate::server_players::world_name(Some(content.as_bytes()))?;
    if active != name {
        return Err("Only the active world can be generated again.".into());
    }
    let updated = updated_properties(&content, name, seed, level_type, structures)?;
    Ok((properties, updated))
}
