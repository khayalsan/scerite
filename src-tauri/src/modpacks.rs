use std::{collections::BTreeMap, fs, io::{Cursor, Read, Write}, path::Path, sync::Arc};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest as _, Sha1};
use sha2::{Digest as Sha2Digest, Sha512};
use tauri::State;
use uuid::Uuid;
use zip::ZipArchive;

use crate::{remote_ssh::{self, RemoteSessionManager}, server_files::local_server_root,
    server_profiles::{ServerProfileStore, ServerTarget}, server_sessions::{LocalServerState, LocalSessionManager},
    vanilla_install::client};

const MAX_ARCHIVE: u64 = 64 * 1024 * 1024;
const MAX_FILE: u64 = 128 * 1024 * 1024;
const MAX_TOTAL: u64 = 256 * 1024 * 1024;
const MAX_FILES: usize = 2000;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackIndex {
    format_version: u32,
    game: String,
    files: Vec<PackFile>,
    dependencies: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackFile {
    path: String,
    hashes: BTreeMap<String, String>,
    downloads: Vec<String>,
    file_size: u64,
    #[serde(default)]
    env: PackEnvironment,
}

#[derive(Default, Deserialize)]
struct PackEnvironment { server: Option<String> }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackPreview {
    archive_hash: String,
    required_files: usize,
    optional_files: usize,
    override_files: usize,
    download_bytes: u64,
    loader_version: String,
}

struct PreparedPack {
    archive_hash: String,
    index: PackIndex,
    archive: Vec<u8>,
    loader_version: String,
    override_files: usize,
}

fn safe_path(path: &str) -> Result<(), String> {
    if path.is_empty() || path.len() > 240 || path.contains('\\') || path.starts_with('/')
        || path.chars().any(|character| matches!(character, ':' | '<' | '>' | '"' | '|' | '?' | '*')) {
        return Err(format!("The pack contains an unsafe path: {path}"));
    }
    for segment in path.split('/') {
        if segment.is_empty() || matches!(segment, "." | "..") || segment.ends_with(['.', ' '])
            || segment.chars().any(char::is_control) {
            return Err(format!("The pack contains an unsafe path: {path}"));
        }
        let stem = segment.split('.').next().unwrap_or_default().to_ascii_uppercase();
        if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9" | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9") {
            return Err(format!("The pack contains an unsafe path: {path}"));
        }
    }
    Ok(())
}

fn safe_download_url(text: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(text).map_err(|_| "The pack has an invalid download URL.")?;
    let host = url.host_str().unwrap_or_default();
    if url.scheme() != "https" || url.port().is_some() || !url.username().is_empty() || url.password().is_some()
        || !matches!(host, "cdn.modrinth.com" | "github.com" | "raw.githubusercontent.com" | "gitlab.com") {
        return Err("The pack contains a download URL outside Modrinth's approved hosts.".into());
    }
    Ok(url)
}

async fn bounded_download(client: &reqwest::Client, url: reqwest::Url, limit: u64) -> Result<Vec<u8>, String> {
    let mut current = url;
    let mut redirects = 0;
    let mut response = loop {
        let reply = client.get(current.clone()).send().await
            .map_err(|error| format!("Could not download a pack file: {error}"))?;
        if reply.status().is_redirection() {
            if redirects == 3 { return Err("The pack download redirected too many times.".into()); }
            let location = reply.headers().get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok()).ok_or("The pack download redirected without a location.")?;
            let next = current.join(location).map_err(|_| "The pack download redirected to an invalid URL.")?;
            let github_asset = current.host_str() == Some("github.com")
                && next.host_str() == Some("release-assets.githubusercontent.com");
            if !github_asset { safe_download_url(next.as_str())?; }
            if next.scheme() != "https" || next.port().is_some() || !next.username().is_empty() || next.password().is_some() {
                return Err("The pack download redirected to an unsafe URL.".into());
            }
            current = next;
            redirects += 1;
            continue;
        }
        break reply.error_for_status().map_err(|error| format!("Pack download failed: {error}"))?;
    };
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| format!("Pack download stopped: {error}"))? {
        if bytes.len() as u64 + chunk.len() as u64 > limit {
            return Err("The pack exceeds the supported download size.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn check_hash(bytes: &[u8], hashes: &BTreeMap<String, String>) -> Result<(), String> {
    if let Some(expected) = hashes.get("sha512") {
        if expected.len() != 128 || hex(&Sha512::digest(bytes)) != expected.to_ascii_lowercase() {
            return Err("A pack file did not match its SHA-512 checksum.".into());
        }
    } else if let Some(expected) = hashes.get("sha1") {
        if expected.len() != 40 || hex(&Sha1::digest(bytes)) != expected.to_ascii_lowercase() {
            return Err("A pack file did not match its SHA-1 checksum.".into());
        }
    } else {
        return Err("A pack file has no supported checksum.".into());
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn read_pack_index(archive: &[u8], game_version: &str, loader: &str) -> Result<(PackIndex, String, usize), String> {
    let mut zip = ZipArchive::new(Cursor::new(archive)).map_err(|_| "The modpack archive is invalid.")?;
    if zip.len() > MAX_FILES + 1 { return Err("The modpack contains too many files.".into()); }
    let index: PackIndex = {
        let mut entry = zip.by_name("modrinth.index.json").map_err(|_| "The pack has no Modrinth index.")?;
        if entry.size() > 2 * 1024 * 1024 { return Err("The pack index is too large.".into()); }
        let mut data = Vec::new();
        entry.read_to_end(&mut data).map_err(|error| format!("Could not read the pack index: {error}"))?;
        serde_json::from_slice(&data).map_err(|_| "The pack index is invalid.")?
    };
    if index.format_version != 1 || index.game != "minecraft" || index.dependencies.get("minecraft").map(String::as_str) != Some(game_version) {
        return Err("The pack format or Minecraft version is incompatible with this server.".into());
    }
    let loader_key = format!("{loader}-loader");
    let loader_version = index.dependencies.get(&loader_key).or_else(|| index.dependencies.get(loader))
        .ok_or("The pack uses a different server loader.")?.clone();
    if index.files.len() > MAX_FILES { return Err("The pack contains too many files.".into()); }
    let mut total = 0_u64;
    for item in &index.files {
        safe_path(&item.path)?;
        if item.file_size > MAX_FILE { return Err("A pack file is too large.".into()); }
        if item.downloads.is_empty() { return Err("A pack file has no download URL.".into()); }
        for url in &item.downloads { safe_download_url(url)?; }
        total = total.checked_add(item.file_size).ok_or("The pack is too large.")?;
    }
    let mut overrides = 0;
    for n in 0..zip.len() {
        let entry = zip.by_index(n).map_err(|_| "The modpack archive is invalid.")?;
        let name = entry.name();
        if name == "modrinth.index.json" || name.ends_with('/') { continue; }
        if let Some(path) = name.strip_prefix("overrides/").or_else(|| name.strip_prefix("server-overrides/")) {
            safe_path(path)?;
            if entry.size() > MAX_FILE || entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) {
                return Err("The pack has an unsupported override file.".into());
            }
            total = total.checked_add(entry.size()).ok_or("The pack is too large.")?;
            overrides += 1;
        }
    }
    if total > MAX_TOTAL { return Err("The pack exceeds the supported 256 MB install size.".into()); }
    Ok((index, loader_version, overrides))
}

async fn prepare_pack(project_id: &str, version_id: &str, game_version: &str, loader: &str) -> Result<PreparedPack, String> {
    if !project_id.bytes().all(|b| b.is_ascii_alphanumeric()) || project_id.len() > 32 || project_id.is_empty()
        || !version_id.bytes().all(|b| b.is_ascii_alphanumeric()) || version_id.len() > 32 || version_id.is_empty()
        || !crate::vanilla_install::safe_version(game_version) || !matches!(loader, "fabric" | "quilt" | "forge" | "neoforge") {
        return Err("Choose a valid pack release and server version.".into());
    }
    let client = client()?;
    let version: Value = client.get(format!("https://api.modrinth.com/v2/version/{version_id}"))
        .send().await.map_err(|error| format!("Could not reach Modrinth: {error}"))?
        .error_for_status().map_err(|error| format!("Modrinth rejected the pack release: {error}"))?
        .json().await.map_err(|_| "Modrinth returned an invalid pack release.")?;
    if version["project_id"].as_str() != Some(project_id)
        || !version["game_versions"].as_array().is_some_and(|items| items.iter().any(|value| value.as_str() == Some(game_version)))
        || !version["loaders"].as_array().is_some_and(|items| items.iter().any(|value| value.as_str() == Some(loader))) {
        return Err("This pack release is not compatible with the server.".into());
    }
    let file = version["files"].as_array().and_then(|items| items.iter().find(|item| item["filename"].as_str().is_some_and(|name| name.ends_with(".mrpack"))))
        .ok_or("This release does not contain a Modrinth modpack.")?;
    let size = file["size"].as_u64().ok_or("The pack has no published size.")?;
    if size > MAX_ARCHIVE { return Err("The modpack archive exceeds 64 MB.".into()); }
    let url = safe_download_url(file["url"].as_str().ok_or("The pack has no download URL.")?)?;
    let archive = bounded_download(&client, url, size).await?;
    if archive.len() as u64 != size { return Err("The pack download size did not match Modrinth.".into()); }
    let expected = file["hashes"]["sha512"].as_str().ok_or("Modrinth did not provide a pack SHA-512 checksum.")?;
    let archive_hash = hex(&Sha512::digest(&archive));
    if expected.len() != 128 || archive_hash != expected.to_ascii_lowercase() {
        return Err("The pack archive did not match Modrinth's checksum.".into());
    }
    let (index, loader_version, override_files) = read_pack_index(&archive, game_version, loader)?;
    Ok(PreparedPack { archive_hash, index, archive, loader_version, override_files })
}

#[tauri::command]
pub async fn preview_modrinth_modpack(project_id: String, version_id: String, game_version: String, loader: String) -> Result<PackPreview, String> {
    let pack = prepare_pack(&project_id, &version_id, &game_version, &loader).await?;
    Ok(PackPreview {
        archive_hash: pack.archive_hash,
        required_files: pack.index.files.iter().filter(|f| f.env.server.as_deref().unwrap_or("required") == "required").count(),
        optional_files: pack.index.files.iter().filter(|f| f.env.server.as_deref() == Some("optional")).count(),
        override_files: pack.override_files,
        download_bytes: pack.index.files.iter().filter(|f| f.env.server.as_deref().unwrap_or("required") != "unsupported").map(|f| f.file_size).sum(),
        loader_version: pack.loader_version,
    })
}

fn local_install(root: &Path, files: Vec<(String, Vec<u8>)>) -> Result<(usize, Vec<String>), String> {
    let mut installed = 0;
    let mut skipped = Vec::new();
    'files: for (name, bytes) in files {
        let mut parent = root.to_path_buf();
        for segment in name.split('/').take(name.split('/').count() - 1) {
            parent.push(segment);
            match fs::symlink_metadata(&parent) {
                Ok(meta) if meta.file_type().is_dir() => {},
                Ok(_) => { skipped.push(name); continue 'files; },
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&parent).map_err(|error| format!("Could not create pack folder: {error}"))?,
                Err(error) => return Err(format!("Could not inspect pack path: {error}")),
            }
        }
        let destination = root.join(&name);
        match fs::OpenOptions::new().write(true).create_new(true).open(&destination) {
            Ok(mut output) => {
                if let Err(error) = output.write_all(&bytes) {
                    let _ = fs::remove_file(&destination);
                    return Err(format!("Could not install {name}: {error}. {installed} files were already added."));
                }
                installed += 1;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => skipped.push(name),
            Err(error) => return Err(format!("Could not install {name}: {error}. {installed} files were already added.")),
        }
    }
    Ok((installed, skipped))
}

#[tauri::command]
pub async fn install_modrinth_modpack(profile_id: Uuid, project_id: String, version_id: String,
    game_version: String, loader: String, expected_archive_hash: String, include_optional: bool,
    profiles: State<'_, ServerProfileStore>, local_sessions: State<'_, LocalSessionManager>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>) -> Result<String, String> {
    let profile = profiles.list()?.into_iter().find(|profile| profile.id == profile_id).ok_or("Server profile was not found.")?;
    if profile.installed_software.as_deref().is_some_and(|software| !software.eq_ignore_ascii_case(&loader))
        || profile.installed_version.as_deref().is_some_and(|version| version != game_version) {
        return Err("The pack does not match this server's installed software and Minecraft version.".into());
    }
    let pack = prepare_pack(&project_id, &version_id, &game_version, &loader).await?;
    if pack.archive_hash != expected_archive_hash { return Err("The pack changed since preview. Review the release again.".into()); }
    let client = client()?;
    let mut files = BTreeMap::<String, Vec<u8>>::new();
    for item in pack.index.files {
        if item.env.server.as_deref() == Some("unsupported") || (item.env.server.as_deref() == Some("optional") && !include_optional) { continue; }
        if !matches!(item.env.server.as_deref().unwrap_or("required"), "required" | "optional") { return Err("The pack has an invalid server environment setting.".into()); }
        if files.contains_key(&item.path) { return Err("The pack declares the same destination twice.".into()); }
        let url = safe_download_url(&item.downloads[0])?;
        let bytes = bounded_download(&client, url, item.file_size).await?;
        if bytes.len() as u64 != item.file_size { return Err(format!("Pack file {} has an unexpected size.", item.path)); }
        check_hash(&bytes, &item.hashes)?;
        files.insert(item.path, bytes);
    }
    let mut zip = ZipArchive::new(Cursor::new(pack.archive)).map_err(|_| "The modpack archive is invalid.")?;
    for prefix in ["overrides/", "server-overrides/"] {
        for n in 0..zip.len() {
            let mut entry = zip.by_index(n).map_err(|_| "The modpack archive is invalid.")?;
            let Some(path) = entry.name().strip_prefix(prefix).map(str::to_owned) else { continue; };
            if path.is_empty() || path.ends_with('/') { continue; }
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(|error| format!("Could not read pack override: {error}"))?;
            files.insert(path, bytes);
        }
    }
    let files: Vec<_> = files.into_iter().collect();
    let (installed, skipped) = match &profile.target {
        ServerTarget::Local { .. } => {
            if !matches!(local_sessions.status(profile_id)?.state, LocalServerState::Stopped) { return Err("Stop the server before installing a modpack.".into()); }
            local_install(&local_server_root(profile_id, &profiles)?, files)?
        }
        ServerTarget::Remote { environment_id, .. } => {
            let environment = profiles.environment(*environment_id)?;
            remote_ssh::install_remote_modpack_files(profile, environment, Arc::clone(remote_sessions.inner()), files).await?
        }
    };
    Ok(format!("Installed {installed} pack files. {} existing paths were preserved{}.", skipped.len(),
        if skipped.is_empty() { String::new() } else { format!(": {}", skipped.join(", ")) }))
}
