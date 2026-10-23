use std::{
    collections::HashMap,
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use fastnbt::Value;
use flate2::read::{GzDecoder, ZlibDecoder};
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::{
    server_files::{local_server_root, resolve_existing},
    server_players,
    server_profiles::ServerProfileStore,
};

pub(crate) const MAX_TILES_PER_REQUEST: usize = 16;
pub(crate) const MAX_CHUNK_BYTES: usize = 1024 * 1024;
const MAX_DECOMPRESSED_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) const REGION_HEADER_BYTES: usize = 8192;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerrainRequest {
    pub x: i32,
    pub z: i32,
    pub known_timestamp: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerrainTile {
    pub x: i32,
    pub z: i32,
    pub timestamp: Option<u32>,
    pub status: &'static str,
    pub resolution: u8,
    pub rgba: Option<Vec<u8>>,
    pub read_bytes: u32,
}

pub(crate) struct RegionEntry {
    pub offset: u64,
    pub sectors: u8,
    pub timestamp: u32,
}

pub(crate) fn validate_request(
    dimension: &str,
    resolution: u8,
    requests: &[TerrainRequest],
) -> Result<(), String> {
    if !matches!(resolution, 1 | 4 | 16) {
        return Err("Choose a supported map detail level.".into());
    }
    if requests.len() > MAX_TILES_PER_REQUEST {
        return Err("Too many map tiles were requested at once.".into());
    }
    region_path_candidates("world", dimension, 0, 0)?;
    Ok(())
}

pub(crate) fn region_path_candidates(
    world: &str,
    dimension: &str,
    rx: i32,
    rz: i32,
) -> Result<Vec<String>, String> {
    let file = format!("r.{rx}.{rz}.mca");
    let base = match dimension {
        "minecraft:overworld" => return Ok(vec![format!("{world}/region/{file}")]),
        "minecraft:the_nether" => {
            return Ok(vec![
                format!("{world}/DIM-1/region/{file}"),
                format!("{world}_nether/DIM-1/region/{file}"),
            ]);
        }
        "minecraft:the_end" => {
            return Ok(vec![
                format!("{world}/DIM1/region/{file}"),
                format!("{world}_the_end/DIM1/region/{file}"),
            ]);
        }
        _ => dimension,
    };
    let (namespace, path) = base
        .split_once(':')
        .ok_or("This dimension has no readable terrain folder.")?;
    let safe = |segment: &str| {
        !segment.is_empty()
            && segment != "."
            && segment != ".."
            && segment.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'_' | b'-' | b'.')
            })
    };
    if !safe(namespace) || !path.split('/').all(safe) {
        return Err("This dimension has no readable terrain folder.".into());
    }
    Ok(vec![format!(
        "{world}/dimensions/{namespace}/{path}/region/{file}"
    )])
}

pub(crate) fn region_index(x: i32, z: i32) -> usize {
    (x.rem_euclid(32) + z.rem_euclid(32) * 32) as usize
}

pub(crate) fn region_entry(header: &[u8], index: usize) -> Result<Option<RegionEntry>, String> {
    if header.len() < REGION_HEADER_BYTES || index >= 1024 {
        return Err("The region header is incomplete.".into());
    }
    let offset_index = index * 4;
    let offset = u32::from_be_bytes([
        0,
        header[offset_index],
        header[offset_index + 1],
        header[offset_index + 2],
    ]) as u64;
    let sectors = header[offset_index + 3];
    if offset == 0 && sectors == 0 {
        return Ok(None);
    }
    if offset < 2 || sectors == 0 {
        return Err("The region contains an invalid chunk location.".into());
    }
    let timestamp_index = 4096 + offset_index;
    let timestamp = u32::from_be_bytes(
        header[timestamp_index..timestamp_index + 4]
            .try_into()
            .unwrap(),
    );
    Ok(Some(RegionEntry {
        offset: offset * 4096,
        sectors,
        timestamp,
    }))
}

pub(crate) fn missing(request: &TerrainRequest, resolution: u8) -> TerrainTile {
    TerrainTile {
        x: request.x,
        z: request.z,
        timestamp: None,
        status: "missing",
        resolution,
        rgba: None,
        read_bytes: 0,
    }
}

pub(crate) fn unchanged(request: &TerrainRequest, resolution: u8, timestamp: u32) -> TerrainTile {
    TerrainTile {
        x: request.x,
        z: request.z,
        timestamp: Some(timestamp),
        status: "unchanged",
        resolution,
        rgba: None,
        read_bytes: 0,
    }
}

pub(crate) fn failed(request: &TerrainRequest, resolution: u8) -> TerrainTile {
    TerrainTile {
        x: request.x,
        z: request.z,
        timestamp: None,
        status: "error",
        resolution,
        rgba: None,
        read_bytes: 0,
    }
}

pub(crate) fn decode_region_payload(
    request: &TerrainRequest,
    resolution: u8,
    timestamp: u32,
    data: &[u8],
) -> TerrainTile {
    match decode_pixels(data, resolution) {
        Ok(Some(rgba)) => TerrainTile {
            x: request.x,
            z: request.z,
            timestamp: Some(timestamp),
            status: "tile",
            resolution,
            rgba: Some(rgba),
            read_bytes: data.len() as u32,
        },
        Ok(None) => TerrainTile {
            read_bytes: data.len() as u32,
            ..missing(request, resolution)
        },
        Err(_) => TerrainTile {
            read_bytes: data.len() as u32,
            ..failed(request, resolution)
        },
    }
}

fn decode_pixels(data: &[u8], resolution: u8) -> Result<Option<Vec<u8>>, String> {
    let (&compression, compressed) = data.split_first().ok_or("The chunk is empty.")?;
    if compression & 0x80 != 0 {
        return Err("External chunk storage is not supported by this map.".into());
    }
    let mut decoded = Vec::new();
    match compression {
        1 => GzDecoder::new(compressed)
            .take(MAX_DECOMPRESSED_BYTES + 1)
            .read_to_end(&mut decoded),
        2 => ZlibDecoder::new(compressed)
            .take(MAX_DECOMPRESSED_BYTES + 1)
            .read_to_end(&mut decoded),
        3 => compressed
            .take(MAX_DECOMPRESSED_BYTES + 1)
            .read_to_end(&mut decoded),
        _ => return Err("This chunk compression is not supported by the map.".into()),
    }
    .map_err(|error| format!("Could not decode chunk: {error}"))?;
    if decoded.len() as u64 > MAX_DECOMPRESSED_BYTES {
        return Err("The chunk is too large to map.".into());
    }
    let root: HashMap<String, Value> =
        fastnbt::from_bytes(&decoded).map_err(|error| error.to_string())?;
    let chunk = root.get("Level").and_then(compound).unwrap_or(&root);
    let status = chunk.get("Status").and_then(string).unwrap_or("");
    if status != "full" && status != "minecraft:full" {
        return Ok(None);
    }
    let Some(Value::List(raw_sections)) = chunk.get("sections").or_else(|| chunk.get("Sections"))
    else {
        return Ok(None);
    };
    let mut sections = raw_sections
        .iter()
        .filter_map(parse_section)
        .collect::<Vec<_>>();
    sections.sort_by(|left, right| right.y.cmp(&left.y));
    if sections.is_empty() {
        return Ok(None);
    }
    let resolution = resolution as usize;
    let mut rgba = vec![0_u8; resolution * resolution * 4];
    for tz in 0..resolution {
        for tx in 0..resolution {
            let x = (tx * 16 + 8) / resolution;
            let z = (tz * 16 + 8) / resolution;
            if let Some((block, height)) = surface_block(&sections, x, z) {
                let color = block_color(block);
                let north_height = if z > 0 {
                    surface_block(&sections, x, z - 1).map(|(_, y)| y).unwrap_or(height)
                } else {
                    height
                };
                let slope = (height - north_height).clamp(-3, 3);
                let shade = if block.contains("water") {
                    let depth = water_depth(&sections, x, z, height);
                    (1.05 - depth as f32 * 0.055).clamp(0.55, 1.05)
                } else {
                    (0.94 + slope as f32 * 0.10).clamp(0.64, 1.18)
                };
                let at = (tz * resolution + tx) * 4;
                for channel in 0..3 {
                    rgba[at + channel] = (color[channel] as f32 * shade).min(255.0) as u8;
                }
                rgba[at + 3] = 255;
            }
        }
    }
    Ok(Some(rgba))
}

fn compound(value: &Value) -> Option<&HashMap<String, Value>> {
    if let Value::Compound(value) = value {
        Some(value)
    } else {
        None
    }
}

fn string(value: &Value) -> Option<&str> {
    if let Value::String(value) = value {
        Some(value)
    } else {
        None
    }
}

struct Section<'a> {
    y: i32,
    palette: Vec<&'a str>,
    data: Option<&'a [i64]>,
}

fn parse_section(value: &Value) -> Option<Section<'_>> {
    let section = compound(value)?;
    let y = section.get("Y")?.as_i64()? as i32;
    let states = section.get("block_states").and_then(compound);
    let palette = states
        .and_then(|states| states.get("palette"))
        .or_else(|| section.get("Palette"))?;
    let Value::List(palette) = palette else {
        return None;
    };
    let names = palette
        .iter()
        .filter_map(|block| compound(block)?.get("Name").and_then(string))
        .collect::<Vec<_>>();
    if names.len() != palette.len() || names.is_empty() {
        return None;
    }
    let data = states
        .and_then(|states| states.get("data"))
        .or_else(|| section.get("BlockStates"));
    let data = match data {
        Some(Value::LongArray(longs)) => Some(&longs[..]),
        _ => None,
    };
    Some(Section {
        y,
        palette: names,
        data,
    })
}

fn surface_block<'a>(sections: &'a [Section<'a>], x: usize, z: usize) -> Option<(&'a str, i32)> {
    for section in sections {
        for y in (0..16).rev() {
            let Some(block) = block_at(section, x, y, z) else {
                continue;
            };
            if !matches!(
                block,
                "minecraft:air"
                    | "minecraft:cave_air"
                    | "minecraft:void_air"
                    | "minecraft:structure_void"
                    | "minecraft:bedrock"
            ) {
                return Some((block, section.y * 16 + y as i32));
            }
        }
    }
    None
}

fn water_depth(sections: &[Section<'_>], x: usize, z: usize, surface_y: i32) -> usize {
    let mut depth = 0;
    for y in (surface_y - 11..surface_y).rev() {
        let Some(section) = sections.iter().find(|section| section.y == y.div_euclid(16)) else { break };
        if !block_at(section, x, y.rem_euclid(16) as usize, z)
            .is_some_and(|block| block.contains("water")) { break }
        depth += 1;
    }
    depth
}

fn block_at<'a>(section: &'a Section<'_>, x: usize, y: usize, z: usize) -> Option<&'a str> {
    if section.palette.len() == 1 {
        return Some(section.palette[0]);
    }
    let bits = (usize::BITS - (section.palette.len() - 1).leading_zeros()).max(4) as usize;
    let per_long = 64 / bits;
    let index = (y * 16 + z) * 16 + x;
    let data = section.data?;
    let mask = (1_u64 << bits) - 1;
    let palette_index = if data.len() >= 4096_usize.div_ceil(per_long) {
        let packed = *data.get(index / per_long)? as u64;
        ((packed >> ((index % per_long) * bits)) & mask) as usize
    } else {
        let bit_index = index * bits;
        let offset = bit_index % 64;
        let low = (*data.get(bit_index / 64)? as u64) >> offset;
        let high = if offset + bits > 64 {
            (*data.get(bit_index / 64 + 1)? as u64) << (64 - offset)
        } else {
            0
        };
        ((low | high) & mask) as usize
    };
    section.palette.get(palette_index).copied()
}

fn block_color(block: &str) -> [u8; 3] {
    let name = block.rsplit(':').next().unwrap_or(block);
    if name.contains("water") {
        [24, 99, 198]
    } else if name.contains("lava") {
        [241, 104, 35]
    } else if name.contains("snow") || name.contains("quartz") {
        [244, 244, 237]
    } else if name.contains("ice") {
        [159, 211, 227]
    } else if name.contains("sand") || name.contains("sandstone") {
        [237, 218, 153]
    } else if name.contains("leaves") || name.contains("leaf") || name.contains("vine") {
        [65, 139, 52]
    } else if name.contains("grass")
        || name.contains("moss")
        || name.contains("fern")
    {
        [123, 176, 71]
    } else if name.contains("log") || name.contains("wood") || name.contains("plank") {
        [156, 121, 75]
    } else if name.contains("dirt") || name.contains("mud") || name.contains("podzol") {
        [143, 106, 75]
    } else if name.contains("netherrack") || name.contains("crimson") {
        [150, 66, 68]
    } else if name.contains("warped") {
        [54, 145, 141]
    } else if name.contains("end_stone") {
        [224, 223, 164]
    } else if name.contains("deepslate") || name.contains("basalt") {
        [87, 90, 96]
    } else if name.contains("stone") || name.contains("ore") || name.contains("tuff") {
        [146, 148, 145]
    } else {
        let hash = name.bytes().fold(0_u32, |value, byte| {
            value.wrapping_mul(31).wrapping_add(byte as u32)
        });
        [
            95 + (hash % 55) as u8,
            95 + ((hash >> 7) % 55) as u8,
            90 + ((hash >> 14) % 55) as u8,
        ]
    }
}

enum LocalRegion {
    Missing,
    Failed,
    Open {
        file: fs::File,
        header: [u8; REGION_HEADER_BYTES],
    },
}

fn open_local_region(root: &Path, candidates: Vec<String>) -> LocalRegion {
    for relative in candidates {
        let candidate = root.join(&relative);
        match fs::symlink_metadata(&candidate) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return LocalRegion::Failed,
            Ok(metadata) if !metadata.file_type().is_file() => return LocalRegion::Failed,
            Ok(_) => {}
        }
        let Ok(path) = resolve_existing(root, &relative) else {
            return LocalRegion::Failed;
        };
        let Ok(mut file) = fs::File::open(path) else {
            return LocalRegion::Failed;
        };
        let mut header = [0_u8; REGION_HEADER_BYTES];
        if file.read_exact(&mut header).is_err() {
            return LocalRegion::Failed;
        }
        return LocalRegion::Open { file, header };
    }
    LocalRegion::Missing
}

#[tauri::command]
pub async fn read_local_terrain_tiles(
    profile_id: Uuid,
    dimension: String,
    resolution: u8,
    requests: Vec<TerrainRequest>,
    profiles: State<'_, ServerProfileStore>,
) -> Result<Vec<TerrainTile>, String> {
    validate_request(&dimension, resolution, &requests)?;
    let root = local_server_root(profile_id, &profiles)?;
    tauri::async_runtime::spawn_blocking(move || {
        read_local_terrain_tiles_from_root(root, dimension, resolution, requests)
    })
    .await
    .map_err(|error| format!("Terrain work did not finish: {error}"))?
}

fn read_local_terrain_tiles_from_root(
    root: PathBuf,
    dimension: String,
    resolution: u8,
    requests: Vec<TerrainRequest>,
) -> Result<Vec<TerrainTile>, String> {
    let properties = match resolve_existing(&root, "server.properties") {
        Ok(path) => Some(
            fs::read(path).map_err(|error| format!("Could not read server.properties: {error}"))?,
        ),
        Err(_) if !root.join("server.properties").exists() => None,
        Err(error) => return Err(error),
    };
    let world = server_players::world_name(properties.as_deref())?;
    let mut regions = HashMap::<(i32, i32), LocalRegion>::new();
    let mut results = Vec::with_capacity(requests.len());
    for request in &requests {
        let region = (request.x.div_euclid(32), request.z.div_euclid(32));
        if !regions.contains_key(&region) {
            let candidates = region_path_candidates(&world, &dimension, region.0, region.1)?;
            regions.insert(region, open_local_region(&root, candidates));
        }
        let Some(LocalRegion::Open { file, header }) = regions.get_mut(&region) else {
            results.push(
                if matches!(regions.get(&region), Some(LocalRegion::Failed)) {
                    failed(request, resolution)
                } else {
                    missing(request, resolution)
                },
            );
            continue;
        };
        let result = (|| -> Result<TerrainTile, String> {
            let Some(entry) = region_entry(header, region_index(request.x, request.z))? else {
                return Ok(missing(request, resolution));
            };
            if request.known_timestamp == Some(entry.timestamp) {
                return Ok(unchanged(request, resolution, entry.timestamp));
            }
            file.seek(SeekFrom::Start(entry.offset))
                .map_err(|error| error.to_string())?;
            let mut length = [0_u8; 4];
            file.read_exact(&mut length)
                .map_err(|error| error.to_string())?;
            let length = u32::from_be_bytes(length) as usize;
            if length == 0 || length > MAX_CHUNK_BYTES || length + 4 > entry.sectors as usize * 4096
            {
                return Err("The chunk length is invalid.".into());
            }
            let mut data = vec![0_u8; length];
            file.read_exact(&mut data)
                .map_err(|error| error.to_string())?;
            Ok(decode_region_payload(
                request,
                resolution,
                entry.timestamp,
                &data,
            ))
        })();
        results.push(result.unwrap_or_else(|_| failed(request, resolution)));
    }
    Ok(results)
}
