use std::{fs, io::{Read, Seek, SeekFrom}};

use flate2::read::GzDecoder;
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::server_files::{local_server_root, resolve_existing};
use crate::server_profiles::ServerProfileStore;

const TAIL_BYTES: usize = 256 * 1024;
const MAX_COMPRESSED_BYTES: u64 = 16 * 1024 * 1024;
const MAX_DECOMPRESSED_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerLogSnapshot {
    pub text: String,
    pub truncated: bool,
}

pub(crate) fn valid_log_name(name: &str) -> bool {
    name.len() <= 128
        && !name.contains("..")
        && (name.ends_with(".log") || name.ends_with(".log.gz"))
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

pub(crate) fn max_compressed_bytes() -> u64 { MAX_COMPRESSED_BYTES }
pub(crate) fn tail_bytes() -> usize { TAIL_BYTES }

pub(crate) fn decode_log(name: &str, bytes: Vec<u8>, truncated: bool) -> Result<ServerLogSnapshot, String> {
    let (bytes, truncated) = if name.ends_with(".gz") {
        let mut decoded = Vec::new();
        GzDecoder::new(bytes.as_slice())
            .take(MAX_DECOMPRESSED_BYTES + 1)
            .read_to_end(&mut decoded)
            .map_err(|error| format!("Could not decompress the server log: {error}"))?;
        if decoded.len() as u64 > MAX_DECOMPRESSED_BYTES {
            return Err("This archived log is too large to display (over 64 MB).".into());
        }
        let cut = decoded.len().saturating_sub(TAIL_BYTES);
        (decoded[cut..].to_vec(), cut > 0)
    } else {
        (bytes, truncated)
    };
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let text = if truncated { text.split_once('\n').map_or("", |(_, rest)| rest).to_owned() } else { text };
    Ok(ServerLogSnapshot { text, truncated })
}

#[tauri::command]
pub fn read_local_server_log(
    profile_id: Uuid,
    name: String,
    profiles: State<'_, ServerProfileStore>,
) -> Result<ServerLogSnapshot, String> {
    if !valid_log_name(&name) {
        return Err("Choose a log file from this server's logs folder.".into());
    }
    let root = local_server_root(profile_id, &profiles)?;
    let path = resolve_existing(&root, &format!("logs/{name}"))?;
    if !path.is_file() {
        return Err("The selected log is not a regular file.".into());
    }
    let mut file = fs::File::open(path).map_err(|error| format!("Could not open the server log: {error}"))?;
    let size = file.metadata().map_err(|error| format!("Could not inspect the server log: {error}"))?.len();
    let mut bytes = Vec::new();
    let truncated = if name.ends_with(".gz") {
        if size > MAX_COMPRESSED_BYTES {
            return Err("This archived log is too large to display (over 16 MB compressed).".into());
        }
        file.take(MAX_COMPRESSED_BYTES + 1).read_to_end(&mut bytes)
            .map_err(|error| format!("Could not read the server log: {error}"))?;
        if bytes.len() as u64 > MAX_COMPRESSED_BYTES {
            return Err("This archived log is too large to display (over 16 MB compressed).".into());
        }
        false
    } else {
        let truncated = size > TAIL_BYTES as u64;
        file.seek(SeekFrom::Start(size.saturating_sub(TAIL_BYTES as u64)))
            .map_err(|error| format!("Could not seek in the server log: {error}"))?;
        file.take(TAIL_BYTES as u64).read_to_end(&mut bytes)
            .map_err(|error| format!("Could not read the server log: {error}"))?;
        truncated
    };
    decode_log(&name, bytes, truncated)
}
