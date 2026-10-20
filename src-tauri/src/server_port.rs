use std::{fs, io, net::TcpListener};

const DEFAULT_PORT: u16 = 25565;
pub const MAX_PROPERTIES_BYTES: u64 = 1024 * 1024;

pub fn configured_port(content: Option<&str>) -> Result<u16, String> {
    let value = content.unwrap_or("").lines().filter_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "server-port").then_some(value.trim())
    }).last();
    match value {
        Some(value) => value.parse::<u16>().ok().filter(|port| *port > 0)
            .ok_or_else(|| "server.properties has an invalid server-port. Set it to a port from 1 to 65535.".into()),
        None => Ok(DEFAULT_PORT),
    }
}

pub fn updated_content(current: Option<&str>, port: u16) -> String {
    let current = current.unwrap_or("");
    let mut updated = String::new();
    let mut found = false;
    for line in current.split_inclusive('\n') {
        let key = line.trim_end_matches(['\r', '\n']).split_once('=').map(|(key, _)| key.trim());
        if key == Some("server-port") {
            updated.push_str(&format!("server-port={port}"));
            if line.ends_with("\r\n") { updated.push_str("\r\n"); }
            else if line.ends_with('\n') { updated.push('\n'); }
            found = true;
        } else {
            updated.push_str(line);
        }
    }
    if !found {
        if !updated.is_empty() && !updated.ends_with('\n') { updated.push('\n'); }
        updated.push_str(&format!("server-port={port}\n"));
    }
    updated
}

pub fn available_port(requested: u16, mut in_use: impl FnMut(u16) -> Result<bool, String>) -> Result<Option<u16>, String> {
    if !in_use(requested)? { return Ok(None); }
    for candidate in ((requested as u32 + 1).max(DEFAULT_PORT as u32)..=u16::MAX as u32)
        .chain(DEFAULT_PORT as u32..requested as u32)
    {
        let candidate = candidate as u16;
        if !in_use(candidate)? { return Ok(Some(candidate)); }
    }
    Err(format!("Port {requested} is in use and no free Minecraft port was found. Stop the other server or set a free server-port in server.properties."))
}

fn local_port_in_use(port: u16) -> Result<bool, String> {
    match TcpListener::bind(("0.0.0.0", port)) {
        Ok(listener) => { drop(listener); Ok(false) }
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => Ok(true),
        Err(error) => Err(format!("Could not check whether Minecraft port {port} is available: {error}")),
    }
}

pub fn ensure_local_port(directory: &str) -> Result<(), String> {
    let root = fs::canonicalize(directory).map_err(|error| format!("Could not open the server folder to check its port: {error}"))?;
    let path = root.join("server.properties");
    let current = match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err("server.properties must be a regular file, not a symbolic link.".into());
            }
            if metadata.len() > MAX_PROPERTIES_BYTES {
                return Err("server.properties is too large to update its port safely.".into());
            }
            Some(fs::read_to_string(&path).map_err(|error| format!("Could not read server.properties: {error}"))?)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("Could not inspect server.properties: {error}")),
    };
    let requested = configured_port(current.as_deref())?;
    if let Some(port) = available_port(requested, local_port_in_use)? {
        fs::write(&path, updated_content(current.as_deref(), port))
            .map_err(|error| format!("Port {requested} is in use, but server.properties could not be updated to port {port}: {error}"))?;
    }
    Ok(())
}
