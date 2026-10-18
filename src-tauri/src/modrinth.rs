use std::{
    fs,
    io::{Read, Write},
    path::Path,
    sync::Arc,
};

use serde::Serialize;
use serde_json::Value;
use sha1::{Digest as Sha1Digest, Sha1};
use sha2::{Digest, Sha512};
use tauri::State;
use uuid::Uuid;

use crate::{
    remote_ssh::{self, RemoteSessionManager},
    server_profiles::{ServerProfileStore, ServerTarget},
    server_sessions::{LocalServerState, LocalSessionManager},
    vanilla_install::{client, safe_version},
};

const MAX_PACKAGE_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogProject {
    id: String,
    title: String,
    description: String,
    author: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogVersion {
    id: String,
    name: String,
    version_number: String,
    required_dependencies: usize,
}

fn package_folder(kind: &str) -> Result<&'static str, String> {
    match kind {
        "mod" => Ok("mods"),
        "plugin" => Ok("plugins"),
        _ => Err("Choose mods or plugins.".into()),
    }
}

fn valid_loader(kind: &str, loader: &str) -> bool {
    match kind {
        "mod" => matches!(loader, "fabric" | "quilt" | "forge" | "neoforge"),
        "plugin" => matches!(loader, "bukkit" | "spigot" | "paper" | "purpur" | "folia"),
        _ => false,
    }
}

fn catalog_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 32 && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

async fn get_json(client: &reqwest::Client, url: reqwest::Url) -> Result<Value, String> {
    if url.scheme() != "https" || url.host_str() != Some("api.modrinth.com") || url.port().is_some()
    {
        return Err("The catalog address is invalid.".into());
    }
    client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Could not reach Modrinth: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Modrinth rejected the request: {error}"))?
        .json()
        .await
        .map_err(|error| format!("Could not read Modrinth data: {error}"))
}

fn api_url(path: &str) -> Result<reqwest::Url, String> {
    reqwest::Url::parse(&format!("https://api.modrinth.com/v2/{path}"))
        .map_err(|_| "The catalog address is invalid.".into())
}

#[tauri::command]
pub async fn search_modrinth(
    query: String,
    kind: String,
    game_version: String,
    loader: String,
) -> Result<Vec<CatalogProject>, String> {
    package_folder(&kind)?;
    if !valid_loader(&kind, &loader) || !safe_version(&game_version) {
        return Err("Choose a valid Minecraft version and loader.".into());
    }
    let query = query.trim();
    if query.len() < 2 || query.len() > 100 {
        return Err("Enter 2 to 100 characters to search.".into());
    }
    let facets = serde_json::json!([
        [format!("project_type:{kind}")],
        [format!("categories:{loader}")],
        [format!("versions:{game_version}")],
        [
            "environment:server_only",
            "environment:server_only_client_optional",
            "environment:client_and_server",
            "environment:client_or_server",
            "environment:client_or_server_prefers_both",
            "environment:dedicated_server_only"
        ]
    ]);
    let mut url = api_url("search")?;
    url.query_pairs_mut()
        .append_pair("query", query)
        .append_pair("facets", &facets.to_string())
        .append_pair("limit", "20");
    let data = get_json(&client()?, url).await?;
    let hits = data["hits"]
        .as_array()
        .ok_or("Modrinth returned an invalid search response.")?;
    Ok(hits
        .iter()
        .filter(|hit| {
            hit["project_type"].as_str() == Some(kind.as_str())
                && hit["categories"].as_array().is_some_and(|items| items.iter().any(|item| item.as_str() == Some(loader.as_str())))
                && hit["versions"].as_array().is_some_and(|items| items.iter().any(|item| item.as_str() == Some(game_version.as_str())))
        })
        .filter_map(|hit| {
            Some(CatalogProject {
                id: hit["project_id"].as_str()?.to_owned(),
                title: hit["title"].as_str()?.to_owned(),
                description: hit["description"].as_str().unwrap_or_default().to_owned(),
                author: hit["author"].as_str().unwrap_or_default().to_owned(),
            })
        })
        .collect())
}

fn versions_url(
    project_id: &str,
    game_version: &str,
    loader: &str,
) -> Result<reqwest::Url, String> {
    if !catalog_id(project_id) || !safe_version(game_version) {
        return Err("The selected project or Minecraft version is invalid.".into());
    }
    let mut url = api_url(&format!("project/{project_id}/version"))?;
    url.query_pairs_mut()
        .append_pair("loaders", &serde_json::json!([loader]).to_string())
        .append_pair(
            "game_versions",
            &serde_json::json!([game_version]).to_string(),
        )
        .append_pair("include_changelog", "false");
    Ok(url)
}

#[tauri::command]
pub async fn list_modrinth_versions(
    project_id: String,
    kind: String,
    game_version: String,
    loader: String,
) -> Result<Vec<CatalogVersion>, String> {
    package_folder(&kind)?;
    if !valid_loader(&kind, &loader) {
        return Err("Choose a compatible loader.".into());
    }
    let data = get_json(
        &client()?,
        versions_url(&project_id, &game_version, &loader)?,
    )
    .await?;
    let versions = data
        .as_array()
        .ok_or("Modrinth returned an invalid version list.")?;
    Ok(versions
        .iter()
        .filter(|item| {
            item["environment"].as_str() != Some("client_only")
                && item["environment"].as_str() != Some("singleplayer_only")
        })
        .filter_map(|item| {
            Some(CatalogVersion {
                id: item["id"].as_str()?.to_owned(),
                name: item["name"].as_str().unwrap_or_default().to_owned(),
                version_number: item["version_number"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                required_dependencies: item["dependencies"].as_array().map_or(0, |items| {
                    items
                        .iter()
                        .filter(|dependency| dependency["dependency_type"] == "required")
                        .count()
                }),
            })
        })
        .collect())
}

fn safe_filename(original: &str, project_id: &str, version_id: &str) -> String {
    if original.len() <= 120
        && !original.starts_with('.')
        && original.ends_with(".jar")
        && original
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'+'))
    {
        original.to_owned()
    } else {
        format!("{project_id}-{version_id}.jar")
    }
}

async fn download_package(
    client: &reqwest::Client,
    file: &Value,
    path: &Path,
) -> Result<String, String> {
    let url = file["url"]
        .as_str()
        .ok_or("Modrinth did not provide a download URL.")?;
    let url = reqwest::Url::parse(url).map_err(|_| "The package download URL is invalid.")?;
    if url.scheme() != "https" || url.host_str() != Some("cdn.modrinth.com") || url.port().is_some()
        || !url.username().is_empty() || url.password().is_some()
    {
        return Err("The package download is not hosted on the approved Modrinth CDN.".into());
    }
    let expected = file["hashes"]["sha512"]
        .as_str()
        .ok_or("Modrinth did not provide a SHA-512 checksum.")?;
    if expected.len() != 128 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Modrinth provided an invalid checksum.".into());
    }
    let expected_size = file["size"]
        .as_u64()
        .ok_or("Modrinth did not provide a file size.")?;
    if expected_size < 1024 || expected_size > MAX_PACKAGE_BYTES {
        return Err("The package is outside the supported size range (1 KB–128 MB).".into());
    }
    let mut reply = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Could not download package: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Modrinth rejected the package download: {error}"))?;
    let mut output = fs::File::create_new(path)
        .map_err(|error| format!("Could not create temporary package file: {error}"))?;
    let mut sha512 = Sha512::new();
    let mut sha1 = Sha1::new();
    let mut total = 0_u64;
    while let Some(chunk) = reply
        .chunk()
        .await
        .map_err(|error| format!("Package download stopped: {error}"))?
    {
        total += chunk.len() as u64;
        if total > expected_size {
            return Err("The package download exceeded its published size.".into());
        }
        output
            .write_all(&chunk)
            .map_err(|error| format!("Could not save package: {error}"))?;
        sha512.update(&chunk);
        sha1.update(&chunk);
    }
    output
        .sync_all()
        .map_err(|error| format!("Could not finish package download: {error}"))?;
    if total != expected_size || format!("{:x}", sha512.finalize()) != expected.to_ascii_lowercase()
    {
        return Err("The package did not match Modrinth's published size and checksum.".into());
    }
    let mut magic = [0; 2];
    fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut magic))
        .map_err(|error| format!("Could not inspect package: {error}"))?;
    if magic != *b"PK" {
        return Err("The downloaded package is not a JAR file.".into());
    }
    Ok(sha1
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn install_local_package(
    profiles: &ServerProfileStore,
    profile_id: Uuid,
    folder: &str,
    filename: &str,
    source: &Path,
) -> Result<(), String> {
    let profile = profiles
        .list()?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or("Server profile was not found.")?;
    let ServerTarget::Local { directory, .. } = profile.target else {
        return Err("This is not a local server.".into());
    };
    let root =
        fs::canonicalize(&directory).map_err(|_| "The server folder could not be opened.")?;
    let target = root.join(folder);
    fs::create_dir_all(&target)
        .map_err(|error| format!("Could not create {folder} folder: {error}"))?;
    if fs::canonicalize(&target)
        .map_err(|error| format!("Could not inspect {folder} folder: {error}"))?
        != target
    {
        return Err("The package folder is a symbolic link or leaves the server folder.".into());
    }
    let destination = target.join(filename);
    let temporary = target.join(format!(".servercrafter-{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut input =
            fs::File::open(source).map_err(|error| format!("Could not read package: {error}"))?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("Could not stage package: {error}"))?;
        std::io::copy(&mut input, &mut output)
            .map_err(|error| format!("Could not stage package: {error}"))?;
        output
            .sync_all()
            .map_err(|error| format!("Could not finish staging package: {error}"))?;
        fs::hard_link(&temporary, &destination).map_err(|error| {
            format!("Could not install {filename} (an existing file is never replaced): {error}")
        })
    })();
    let _ = fs::remove_file(&temporary);
    result
}

#[tauri::command]
pub async fn install_modrinth_version(
    profile_id: Uuid,
    kind: String,
    game_version: String,
    loader: String,
    project_id: String,
    version_id: String,
    profiles: State<'_, ServerProfileStore>,
    local_sessions: State<'_, LocalSessionManager>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<String, String> {
    let folder = package_folder(&kind)?;
    if !valid_loader(&kind, &loader)
        || !safe_version(&game_version)
        || !catalog_id(&project_id)
        || !catalog_id(&version_id)
    {
        return Err("The selected catalog version is invalid.".into());
    }
    let profile = profiles
        .list()?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or("Server profile was not found.")?;
    profile.target.validate()?;
    if profile.installed_version.as_deref().is_some_and(|version| version != game_version) {
        return Err("This package release does not match the server's installed Minecraft version.".into());
    }
    if let Some(software) = profile.installed_software.as_deref() {
        let supported_loader = if let Some(variant) = software.strip_prefix("arclight-") {
            if kind == "plugin" { Some("bukkit") } else { Some(variant) }
        } else if kind == "mod" && matches!(software, "fabric" | "quilt" | "forge" | "neoforge") {
            Some(software)
        } else if kind == "plugin" && matches!(software, "paper" | "purpur" | "spigot" | "bukkit" | "folia") {
            Some(software)
        } else {
            None
        };
        if supported_loader != Some(loader.as_str()) {
            return Err("This package type is not supported by the installed server software.".into());
        }
    }
    if matches!(profile.target, ServerTarget::Local { .. })
        && !matches!(
            local_sessions.status(profile_id)?.state,
            LocalServerState::Stopped
        )
    {
        return Err("Stop the server before installing packages.".into());
    }
    let client = client()?;
    let project = get_json(&client, api_url(&format!("project/{project_id}"))?).await?;
    if project["id"].as_str() != Some(&project_id)
        || project["project_type"].as_str() != Some(&kind)
    {
        return Err("The selected project is not the requested package type.".into());
    }
    let versions = get_json(&client, versions_url(&project_id, &game_version, &loader)?).await?;
    let version = versions.as_array().and_then(|items| items.iter().find(|item| item["id"].as_str() == Some(&version_id)))
        .ok_or("This package version is not compatible with the selected Minecraft version and loader.")?;
    if matches!(
        version["environment"].as_str(),
        Some("client_only" | "singleplayer_only")
    ) {
        return Err("This package version cannot run on a server.".into());
    }
    let files = version["files"]
        .as_array()
        .ok_or("Modrinth did not provide package files.")?;
    let file = files
        .iter()
        .find(|file| {
            file["primary"] == true
                && file["filename"]
                    .as_str()
                    .is_some_and(|name| name.ends_with(".jar"))
        })
        .or_else(|| {
            files.iter().find(|file| {
                file["filename"]
                    .as_str()
                    .is_some_and(|name| name.ends_with(".jar"))
            })
        })
        .ok_or("This version does not contain an installable JAR file.")?;
    let filename = safe_filename(
        file["filename"].as_str().unwrap_or_default(),
        &project_id,
        &version_id,
    );
    if let ServerTarget::Remote { environment_id, .. } = &profile.target {
        let url = file["url"].as_str().ok_or("Modrinth did not provide a download URL.")?;
        let parsed = reqwest::Url::parse(url).map_err(|_| "The package download URL is invalid.")?;
        if parsed.scheme() != "https" || parsed.host_str() != Some("cdn.modrinth.com") || parsed.port().is_some()
            || !parsed.username().is_empty() || parsed.password().is_some() {
            return Err("The package download is not hosted on the approved Modrinth CDN.".into());
        }
        let sha512 = file["hashes"]["sha512"].as_str().ok_or("Modrinth did not provide a SHA-512 checksum.")?;
        if sha512.len() != 128 || !sha512.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("Modrinth provided an invalid checksum.".into());
        }
        let size = file["size"].as_u64().ok_or("Modrinth did not provide a file size.")?;
        if size < 1024 || size > MAX_PACKAGE_BYTES {
            return Err("The package is outside the supported size range (1 KB–128 MB).".into());
        }
        let environment = profiles.environment(*environment_id)?;
        let direct = remote_ssh::install_remote_package(
            profile.clone(), environment, Arc::clone(remote_sessions.inner()), folder.to_owned(),
            filename.clone(), None, String::new(), url.to_owned(), Some(sha512.to_owned()),
        ).await;
        match direct {
            Ok(()) => {
                let dependencies = version["dependencies"].as_array().map_or(0, |items| items.iter().filter(|item| item["dependency_type"] == "required").count());
                return if dependencies > 0 {
                    Ok(format!("Installed {filename}. This version lists {dependencies} required dependencies; install those before starting the server."))
                } else { Ok(format!("Installed {filename}.")) };
            }
            Err(error) if error.starts_with(remote_ssh::REMOTE_DOWNLOAD_UNAVAILABLE) => {}
            Err(error) => return Err(error),
        }
    }
    let temporary =
        std::env::temp_dir().join(format!("servercrafter-package-{}.jar", Uuid::new_v4()));
    let result = async {
        let sha1 = download_package(&client, file, &temporary).await?;
        match profile.target {
            ServerTarget::Local { .. } => {
                if !matches!(local_sessions.status(profile_id)?.state, LocalServerState::Stopped) {
                    return Err("The server started during the download. Stop it before installing packages.".into());
                }
                install_local_package(&profiles, profile_id, folder, &filename, &temporary)
            }
            ServerTarget::Remote { environment_id, .. } => {
                let environment = profiles.environment(environment_id)?;
                remote_ssh::install_remote_package(profile, environment, Arc::clone(remote_sessions.inner()), folder.to_owned(), filename.clone(), Some(temporary.clone()), sha1, file["url"].as_str().unwrap_or_default().to_owned(), None).await
            }
        }
    }.await;
    let _ = fs::remove_file(&temporary);
    result?;
    let dependencies = version["dependencies"].as_array().map_or(0, |items| {
        items
            .iter()
            .filter(|item| item["dependency_type"] == "required")
            .count()
    });
    if dependencies > 0 {
        Ok(format!("Installed {filename}. This version lists {dependencies} required dependencies; install those before starting the server."))
    } else {
        Ok(format!("Installed {filename}."))
    }
}
