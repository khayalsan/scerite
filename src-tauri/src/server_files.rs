use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::server_profiles::{ServerProfileStore, ServerTarget};
use crate::server_sessions::{LocalServerState, LocalSessionManager};

const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 1000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerFileEntry {
    pub name: String,
    pub path: String,
    pub directory: bool,
    pub size: u64,
}

#[tauri::command]
pub fn list_local_server_files(
    profile_id: Uuid,
    relative_path: String,
    profiles: State<'_, ServerProfileStore>,
) -> Result<Vec<ServerFileEntry>, String> {
    let root = local_server_root(profile_id, &profiles)?;
    let directory = resolve_existing(&root, &relative_path)?;
    if !directory.is_dir() {
        return Err("The selected server path is not a folder.".into());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(&directory)
        .map_err(|error| format!("Could not read the server folder: {error}"))?
    {
        let entry =
            entry.map_err(|error| format!("Could not read a server file entry: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("Could not inspect a server file: {error}"))?;
        if file_type.is_symlink() {
            continue;
        }
        if entries.len() == MAX_DIRECTORY_ENTRIES {
            return Err(format!(
                "This folder has more than {MAX_DIRECTORY_ENTRIES} entries; open a subfolder to continue."
            ));
        }
        let metadata = entry
            .metadata()
            .map_err(|error| format!("Could not inspect a server file: {error}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let child_relative = if relative_path.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", relative_path.trim_end_matches('/'), name)
        };
        entries.push(ServerFileEntry {
            name,
            path: child_relative,
            directory: file_type.is_dir(),
            size: metadata.len(),
        });
    }
    entries.sort_by(|left, right| {
        right
            .directory
            .cmp(&left.directory)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(entries)
}

#[tauri::command]
pub fn read_local_server_file(
    profile_id: Uuid,
    relative_path: String,
    profiles: State<'_, ServerProfileStore>,
) -> Result<String, String> {
    let root = local_server_root(profile_id, &profiles)?;
    let path = resolve_existing(&root, &relative_path)?;
    if !path.is_file() {
        return Err("The selected server path is not a file.".into());
    }
    let metadata = fs::metadata(&path)
        .map_err(|error| format!("Could not inspect the server file: {error}"))?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err("Files larger than 1 MB cannot be opened in the editor.".into());
    }
    let bytes =
        fs::read(path).map_err(|error| format!("Could not read the server file: {error}"))?;
    String::from_utf8(bytes)
        .map_err(|_| "This file is not UTF-8 text and cannot be opened in the editor.".into())
}

#[tauri::command]
pub fn write_local_server_file(
    profile_id: Uuid,
    relative_path: String,
    content: String,
    overwrite: bool,
    profiles: State<'_, ServerProfileStore>,
    manager: State<'_, LocalSessionManager>,
) -> Result<(), String> {
    if !matches!(manager.status(profile_id)?.state, LocalServerState::Stopped) {
        return Err("Stop the server before editing its files.".into());
    }
    if content.len() as u64 > MAX_FILE_BYTES {
        return Err("Files larger than 1 MB cannot be edited here.".into());
    }
    let root = local_server_root(profile_id, &profiles)?;
    let path = resolve_for_write(&root, &relative_path)?;
    if path.exists() && !overwrite {
        return Err("The file already exists. Confirm overwrite before saving.".into());
    }
    fs::write(path, content).map_err(|error| format!("Could not save the server file: {error}"))
}

fn local_server_root(
    profile_id: Uuid,
    profiles: &State<'_, ServerProfileStore>,
) -> Result<PathBuf, String> {
    let profile = profiles
        .list()?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| "Server profile was not found.".to_owned())?;
    let ServerTarget::Local { directory, .. } = profile.target else {
        return Err("File access is only available for local server profiles.".into());
    };
    Path::new(&directory)
        .canonicalize()
        .map_err(|_| "The server folder could not be found or opened.".to_owned())
}

fn resolve_existing(root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let relative = safe_relative_path(relative_path)?;
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|_| "The requested server file or folder could not be found.".to_owned())?;
    ensure_inside(root, &path)?;
    Ok(path)
}

fn resolve_for_write(root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let relative = safe_relative_path(relative_path)?;
    if relative.as_os_str().is_empty() {
        return Err("Choose a file inside the server folder.".into());
    }
    let path = root.join(relative);
    let parent = path
        .parent()
        .ok_or_else(|| "Choose a file inside the server folder.".to_owned())?
        .canonicalize()
        .map_err(|_| "The parent folder could not be found or opened.".to_owned())?;
    ensure_inside(root, &parent)?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err("Saving through a symbolic link is not allowed.".into());
            }
            let canonical = path
                .canonicalize()
                .map_err(|_| "The server file could not be opened.".to_owned())?;
            ensure_inside(root, &canonical)?;
            if !canonical.is_file() {
                return Err("The selected server path is not a file.".into());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Could not inspect the server file: {error}")),
    }
    Ok(path)
}

fn safe_relative_path(path: &str) -> Result<PathBuf, String> {
    let relative = Path::new(path);
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err("The requested path must stay inside the server folder.".into());
    }
    Ok(relative.to_path_buf())
}

fn ensure_inside(root: &Path, path: &Path) -> Result<(), String> {
    if path.starts_with(root) {
        Ok(())
    } else {
        Err("The requested path must stay inside the server folder.".into())
    }
}
