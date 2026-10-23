use std::{
    collections::{HashMap, HashSet, VecDeque},
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
    server_profiles::{InstalledPackage, ServerProfile, ServerProfileStore, ServerTarget},
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
    icon_url: Option<String>,
    downloads: u64,
    badges: Vec<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogVersion {
    id: String,
    name: String,
    version_number: String,
    required_dependencies: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPlanEntry {
    project_id: String,
    version_id: String,
    title: String,
    version_number: String,
    filename: String,
    required_by: Vec<String>,
    already_installed: bool,
    direct: bool,
}

struct ResolvedPackage {
    entry: InstallPlanEntry,
    file: Value,
    requires: Vec<String>,
}

struct DependencyRequest {
    project_id: Option<String>,
    version_id: Option<String>,
    parent_id: Option<String>,
    parent_title: Option<String>,
}

fn package_folder(kind: &str) -> Result<&'static str, String> {
    match kind {
        "mod" => Ok("mods"),
        "plugin" => Ok("plugins"),
        _ => Err("Choose mods or plugins.".into()),
    }
}

fn catalog_kind(kind: &str) -> bool {
    matches!(kind, "mod" | "plugin" | "modpack")
}

fn environment_badge(environment: &str) -> Option<&'static str> {
    match environment {
        "client_and_server" => Some("required"),
        "server_only_client_optional" | "client_or_server" | "client_or_server_prefers_both" => Some("optional"),
        "server_only" | "dedicated_server_only" => Some("server"),
        _ => None,
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
    index: String,
    badge_filter: Option<String>,
) -> Result<Vec<CatalogProject>, String> {
    if !catalog_kind(&kind) || !valid_loader(if kind == "modpack" { "mod" } else { &kind }, &loader) || !safe_version(&game_version) {
        return Err("Choose a valid Minecraft version and loader.".into());
    }
    let badge_filter = badge_filter.as_deref().unwrap_or("all");
    if !matches!(badge_filter, "all" | "required" | "optional" | "server") || (kind != "mod" && badge_filter != "all") {
        return Err("Choose a valid mod badge filter.".into());
    }
    let query = query.trim();
    if query.len() > 100 {
        return Err("Search terms must be at most 100 characters long.".into());
    }
    if !matches!(index.as_str(), "relevance" | "downloads" | "updated") {
        return Err("Choose a valid search order.".into());
    }
    let mut facets = vec![
        vec![format!("all_project_types:{kind}")],
        vec![format!("categories:{loader}")],
        vec![format!("versions:{game_version}")],
    ];
    if kind == "mod" || kind == "modpack" {
        let environments: &[&str] = match badge_filter {
            "required" => &["client_and_server"],
            "optional" => &["server_only_client_optional", "client_or_server", "client_or_server_prefers_both"],
            "server" => &["server_only", "dedicated_server_only"],
            _ => &["server_only", "server_only_client_optional", "client_and_server", "client_or_server", "client_or_server_prefers_both", "dedicated_server_only"],
        };
        facets.push(environments.iter().map(|value| format!("environment:{value}")).collect());
    }
    let mut url = api_url("search")?;
    url.query_pairs_mut()
        .append_pair("query", query)
        .append_pair("facets", &serde_json::json!(facets).to_string())
        .append_pair("index", &index)
        .append_pair("limit", "40");
    let data = get_json(&client()?, url).await?;
    let hits = data["hits"]
        .as_array()
        .ok_or("Modrinth returned an invalid search response.")?;
    Ok(hits
        .iter()
        .filter(|hit| {
            hit["all_project_types"].as_array().is_some_and(|items| items.iter().any(|item| item.as_str() == Some(kind.as_str())))
                && hit["categories"].as_array().is_some_and(|items| items.iter().any(|item| item.as_str() == Some(loader.as_str())))
                && hit["versions"].as_array().is_some_and(|items| items.iter().any(|item| item.as_str() == Some(game_version.as_str())))
        })
        .filter_map(|hit| {
            Some(CatalogProject {
                id: hit["project_id"].as_str()?.to_owned(),
                title: hit["title"].as_str()?.to_owned(),
                description: hit["description"].as_str().unwrap_or_default().to_owned(),
                author: hit["author"].as_str().unwrap_or_default().to_owned(),
                icon_url: hit["icon_url"].as_str().filter(|url| url.starts_with("https://cdn.modrinth.com/")).map(str::to_owned),
                downloads: hit["downloads"].as_u64().unwrap_or(0),
                badges: ["required", "optional", "server"].into_iter().filter(|badge| {
                    hit["environment"].as_array().is_some_and(|values| values.iter()
                        .any(|value| value.as_str().and_then(environment_badge) == Some(*badge)))
                }).collect(),
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
    if !catalog_kind(&kind) || !valid_loader(if kind == "modpack" { "mod" } else { &kind }, &loader) {
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

async fn package_names(
    profile: &ServerProfile,
    folder: &str,
    profiles: &ServerProfileStore,
    remote_sessions: &Arc<RemoteSessionManager>,
) -> Result<HashSet<String>, String> {
    let names = match &profile.target {
        ServerTarget::Local { directory, .. } => {
            let root = fs::canonicalize(directory).map_err(|_| "The server folder could not be opened.")?;
            let path = root.join(folder);
            if !path.exists() { Vec::new() } else {
                if !path.is_dir() || fs::canonicalize(&path).map_err(|error| error.to_string())? != path {
                    return Err("The package folder is not a regular folder inside this server.".into());
                }
                fs::read_dir(path).map_err(|error| format!("Could not list installed packages: {error}"))?
                    .filter_map(|entry| entry.ok().filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file())).map(|entry| entry.file_name().to_string_lossy().into_owned()))
                    .collect()
            }
        }
        ServerTarget::Remote { environment_id, .. } => {
            let environment = profiles.environment(*environment_id)?;
            remote_ssh::remote_package_names(profile.clone(), environment, Arc::clone(remote_sessions), folder).await?
        }
    };
    Ok(names.into_iter().collect())
}

fn installable_file<'a>(version: &'a Value) -> Result<&'a Value, String> {
    let files = version["files"].as_array().ok_or("Modrinth did not provide package files.")?;
    files.iter().find(|file| file["primary"] == true && file["filename"].as_str().is_some_and(|name| name.ends_with(".jar")))
        .or_else(|| files.iter().find(|file| file["filename"].as_str().is_some_and(|name| name.ends_with(".jar"))))
        .ok_or_else(|| "A required package has no installable JAR file.".into())
}

fn server_compatible(version: &Value, game_version: &str, loader: &str) -> bool {
    version["game_versions"].as_array().is_some_and(|items| items.iter().any(|item| item.as_str() == Some(game_version)))
        && version["loaders"].as_array().is_some_and(|items| items.iter().any(|item| item.as_str() == Some(loader)))
        && !matches!(version["environment"].as_str(), Some("client_only" | "singleplayer_only"))
}

fn order_packages(index: usize, packages: &[ResolvedPackage], lookup: &HashMap<String, usize>, visiting: &mut HashSet<usize>, done: &mut HashSet<usize>, order: &mut Vec<usize>) -> Result<(), String> {
    if done.contains(&index) { return Ok(()); }
    if !visiting.insert(index) { return Err("Required packages contain a dependency cycle.".into()); }
    for dependency in &packages[index].requires {
        let next = *lookup.get(dependency).ok_or("A required package could not be resolved.")?;
        order_packages(next, packages, lookup, visiting, done, order)?;
    }
    visiting.remove(&index);
    done.insert(index);
    order.push(index);
    Ok(())
}

async fn existing_package_matches(
    profile: &ServerProfile,
    folder: &str,
    filename: &str,
    expected: &str,
    profiles: &ServerProfileStore,
    remote_sessions: &Arc<RemoteSessionManager>,
) -> Result<bool, String> {
    if expected.len() != 128 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Modrinth provided an invalid package checksum.".into());
    }
    let actual = match &profile.target {
        ServerTarget::Local { directory, .. } => {
            let root = fs::canonicalize(directory).map_err(|_| "The server folder could not be opened.")?;
            let path = root.join(folder).join(filename);
            if fs::symlink_metadata(&path).map_err(|error| error.to_string())?.file_type().is_symlink() {
                return Err("The existing package is a symbolic link.".into());
            }
            let path = fs::canonicalize(path).map_err(|error| format!("Could not inspect existing package: {error}"))?;
            if !path.starts_with(root.join(folder)) { return Err("The package leaves the server folder.".into()); }
            let mut file = fs::File::open(path).map_err(|error| format!("Could not read existing package: {error}"))?;
            let mut digest = Sha512::new();
            let mut buffer = [0_u8; 8192];
            loop {
                let count = file.read(&mut buffer).map_err(|error| format!("Could not read existing package: {error}"))?;
                if count == 0 { break; }
                digest.update(&buffer[..count]);
            }
            format!("{:x}", digest.finalize())
        }
        ServerTarget::Remote { environment_id, .. } => {
            let environment = profiles.environment(*environment_id)?;
            remote_ssh::remote_package_sha512(profile.clone(), environment, Arc::clone(remote_sessions), folder, filename).await?
        }
    };
    Ok(actual.eq_ignore_ascii_case(expected))
}

async fn resolve_install_plan(
    client: &reqwest::Client,
    profile: &ServerProfile,
    kind: &str,
    game_version: &str,
    loader: &str,
    project_id: &str,
    version_id: &str,
    existing_names: &HashSet<String>,
    profiles: &ServerProfileStore,
    remote_sessions: &Arc<RemoteSessionManager>,
) -> Result<Vec<ResolvedPackage>, String> {
    let mut queue = VecDeque::from([DependencyRequest { project_id: Some(project_id.to_owned()), version_id: Some(version_id.to_owned()), parent_id: None, parent_title: None }]);
    let mut packages: Vec<ResolvedPackage> = Vec::new();
    let mut lookup: HashMap<String, usize> = HashMap::new();
    while let Some(request) = queue.pop_front() {
        if queue.len() > 128 { return Err("This package lists too many dependencies to install safely.".into()); }
        let version = if let Some(ref id) = request.version_id {
            if !catalog_id(id) { return Err("A required package has an invalid version ID.".into()); }
            get_json(client, api_url(&format!("version/{id}"))?).await?
        } else {
            let id = request.project_id.as_deref().ok_or("A required dependency has no Modrinth project or version ID.")?;
            if !catalog_id(id) { return Err("A required package has an invalid project ID.".into()); }
            let versions = get_json(client, versions_url(id, game_version, loader)?).await?;
            versions.as_array().and_then(|items| {
                items.iter().find(|item| server_compatible(item, game_version, loader) && item["version_type"] == "release")
                    .or_else(|| items.iter().find(|item| server_compatible(item, game_version, loader)))
            }).cloned()
                .ok_or_else(|| format!("A required dependency has no server release for Minecraft {game_version} and {loader}."))?
        };
        let id = version["project_id"].as_str().ok_or("A required version has no project ID.")?;
        if !catalog_id(id) || request.project_id.as_deref().is_some_and(|expected| expected != id) {
            return Err("A dependency refers to a different Modrinth project than its version.".into());
        }
        let selected_id = version["id"].as_str().ok_or("A required version has no ID.")?;
        if !catalog_id(selected_id) { return Err("A required package has an invalid version ID.".into()); }
        if !server_compatible(&version, game_version, loader) {
            return Err(format!("A required package is not compatible with Minecraft {game_version}, {loader}, and a dedicated server."));
        }
        if let Some(&index) = lookup.get(id) {
            if packages[index].entry.version_id != selected_id {
                return Err("Required packages request conflicting versions of the same project.".into());
            }
            if let Some(parent) = request.parent_title {
                packages[index].entry.required_by.push(parent);
            }
            if let Some(parent_id) = request.parent_id {
                packages[*lookup.get(&parent_id).ok_or("A dependency parent was not resolved.")?].requires.push(id.to_owned());
            }
            continue;
        }
        if packages.len() >= 32 { return Err("This install requires more than 32 packages. Install it manually.".into()); }
        let project = get_json(client, api_url(&format!("project/{id}"))?).await?;
        // Modrinth's project detail reports plugins as the legacy "mod" type;
        // the selected release's loader and server compatibility distinguish them.
        let project_type = project["project_type"].as_str();
        if project["id"].as_str() != Some(id)
            || (project_type != Some("mod") && !(kind == "plugin" && project_type == Some("plugin"))) {
            return Err("A required dependency has the wrong Modrinth project type.".into());
        }
        let title = project["title"].as_str().unwrap_or(id).to_owned();
        let file = installable_file(&version)?.clone();
        let suggested_filename = safe_filename(file["filename"].as_str().unwrap_or_default(), id, selected_id);
        let installed = profile.installed_packages.iter().find(|item| item.kind == kind && item.project_id == id && existing_names.contains(&item.filename));
        let updating = installed.filter(|item| item.version_id != selected_id);
        if updating.is_some() && request.parent_id.is_some() {
            return Err(format!("{title} is already installed at another version. Remove its JAR before installing this release."));
        }
        if updating.is_some_and(|item| !item.required_by.is_empty()) {
            return Err("A package required by another installed package cannot be updated here.".into());
        }
        let filename = if let Some(item) = updating {
            if suggested_filename == item.filename { format!("{id}-{selected_id}.jar") } else { suggested_filename }
        } else { installed.map(|item| item.filename.clone()).unwrap_or(suggested_filename) };
        if packages.iter().any(|item| item.entry.filename == filename) {
            return Err(format!("Multiple packages use the filename {filename}. Rename the existing package before installing."));
        }
        let already_installed = if existing_names.contains(&filename) {
            let expected = file["hashes"]["sha512"].as_str().ok_or("Modrinth did not provide a package checksum.")?;
            if !existing_package_matches(profile, package_folder(kind)?, &filename, expected, profiles, remote_sessions).await? {
                return Err(format!("{filename} already exists but does not match Modrinth's checksum. Remove it before installing."));
            }
            true
        } else { false };
        let index = packages.len();
        lookup.insert(id.to_owned(), index);
        packages.push(ResolvedPackage {
            entry: InstallPlanEntry { project_id: id.to_owned(), version_id: selected_id.to_owned(), title: title.clone(), version_number: version["version_number"].as_str().unwrap_or_default().to_owned(), filename, required_by: request.parent_title.into_iter().collect(), already_installed, direct: request.parent_id.is_none() },
            file,
            requires: Vec::new(),
        });
        if let Some(parent_id) = request.parent_id {
            packages[*lookup.get(&parent_id).ok_or("A dependency parent was not resolved.")?].requires.push(id.to_owned());
        }
        if let Some(dependencies) = version["dependencies"].as_array() {
            for dependency in dependencies.iter().filter(|item| item["dependency_type"] == "required") {
                let dependency_project = dependency["project_id"].as_str().map(str::to_owned);
                let dependency_version = dependency["version_id"].as_str().map(str::to_owned);
                if dependency_project.is_none() && dependency_version.is_none() {
                    return Err(format!("{title} requires an external dependency that Modrinth cannot download automatically."));
                }
                queue.push_back(DependencyRequest { project_id: dependency_project, version_id: dependency_version, parent_id: Some(id.to_owned()), parent_title: Some(title.clone()) });
            }
        }
    }
    let mut order = Vec::new();
    order_packages(0, &packages, &lookup, &mut HashSet::new(), &mut HashSet::new(), &mut order)?;
    let mut packages: Vec<_> = packages.into_iter().map(Some).collect();
    let mut ordered = Vec::new();
    for index in order {
        let mut package = packages[index].take().ok_or("A package was resolved twice.")?;
        package.entry.required_by.sort();
        package.entry.required_by.dedup();
        ordered.push(package);
    }
    Ok(ordered)
}

fn package_profile(profiles: &ServerProfileStore, profile_id: Uuid, kind: &str, game_version: &str, loader: &str, project_id: &str, version_id: &str) -> Result<ServerProfile, String> {
    package_folder(kind)?;
    if !valid_loader(kind, loader) || !safe_version(game_version) || !catalog_id(project_id) || !catalog_id(version_id) {
        return Err("The selected package or server compatibility is invalid.".into());
    }
    let profile = profiles.list()?.into_iter().find(|profile| profile.id == profile_id)
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
        } else { None };
        if supported_loader != Some(loader) {
            return Err("This package type is not supported by the installed server software.".into());
        }
    }
    Ok(profile)
}

#[tauri::command]
pub async fn preview_modrinth_install(
    profile_id: Uuid,
    kind: String,
    game_version: String,
    loader: String,
    project_id: String,
    version_id: String,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<Vec<InstallPlanEntry>, String> {
    let profile = package_profile(&profiles, profile_id, &kind, &game_version, &loader, &project_id, &version_id)?;
    let names = package_names(&profile, package_folder(&kind)?, &profiles, &remote_sessions).await?;
    let plan = resolve_install_plan(&client()?, &profile, &kind, &game_version, &loader, &project_id, &version_id, &names, &profiles, &remote_sessions).await?;
    Ok(plan.into_iter().map(|item| item.entry).collect())
}

async fn install_planned_package(
    client: &reqwest::Client,
    profile: &ServerProfile,
    package: &ResolvedPackage,
    kind: &str,
    profiles: &ServerProfileStore,
    local_sessions: &LocalSessionManager,
    remote_sessions: &Arc<RemoteSessionManager>,
) -> Result<(), String> {
    let folder = package_folder(kind)?;
    let file = &package.file;
    let filename = &package.entry.filename;
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
            profile.clone(), environment, Arc::clone(remote_sessions), folder.to_owned(),
            filename.clone(), None, String::new(), url.to_owned(), Some(sha512.to_owned()),
        ).await;
        match direct {
            Ok(()) => return Ok(()),
            Err(error) if error.starts_with(remote_ssh::REMOTE_DOWNLOAD_UNAVAILABLE) => {}
            Err(error) => return Err(error),
        }
    }
    let temporary = std::env::temp_dir().join(format!("scerite-package-{}.jar", Uuid::new_v4()));
    let result = async {
        let sha1 = download_package(client, file, &temporary).await?;
        match &profile.target {
            ServerTarget::Local { .. } => {
                if !matches!(local_sessions.status(profile.id)?.state, LocalServerState::Stopped) {
                    return Err("The server started during the download. Stop it before installing packages.".into());
                }
                install_local_package(profiles, profile.id, folder, filename, &temporary)
            }
            ServerTarget::Remote { environment_id, .. } => {
                let environment = profiles.environment(*environment_id)?;
                remote_ssh::install_remote_package(profile.clone(), environment, Arc::clone(remote_sessions), folder.to_owned(), filename.clone(), Some(temporary.clone()), sha1, file["url"].as_str().unwrap_or_default().to_owned(), None).await
            }
        }
    }.await;
    let _ = fs::remove_file(&temporary);
    result
}

#[tauri::command]
pub async fn install_modrinth_plan(
    profile_id: Uuid,
    kind: String,
    game_version: String,
    loader: String,
    project_id: String,
    version_id: String,
    expected_versions: Vec<String>,
    profiles: State<'_, ServerProfileStore>,
    local_sessions: State<'_, LocalSessionManager>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<String, String> {
    let profile = package_profile(&profiles, profile_id, &kind, &game_version, &loader, &project_id, &version_id)?;
    if matches!(profile.target, ServerTarget::Local { .. })
        && !matches!(local_sessions.status(profile_id)?.state, LocalServerState::Stopped)
    {
        return Err("Stop the server before installing packages.".into());
    }
    let names = package_names(&profile, package_folder(&kind)?, &profiles, &remote_sessions).await?;
    let client = client()?;
    let plan = resolve_install_plan(&client, &profile, &kind, &game_version, &loader, &project_id, &version_id, &names, &profiles, &remote_sessions).await?;
    let actual_versions: Vec<_> = plan.iter().map(|item| item.entry.version_id.clone()).collect();
    if actual_versions != expected_versions {
        return Err("The required package list changed. Review the install plan again before confirming.".into());
    }
    let previous_package = profile.installed_packages.iter().find(|item| item.kind == kind && item.project_id == project_id && item.version_id != version_id && names.contains(&item.filename)).cloned();
    let current_title = plan.iter().find(|item| item.entry.project_id == project_id)
        .map(|item| item.entry.title.clone()).unwrap_or_default();
    let required_ids = plan.iter().find(|item| item.entry.project_id == project_id)
        .map(|item| item.requires.clone()).unwrap_or_default();
    let mut installed = 0;
    for package in plan {
        if !package.entry.already_installed {
            install_planned_package(&client, &profile, &package, &kind, &profiles, &local_sessions, &remote_sessions)
                .await
                .map_err(|error| format!("Could not finish installing packages after {installed} JAR(s): {error}. Refresh the installed list before retrying."))?;
            installed += 1;
        }
        if package.entry.project_id == project_id {
            if let Some(previous) = &previous_package {
                remove_tracked_package(profile_id, &kind, &project_id, &previous.version_id, false, &profiles, &local_sessions, &remote_sessions)
                    .await.map_err(|error| format!("The new {kind} JAR was installed, but the previous version remains: {error}"))?;
            }
        }
        profiles.record_installed_package(profile_id, InstalledPackage {
            kind: kind.clone(),
            project_id: package.entry.project_id,
            version_id: package.entry.version_id,
            title: package.entry.title,
            filename: package.entry.filename,
            required_by: package.entry.required_by,
            requires: Some(package.requires),
            direct: package.entry.direct,
        })?;
    }
    if let Some(previous) = previous_package {
        profiles.reconcile_package_dependencies(profile_id, &kind, &previous.title, &current_title, &required_ids)?;
    }
    if installed == 0 { Ok("All selected packages are already installed. Dependency labels are up to date.".into()) }
    else { Ok(format!("Installed {installed} JAR(s), including required dependencies.")) }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalCandidate {
    project_id: String,
    title: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalPreview {
    dependents: Vec<RemovalCandidate>,
    cleanup: Vec<RemovalCandidate>,
}

fn package_requires(parent: &InstalledPackage, dependency: &InstalledPackage) -> bool {
    if parent.kind != dependency.kind || parent.project_id == dependency.project_id { return false; }
    match &parent.requires {
        Some(ids) => ids.contains(&dependency.project_id),
        None => dependency.required_by.contains(&parent.title),
    }
}

fn removal_preview(profile: &ServerProfile, kind: &str, project_id: &str) -> Result<RemovalPreview, String> {
    let packages: Vec<_> = profile.installed_packages.iter().filter(|item| item.kind == kind).collect();
    let target = packages.iter().find(|item| item.project_id == project_id)
        .ok_or("The installed package changed. Refresh the list before removing it.")?;
    let mut pending = VecDeque::from([target.project_id.clone()]);
    let mut dependent_ids = HashSet::new();
    while let Some(id) = pending.pop_front() {
        let dependency = packages.iter().find(|item| item.project_id == id)
            .ok_or("An installed dependency record is missing.")?;
        for parent in &packages {
            if package_requires(parent, dependency) && dependent_ids.insert(parent.project_id.clone()) {
                pending.push_back(parent.project_id.clone());
            }
        }
    }
    let mut dependents: Vec<_> = packages.iter().filter(|item| dependent_ids.contains(&item.project_id))
        .map(|item| RemovalCandidate { project_id: item.project_id.clone(), title: item.title.clone() }).collect();
    for title in &target.required_by {
        if !dependents.iter().any(|item| item.title == *title) {
            dependents.push(RemovalCandidate { project_id: String::new(), title: title.clone() });
        }
    }
    dependents.sort_by(|left, right| left.title.cmp(&right.title).then(left.project_id.cmp(&right.project_id)));
    if !dependents.is_empty() { return Ok(RemovalPreview { dependents, cleanup: Vec::new() }); }

    let mut removing = HashSet::from([project_id.to_owned()]);
    let mut cleanup = Vec::new();
    loop {
        let mut changed = false;
        for dependency in &packages {
            if dependency.direct || removing.contains(&dependency.project_id) { continue; }
            let parents: Vec<_> = packages.iter().filter(|parent| package_requires(parent, dependency)).collect();
            if parents.is_empty() || !parents.iter().all(|parent| removing.contains(&parent.project_id)) { continue; }
            if dependency.required_by.iter().any(|title| !parents.iter().any(|parent| parent.title == *title)) { continue; }
            removing.insert(dependency.project_id.clone());
            cleanup.push(RemovalCandidate { project_id: dependency.project_id.clone(), title: dependency.title.clone() });
            changed = true;
        }
        if !changed { break; }
    }
    Ok(RemovalPreview { dependents, cleanup })
}

#[tauri::command]
pub fn preview_modrinth_mod_removal(
    profile_id: Uuid,
    project_id: String,
    profiles: State<'_, ServerProfileStore>,
) -> Result<RemovalPreview, String> {
    let profile = profiles.list()?.into_iter().find(|item| item.id == profile_id)
        .ok_or("Server profile was not found.")?;
    removal_preview(&profile, "mod", &project_id)
}

async fn remove_tracked_package(
    profile_id: Uuid,
    kind: &str,
    project_id: &str,
    version_id: &str,
    clear_dependencies: bool,
    profiles: &ServerProfileStore,
    local_sessions: &LocalSessionManager,
    remote_sessions: &Arc<RemoteSessionManager>,
) -> Result<(), String> {
    let profile = profiles.list()?.into_iter().find(|item| item.id == profile_id)
        .ok_or("Server profile was not found.")?;
    profile.target.validate()?;
    let package = profile.installed_packages.iter().find(|item| item.kind == kind && item.project_id == project_id && item.version_id == version_id)
        .ok_or("The installed package changed. Refresh the list before removing it.")?;
    let preview = removal_preview(&profile, kind, project_id)?;
    if !preview.dependents.is_empty() {
        let titles = preview.dependents.iter().map(|item| item.title.as_str()).collect::<Vec<_>>().join(", ");
        return Err(format!("This {kind} is required by: {titles}. Remove those packages first."));
    }
    if profile.installed_packages.iter().any(|item| item.kind == kind && item.project_id != project_id && item.filename == package.filename) {
        return Err("Another package record uses this JAR. It was not removed.".into());
    }
    if !catalog_id(project_id) || !catalog_id(version_id)
        || safe_filename(&package.filename, project_id, version_id) != package.filename {
        return Err("The saved package record has an invalid file name or catalog ID.".into());
    }
    if matches!(profile.target, ServerTarget::Local { .. })
        && !matches!(local_sessions.status(profile_id)?.state, LocalServerState::Stopped) {
        return Err("Stop the server before removing packages.".into());
    }
    let version = get_json(&client()?, api_url(&format!("version/{version_id}"))?).await?;
    if version["project_id"].as_str() != Some(project_id) {
        return Err("The saved package does not match its Modrinth project.".into());
    }
    let expected = installable_file(&version)?["hashes"]["sha512"].as_str()
        .ok_or("Modrinth did not provide the installed package checksum.")?;
    if !existing_package_matches(&profile, package_folder(kind)?, &package.filename, expected, &profiles, &remote_sessions).await? {
        return Err("The installed JAR differs from its published version. It was not removed.".into());
    }
    match &profile.target {
        ServerTarget::Local { directory, .. } => {
            if !matches!(local_sessions.status(profile_id)?.state, LocalServerState::Stopped) {
                return Err("Stop the server before removing packages.".into());
            }
            let root = fs::canonicalize(directory).map_err(|_| "The server folder could not be opened.")?;
            let folder = root.join(package_folder(kind)?);
            if fs::canonicalize(&folder).map_err(|error| format!("Could not inspect package folder: {error}"))? != folder {
                return Err("The package folder is a symbolic link or leaves the server folder.".into());
            }
            let path = folder.join(&package.filename);
            if fs::symlink_metadata(&path).map_err(|error| format!("Could not inspect package: {error}"))?.file_type().is_symlink() {
                return Err("The installed package is a symbolic link.".into());
            }
            fs::remove_file(path).map_err(|error| format!("Could not remove package: {error}"))?;
        }
        ServerTarget::Remote { environment_id, .. } => {
            let environment = profiles.environment(*environment_id)?;
            remote_ssh::remove_remote_package(profile.clone(), environment, Arc::clone(remote_sessions), kind.to_owned(), package.filename.clone()).await?;
        }
    }
    profiles.remove_installed_package(profile_id, kind, project_id, version_id, &package.filename, clear_dependencies)
        .map_err(|error| format!("The package JAR was removed, but its saved record could not be updated: {error}"))
}

#[tauri::command]
pub async fn remove_modrinth_plugin(
    profile_id: Uuid,
    project_id: String,
    version_id: String,
    profiles: State<'_, ServerProfileStore>,
    local_sessions: State<'_, LocalSessionManager>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<(), String> {
    remove_tracked_package(profile_id, "plugin", &project_id, &version_id, true, &profiles, &local_sessions, &remote_sessions).await
}

#[tauri::command]
pub async fn remove_modrinth_mod(
    profile_id: Uuid,
    project_id: String,
    version_id: String,
    cleanup_project_ids: Vec<String>,
    profiles: State<'_, ServerProfileStore>,
    local_sessions: State<'_, LocalSessionManager>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<String, String> {
    let profile = profiles.list()?.into_iter().find(|item| item.id == profile_id)
        .ok_or("Server profile was not found.")?;
    let preview = removal_preview(&profile, "mod", &project_id)?;
    if !preview.dependents.is_empty() {
        return Err(format!("Other mods still depend on this mod: {}.", preview.dependents.iter().map(|item| item.title.as_str()).collect::<Vec<_>>().join(", ")));
    }
    if !cleanup_project_ids.is_empty() && cleanup_project_ids != preview.cleanup.iter().map(|item| item.project_id.clone()).collect::<Vec<_>>() {
        return Err("The unused dependency list changed. Review removal again before confirming.".into());
    }
    remove_tracked_package(profile_id, "mod", &project_id, &version_id, true, &profiles, &local_sessions, &remote_sessions).await?;
    for candidate in preview.cleanup.iter().filter(|item| cleanup_project_ids.contains(&item.project_id)) {
        let version = profile.installed_packages.iter().find(|item| item.kind == "mod" && item.project_id == candidate.project_id)
            .ok_or("An unused dependency record changed.")?;
        remove_tracked_package(profile_id, "mod", &candidate.project_id, &version.version_id, true, &profiles, &local_sessions, &remote_sessions)
            .await.map_err(|error| format!("Removed the mod, but could not remove unused dependency {}: {error}. Refresh the installed list.", candidate.title))?;
    }
    Ok(if cleanup_project_ids.is_empty() { "Removed the mod. Its dependencies were kept.".into() }
        else { format!("Removed the mod and {} unused dependencies.", cleanup_project_ids.len()) })
}
