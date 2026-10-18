use std::{fs, io};

pub const MAX_EULA_BYTES: u64 = 64 * 1024;

pub fn enabled_content(current: Option<&str>) -> Option<String> {
    let current = current.unwrap_or("");
    let existing_values: Vec<_> = current.lines().filter_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "eula").then_some(value.trim())
    }).collect();
    if existing_values == ["true"] {
        return None;
    }
    let mut updated = String::new();
    let mut found = false;
    for line in current.lines() {
        let key = line.split_once('=').map(|(key, _)| key.trim());
        if key == Some("eula") {
            if !found {
                updated.push_str("eula=true\n");
                found = true;
            }
        } else {
            updated.push_str(line);
            updated.push('\n');
        }
    }
    if !found {
        updated.push_str("eula=true\n");
    }
    (updated != current).then_some(updated)
}

pub fn enable_local(directory: &str) -> Result<(), String> {
    let root = fs::canonicalize(directory)
        .map_err(|error| format!("Could not open the server folder to enable the EULA: {error}"))?;
    let path = root.join("eula.txt");
    let current = match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err("eula.txt must be a regular file, not a symbolic link.".into());
            }
            if metadata.len() > MAX_EULA_BYTES {
                return Err("eula.txt is unexpectedly large and was not changed.".into());
            }
            Some(fs::read_to_string(&path)
                .map_err(|error| format!("Could not read eula.txt: {error}"))?)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("Could not inspect eula.txt: {error}")),
    };
    if let Some(updated) = enabled_content(current.as_deref()) {
        fs::write(&path, updated)
            .map_err(|error| format!("Could not enable the EULA in eula.txt: {error}"))?;
    }
    Ok(())
}
