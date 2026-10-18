use std::{
    fs,
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};

use flate2::read::GzDecoder;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 100_000;

#[derive(Deserialize)]
struct AdoptiumAsset {
    release_name: String,
    binary: AdoptiumBinary,
}

#[derive(Deserialize)]
struct AdoptiumBinary {
    architecture: String,
    os: String,
    image_type: String,
    package: AdoptiumPackage,
}

#[derive(Deserialize)]
struct AdoptiumPackage {
    link: String,
    checksum: String,
    size: u64,
}

#[derive(Clone)]
pub struct JavaPackage {
    pub major: u16,
    pub platform: String,
    pub build: String,
    pub link: String,
    pub sha256: String,
    pub size: u64,
    pub extension: &'static str,
}

impl JavaPackage {
    pub fn runtime_relative(&self) -> PathBuf {
        PathBuf::from("runtimes")
            .join("java")
            .join(self.major.to_string())
            .join(&self.platform)
            .join(&self.build)
    }

    pub fn cache_relative(&self) -> PathBuf {
        PathBuf::from("cache")
            .join("java")
            .join(self.major.to_string())
            .join(&self.platform)
            .join(&self.build)
            .join(format!("jdk.{}", self.extension))
    }

    pub fn java_relative(&self) -> PathBuf {
        let mut path = self.runtime_relative();
        if self.platform.starts_with("mac-") {
            path.push("Contents/Home");
        }
        path.push("bin");
        path.push(if self.platform.starts_with("windows-") {
            "java.exe"
        } else {
            "java"
        });
        path
    }
}

pub fn local_platform() -> Result<(&'static str, &'static str), String> {
    let os = match std::env::consts::OS {
        "windows" => "windows",
        "linux" => "linux",
        "macos" => "mac",
        _ => return Err("This desktop operating system has no managed Temurin JDK build.".into()),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        _ => return Err("This desktop CPU architecture has no managed Temurin JDK build.".into()),
    };
    Ok((os, arch))
}

pub fn ensure_managed_directory(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            return Err("The managed Java path is invalid.".into());
        };
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err("A managed Java folder is a file or symbolic link. Choose another Minecraft root.".into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current)
                    .map_err(|error| format!("Could not create managed Java folder: {error}"))?;
            }
            Err(error) => return Err(format!("Could not inspect managed Java folder: {error}")),
        }
    }
    let canonical = fs::canonicalize(&current)
        .map_err(|error| format!("Could not resolve managed Java folder: {error}"))?;
    if !canonical.starts_with(root) {
        return Err(
            "The managed Java folder resolves outside the configured Minecraft root.".into(),
        );
    }
    Ok(canonical)
}

pub async fn package(
    client: &reqwest::Client,
    major: u16,
    os: &str,
    arch: &str,
) -> Result<JavaPackage, String> {
    if !(8..=30).contains(&major)
        || !matches!(os, "windows" | "linux" | "mac")
        || !matches!(arch, "x64" | "aarch64")
    {
        return Err("This Minecraft release needs a Java build that ServerCrafter cannot install for this platform.".into());
    }
    let url = format!(
        "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?architecture={arch}&image_type=jdk&os={os}&vendor=eclipse"
    );
    let assets: Vec<AdoptiumAsset> = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Could not find a Temurin JDK: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Could not find a Temurin JDK: {error}"))?
        .json()
        .await
        .map_err(|error| format!("Could not read Temurin JDK details: {error}"))?;
    let asset = assets
        .into_iter()
        .find(|asset| {
            asset.binary.os == os
                && asset.binary.architecture == arch
                && asset.binary.image_type == "jdk"
        })
        .ok_or_else(|| format!("No Temurin JDK {major} build is available for {os}/{arch}."))?;
    let build = asset.release_name.replace('+', "_");
    if build.is_empty()
        || build.len() > 80
        || !build
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err("The Temurin build name is invalid.".into());
    }
    let extension = if os == "windows" { "zip" } else { "tar.gz" };
    let link = reqwest::Url::parse(&asset.binary.package.link)
        .map_err(|_| "The Temurin download URL is invalid.".to_owned())?;
    let expected_prefix = format!("/adoptium/temurin{major}-binaries/releases/download/");
    if link.scheme() != "https"
        || link.host_str() != Some("github.com")
        || link.port().is_some()
        || !link.path().starts_with(&expected_prefix)
        || !link.path().ends_with(&format!(".{extension}"))
    {
        return Err("The Temurin download URL is not an official Adoptium release asset.".into());
    }
    let data = asset.binary.package;
    if data.size == 0
        || data.size > MAX_ARCHIVE_BYTES
        || data.checksum.len() != 64
        || !data.checksum.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("The Temurin download checksum or size is invalid.".into());
    }
    Ok(JavaPackage {
        major,
        platform: format!("{os}-{arch}"),
        build,
        link: data.link,
        sha256: data.checksum,
        size: data.size,
        extension,
    })
}

pub fn archive_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let url = attempt.url();
            if attempt.previous().len() < 5
                && url.scheme() == "https"
                && matches!(
                    url.host_str(),
                    Some(
                        "github.com"
                            | "release-assets.githubusercontent.com"
                            | "objects.githubusercontent.com"
                    )
                )
            {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|error| format!("Could not initialize Java downloads: {error}"))
}

pub fn archive_verified(path: &Path, package: &JavaPackage) -> Result<bool, String> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err("The cached JDK archive is a symbolic link. Remove it before retrying.".into());
    }
    let Ok(mut file) = fs::File::open(path) else {
        return Ok(false);
    };
    let mut digest = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 65536];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("Could not read cached JDK: {error}"))?;
        if count == 0 {
            break;
        }
        size += count as u64;
        if size > package.size {
            return Ok(false);
        }
        digest.update(&buffer[..count]);
    }
    let actual: String = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(size == package.size && actual.eq_ignore_ascii_case(&package.sha256))
}

pub async fn download_archive(
    client: &reqwest::Client,
    destination: &Path,
    package: &JavaPackage,
    report: impl Fn(u64, u64),
) -> Result<(), String> {
    if archive_verified(destination, package)? {
        report(package.size, package.size);
        return Ok(());
    }
    if fs::symlink_metadata(destination).is_ok() {
        return Err("The cached JDK archive has a different checksum. Remove that cache file before retrying.".into());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create Java cache: {error}"))?;
    }
    let temporary = destination.with_file_name(format!(".servercrafter-{}.tmp", Uuid::new_v4()));
    let result = async {
        let mut response = client
            .get(&package.link)
            .send()
            .await
            .map_err(|error| format!("Could not download Temurin JDK: {error}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "Temurin JDK download returned HTTP {}.",
                response.status()
            ));
        }
        let mut file = fs::File::create_new(&temporary)
            .map_err(|error| format!("Could not create Java cache file: {error}"))?;
        let mut digest = Sha256::new();
        let mut size = 0_u64;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| format!("JDK download stopped: {error}"))?
        {
            size += chunk.len() as u64;
            if size > package.size {
                return Err("The JDK download is larger than expected.".into());
            }
            file.write_all(&chunk)
                .map_err(|error| format!("Could not save JDK archive: {error}"))?;
            digest.update(&chunk);
            report(size, package.size);
        }
        file.sync_all()
            .map_err(|error| format!("Could not finish caching JDK archive: {error}"))?;
        let actual: String = digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if size != package.size || !actual.eq_ignore_ascii_case(&package.sha256) {
            return Err("The JDK archive did not match Adoptium's SHA-256 checksum.".into());
        }
        fs::rename(&temporary, destination)
            .map_err(|error| format!("Could not finish caching JDK archive: {error}"))?;
        Ok(())
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn top_folder(path: &Path, top: &mut Option<String>) -> Result<(), String> {
    let mut parts = path.components();
    let first = match parts.next() {
        Some(Component::CurDir) => parts.next(),
        other => other,
    };
    let Some(Component::Normal(first)) = first else {
        return Err("The JDK archive has an unsafe path.".into());
    };
    let first = first.to_string_lossy();
    if first.is_empty()
        || first.len() > 80
        || !first
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'+'))
    {
        return Err("The JDK archive has an unsafe path.".into());
    }
    if let Some(existing) = top {
        if existing != &first {
            return Err("The JDK archive has multiple top-level folders.".into());
        }
    } else {
        *top = Some(first.into_owned());
    }
    if parts.any(|part| !matches!(part, Component::Normal(_))) {
        return Err("The JDK archive contains a path outside its own folder.".into());
    }
    Ok(())
}

pub fn validate_archive(path: &Path, package: &JavaPackage) -> Result<String, String> {
    let mut top = None;
    let mut total = 0_u64;
    if package.extension == "zip" {
        let file =
            fs::File::open(path).map_err(|error| format!("Could not open JDK archive: {error}"))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|error| format!("Could not read JDK zip: {error}"))?;
        if archive.len() > MAX_ARCHIVE_ENTRIES {
            return Err("The JDK archive contains too many files.".into());
        }
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|error| format!("Could not read JDK zip entry: {error}"))?;
            let entry_path = entry
                .enclosed_name()
                .ok_or_else(|| "The JDK archive has an unsafe path.".to_owned())?;
            top_folder(&entry_path, &mut top)?;
            if entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            {
                return Err("The JDK zip contains a symbolic link.".into());
            }
            total = total.saturating_add(entry.size());
            if total > MAX_EXTRACTED_BYTES {
                return Err("The JDK archive expands beyond the size limit.".into());
            }
        }
    } else {
        let file =
            fs::File::open(path).map_err(|error| format!("Could not open JDK archive: {error}"))?;
        let mut archive = tar::Archive::new(GzDecoder::new(file));
        for (index, entry) in archive
            .entries()
            .map_err(|error| format!("Could not read JDK tar: {error}"))?
            .enumerate()
        {
            if index >= MAX_ARCHIVE_ENTRIES {
                return Err("The JDK archive contains too many files.".into());
            }
            let entry = entry.map_err(|error| format!("Could not read JDK tar entry: {error}"))?;
            let kind = entry.header().entry_type();
            if kind.is_gnu_longname()
                || kind.is_gnu_longlink()
                || kind.is_pax_global_extensions()
                || kind.is_pax_local_extensions()
            {
                continue;
            }
            let path = entry
                .path()
                .map_err(|error| format!("Could not read JDK tar path: {error}"))?;
            top_folder(&path, &mut top)?;
            if kind.is_symlink() {
                let link = entry
                    .link_name()
                    .map_err(|error| format!("Could not read JDK symlink: {error}"))?
                    .ok_or_else(|| "The JDK tar has an empty symbolic link.".to_owned())?;
                let mut depth = path
                    .components()
                    .filter(|part| matches!(part, Component::Normal(_)))
                    .count()
                    .saturating_sub(1);
                for component in link.components() {
                    match component {
                        Component::Normal(_) => depth += 1,
                        Component::ParentDir if depth > 1 => depth -= 1,
                        Component::CurDir => {}
                        _ => {
                            return Err(
                                "The JDK archive has a symbolic link outside its folder.".into()
                            );
                        }
                    }
                }
            } else if !kind.is_file() && !kind.is_dir() {
                return Err("The JDK archive contains an unsupported file type.".into());
            }
            total = total.saturating_add(entry.size());
            if total > MAX_EXTRACTED_BYTES {
                return Err("The JDK archive expands beyond the size limit.".into());
            }
        }
    }
    top.ok_or_else(|| "The JDK archive is empty.".into())
}

pub fn installed_local(root: &Path, package: &JavaPackage) -> Result<Option<PathBuf>, String> {
    let runtime = root.join(package.runtime_relative());
    let java = root.join(package.java_relative());
    if !runtime.exists() && fs::symlink_metadata(&runtime).is_err() {
        return Ok(None);
    }
    let canonical = fs::canonicalize(&runtime)
        .map_err(|error| format!("Could not inspect managed JDK: {error}"))?;
    let executable = fs::canonicalize(&java).map_err(|_| "The managed JDK folder exists but its Java executable is missing. Remove that incomplete folder before retrying.".to_owned())?;
    if canonical.starts_with(root.join("runtimes").join("java"))
        && executable.starts_with(&canonical)
        && java.is_file()
    {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if fs::metadata(&java)
                .map_err(|error| format!("Could not inspect managed Java: {error}"))?
                .permissions()
                .mode()
                & 0o111
                == 0
            {
                return Err("The managed Java executable has no execute permission.".into());
            }
        }
        return Ok(Some(java));
    }
    Err("The managed JDK folder resolves outside the configured Minecraft root.".into())
}

pub fn install_local(
    root: &Path,
    archive_path: &Path,
    package: &JavaPackage,
    top: &str,
) -> Result<PathBuf, String> {
    let runtime = root.join(package.runtime_relative());
    if let Some(java) = installed_local(root, package)? {
        return Ok(java);
    }
    let parent = runtime
        .parent()
        .ok_or_else(|| "The Java runtime path is invalid.".to_owned())?;
    let relative = parent.strip_prefix(root).map_err(|_| {
        "The Java runtime path is outside the configured Minecraft root.".to_owned()
    })?;
    let canonical = ensure_managed_directory(root, relative)?;
    if !canonical.starts_with(root.join("runtimes").join("java")) {
        return Err(
            "The Java runtime folder resolves outside the configured Minecraft root.".into(),
        );
    }
    let staging = parent.join(format!(".servercrafter-{}.tmp", Uuid::new_v4()));
    fs::create_dir(&staging).map_err(|error| format!("Could not stage JDK extraction: {error}"))?;
    let result: Result<(), String> = (|| {
        if package.extension == "zip" {
            let file = fs::File::open(archive_path).map_err(|error| error.to_string())?;
            let mut zip = zip::ZipArchive::new(file).map_err(|error| error.to_string())?;
            for index in 0..zip.len() {
                let mut entry = zip.by_index(index).map_err(|error| error.to_string())?;
                let name = entry
                    .enclosed_name()
                    .ok_or_else(|| "The JDK zip has an unsafe path.".to_owned())?;
                let output = staging.join(name);
                if entry.is_dir() {
                    fs::create_dir_all(&output).map_err(|error| error.to_string())?;
                } else {
                    fs::create_dir_all(
                        output
                            .parent()
                            .ok_or_else(|| "The JDK zip path is invalid.".to_owned())?,
                    )
                    .map_err(|error| error.to_string())?;
                    let mut file =
                        fs::File::create_new(&output).map_err(|error| error.to_string())?;
                    io::copy(&mut entry, &mut file).map_err(|error| error.to_string())?;
                }
            }
        } else {
            let file = fs::File::open(archive_path).map_err(|error| error.to_string())?;
            tar::Archive::new(GzDecoder::new(file))
                .unpack(&staging)
                .map_err(|error| error.to_string())?;
        }
        let extracted = staging.join(top);
        let executable = if package.platform.starts_with("mac-") {
            extracted.join("Contents/Home/bin/java")
        } else if package.platform.starts_with("windows-") {
            extracted.join("bin/java.exe")
        } else {
            extracted.join("bin/java")
        };
        if !executable.is_file() {
            return Err("The extracted JDK has no Java executable.".into());
        }
        fs::rename(&extracted, &runtime)
            .map_err(|error| format!("Could not finish installing JDK: {error}"))?;
        Ok(())
    })();
    let _ = fs::remove_dir_all(&staging);
    result?;
    installed_local(root, package)?
        .ok_or_else(|| "The JDK installation did not create a Java executable.".into())
}
