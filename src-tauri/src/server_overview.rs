use std::{sync::Arc, time::Duration};

use serde::Serialize;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::State;
use tokio::{io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt}, net::TcpStream, time::timeout};
use uuid::Uuid;

use crate::{remote_ssh::{self, RemoteSessionManager}, server_profiles::{ServerProfileStore, ServerTarget}, server_sessions::{LocalServerState, LocalSessionManager}};

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerUsage {
    players_online: Option<u32>,
    max_players: Option<u32>,
    cpu_percent: Option<f32>,
    memory_bytes: Option<u64>,
}

#[tauri::command]
pub async fn server_usage(
    profile_id: Uuid,
    port: u16,
    profiles: State<'_, ServerProfileStore>,
    local_sessions: State<'_, LocalSessionManager>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<ServerUsage, String> {
    let profile = profiles.list()?.into_iter().find(|item| item.id == profile_id)
        .ok_or("Server profile was not found.")?;
    let mut usage = ServerUsage::default();
    match &profile.target {
        ServerTarget::Local { .. } => {
            let status = local_sessions.status(profile_id)?;
            let Some(pid) = status.process_id.filter(|_| matches!(status.state, LocalServerState::Running)) else { return Ok(usage); };
            if let Ok((cpu, memory)) = tauri::async_runtime::spawn_blocking(move || local_process_usage(pid)).await {
                usage.cpu_percent = cpu;
                usage.memory_bytes = memory;
            }
            if let Ok(Ok((online, max))) = timeout(Duration::from_secs(3), ping_local(port)).await {
                usage.players_online = Some(online);
                usage.max_players = Some(max);
            }
        }
        ServerTarget::Remote { environment_id, .. } => {
            let environment = profiles.environment(*environment_id)?;
            if let Ok(Some((cpu, memory))) = remote_ssh::remote_process_usage(profile.clone(), environment.clone(), Arc::clone(remote_sessions.inner())).await {
                usage.cpu_percent = Some(cpu);
                usage.memory_bytes = Some(memory);
            }
            if let Ok((online, max)) = remote_ssh::ping_remote_server(profile, environment, port, Arc::clone(remote_sessions.inner())).await {
                usage.players_online = Some(online);
                usage.max_players = Some(max);
            }
        }
    }
    Ok(usage)
}

fn local_process_usage(pid: u32) -> (Option<f32>, Option<u64>) {
    let mut system = System::new();
    let pid = Pid::from_u32(pid);
    let refresh = ProcessRefreshKind::nothing().with_cpu().with_memory();
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), false, refresh);
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    system.refresh_processes_specifics(ProcessesToUpdate::Some(&[pid]), false, refresh);
    system.process(pid).map(|process| (Some(process.cpu_usage()), Some(process.memory())))
        .unwrap_or((None, None))
}

async fn ping_local(port: u16) -> Result<(u32, u32), String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.map_err(|error| error.to_string())?;
    ping_minecraft_stream(&mut stream, port).await
}

fn put_varint(mut value: u32, output: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 { byte |= 0x80; }
        output.push(byte);
        if value == 0 { break; }
    }
}

async fn read_varint<R: AsyncRead + Unpin>(stream: &mut R) -> Result<u32, String> {
    let mut value = 0u32;
    for shift in (0..35).step_by(7) {
        let byte = stream.read_u8().await.map_err(|error| error.to_string())?;
        value |= ((byte & 0x7f) as u32) << shift;
        if byte & 0x80 == 0 { return Ok(value); }
    }
    Err("The Minecraft status response has an invalid length.".into())
}

pub(crate) async fn ping_minecraft_stream<S: AsyncRead + AsyncWrite + Unpin>(stream: &mut S, port: u16) -> Result<(u32, u32), String> {
    let mut handshake = vec![0];
    put_varint(47, &mut handshake);
    put_varint(9, &mut handshake);
    handshake.extend_from_slice(b"localhost");
    handshake.extend_from_slice(&port.to_be_bytes());
    handshake.push(1);
    let mut packet = Vec::new();
    put_varint(handshake.len() as u32, &mut packet);
    packet.extend_from_slice(&handshake);
    packet.extend_from_slice(&[1, 0]);
    stream.write_all(&packet).await.map_err(|error| error.to_string())?;
    let length = read_varint(stream).await?;
    if length > 1024 * 1024 { return Err("The Minecraft status response is too large.".into()); }
    if read_varint(stream).await? != 0 { return Err("The Minecraft status response is invalid.".into()); }
    let json_length = read_varint(stream).await?;
    if json_length > length || json_length > 1024 * 1024 { return Err("The Minecraft status response is invalid.".into()); }
    let mut json = vec![0; json_length as usize];
    stream.read_exact(&mut json).await.map_err(|error| error.to_string())?;
    let response: serde_json::Value = serde_json::from_slice(&json).map_err(|error| error.to_string())?;
    let players = &response["players"];
    let online = players["online"].as_u64().and_then(|value| u32::try_from(value).ok()).ok_or("The Minecraft status response has no player count.")?;
    let max = players["max"].as_u64().and_then(|value| u32::try_from(value).ok()).ok_or("The Minecraft status response has no player limit.")?;
    Ok((online, max))
}
