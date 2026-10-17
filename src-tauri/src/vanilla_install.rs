use std::{
    fs,
    io::{Read, Write},
    path::Path,
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tauri::{Emitter, State};
use uuid::Uuid;

use crate::{
    remote_ssh::{self, RemoteSessionManager},
    server_profiles::{ServerProfileStore, ServerTarget},
    server_sessions::{LocalServerState, LocalSessionManager},
};

const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

fn client() -> Result<reqwest::Client, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| format!("Could not initialize Minecraft downloads: {error}"))
}

#[derive(Deserialize)]
struct Manifest {
    versions: Vec<ManifestVersion>,
}

#[derive(Deserialize)]
struct ManifestVersion {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    url: String,
}

#[derive(Deserialize)]
struct VersionMetadata {
    downloads: VersionDownloads,
}

#[derive(Deserialize)]
struct VersionDownloads {
    server: Option<ServerDownload>,
}

#[derive(Deserialize)]
struct ServerDownload {
    url: String,
    sha1: String,
    size: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallProgress {
    profile_id: Uuid,
    phase: &'static str,
    downloaded: u64,
    total: u64,
}

fn progress(
    app: &tauri::AppHandle,
    profile_id: Uuid,
    phase: &'static str,
    downloaded: u64,
    total: u64,
) {
    let _ = app.emit(
        "vanilla-install-progress",
        InstallProgress {
            profile_id,
            phase,
            downloaded,
            total,
        },
    );
}

fn official_url(value: &str, host: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(value)
        .map_err(|_| "The Minecraft download URL is invalid.".to_owned())?;
    if url.scheme() != "https"
        || url.host_str() != Some(host)
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("The Minecraft download URL is not an official Mojang HTTPS URL.".into());
    }
    Ok(())
}

async fn manifest(client: &reqwest::Client) -> Result<Manifest, String> {
    client
        .get(MANIFEST_URL)
        .send()
        .await
        .map_err(|error| format!("Could not load Minecraft versions: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Could not load Minecraft versions: {error}"))?
        .json()
        .await
        .map_err(|error| format!("Could not read Minecraft versions: {error}"))
}

#[tauri::command]
pub async fn list_vanilla_versions() -> Result<Vec<String>, String> {
    let client = client()?;
    Ok(manifest(&client)
        .await?
        .versions
        .into_iter()
        .filter(|version| version.kind == "release" && safe_version(&version.id))
        .map(|version| version.id)
        .collect())
}

fn safe_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 48
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

fn checksum_matches(digest: Sha1, expected: &str) -> bool {
    let actual: String = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    actual.eq_ignore_ascii_case(expected)
}

async fn server_download(
    client: &reqwest::Client,
    version_id: &str,
) -> Result<ServerDownload, String> {
    if !safe_version(version_id) {
        return Err("Choose a valid Minecraft version.".into());
    }
    let version = manifest(client)
        .await?
        .versions
        .into_iter()
        .find(|version| version.id == version_id && version.kind == "release")
        .ok_or_else(|| {
            "That Minecraft release is not available in the official manifest.".to_owned()
        })?;
    official_url(&version.url, "piston-meta.mojang.com")?;
    let metadata: VersionMetadata = client
        .get(version.url)
        .send()
        .await
        .map_err(|error| format!("Could not load version details: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Could not load version details: {error}"))?
        .json()
        .await
        .map_err(|error| format!("Could not read version details: {error}"))?;
    let server = metadata
        .downloads
        .server
        .ok_or_else(|| "This release has no downloadable server jar.".to_owned())?;
    official_url(&server.url, "piston-data.mojang.com")?;
    if server.size == 0
        || server.size > 512 * 1024 * 1024
        || server.sha1.len() != 40
        || !server.sha1.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("The official server download metadata is invalid.".into());
    }
    Ok(server)
}

fn verified_file(path: &Path, download: &ServerDownload) -> Result<bool, String> {
    let Ok(mut file) = fs::File::open(path) else {
        return Ok(false);
    };
    let mut digest = Sha1::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 65536];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("Could not read cached server jar: {error}"))?;
        if count == 0 {
            break;
        }
        size += count as u64;
        if size > download.size {
            return Ok(false);
        }
        digest.update(&buffer[..count]);
    }
    Ok(size == download.size && checksum_matches(digest, &download.sha1))
}

async fn download_jar(
    client: &reqwest::Client,
    app: &tauri::AppHandle,
    profile_id: Uuid,
    destination: &Path,
    download: &ServerDownload,
) -> Result<(), String> {
    if verified_file(destination, download)? {
        progress(app, profile_id, "cached", download.size, download.size);
        return Ok(());
    }
    if fs::symlink_metadata(destination).is_ok() {
        return Err("The cached server jar has a different checksum. Remove that cache file before retrying.".into());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create Minecraft cache: {error}"))?;
    }
    let temporary = destination.with_file_name(format!(".servercrafter-{}.tmp", Uuid::new_v4()));
    let result = async {
        let mut response = client
            .get(&download.url)
            .send()
            .await
            .map_err(|error| format!("Could not download server jar: {error}"))?
            .error_for_status()
            .map_err(|error| format!("Could not download server jar: {error}"))?;
        let mut file = fs::File::create_new(&temporary)
            .map_err(|error| format!("Could not create cache file: {error}"))?;
        let mut digest = Sha1::new();
        let mut size = 0_u64;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| format!("Server jar download stopped: {error}"))?
        {
            size += chunk.len() as u64;
            if size > download.size {
                return Err("The downloaded server jar is larger than expected.".into());
            }
            file.write_all(&chunk)
                .map_err(|error| format!("Could not save server jar: {error}"))?;
            digest.update(&chunk);
            progress(app, profile_id, "downloading", size, download.size);
        }
        file.sync_all()
            .map_err(|error| format!("Could not finish caching server jar: {error}"))?;
        if size != download.size || !checksum_matches(digest, &download.sha1) {
            return Err("The server jar did not match Mojang's checksum. Try again.".into());
        }
        fs::rename(&temporary, destination)
            .map_err(|error| format!("Could not finish caching server jar: {error}"))?;
        Ok(())
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn install_local(
    root: &Path,
    directory: &str,
    cache: &Path,
    profile_id: Uuid,
    sessions: &LocalSessionManager,
) -> Result<(), String> {
    if !matches!(
        sessions.status(profile_id)?.state,
        LocalServerState::Stopped
    ) {
        return Err("Stop this server before installing a version.".into());
    }
    let servers = root.join("servers");
    fs::create_dir_all(&servers)
        .map_err(|error| format!("Could not create servers folder: {error}"))?;
    let servers = fs::canonicalize(&servers)
        .map_err(|error| format!("Could not open servers folder: {error}"))?;
    if servers.parent() != Some(root) {
        return Err("The servers folder resolves outside the configured Minecraft root.".into());
    }
    let requested = Path::new(directory);
    let parent = requested
        .parent()
        .and_then(|path| fs::canonicalize(path).ok())
        .ok_or_else(|| {
            "The server folder must be inside the configured Minecraft root/servers folder."
                .to_owned()
        })?;
    if parent != servers {
        return Err("The server folder must be directly inside this computer's Minecraft root/servers folder.".into());
    }
    if !requested.exists() {
        fs::create_dir(requested)
            .map_err(|error| format!("Could not create server folder: {error}"))?;
    }
    let target = fs::canonicalize(requested)
        .map_err(|error| format!("Could not open server folder: {error}"))?;
    if target.parent() != Some(servers.as_path()) {
        return Err("The server folder must be directly inside this computer's Minecraft root/servers folder.".into());
    }
    let destination = target.join("server.jar");
    if destination.exists() || fs::symlink_metadata(&destination).is_ok() {
        return Err("server.jar already exists. Existing server files are not overwritten.".into());
    }
    let temporary = target.join(format!(".servercrafter-{}.tmp", Uuid::new_v4()));
    fs::copy(cache, &temporary)
        .map_err(|error| format!("Could not copy server jar into the server folder: {error}"))?;
    let result = fs::hard_link(&temporary, &destination).map_err(|error| {
        format!("Could not install server.jar (existing files are preserved): {error}")
    });
    let _ = fs::remove_file(&temporary);
    result
}

#[tauri::command]
pub async fn install_vanilla_version(
    app: tauri::AppHandle,
    profile_id: Uuid,
    version_id: String,
    profiles: State<'_, ServerProfileStore>,
    local_sessions: State<'_, LocalSessionManager>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<String, String> {
    let profile = profiles
        .list()?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| "Server profile was not found.".to_owned())?;
    profile.target.validate()?;
    progress(&app, profile_id, "checking", 0, 0);
    let client = client()?;
    let download = server_download(&client, &version_id).await?;
    match &profile.target {
        ServerTarget::Local { directory, .. } => {
            let root = profiles.local_root()?.ok_or_else(|| {
                "Set this computer's Minecraft root on the Servers page first.".to_owned()
            })?;
            let root = fs::canonicalize(&root).map_err(|_| {
                "Create the configured Minecraft root folder before installing a version."
                    .to_owned()
            })?;
            let cache = root
                .join("cache")
                .join("minecraft")
                .join(&version_id)
                .join("server.jar");
            fs::create_dir_all(cache.parent().expect("cache jar has a parent"))
                .map_err(|error| format!("Could not create Minecraft cache: {error}"))?;
            let cache_parent = fs::canonicalize(cache.parent().expect("cache jar has a parent"))
                .map_err(|error| format!("Could not resolve Minecraft cache: {error}"))?;
            if !cache_parent.starts_with(root.join("cache")) {
                return Err("The Minecraft cache resolves outside the configured root.".into());
            }
            download_jar(&client, &app, profile_id, &cache, &download).await?;
            progress(&app, profile_id, "installing", download.size, download.size);
            install_local(&root, directory, &cache, profile_id, &local_sessions)?;
        }
        ServerTarget::Remote {
            environment_id,
            directory,
            ..
        } => {
            let environment = profiles.environment(*environment_id)?;
            if environment.root_directory.is_empty() {
                return Err("Set this remote environment's Minecraft root first.".into());
            }
            let temporary =
                std::env::temp_dir().join(format!("servercrafter-{}.jar", Uuid::new_v4()));
            let result = async {
                download_jar(&client, &app, profile_id, &temporary, &download).await?;
                progress(&app, profile_id, "uploading", download.size, download.size);
                remote_ssh::install_remote_jar(
                    profile.clone(),
                    environment,
                    Arc::clone(remote_sessions.inner()),
                    directory.clone(),
                    temporary.clone(),
                    version_id.clone(),
                    download.sha1.clone(),
                )
                .await
            }
            .await;
            let _ = fs::remove_file(&temporary);
            result?;
        }
    }
    progress(&app, profile_id, "complete", download.size, download.size);
    Ok(format!("Minecraft {version_id} installed as server.jar."))
}
