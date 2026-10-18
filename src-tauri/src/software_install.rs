use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use serde_json::Value;
use sha1::{Digest as Sha1Digest, Sha1};
use sha2::{Digest as Sha2Digest, Sha256};
use tauri::State;
use uuid::Uuid;

use crate::{
    remote_ssh::{self, RemoteSessionManager},
    server_profiles::{ServerProfileStore, ServerTarget},
    server_sessions::{LocalServerState, LocalSessionManager},
    vanilla_install::{
        client, install_local, install_local_java, install_remote_java, java_major_for_version,
        progress, safe_version, update_java_launch,
    },
};

const MAX_JAR_SIZE: u64 = 512 * 1024 * 1024;

struct Artifact {
    url: String,
    build: String,
    filename: &'static str,
    sha256: Option<String>,
    size: Option<u64>,
}

fn hex_hash(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn allowed_download_url(value: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value).map_err(|_| "The server download URL is invalid.")?;
    let allowed = matches!(
        url.host_str(),
        Some(
            "fill.papermc.io"
                | "fill-data.papermc.io"
                | "api.purpurmc.org"
                | "meta.fabricmc.net"
                | "api.github.com"
                | "github.com"
                | "release-assets.githubusercontent.com"
                | "objects.githubusercontent.com"
                | "maven.minecraftforge.net"
                | "maven.neoforged.net"
                | "meta.quiltmc.org"
                | "maven.quiltmc.org"
        )
    );
    if url.scheme() != "https"
        || !allowed
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("The server download URL is not an approved HTTPS source.".into());
    }
    Ok(url)
}

async fn response(client: &reqwest::Client, url: &str) -> Result<reqwest::Response, String> {
    let mut url = allowed_download_url(url)?;
    for _ in 0..4 {
        let reply = client
            .get(url.clone())
            .send()
            .await
            .map_err(|error| format!("Could not reach the server software source: {error}"))?;
        if reply.status().is_redirection() {
            let location = reply
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or("The download source redirected without a valid location.")?;
            url = allowed_download_url(
                url.join(location)
                    .map_err(|_| "The download redirect is invalid.")?
                    .as_str(),
            )?;
            continue;
        }
        return reply
            .error_for_status()
            .map_err(|error| format!("The server software source rejected the request: {error}"));
    }
    Err("The server software source redirected too many times.".into())
}

async fn json(client: &reqwest::Client, url: &str) -> Result<Value, String> {
    response(client, url)
        .await?
        .json()
        .await
        .map_err(|error| format!("Could not read server software metadata: {error}"))
}

async fn metadata_versions(client: &reqwest::Client, url: &str) -> Result<Vec<String>, String> {
    let xml = response(client, url)
        .await?
        .text()
        .await
        .map_err(|error| format!("Could not read loader versions: {error}"))?;
    Ok(xml
        .split("<version>")
        .skip(1)
        .filter_map(|part| {
            part.split_once("</version>")
                .map(|(version, _)| version.to_owned())
        })
        .collect())
}

fn valid_build(value: &str) -> Result<String, String> {
    if safe_version(value) {
        Ok(value.to_owned())
    } else {
        Err("The server software source returned an invalid build version.".into())
    }
}

async fn paper(client: &reqwest::Client, version: &str) -> Result<Artifact, String> {
    let value = json(
        client,
        &format!("https://fill.papermc.io/v3/projects/paper/versions/{version}/builds"),
    )
    .await?;
    let builds = value
        .as_array()
        .or_else(|| value.get("builds").and_then(Value::as_array))
        .ok_or("Paper returned an invalid build list.")?;
    let build = builds
        .iter()
        .find(|build| build.get("channel").and_then(Value::as_str) == Some("STABLE"))
        .ok_or("Paper has no stable server build for this Minecraft version.")?;
    let download = &build["downloads"]["server:default"];
    let url = download["url"]
        .as_str()
        .ok_or("Paper did not provide a server jar URL.")?;
    let sha256 = download["checksums"]["sha256"]
        .as_str()
        .ok_or("Paper did not provide a SHA-256 checksum.")?;
    let id = build["id"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| build["id"].as_u64().map(|value| value.to_string()))
        .ok_or("Paper did not provide a build ID.")?;
    Ok(Artifact {
        url: url.into(),
        build: valid_build(&id)?,
        filename: "server.jar",
        sha256: Some(sha256.into()),
        size: download["size"].as_u64(),
    })
}

async fn purpur(client: &reqwest::Client, version: &str) -> Result<Artifact, String> {
    let value = json(
        client,
        &format!("https://api.purpurmc.org/v2/purpur/{version}"),
    )
    .await?;
    let latest = value["builds"]["latest"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| {
            value["builds"]["latest"]
                .as_u64()
                .map(|number| number.to_string())
        })
        .ok_or("Purpur has no build for this Minecraft version.")?;
    let build = valid_build(&latest)?;
    Ok(Artifact {
        url: format!("https://api.purpurmc.org/v2/purpur/{version}/{build}/download"),
        build,
        filename: "server.jar",
        sha256: None,
        size: None,
    })
}

async fn fabric(client: &reqwest::Client, version: &str) -> Result<Artifact, String> {
    let loaders = json(
        client,
        &format!("https://meta.fabricmc.net/v2/versions/loader/{version}"),
    )
    .await?;
    let loader = loaders
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["loader"]["stable"].as_bool() == Some(true))
        })
        .and_then(|item| item["loader"]["version"].as_str())
        .ok_or("Fabric has no stable loader for this Minecraft version.")?;
    let installers = json(client, "https://meta.fabricmc.net/v2/versions/installer").await?;
    let installer = installers
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["stable"].as_bool() == Some(true))
        })
        .and_then(|item| item["version"].as_str())
        .ok_or("Fabric has no stable server installer.")?;
    let loader = valid_build(loader)?;
    let installer = valid_build(installer)?;
    Ok(Artifact { url: format!("https://meta.fabricmc.net/v2/versions/loader/{version}/{loader}/{installer}/server/jar"), build: loader, filename: "fabric-server-launch.jar", sha256: None, size: None })
}

async fn arclight(
    client: &reqwest::Client,
    version: &str,
    variant: &str,
) -> Result<Artifact, String> {
    let releases = json(
        client,
        "https://api.github.com/repos/IzzelAliz/Arclight/releases?per_page=100",
    )
    .await?;
    let prefix = format!("arclight-{variant}-{version}-");
    let asset = releases
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .flat_map(|release| release["assets"].as_array().into_iter().flatten())
                .find(|asset| {
                    asset["name"]
                        .as_str()
                        .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".jar"))
                })
        })
        .ok_or("Arclight has no published jar for this Minecraft version and variant.")?;
    let url = asset["browser_download_url"]
        .as_str()
        .ok_or("Arclight did not provide a download URL.")?;
    let id = asset["id"]
        .as_u64()
        .ok_or("Arclight did not provide an asset ID.")?;
    let sha256 = asset["digest"]
        .as_str()
        .and_then(|value| value.strip_prefix("sha256:"))
        .map(str::to_owned);
    Ok(Artifact {
        url: url.into(),
        build: id.to_string(),
        filename: "arclight.jar",
        sha256,
        size: asset["size"].as_u64(),
    })
}

async fn artifact(
    client: &reqwest::Client,
    loader: &str,
    version: &str,
) -> Result<Artifact, String> {
    match loader {
        "paper" => paper(client, version).await,
        "purpur" => purpur(client, version).await,
        "fabric" => fabric(client, version).await,
        "arclight-forge" | "arclight-neoforge" | "arclight-fabric" => {
            arclight(client, version, loader.trim_start_matches("arclight-")).await
        }
        _ => Err("Choose a supported server software type.".into()),
    }
}

async fn installer_artifact(
    client: &reqwest::Client,
    loader: &str,
    version: &str,
) -> Result<(Artifact, Option<String>), String> {
    match loader {
        "forge" => {
            if version.starts_with("1.")
                && version
                    .split('.')
                    .nth(1)
                    .and_then(|minor| minor.parse::<u32>().ok())
                    .is_some_and(|minor| minor < 17)
            {
                return Err("Forge versions before Minecraft 1.17 use a different launch format and are not supported yet.".into());
            }
            let versions = metadata_versions(
                client,
                "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml",
            )
            .await?;
            let build = versions
                .into_iter()
                .rev()
                .find(|build| build.starts_with(&format!("{version}-")) && safe_version(build))
                .ok_or("Forge has no installer for this Minecraft version.")?;
            Ok((Artifact { url: format!("https://maven.minecraftforge.net/net/minecraftforge/forge/{build}/forge-{build}-installer.jar"), build, filename: "installer.jar", sha256: None, size: None }, None))
        }
        "neoforge" => {
            let prefix = if let Some(minor) = version.strip_prefix("1.") {
                format!("{minor}.")
            } else {
                format!("{version}.")
            };
            let versions = metadata_versions(
                client,
                "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml",
            )
            .await?;
            let build = versions
                .into_iter()
                .rev()
                .find(|build| build.starts_with(&prefix) && safe_version(build))
                .ok_or("NeoForge has no installer for this Minecraft version.")?;
            Ok((Artifact { url: format!("https://maven.neoforged.net/releases/net/neoforged/neoforge/{build}/neoforge-{build}-installer.jar"), build, filename: "installer.jar", sha256: None, size: None }, None))
        }
        "quilt" => {
            let loaders = json(
                client,
                &format!("https://meta.quiltmc.org/v3/versions/loader/{version}"),
            )
            .await?;
            let loader_version = loaders
                .as_array()
                .and_then(|items| {
                    items
                        .iter()
                        .find(|item| item["loader"]["stable"].as_bool() == Some(true))
                })
                .and_then(|item| item["loader"]["version"].as_str())
                .ok_or("Quilt has no stable loader for this Minecraft version.")?;
            let installers = json(client, "https://meta.quiltmc.org/v3/versions/installer").await?;
            let installer_info = installers
                .as_array()
                .and_then(|items| items.first())
                .ok_or("Quilt has no server installer.")?;
            let installer = installer_info["version"]
                .as_str()
                .ok_or("Quilt returned an invalid installer version.")?;
            let loader_version = valid_build(loader_version)?;
            let installer = valid_build(installer)?;
            let url = installer_info["url"]
                .as_str()
                .ok_or("Quilt did not provide an installer URL.")?;
            Ok((
                Artifact {
                    url: url.into(),
                    build: format!("{loader_version}-{installer}"),
                    filename: "installer.jar",
                    sha256: installer_info["hashes"]["sha256"]
                        .as_str()
                        .map(str::to_owned),
                    size: installer_info["file_size"].as_u64(),
                },
                Some(loader_version),
            ))
        }
        _ => Err("Choose a supported server software type.".into()),
    }
}

fn jar_sha1(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path)
        .map_err(|error| format!("Could not read the downloaded jar: {error}"))?;
    let mut hash = Sha1::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("Could not read the downloaded jar: {error}"))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hex_hash(hash.finalize()))
}

fn empty_local_target(root: &Path, directory: &str) -> Result<PathBuf, String> {
    let servers = root.join("servers");
    fs::create_dir_all(&servers)
        .map_err(|error| format!("Could not create servers folder: {error}"))?;
    let servers = fs::canonicalize(&servers)
        .map_err(|error| format!("Could not open servers folder: {error}"))?;
    if servers.parent() != Some(root) {
        return Err("The servers folder resolves outside the Minecraft root.".into());
    }
    let requested = Path::new(directory);
    let parent = requested
        .parent()
        .and_then(|path| fs::canonicalize(path).ok())
        .ok_or("The server folder must be directly inside the Minecraft root/servers folder.")?;
    if parent != servers {
        return Err(
            "The server folder must be directly inside the Minecraft root/servers folder.".into(),
        );
    }
    if !requested.exists() {
        fs::create_dir(requested)
            .map_err(|error| format!("Could not create server folder: {error}"))?;
    }
    let target = fs::canonicalize(requested)
        .map_err(|error| format!("Could not open server folder: {error}"))?;
    if target.parent() != Some(servers.as_path()) {
        return Err(
            "The server folder must be directly inside the Minecraft root/servers folder.".into(),
        );
    }
    if fs::read_dir(&target)
        .map_err(|error| format!("Could not read server folder: {error}"))?
        .next()
        .is_some()
    {
        return Err("Forge, NeoForge, and Quilt installers require an empty server folder. Existing files were not changed.".into());
    }
    Ok(target)
}

fn install_local_loader(
    target: &Path,
    java: &str,
    installer: &Path,
    loader: &str,
    version: &str,
    quilt_loader: Option<&str>,
    build: &str,
) -> Result<(), String> {
    let mut command = Command::new(java);
    command.current_dir(target).arg("-jar").arg(installer);
    if loader == "quilt" {
        command.args([
            "install",
            "server",
            version,
            quilt_loader.ok_or("Quilt loader version is missing.")?,
            "--download-server",
            "--install-dir=\".\"",
        ]);
    } else {
        command.arg("--installServer");
    }
    let output = command
        .output()
        .map_err(|error| format!("Could not start {loader} installer: {error}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "{loader} installer failed: {}",
            detail.chars().take(2000).collect::<String>()
        ));
    }
    let expected = if loader == "quilt" {
        target.join("quilt-server-launch.jar")
    } else {
        let path = if loader == "forge" {
            format!("libraries/net/minecraftforge/forge/{build}")
        } else {
            format!("libraries/net/neoforged/neoforge/{build}")
        };
        target.join(path).join(if cfg!(windows) {
            "win_args.txt"
        } else {
            "unix_args.txt"
        })
    };
    if !expected.is_file() {
        return Err(format!(
            "{loader} installer finished without creating its launch files."
        ));
    }
    Ok(())
}

fn update_installer_launch(
    profile: &crate::server_profiles::ServerProfile,
    profiles: &ServerProfileStore,
    java: &str,
    loader: &str,
    build: &str,
) -> Result<bool, String> {
    let mut target = profile.target.clone();
    let remote = matches!(target, ServerTarget::Remote { .. });
    let launch = match &mut target {
        ServerTarget::Local { launch, .. } | ServerTarget::Remote { launch, .. } => launch,
    };
    let executable = launch.executable.trim();
    let name = Path::new(executable)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(executable);
    launch.managed_java = Some(java.to_owned());
    let updated = executable.is_empty()
        || name.eq_ignore_ascii_case("java")
        || name.eq_ignore_ascii_case("java.exe");
    if updated {
        let mut arguments: Vec<String> = launch
            .arguments
            .iter()
            .take_while(|argument| *argument != "-jar" && !argument.starts_with("@libraries/"))
            .filter(|argument| argument.starts_with("-X") || argument.starts_with("-D"))
            .cloned()
            .collect();
        let family = if loader == "forge" {
            "net/minecraftforge/forge"
        } else {
            "net/neoforged/neoforge"
        };
        let file = if remote || !cfg!(windows) {
            "unix_args.txt"
        } else {
            "win_args.txt"
        };
        arguments.push(format!("@libraries/{family}/{build}/{file}"));
        arguments.push("nogui".into());
        launch.executable = java.to_owned();
        launch.arguments = arguments;
    }
    profiles.update(profile.id, profile.name.clone(), target)?;
    Ok(updated)
}

async fn download_jar(
    client: &reqwest::Client,
    app: &tauri::AppHandle,
    profile_id: Uuid,
    path: &Path,
    artifact: &Artifact,
) -> Result<String, String> {
    let expected = artifact.sha256.as_deref();
    if let Some(checksum) = expected {
        if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("The server software source returned an invalid checksum.".into());
        }
    }
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if !metadata.file_type().is_file() {
            return Err("The cached server jar is not a regular file.".into());
        }
        if metadata.len() < 1024
            || metadata.len() > MAX_JAR_SIZE
            || artifact.size.is_some_and(|size| size != metadata.len())
        {
            return Err(
                "The cached server jar has an invalid size. Remove it before retrying.".into(),
            );
        }
        let mut file = fs::File::open(path)
            .map_err(|error| format!("Could not read cached server jar: {error}"))?;
        let mut magic = [0; 2];
        file.read_exact(&mut magic)
            .map_err(|error| format!("Could not inspect cached server jar: {error}"))?;
        if magic != *b"PK" {
            return Err(
                "The cached server jar is not a jar file. Remove it before retrying.".into(),
            );
        }
        if let Some(checksum) = expected {
            let mut hash = Sha256::new();
            hash.update(magic);
            let mut buffer = [0; 65536];
            loop {
                let count = file
                    .read(&mut buffer)
                    .map_err(|error| format!("Could not read cached server jar: {error}"))?;
                if count == 0 {
                    break;
                }
                hash.update(&buffer[..count]);
            }
            let actual = format!("{:x}", hash.finalize());
            if !actual.eq_ignore_ascii_case(checksum) {
                return Err(
                    "The cached server jar has a different checksum. Remove it before retrying."
                        .into(),
                );
            }
        }
        progress(app, profile_id, "cached", 0, 0);
        return jar_sha1(path);
    }
    let parent = path.parent().ok_or("The server cache path is invalid.")?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Could not create server software cache: {error}"))?;
    let temp = path.with_file_name(format!(".servercrafter-{}.tmp", Uuid::new_v4()));
    let result = async {
        let mut reply = response(client, &artifact.url).await?;
        let total = artifact
            .size
            .or_else(|| reply.content_length())
            .unwrap_or(0);
        if total > MAX_JAR_SIZE {
            return Err("The server jar is too large.".into());
        }
        let mut output = fs::File::create_new(&temp)
            .map_err(|error| format!("Could not create cache file: {error}"))?;
        let mut sha256 = Sha256::new();
        let mut sha1 = Sha1::new();
        let mut size = 0;
        while let Some(chunk) = reply
            .chunk()
            .await
            .map_err(|error| format!("Server jar download stopped: {error}"))?
        {
            size += chunk.len() as u64;
            if size > MAX_JAR_SIZE || total > 0 && size > total {
                return Err("The downloaded server jar is larger than expected.".into());
            }
            output
                .write_all(&chunk)
                .map_err(|error| format!("Could not save server jar: {error}"))?;
            sha256.update(&chunk);
            sha1.update(&chunk);
            progress(app, profile_id, "downloading", size, total);
        }
        output
            .sync_all()
            .map_err(|error| format!("Could not finish caching server jar: {error}"))?;
        if size < 1024 || total > 0 && size != total {
            return Err("The server jar download is incomplete.".into());
        }
        if let Some(checksum) = expected {
            if !format!("{:x}", sha256.finalize()).eq_ignore_ascii_case(checksum) {
                return Err("The server jar failed its checksum check.".into());
            }
        }
        let mut magic = [0; 2];
        fs::File::open(&temp)
            .and_then(|mut file| file.read_exact(&mut magic))
            .map_err(|error| format!("Could not inspect downloaded jar: {error}"))?;
        if magic != *b"PK" {
            return Err("The server software source did not return a jar file.".into());
        }
        fs::rename(&temp, path)
            .map_err(|error| format!("Could not finish caching server jar: {error}"))?;
        Ok(hex_hash(sha1.finalize()))
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[tauri::command]
pub async fn install_server_software(
    app: tauri::AppHandle,
    profile_id: Uuid,
    loader: String,
    version_id: String,
    profiles: State<'_, ServerProfileStore>,
    local_sessions: State<'_, LocalSessionManager>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<String, String> {
    if !safe_version(&version_id) {
        return Err("Choose a valid Minecraft version.".into());
    }
    let profile = profiles
        .list()?
        .into_iter()
        .find(|item| item.id == profile_id)
        .ok_or("Server profile was not found.")?;
    profile.target.validate()?;
    if matches!(profile.target, ServerTarget::Local { .. })
        && !matches!(
            local_sessions.status(profile_id)?.state,
            LocalServerState::Stopped
        )
    {
        return Err("Stop this server before installing software.".into());
    }
    progress(&app, profile_id, "checking", 0, 0);
    let client = client()?;
    let java_major = java_major_for_version(&client, &version_id).await?;
    let installer = matches!(loader.as_str(), "forge" | "neoforge" | "quilt");
    let (artifact, quilt_loader) = if installer {
        installer_artifact(&client, &loader, &version_id).await?
    } else {
        (artifact(&client, &loader, &version_id).await?, None)
    };
    let java = match &profile.target {
        ServerTarget::Local { .. } => {
            let root = profiles
                .local_root()?
                .ok_or("Set this computer's Minecraft root on the Servers page first.")?;
            let root = fs::canonicalize(root).map_err(|_| {
                "Create the configured Minecraft root folder before installing software."
            })?;
            install_local_java(&client, &app, profile_id, &root, java_major).await?
        }
        ServerTarget::Remote { environment_id, .. } => {
            let environment = profiles.environment(*environment_id)?;
            if environment.root_directory.is_empty() {
                return Err("Set this remote environment's Minecraft root first.".into());
            }
            install_remote_java(
                &client,
                &app,
                &profile,
                &environment,
                Arc::clone(remote_sessions.inner()),
                java_major,
            )
            .await?
        }
    };
    let cache_version = format!("{version_id}/{}", artifact.build);
    match &profile.target {
        ServerTarget::Local { directory, .. } => {
            let root = fs::canonicalize(
                profiles
                    .local_root()?
                    .ok_or("Set this computer's Minecraft root first.")?,
            )
            .map_err(|error| format!("Could not open Minecraft root: {error}"))?;
            let cache = root
                .join("cache")
                .join(&loader)
                .join(&cache_version)
                .join(artifact.filename);
            let parent = cache.parent().ok_or("The server cache path is invalid.")?;
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create server software cache: {error}"))?;
            let canonical = fs::canonicalize(parent)
                .map_err(|error| format!("Could not open server software cache: {error}"))?;
            if !canonical.starts_with(root.join("cache")) {
                return Err(
                    "The server software cache resolves outside the configured root.".into(),
                );
            }
            download_jar(&client, &app, profile_id, &cache, &artifact).await?;
            progress(
                &app,
                profile_id,
                if installer {
                    "installing_loader"
                } else {
                    "installing"
                },
                0,
                0,
            );
            if installer {
                let target = empty_local_target(&root, directory)?;
                let java = java.clone();
                let loader = loader.clone();
                let version = version_id.clone();
                let build = artifact.build.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    install_local_loader(
                        &target,
                        &java,
                        &cache,
                        &loader,
                        &version,
                        quilt_loader.as_deref(),
                        &build,
                    )
                })
                .await
                .map_err(|error| format!("The loader installer did not finish: {error}"))??;
            } else {
                install_local(
                    &root,
                    directory,
                    &cache,
                    artifact.filename,
                    profile_id,
                    &local_sessions,
                )?;
            }
        }
        ServerTarget::Remote {
            environment_id,
            directory,
            ..
        } => {
            let environment = profiles.environment(*environment_id)?;
            let temporary =
                std::env::temp_dir().join(format!("servercrafter-{}.jar", Uuid::new_v4()));
            let result = async {
                let sha1 = download_jar(&client, &app, profile_id, &temporary, &artifact).await?;
                progress(&app, profile_id, "uploading", 0, 0);
                if installer {
                    progress(&app, profile_id, "installing_loader", 0, 0);
                    remote_ssh::install_remote_loader(
                        profile.clone(),
                        environment,
                        Arc::clone(remote_sessions.inner()),
                        directory.clone(),
                        temporary.clone(),
                        loader.clone(),
                        version_id.clone(),
                        artifact.build.clone(),
                        quilt_loader,
                        java.clone(),
                        sha1,
                    )
                    .await
                } else {
                    remote_ssh::install_remote_jar(
                        profile.clone(),
                        environment,
                        Arc::clone(remote_sessions.inner()),
                        directory.clone(),
                        temporary.clone(),
                        cache_version,
                        sha1,
                        loader.clone(),
                        artifact.filename.into(),
                    )
                    .await
                }
            }
            .await;
            let _ = fs::remove_file(&temporary);
            result?;
        }
    }
    let launch = if loader == "quilt" {
        update_java_launch(&profile, &java, &profiles, "quilt-server-launch.jar")
    } else if loader == "forge" || loader == "neoforge" {
        update_installer_launch(&profile, &profiles, &java, &loader, &artifact.build)
    } else {
        update_java_launch(&profile, &java, &profiles, artifact.filename)
    };
    progress(&app, profile_id, "complete", 0, 0);
    match launch {
        Ok(true) => Ok(format!("{loader} for Minecraft {version_id} (build {}) installed with Java {java_major}.", artifact.build)),
        Ok(false) => Ok(format!("{loader} for Minecraft {version_id} installed with Java {java_major}. Update the custom launch command to start the new software.")),
        Err(error) => Ok(format!("{loader} installed, but the launch profile could not be updated: {error}")),
    }
}
