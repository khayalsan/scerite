use std::{
    collections::HashMap,
    env,
    fs::File,
    path::{Path, PathBuf},
    io::SeekFrom,
    sync::{Arc, Mutex},
    time::Duration,
    time::Instant,
};

use russh::{
    ChannelMsg,
    client::{self, Handle},
    keys::{
        self, PublicKeyOrCertificate,
        agent::{AgentIdentity, client::AgentClient},
    },
};
use russh_sftp::{
    client::{SftpSession, error::Error as SftpError, rawsession::RawSftpSession},
    protocol::{FileAttributes, FileType, OpenFlags, Packet, StatusCode},
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};
use tokio::{
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    runtime::{Builder, Runtime},
};
use uuid::Uuid;

use crate::eula;
use crate::server_profiles::{
    LocalLaunchConfig, ServerEnvironment, ServerProfile, ServerProfileStore, ServerTarget,
};
use crate::java_runtime::JavaPackage;
use crate::server_logs::{self, ServerLogSnapshot};
use crate::server_players::{self, PlayerData, PlayerRosterFiles};
use crate::server_port;
use crate::terrain::{self, TerrainRequest, TerrainTile};
use crate::server_files::validate_server_icon;
use crate::world_management::{self, WorldBackup, WorldSettings};
use crate::velocity_download;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_CONSOLE_BYTES: u64 = 512 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 1000;
const CONNECTION_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const SCERITE_VELOCITY_JAR: &[u8] = include_bytes!("../resources/scerite-velocity.jar");

pub struct RemoteSessionManager {
    sessions: Mutex<HashMap<Uuid, CachedConnection>>,
    runtime: Arc<Runtime>,
}

struct CachedConnection {
    config: RemoteConnectionConfig,
    session: Arc<Handle<SshHandler>>,
    last_used: Instant,
}

impl Default for RemoteSessionManager {
    fn default() -> Self {
        let runtime = Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("create remote SSH runtime");
        Self {
            sessions: Mutex::new(HashMap::new()),
            runtime: Arc::new(runtime),
        }
    }
}

#[derive(Clone)]
struct RemoteConnection {
    runtime: Arc<Runtime>,
    handle: Arc<Handle<SshHandler>>,
}

struct RemoteSftp {
    runtime: Arc<Runtime>,
    handle: Arc<Handle<SshHandler>>,
    inner: SftpSession,
}

#[derive(Clone, PartialEq, Eq)]
struct RemoteConnectionConfig {
    host: String,
    port: u16,
    username: String,
    identity_file: Option<String>,
}

impl RemoteSessionManager {
    fn session(
        self: &Arc<Self>,
        profile: &ServerProfile,
        environment: &ServerEnvironment,
    ) -> Result<RemoteConnection, String> {
        let config = remote_connection_config(environment);
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "Remote session manager is unavailable.".to_owned())?;
        sessions.retain(|_, connection| connection.last_used.elapsed() < CONNECTION_IDLE_TIMEOUT);
        if let Some(connection) = sessions.get_mut(&profile.id) {
            if connection.config == config {
                connection.last_used = Instant::now();
                if !connection.session.is_closed() {
                    return Ok(RemoteConnection {
                        runtime: Arc::clone(&self.runtime),
                        handle: Arc::clone(&connection.session),
                    });
                }
            }
            sessions.remove(&profile.id);
        }
        drop(sessions);

        let session = connect_remote(
            &self.runtime,
            &config.host,
            config.port,
            &config.username,
            config.identity_file.as_deref(),
        )?;
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "Remote session manager is unavailable.".to_owned())?;
        sessions.insert(
            profile.id,
            CachedConnection {
                config,
                session: Arc::clone(&session),
                last_used: Instant::now(),
            },
        );
        Ok(RemoteConnection {
            runtime: Arc::clone(&self.runtime),
            handle: session,
        })
    }

    fn invalidate(&self, profile_id: Uuid) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.remove(&profile_id);
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServerStatus {
    pub state: String,
    pub tmux_available: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteConsoleSnapshot {
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServerFileEntry {
    pub name: String,
    pub path: String,
    pub directory: bool,
    pub size: u64,
}

#[tauri::command]
pub async fn test_remote_connection(
    profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<(), String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(
        profile,
        environment,
        Arc::clone(remote_sessions.inner()),
        |_, _| Ok(()),
    )
    .await
}

#[tauri::command]
pub async fn test_remote_connection_draft(
    host: String,
    port: u16,
    username: String,
    identity_file: Option<String>,
) -> Result<(), String> {
    let host = host.trim().to_owned();
    let username = username.trim().to_owned();
    if host.is_empty() || username.is_empty() || port == 0 {
        return Err("Enter a host, SSH user, and valid port before testing the connection.".into());
    }
    let identity_file = identity_file.filter(|path| !path.trim().is_empty());
    tauri::async_runtime::spawn_blocking(move || {
        let runtime = Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|error| format!("Could not start the SSH runtime: {error}"))?;
        connect_remote(&runtime, &host, port, &username, identity_file.as_deref()).map(|_| ())
    })
    .await
    .map_err(|error| format!("SSH connection test failed to finish: {error}"))?
}

#[tauri::command]
pub async fn remote_server_status(
    profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<RemoteServerStatus, String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(
        profile,
        environment,
        Arc::clone(remote_sessions.inner()),
        |session, profile| {
            let running = remote_is_running(session, profile)?;
            let state = if running == Some(true) {
                "running"
            } else if let Some(root) = optional_remote_root(session, profile)? {
                if remote_directory_has_java_process(session, &root)? {
                    "unmanaged"
                } else {
                    "stopped"
                }
            } else {
                "stopped"
            };
            Ok(RemoteServerStatus {
                state: state.into(),
                tmux_available: running.is_some(),
            })
        },
    )
    .await
}

#[tauri::command]
pub async fn start_remote_server(
    profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<RemoteServerStatus, String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    let auto_accept_eula = profiles.auto_accept_eula()?;
    let environment_root = environment.root_directory.clone();
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        let ServerTarget::Remote { directory, launch, .. } = &profile.target else {
            return Err("Remote server controls are only available for remote profiles.".into());
        };
        if directory.trim().is_empty() || launch.executable.trim().is_empty() {
            return Err("Set the remote server folder and launch executable in the profile before starting it.".into());
        }
        let managed = remote_is_running(session, profile)?;
        if managed == Some(true) {
            return Ok(RemoteServerStatus { state: "running".into(), tmux_available: true });
        }
        if managed.is_none() {
            return Err("tmux is required on the remote Linux server for persistent server control.".into());
        }

        let root = remote_root(session, directory)?;
        if remote_directory_has_java_process(session, &root)? {
            return Err("A Java process is already using the remote server folder outside scerite. Stop it there before starting this profile.".into());
        }
        ensure_remote_port(session, &root)?;
        if auto_accept_eula {
            enable_remote_eula(session, &root)?;
        }
        let managed_java = if let Some(java) = launch.managed_java.as_deref() {
            let runtime_root = remote_root(session, &environment_root)?;
            let actual = session.sftp()?.realpath(Path::new(java))?;
            if !Path::new(&actual).starts_with(Path::new(&runtime_root).join("runtimes/java")) {
                return Err("The managed Java executable is outside the configured runtime folder.".into());
            }
            let usable = execute(session, &format!("test -x {}", shell_quote(&actual)))?;
            if usable.exit_code != 0 {
                return Err("The managed Java executable is missing or is not executable.".into());
            }
            Some(actual)
        } else { None };
        let command = launch_command(launch, managed_java.as_deref());
        let name = shell_quote(&session_name(profile.id));
        let existing = execute(session, &format!("tmux has-session -t {name}"))?;
        if existing.exit_code == 0 {
            let removed = execute(session, &format!("tmux kill-session -t {name}"))?;
            if removed.exit_code != 0 {
                return Err(remote_command_error("Could not clear the previous remote console", removed));
            }
        }
        let tmux_command = format!(
            "tmux new-session -d -s {name} -c {} 'sleep 30'",
            shell_quote(&root),
        );
        let result = execute(session, &tmux_command)?;
        if result.exit_code != 0 {
            return Err(remote_command_error("Could not start the remote server", result));
        }
        let retain = execute(
            session,
            &format!("tmux set-window-option -t {name} remain-on-exit on"),
        )?;
        if retain.exit_code != 0 {
            let _ = execute(session, &format!("tmux kill-session -t {name}"));
            return Err(remote_command_error("Could not retain the remote console after exit", retain));
        }
        let launch = execute(
            session,
            &format!(
                "tmux respawn-pane -k -t {name} -c {} {}",
                shell_quote(&root),
                shell_quote(&command),
            ),
        )?;
        if launch.exit_code != 0 {
            let _ = execute(session, &format!("tmux kill-session -t {name}"));
            return Err(remote_command_error("Could not launch the remote server", launch));
        }
        if remote_is_running(session, profile)? == Some(false) {
            let output = execute(session, &format!("tmux capture-pane -p -S -1000 -t {name}"))?;
            let details = String::from_utf8_lossy(&output.stdout);
            let last_lines = details
                .lines()
                .rev()
                .take(12)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            return Err(format!(
                "The server exited during startup. Check its console for the full output.\n{last_lines}"
            ));
        }
        Ok(RemoteServerStatus { state: "running".into(), tmux_available: true })
    })
    .await
}

#[tauri::command]
pub async fn stop_remote_server(
    profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<RemoteServerStatus, String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), |session, profile| {
        let running = remote_is_running(session, profile)?;
        if running != Some(true) {
            if let Some(root) = optional_remote_root(session, profile)? {
                if remote_directory_has_java_process(session, &root)? {
                    return Err("The server is running outside scerite's tmux session. Stop it using its current manager first.".into());
                }
            }
            return Ok(RemoteServerStatus { state: "stopped".into(), tmux_available: running.is_some() });
        }
        let name = shell_quote(&session_name(profile.id));
        let command = format!(
            "tmux send-keys -t {name} -l stop && tmux send-keys -t {name} Enter"
        );
        let result = execute(session, &command)?;
        if result.exit_code != 0 {
            let details = String::from_utf8_lossy(&result.stderr);
            let pane_missing = details.contains("can't find pane")
                || details.contains("can't find window")
                || details.contains("can't find session");
            if pane_missing || remote_is_running(session, profile)? == Some(false) {
                if let Some(root) = optional_remote_root(session, profile)? {
                    if remote_directory_has_java_process(session, &root)? {
                        return Err("The server is running outside scerite's tmux session. Stop it using its current manager first.".into());
                    }
                }
                return Ok(RemoteServerStatus { state: "stopped".into(), tmux_available: true });
            }
            return Err(remote_command_error("Could not send the stop command", result));
        }
        Ok(RemoteServerStatus { state: "stopping".into(), tmux_available: true })
    })
    .await
}

#[tauri::command]
pub async fn remote_server_console(
    profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<RemoteConsoleSnapshot, String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), |session, profile| {
        let running = remote_is_running(session, profile)?;
        if running != Some(true) {
            if let Some(root) = optional_remote_root(session, profile)? {
                if remote_directory_has_java_process(session, &root)? {
                    return Err("The server is running outside scerite's tmux session; its console is not attached.".into());
                }
            }
        }
        let command = format!(
            "tmux capture-pane -p -S -1000 -t {}",
            shell_quote(&session_name(profile.id))
        );
        let result = execute(session, &command)?;
        if result.exit_code != 0 {
            let details = String::from_utf8_lossy(&result.stderr);
            if running != Some(true)
                || details.contains("can't find pane")
                || details.contains("can't find window")
                || details.contains("can't find session")
            {
                return Ok(RemoteConsoleSnapshot { lines: Vec::new() });
            }
            return Err(remote_command_error("Could not read the remote console", result));
        }
        let output = String::from_utf8_lossy(&result.stdout);
        Ok(RemoteConsoleSnapshot {
            lines: output.lines().map(str::to_owned).collect(),
        })
    })
    .await
}

#[tauri::command]
pub async fn send_remote_console_input(
    profile_id: Uuid,
    input: String,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<(), String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    if input.len() > 4096
        || input
            .chars()
            .any(|character| matches!(character, '\0' | '\r' | '\n'))
    {
        return Err("Console commands must be one line and 4 KB or less.".into());
    }
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        if remote_is_running(session, profile)? != Some(true) {
            if let Some(root) = optional_remote_root(session, profile)? {
                if remote_directory_has_java_process(session, &root)? {
                    return Err("The server is running outside scerite's tmux session; its console is not attached.".into());
                }
            }
            return Err("The remote server is not running.".into());
        }
        let name = shell_quote(&session_name(profile.id));
        let command = format!(
            "tmux send-keys -t {name} -l {}",
            shell_quote(&input)
        );
        let result = execute(session, &command)?;
        if result.exit_code != 0 {
            return Err(remote_command_error("Could not send the console command", result));
        }
        let enter = execute(session, &format!("tmux send-keys -t {name} Enter"))?;
        if enter.exit_code != 0 {
            return Err(remote_command_error("Could not submit the console command", enter));
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn list_remote_server_files(
    profile_id: Uuid,
    relative_path: String,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<Vec<RemoteServerFileEntry>, String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        let root = configured_remote_root(profile)?;
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let canonical_root = remote_root_with_sftp(&sftp, root)?;
        let requested = resolve_remote_relative(&canonical_root, &relative_path, true)?;
        reject_remote_symlink_path(&sftp, &canonical_root, &requested)?;
        let directory = canonical_inside(&sftp, &canonical_root, &requested)?;
        let directory_stat = sftp.stat(Path::new(&directory)).map_err(|error| format!("Could not read remote folder: {error}"))?;
        if directory_stat.file_type() != FileType::Dir {
            return Err("The remote path is not a folder.".into());
        }

        let prefix = relative_path.trim_matches('/');
        let mut entries = Vec::new();
        let directory_entries = sftp.read_dir(Path::new(&directory)).map_err(|error| format!("Could not list remote files: {error}"))?;
        for (name, stat) in directory_entries {
            if name == "." || name == ".." { continue; }
            if name.is_empty() || name.chars().any(char::is_control) { continue; }
            let kind = stat.file_type();
            if kind == FileType::Symlink { continue; }
            if entries.len() == MAX_DIRECTORY_ENTRIES {
                return Err(format!("This folder has more than {MAX_DIRECTORY_ENTRIES} entries; open a subfolder to continue."));
            }
            let entry_path = if prefix.is_empty() { name.to_owned() } else { format!("{prefix}/{name}") };
            entries.push(RemoteServerFileEntry {
                name: name.to_owned(),
                path: entry_path,
                directory: kind == FileType::Dir,
                size: stat.size.unwrap_or(0),
            });
        }
        entries.sort_by(|left, right| right.directory.cmp(&left.directory).then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase())));
        Ok(entries)
    })
    .await
}

#[tauri::command]
pub async fn list_remote_worlds(
    profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<Vec<RemoteServerFileEntry>, String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(
        profile,
        environment,
        Arc::clone(remote_sessions.inner()),
        move |session, profile| {
            let sftp = session
                .sftp()
                .map_err(|error| format!("Could not open SFTP: {error}"))?;
            let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
            let entries = sftp
                .read_dir(Path::new(&root))
                .map_err(|error| format!("Could not list remote worlds: {error}"))?;
            if entries.len() > MAX_DIRECTORY_ENTRIES {
                return Err(format!(
                    "The server folder has more than {MAX_DIRECTORY_ENTRIES} entries."
                ));
            }
            let mut worlds = Vec::new();
            for (name, attributes) in entries {
                if attributes.file_type() != FileType::Dir
                    || name.is_empty()
                    || name == "."
                    || name == ".."
                    || name.contains('/')
                    || name.chars().any(char::is_control)
                {
                    continue;
                }
                let level_path = format!("{root}/{name}/level.dat");
                if !sftp
                    .lstat(Path::new(&level_path))
                    .is_ok_and(|metadata| metadata.file_type() == FileType::File)
                {
                    continue;
                }
                worlds.push(RemoteServerFileEntry {
                    path: name.clone(),
                    name,
                    directory: true,
                    size: 0,
                });
            }
            worlds.sort_by_key(|world| world.name.to_lowercase());
            Ok(worlds)
        },
    )
    .await
}

#[tauri::command]
pub async fn read_remote_server_file(
    profile_id: Uuid,
    relative_path: String,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<String, String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(
        profile,
        environment,
        Arc::clone(remote_sessions.inner()),
        move |session, profile| {
            let sftp = session
                .sftp()
                .map_err(|error| format!("Could not open SFTP: {error}"))?;
            let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
            let requested = resolve_remote_relative(&root, &relative_path, false)?;
            reject_remote_symlink_path(&sftp, &root, &requested)?;
            let canonical = canonical_inside(&sftp, &root, &requested)?;
            let metadata = sftp
                .lstat(Path::new(&canonical))
                .map_err(|error| format!("Could not read remote file metadata: {error}"))?;
            if metadata.file_type() != FileType::File {
                return Err("Only regular remote files can be opened.".into());
            }
            if metadata.size.unwrap_or(0) > MAX_FILE_BYTES {
                return Err("Files larger than 1 MB cannot be edited here.".into());
            }
            let bytes = sftp
                .read_file(Path::new(&canonical))
                .map_err(|error| format!("Could not read remote file: {error}"))?;
            if bytes.len() as u64 > MAX_FILE_BYTES {
                return Err("Files larger than 1 MB cannot be edited here.".into());
            }
            String::from_utf8(bytes).map_err(|_| "This remote file is not UTF-8 text.".into())
        },
    )
    .await
}

#[tauri::command]
pub async fn read_remote_server_log_start(
    profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<String, String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(
        profile,
        environment,
        Arc::clone(remote_sessions.inner()),
        move |session, profile| {
            let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
            let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
            let requested = resolve_remote_relative(&root, "logs/latest.log", false)?;
            reject_remote_symlink_path(&sftp, &root, &requested)?;
            let canonical = canonical_inside(&sftp, &root, &requested)?;
            let metadata = sftp.lstat(Path::new(&canonical))
                .map_err(|error| format!("Could not inspect the server log: {error}"))?;
            if metadata.file_type() != FileType::File {
                return Err("The server log is not a regular file.".into());
            }
            let bytes = sftp.read_prefix(Path::new(&canonical), 256 * 1024)
                .map_err(|error| format!("Could not read the server log: {error}"))?;
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        },
    ).await
}

#[tauri::command]
pub async fn read_remote_server_log(
    profile_id: Uuid,
    name: String,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<ServerLogSnapshot, String> {
    if !server_logs::valid_log_name(&name) {
        return Err("Choose a log file from this server's logs folder.".into());
    }
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        let requested = resolve_remote_relative(&root, &format!("logs/{name}"), false)?;
        reject_remote_symlink_path(&sftp, &root, &requested)?;
        let canonical = canonical_inside(&sftp, &root, &requested)?;
        let metadata = sftp.lstat(Path::new(&canonical))
            .map_err(|error| format!("Could not inspect the server log: {error}"))?;
        if metadata.file_type() != FileType::File {
            return Err("The selected log is not a regular file.".into());
        }
        let (bytes, truncated) = if name.ends_with(".gz") {
            let limit = server_logs::max_compressed_bytes();
            if metadata.size.is_some_and(|size| size > limit) {
                return Err("This archived log is too large to display (over 16 MB compressed).".into());
            }
            let bytes = sftp.read_prefix(Path::new(&canonical), limit as usize + 1)
                .map_err(|error| format!("Could not read the server log: {error}"))?;
            if bytes.len() as u64 > limit {
                return Err("This archived log is too large to display (over 16 MB compressed).".into());
            }
            (bytes, false)
        } else {
            sftp.read_tail(Path::new(&canonical), server_logs::tail_bytes())
                .map_err(|error| format!("Could not read the server log: {error}"))?
        };
        server_logs::decode_log(&name, bytes, truncated)
    }).await
}

fn optional_remote_player_file(
    sftp: &RemoteSftp,
    root: &str,
    relative: &str,
) -> Result<Option<Vec<u8>>, String> {
    let path = resolve_remote_relative(root, relative, false)?;
    let metadata = match sftp.lstat_optional(Path::new(&path)) {
        Ok(metadata) => metadata,
        Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => return Ok(None),
        Err(error) => return Err(format!("Could not inspect player data: {error}")),
    };
    if metadata.file_type() != FileType::File {
        return Err("A player data path is not a regular file.".into());
    }
    reject_remote_symlink_path(sftp, root, &path)?;
    let canonical = canonical_inside(sftp, root, &path)?;
    let limit = server_players::max_player_file_bytes();
    if metadata.size.is_some_and(|size| size > limit) {
        return Err("A player data file is too large to display (over 8 MB).".into());
    }
    let bytes = sftp.read_prefix(Path::new(&canonical), limit as usize + 1)
        .map_err(|error| format!("Could not read player data: {error}"))?;
    if bytes.len() as u64 > limit {
        return Err("A player data file is too large to display (over 8 MB).".into());
    }
    Ok(Some(bytes))
}

fn optional_remote_player_list(sftp: &RemoteSftp, root: &str, relative: &str) -> Result<Vec<String>, String> {
    let path = resolve_remote_relative(root, relative, false)?;
    let metadata = match sftp.lstat_optional(Path::new(&path)) {
        Ok(metadata) => metadata,
        Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => return Ok(Vec::new()),
        Err(error) => return Err(format!("Could not inspect player files: {error}")),
    };
    if metadata.file_type() != FileType::Dir { return Err("A player list path is not a folder.".into()); }
    reject_remote_symlink_path(sftp, root, &path)?;
    let canonical = canonical_inside(sftp, root, &path)?;
    sftp.read_player_uuids(Path::new(&canonical))
        .map_err(|error| format!("Could not list player files: {error}"))
}

#[tauri::command]
pub async fn read_remote_player_usercache(
    profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<String, String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        optional_remote_player_file(&sftp, &root, "usercache.json")?
            .map(|bytes| String::from_utf8(bytes).map_err(|error| error.to_string()))
            .transpose()
            .map(|value| value.unwrap_or_default())
    }).await
}

#[tauri::command]
pub async fn read_remote_player_roster(
    profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<PlayerRosterFiles, String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        let properties = optional_remote_player_file(&sftp, &root, "server.properties")?;
        let world = server_players::world_name(properties.as_deref())?;
        let read_text = |name| -> Result<String, String> {
            optional_remote_player_file(&sftp, &root, name)?.map(|bytes| String::from_utf8(bytes).map_err(|error| error.to_string()))
                .transpose().map(|value| value.unwrap_or_default())
        };
        let mut uuids = Vec::new();
        for path in server_players::player_list_paths(&world) {
            uuids.extend(optional_remote_player_list(&sftp, &root, &path)?);
        }
        uuids.sort();
        uuids.dedup();
        Ok(PlayerRosterFiles {
            cache: read_text("usercache.json")?, whitelist: read_text("whitelist.json")?,
            ops: read_text("ops.json")?, bans: read_text("banned-players.json")?,
            banned_ips: read_text("banned-ips.json")?, uuids,
        })
    }).await
}

#[tauri::command]
pub async fn read_remote_player_data(
    profile_id: Uuid,
    uuid: Uuid,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<PlayerData, String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        let properties = optional_remote_player_file(&sftp, &root, "server.properties")?;
        let world = server_players::world_name(properties.as_deref())?;
        let mut player_file = None;
        for path in server_players::player_data_paths(&world, uuid) {
            player_file = optional_remote_player_file(&sftp, &root, &path)?;
            if player_file.is_some() { break; }
        }
        let mut stats_file = None;
        for path in server_players::player_stats_paths(&world, uuid) {
            stats_file = optional_remote_player_file(&sftp, &root, &path)?;
            if stats_file.is_some() { break; }
        }
        Ok(server_players::decode_player_data(player_file, stats_file))
    }).await
}

#[tauri::command]
pub async fn read_remote_player_locations(
    profile_id: Uuid,
    uuids: Vec<Uuid>,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<Vec<server_players::PlayerLocation>, String> {
    if uuids.len() > 1000 { return Err("Too many player locations were requested.".into()); }
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        let properties = optional_remote_player_file(&sftp, &root, "server.properties")?;
        let world = server_players::world_name(properties.as_deref())?;
        let mut locations = Vec::with_capacity(uuids.len());
        for uuid in uuids {
            let mut player_file = None;
            for path in server_players::player_data_paths(&world, uuid) {
                player_file = optional_remote_player_file(&sftp, &root, &path)?;
                if player_file.is_some() { break; }
            }
            locations.push(server_players::decode_player_location(uuid, player_file));
        }
        Ok(locations)
    }).await
}

#[tauri::command]
pub async fn read_remote_terrain_tiles(
    profile_id: Uuid, dimension: String, resolution: u8, requests: Vec<TerrainRequest>,
    profiles: State<'_, ServerProfileStore>, remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<Vec<TerrainTile>, String> {
    terrain::validate_request(&dimension, resolution, &requests)?;
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        let properties = optional_remote_player_file(&sftp, &root, "server.properties")?;
        let world = server_players::world_name(properties.as_deref())?;
        let mut groups = HashMap::<(i32, i32), Vec<(usize, TerrainRequest)>>::new();
        for (index, request) in requests.iter().enumerate() {
            groups.entry((request.x.div_euclid(32), request.z.div_euclid(32)))
                .or_default().push((index, request.clone()));
        }
        let mut results = (0..requests.len()).map(|_| None).collect::<Vec<Option<TerrainTile>>>();
        for ((rx, rz), group) in groups {
            let mut region_file = None;
            let mut path_failed = false;
            for relative in terrain::region_path_candidates(&world, &dimension, rx, rz)? {
                let path = resolve_remote_relative(&root, &relative, false)?;
                let metadata = match sftp.lstat_optional(Path::new(&path)) {
                    Ok(metadata) => metadata,
                    Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => continue,
                    Err(_) => { path_failed = true; break; }
                };
                if metadata.file_type() != FileType::File { path_failed = true; break; }
                if reject_remote_symlink_path(&sftp, &root, &path).is_err() { path_failed = true; break; }
                match canonical_inside(&sftp, &root, &path) {
                    Ok(canonical) => { region_file = Some(canonical); break; }
                    Err(_) => { path_failed = true; break; }
                }
            }
            let tiles = if path_failed {
                group.iter().map(|(_, request)| terrain::failed(request, resolution)).collect()
            } else if let Some(path) = region_file {
                let region_requests = group.iter().map(|(_, request)| request.clone()).collect::<Vec<_>>();
                sftp.read_terrain_region(Path::new(&path), &region_requests, resolution)
                    .unwrap_or_else(|_| group.iter().map(|(_, request)| terrain::failed(request, resolution)).collect())
            } else {
                group.iter().map(|(_, request)| terrain::missing(request, resolution)).collect()
            };
            for ((index, _), tile) in group.into_iter().zip(tiles) { results[index] = Some(tile); }
        }
        Ok(results.into_iter().flatten().collect())
    }).await
}

pub(crate) async fn remote_package_names(
    profile: ServerProfile,
    environment: ServerEnvironment,
    sessions: Arc<RemoteSessionManager>,
    folder: &str,
) -> Result<Vec<String>, String> {
    let folder = folder.to_owned();
    run_remote(profile, environment, sessions, move |session, profile| {
        let sftp = session.sftp()?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        let path = resolve_remote_relative(&root, &folder, false)?;
        let metadata = match sftp.lstat(Path::new(&path)) {
            Ok(metadata) => metadata,
            Err(_) => return Ok(Vec::new()),
        };
        if metadata.file_type() != FileType::Dir {
            return Err("The package path is not a folder.".into());
        }
        Ok(sftp.read_dir(Path::new(&path))?.into_iter()
            .filter(|(_, metadata)| metadata.file_type() == FileType::File)
            .map(|(name, _)| name)
            .collect())
    }).await
}

pub(crate) async fn remote_package_sha512(
    profile: ServerProfile,
    environment: ServerEnvironment,
    sessions: Arc<RemoteSessionManager>,
    folder: &str,
    filename: &str,
) -> Result<String, String> {
    let folder = folder.to_owned();
    let filename = filename.to_owned();
    run_remote(profile, environment, sessions, move |session, profile| {
        let sftp = session.sftp()?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        let path = resolve_remote_relative(&root, &format!("{folder}/{filename}"), false)?;
        reject_remote_symlink_path(&sftp, &root, &path)?;
        let canonical = canonical_inside(&sftp, &root, &path)?;
        if sftp.lstat(Path::new(&canonical))?.file_type() != FileType::File {
            return Err("The existing package is not a regular file.".into());
        }
        let result = execute(session, &format!("sha512sum -- {}", shell_quote(&canonical)))?;
        if result.exit_code != 0 { return Err(remote_command_error("Could not hash the existing package", result)); }
        let output = String::from_utf8_lossy(&result.stdout);
        let hash = output.split_whitespace().next().unwrap_or_default();
        if hash.len() != 128 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("The remote package hash is invalid.".into());
        }
        Ok(hash.to_ascii_lowercase())
    }).await
}

#[tauri::command]
pub async fn detect_remote_fabric_version(
    profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<Option<String>, String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    if profile.installed_software.as_deref() != Some("fabric") {
        return Ok(None);
    }
    run_remote(profile, environment.clone(), Arc::clone(remote_sessions.inner()), move |session, profile| {
        let sftp = session.sftp()?;
        let root = remote_root_with_sftp(&sftp, &environment.root_directory)?;
        let server = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        let installed = format!("{server}/fabric-server-launch.jar");
        if !matches!(sftp.lstat(Path::new(&installed)), Ok(metadata) if metadata.file_type() == FileType::File) {
            return Ok(None);
        }
        let cache = format!("{root}/cache/fabric");
        if !matches!(sftp.lstat(Path::new(&cache)), Ok(metadata) if metadata.file_type() == FileType::Dir) {
            return Ok(None);
        }
        let versions = match sftp.read_dir(Path::new(&cache)) {
            Ok(versions) => versions,
            Err(_) => return Ok(None),
        };
        let mut match_version = None;
        for (version, metadata) in versions.into_iter().take(100) {
            if metadata.file_type() != FileType::Dir || !crate::vanilla_install::safe_version(&version) {
                continue;
            }
            let builds = match sftp.read_dir(Path::new(&format!("{cache}/{version}"))) {
                Ok(builds) => builds,
                Err(_) => continue,
            };
            for (build, metadata) in builds.into_iter().take(100) {
                if metadata.file_type() != FileType::Dir || !crate::vanilla_install::safe_version(&build) {
                    continue;
                }
                let candidate = format!("{cache}/{version}/{build}/fabric-server-launch.jar");
                if !matches!(sftp.lstat(Path::new(&candidate)), Ok(metadata) if metadata.file_type() == FileType::File) {
                    continue;
                }
                let compared = execute(session, &format!("cmp -s -- {} {}", shell_quote(&installed), shell_quote(&candidate)))?;
                if compared.exit_code == 0 {
                    if match_version.as_deref().is_some_and(|found| found != version) {
                        return Ok(None);
                    }
                    match_version = Some(version.clone());
                }
            }
        }
        Ok(match_version)
    }).await
}

#[tauri::command]
pub async fn write_remote_server_file(
    profile_id: Uuid,
    relative_path: String,
    content: String,
    overwrite: bool,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<(), String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    if content.len() as u64 > MAX_FILE_BYTES {
        return Err("Files larger than 1 MB cannot be edited here.".into());
    }
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        match remote_is_running(session, profile)? {
            Some(true) => return Err("Stop the server before editing its files.".into()),
            Some(false) => {}
            None => return Err("tmux is required to confirm the remote server is stopped before editing files.".into()),
        }
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        if remote_directory_has_java_process(session, &root)? {
            return Err("A Java process is using the remote server folder. Stop it before editing files.".into());
        }
        let requested = resolve_remote_relative(&root, &relative_path, false)?;
        let parent = Path::new(&requested).parent().ok_or("The remote file path is invalid.")?;
        let name = Path::new(&requested).file_name().and_then(|name| name.to_str()).ok_or("The remote file name is invalid.")?;
        reject_remote_symlink_path(&sftp, &root, &remote_path(parent))?;
        let canonical_parent = canonical_inside(&sftp, &root, &remote_path(parent))?;
        let destination = format!("{}/{}", canonical_parent.trim_end_matches('/'), name);
        if let Ok(metadata) = sftp.lstat(Path::new(&destination)) {
            if metadata.file_type() == FileType::Symlink {
                return Err("Remote symbolic links cannot be overwritten here.".into());
            }
            if metadata.file_type() != FileType::File {
                return Err("Only regular remote files can be overwritten.".into());
            }
            if !overwrite {
                return Err("The file already exists. Confirm overwrite before saving.".into());
            }
            canonical_inside(&sftp, &root, &destination)?;
        }

        let temporary = format!("{}/.scerite-{}.tmp", canonical_parent.trim_end_matches('/'), Uuid::new_v4());
        if let Err(error) = sftp.write_file_exclusive(Path::new(&temporary), content.as_bytes()) {
            let _ = sftp.unlink(Path::new(&temporary));
            return Err(format!("Could not write temporary remote file: {error}"));
        }
        let rename_result = if overwrite {
            sftp.posix_rename(Path::new(&temporary), Path::new(&destination))
        } else {
            sftp.rename(Path::new(&temporary), Path::new(&destination))
        };
        if let Err(error) = rename_result {
            let _ = sftp.unlink(Path::new(&temporary));
            return Err(format!("Could not save remote file: {error}"));
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn upload_remote_server_icon(
    profile_id: Uuid,
    bytes: Vec<u8>,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<(), String> {
    validate_server_icon(&bytes)?;
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        match remote_is_running(session, profile)? {
            Some(true) => return Err("Stop the server before changing its multiplayer icon.".into()),
            Some(false) => {}
            None => return Err("tmux is required to confirm the remote server is stopped before changing its icon.".into()),
        }
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        if remote_directory_has_java_process(session, &root)? {
            return Err("A Java process is using the remote server folder. Stop it before changing the icon.".into());
        }
        let destination = format!("{root}/server-icon.png");
        if let Ok(metadata) = sftp.lstat(Path::new(&destination)) {
            if metadata.file_type() != FileType::File {
                return Err("The server icon path is not a regular file.".into());
            }
            canonical_inside(&sftp, &root, &destination)?;
        }
        let temporary = format!("{root}/.scerite-{}.tmp", Uuid::new_v4());
        if let Err(error) = sftp.write_file_exclusive(Path::new(&temporary), &bytes) {
            let _ = sftp.unlink(Path::new(&temporary));
            return Err(format!("Could not upload the server icon: {error}"));
        }
        if let Err(error) = sftp.posix_rename(Path::new(&temporary), Path::new(&destination)) {
            let _ = sftp.unlink(Path::new(&temporary));
            return Err(format!("Could not save the server icon: {error}"));
        }
        Ok(())
    }).await
}

#[tauri::command]
pub async fn read_remote_server_icon(
    profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<Option<Vec<u8>>, String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        let path = format!("{root}/server-icon.png");
        let metadata = match sftp.lstat_optional(Path::new(&path)) {
            Ok(metadata) => metadata,
            Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => return Ok(None),
            Err(error) => return Err(format!("Could not inspect the remote server icon: {error}")),
        };
        if metadata.file_type() != FileType::File || metadata.size.unwrap_or(MAX_FILE_BYTES + 1) > MAX_FILE_BYTES {
            return Ok(None);
        }
        let bytes = sftp.read_file(Path::new(&path))
            .map_err(|error| format!("Could not read the remote server icon: {error}"))?;
        if validate_server_icon(&bytes).is_err() {
            return Ok(None);
        }
        Ok(Some(bytes))
    }).await
}

async fn run_remote<T, F>(
    profile: ServerProfile,
    environment: ServerEnvironment,
    sessions: Arc<RemoteSessionManager>,
    operation: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&RemoteConnection, &ServerProfile) -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let session = sessions.session(&profile, &environment)?;
        let result = operation(&session, &profile);
        if result.is_err() {
            sessions.invalidate(profile.id);
        }
        result
    })
    .await
    .map_err(|error| format!("Remote operation failed to finish: {error}"))?
}

pub(crate) async fn install_remote_modpack_files(
    profile: ServerProfile,
    environment: ServerEnvironment,
    sessions: Arc<RemoteSessionManager>,
    files: Vec<(String, Vec<u8>)>,
) -> Result<(usize, Vec<String>), String> {
    run_remote(profile, environment, sessions, move |session, profile| {
        match remote_is_running(session, profile)? {
            Some(false) => {},
            Some(true) => return Err("Stop the server before installing a modpack.".into()),
            None => return Err("tmux is required to confirm the remote server is stopped.".into()),
        }
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        if remote_directory_has_java_process(session, &root)? {
            return Err("A Java process is using this server folder. Stop it before installing a modpack.".into());
        }
        let mut installed = 0;
        let mut skipped = Vec::new();
        'files: for (name, bytes) in files {
            let destination = resolve_remote_relative(&root, &name, false)?;
            let mut parent = root.clone();
            for segment in name.split('/').take(name.split('/').count() - 1) {
                parent = format!("{parent}/{segment}");
                match sftp.lstat_optional(Path::new(&parent)) {
                    Ok(meta) if meta.file_type() == FileType::Dir => {},
                    Ok(_) => { skipped.push(name); continue 'files; },
                    Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => {
                        sftp.mkdir(Path::new(&parent)).map_err(|error| format!("Could not create pack folder: {error}. {installed} files were already added."))?;
                    }
                    Err(error) => return Err(format!("Could not inspect pack path: {error}")),
                }
            }
            match sftp.lstat_optional(Path::new(&destination)) {
                Ok(_) => { skipped.push(name); continue; },
                Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => {},
                Err(error) => return Err(format!("Could not inspect pack destination: {error}")),
            }
            if let Err(error) = sftp.write_file_from_reader(Path::new(&destination), &mut std::io::Cursor::new(bytes)) {
                return Err(format!("Could not install {name}: {error}. {installed} files were already added."));
            }
            installed += 1;
        }
        Ok((installed, skipped))
    }).await
}

fn require_remote_profile(
    profiles: &ServerProfileStore,
    profile_id: Uuid,
) -> Result<(ServerProfile, ServerEnvironment), String> {
    let profile = profiles
        .list()?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| "Server profile was not found.".to_owned())?;
    let ServerTarget::Remote { environment_id, .. } = &profile.target else {
        return Err("This operation is only available for remote profiles.".into());
    };
    let environment = profiles.environment(*environment_id)?;
    Ok((profile, environment))
}

fn remote_connection_config(environment: &ServerEnvironment) -> RemoteConnectionConfig {
    RemoteConnectionConfig {
        host: environment.host.clone(),
        port: environment.port,
        username: environment.username.clone(),
        identity_file: environment.identity_file.clone(),
    }
}

pub async fn prepare_remote_server_directory(
    environment: ServerEnvironment,
    name: String,
    sessions: Arc<RemoteSessionManager>,
) -> Result<String, String> {
    let mut slug = String::new();
    let mut separator = false;
    for character in name.trim().chars() {
        if character.is_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.extend(character.to_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    if slug.is_empty() {
        return Err("The server name needs a letter or number for its folder name. Choose a custom folder instead.".into());
    }

    tauri::async_runtime::spawn_blocking(move || {
        let config = remote_connection_config(&environment);
        let connection = RemoteConnection {
            handle: connect_remote(&sessions.runtime, &config.host, config.port, &config.username, config.identity_file.as_deref())?,
            runtime: Arc::clone(&sessions.runtime),
        };
        let sftp = connection.sftp()?;
        let created = execute(&connection, &format!("mkdir -p -- {}", shell_quote(&environment.root_directory)))?;
        if created.exit_code != 0 {
            return Err(remote_command_error("Could not create the configured Minecraft root", created));
        }
        let root = remote_root_with_sftp(&sftp, &environment.root_directory)
            .map_err(|error| format!("Could not verify the configured Minecraft root: {error}"))?;
        let mut parent = root;
        for part in ["servers", slug.as_str()] {
            let path = format!("{parent}/{part}");
            let exists = execute(&connection, &format!("test -e {} || test -L {}", shell_quote(&path), shell_quote(&path)))?;
            if exists.exit_code == 0 {
                if sftp.lstat(Path::new(&path))?.file_type() != FileType::Dir {
                    return Err("A managed server folder is a file or symbolic link. Choose another server name or a custom folder.".into());
                }
            } else {
                let created = execute(&connection, &format!("mkdir -- {}", shell_quote(&path)))?;
                if created.exit_code != 0 {
                    return Err(remote_command_error("Could not create the remote server folder", created));
                }
            }
            let canonical = remote_root_with_sftp(&sftp, &path)
                .map_err(|error| format!("Could not verify the managed server folder {path}: {error}"))?;
            if Path::new(&canonical).parent() != Some(Path::new(&parent)) {
                return Err("The managed server folder resolves outside the configured Minecraft root.".into());
            }
            parent = canonical;
        }
        Ok(parent)
    })
    .await
    .map_err(|error| format!("Remote folder setup failed to finish: {error}"))?
}

pub async fn verify_remote_server_directory(
    environment: ServerEnvironment,
    directory: String,
    sessions: Arc<RemoteSessionManager>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let config = remote_connection_config(&environment);
        let connection = RemoteConnection {
            handle: connect_remote(&sessions.runtime, &config.host, config.port, &config.username, config.identity_file.as_deref())?,
            runtime: Arc::clone(&sessions.runtime),
        };
        remote_root(&connection, &directory)
    })
    .await
    .map_err(|error| format!("Remote folder check failed to finish: {error}"))?
}

pub async fn rename_remote_server_directory(
    profile: ServerProfile,
    environment: ServerEnvironment,
    slug: String,
    sessions: Arc<RemoteSessionManager>,
) -> Result<String, String> {
    run_remote(profile, environment.clone(), sessions, move |session, profile| {
        if remote_is_running(session, profile)? != Some(false) {
            return Err("Stop the server and make sure tmux is available before renaming its folder.".into());
        }
        let sftp = session.sftp()?;
        let configured = configured_remote_root(profile)?;
        if sftp.lstat(Path::new(configured))?.file_type() != FileType::Dir {
            return Err("The server folder must be a real directory, not a symbolic link.".into());
        }
        let source = remote_root_with_sftp(&sftp, configured)?;
        let environment_root = remote_root_with_sftp(&sftp, &environment.root_directory)?;
        if source == environment_root {
            return Err("The configured Minecraft root cannot be renamed as a server folder.".into());
        }
        if remote_directory_has_java_process(session, &source)? {
            return Err("Stop the Java server process before renaming its folder.".into());
        }
        let parent = Path::new(&source).parent().ok_or("The server folder has no parent directory.")?;
        let destination = parent.join(&slug).to_string_lossy().into_owned();
        if destination != source {
            let exists = execute(session, &format!("test -e {} || test -L {}", shell_quote(&destination), shell_quote(&destination)))?;
            if exists.exit_code != 1 {
                return Err("A folder with the new server name already exists or could not be checked.".into());
            }
            sftp.rename(Path::new(&source), Path::new(&destination))
                .map_err(|error| format!("Could not rename the remote server folder: {error}"))?;
        }
        Ok(destination)
    }).await
}

pub async fn move_remote_server_directory(
    profile: ServerProfile,
    environment: ServerEnvironment,
    source: String,
    destination: String,
    sessions: Arc<RemoteSessionManager>,
) -> Result<(), String> {
    run_remote(profile, environment, sessions, move |session, _| {
        let sftp = session.sftp()?;
        let exists = execute(session, &format!("test -e {} || test -L {}", shell_quote(&destination), shell_quote(&destination)))?;
        if exists.exit_code != 1 {
            return Err("The old server folder path is occupied, so the folder cannot be moved back.".into());
        }
        sftp.rename(Path::new(&source), Path::new(&destination))
            .map_err(|error| format!("Could not move the remote server folder back: {error}"))
    }).await
}

pub(crate) async fn remote_process_usage(
    profile: ServerProfile,
    environment: ServerEnvironment,
    sessions: Arc<RemoteSessionManager>,
) -> Result<Option<(f32, u64)>, String> {
    run_remote(profile, environment, sessions, move |session, profile| {
        let root = remote_root(session, configured_remote_root(profile)?)?;
        let command = format!(
            "root={}; for process in /proc/[0-9]*; do [ -r \"$process/comm\" ] || continue; [ \"$(cat \"$process/comm\" 2>/dev/null)\" = java ] || continue; cwd=$(readlink -f -- \"$process/cwd\" 2>/dev/null) || continue; case \"$cwd\" in \"$root\"|\"$root\"/*) ps -p \"${{process##*/}}\" -o %cpu=,rss=; exit $?;; esac; done; exit 1",
            shell_quote(&root),
        );
        let result = execute(session, &command)?;
        if result.exit_code != 0 { return Ok(None); }
        let output = String::from_utf8_lossy(&result.stdout);
        let mut parts = output.split_whitespace();
        let cpu = parts.next().and_then(|value| value.parse::<f32>().ok());
        let memory = parts.next().and_then(|value| value.parse::<u64>().ok());
        Ok(cpu.zip(memory).map(|(cpu, memory)| (cpu, memory * 1024)))
    }).await
}

pub(crate) async fn ping_remote_server(
    profile: ServerProfile,
    environment: ServerEnvironment,
    port: u16,
    sessions: Arc<RemoteSessionManager>,
) -> Result<(u32, u32), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let session = sessions.session(&profile, &environment)?;
        session.runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(3), async {
                let channel = session.handle.channel_open_direct_tcpip("127.0.0.1", port as u32, "127.0.0.1", 0)
                    .await.map_err(|error| error.to_string())?;
                let mut stream = channel.into_stream();
                crate::server_overview::ping_minecraft_stream(&mut stream, port).await
            }).await.map_err(|_| "The Minecraft status ping timed out.".to_owned())?
        })
    }).await.map_err(|error| format!("Remote status ping failed to finish: {error}"))?
}

fn connect_remote(
    runtime: &Runtime,
    host: &str,
    port: u16,
    username: &str,
    identity_file: Option<&str>,
) -> Result<Arc<Handle<SshHandler>>, String> {
    runtime.block_on(connect_remote_async(host, port, username, identity_file))
}

struct SshHandler {
    host: String,
    port: u16,
    known_hosts: PathBuf,
    error: Arc<Mutex<Option<String>>>,
}

impl client::Handler for SshHandler {
    type Error = russh::Error;
    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        if key.certificate().is_some() {
            if let Ok(mut error) = self.error.lock() {
                *error = Some(
                    "OpenSSH host certificates are not trusted unless their CA is configured."
                        .into(),
                );
            }
            return Ok(false);
        }
        match russh::keys::known_hosts::check_known_hosts_path(
            &self.host,
            self.port,
            &key.public_key(),
            &self.known_hosts,
        ) {
            Ok(trusted) => {
                if !trusted && let Ok(mut error) = self.error.lock() {
                    *error = Some(format!(
                        "The SSH host key for {}:{} is not trusted or has changed. Verify its fingerprint and check {}.",
                        self.host,
                        self.port,
                        self.known_hosts.display()
                    ));
                }
                Ok(trusted)
            }
            Err(cause) => {
                if let Ok(mut error) = self.error.lock() {
                    *error = Some(format!(
                        "Could not read trusted SSH hosts from {}: {cause}",
                        self.known_hosts.display()
                    ));
                }
                Ok(false)
            }
        }
    }
}

async fn connect_remote_async(
    host: &str,
    port: u16,
    username: &str,
    identity_file: Option<&str>,
) -> Result<Arc<Handle<SshHandler>>, String> {
    let addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| format!("Could not resolve SSH host: {error}"))?;
    let mut last_error = None;
    for address in addresses {
        let key_error = Arc::new(Mutex::new(None));
        let handler = SshHandler {
            host: host.to_owned(),
            port,
            known_hosts: known_hosts_path()?,
            error: Arc::clone(&key_error),
        };
        let config = Arc::new(client::Config {
            // Quiet installs and long-running commands can exceed the cache's idle lifetime.
            // Keep the transport alive until the operation finishes instead of closing it mid-command.
            inactivity_timeout: None,
            keepalive_interval: Some(Duration::from_secs(15)),
            ..Default::default()
        });
        match tokio::time::timeout(CONNECT_TIMEOUT, client::connect(config, address, handler)).await
        {
            Ok(Ok(mut handle)) => {
                if authenticate(&mut handle, username, identity_file).await? {
                    return Ok(Arc::new(handle));
                }
                return Err("SSH authentication failed. Check the selected key or load the key into your SSH agent; encrypted private keys must be unlocked by the agent.".into());
            }
            Ok(Err(cause)) => {
                if let Ok(detail) = key_error.lock() {
                    if let Some(detail) = detail.clone() {
                        return Err(detail);
                    }
                }
                last_error = Some(cause.to_string());
            }
            Err(_) => last_error = Some("connection timed out".into()),
        }
    }
    Err(format!(
        "SSH handshake or connection failed: {}. Check that the server offers a standard host key and a modern key-exchange and cipher algorithm.",
        last_error.unwrap_or_else(|| "no addresses were returned".into())
    ))
}

fn known_hosts_path() -> Result<PathBuf, String> {
    let home = env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .ok_or_else(|| {
            "Could not locate the current user's home directory for SSH known_hosts.".to_owned()
        })?;
    Ok(PathBuf::from(home).join(".ssh").join("known_hosts"))
}

async fn authenticate(
    session: &mut Handle<SshHandler>,
    username: &str,
    identity_file: Option<&str>,
) -> Result<bool, String> {
    if let Some(identity_file) = identity_file {
        if let Ok(key) = keys::load_secret_key(identity_file, None) {
            let hash = session
                .best_supported_rsa_hash()
                .await
                .ok()
                .flatten()
                .flatten();
            let auth_key = keys::PrivateKeyWithHashAlg::new(Arc::new(key), hash);
            if let Ok(result) = session.authenticate_publickey(username, auth_key).await {
                if result.success() {
                    return Ok(true);
                }
            }
        }
    }
    #[cfg(unix)]
    if let Ok(agent) = AgentClient::connect_env().await {
        if authenticate_agent(session, username, agent).await {
            return Ok(true);
        }
    }
    #[cfg(windows)]
    {
        if let Ok(agent) = AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent").await {
            if authenticate_agent(session, username, agent).await {
                return Ok(true);
            }
        }
        if let Ok(agent) = AgentClient::connect_pageant().await {
            if authenticate_agent(session, username, agent).await {
                return Ok(true);
            }
        }
    }
    if identity_file.is_none() {
        for identity in default_identity_paths() {
            if let Ok(key) = keys::load_secret_key(&identity, None) {
                let hash = session
                    .best_supported_rsa_hash()
                    .await
                    .ok()
                    .flatten()
                    .flatten();
                let auth_key = keys::PrivateKeyWithHashAlg::new(Arc::new(key), hash);
                if let Ok(result) = session.authenticate_publickey(username, auth_key).await {
                    if result.success() {
                        return Ok(true);
                    }
                }
            }
        }
    }
    Ok(false)
}

async fn authenticate_agent<S>(
    session: &mut Handle<SshHandler>,
    username: &str,
    mut agent: AgentClient<S>,
) -> bool
where
    S: russh::keys::agent::client::AgentStream + Send + Unpin + 'static,
{
    let Ok(identities) = agent.request_identities().await else {
        return false;
    };
    for identity in identities {
        let key = match identity {
            AgentIdentity::PublicKey { key, .. } => key,
            AgentIdentity::Certificate { .. } => continue,
        };
        let hash = session
            .best_supported_rsa_hash()
            .await
            .ok()
            .flatten()
            .flatten();
        if session
            .authenticate_publickey_with(username, key, hash, &mut agent)
            .await
            .is_ok_and(|result| result.success())
        {
            return true;
        }
    }
    false
}

fn default_identity_paths() -> Vec<PathBuf> {
    let Some(home) = env::var_os("USERPROFILE").or_else(|| env::var_os("HOME")) else {
        return Vec::new();
    };
    ["id_ed25519", "id_ecdsa", "id_rsa", "id_dsa"]
        .into_iter()
        .map(|name| PathBuf::from(&home).join(".ssh").join(name))
        .collect()
}

fn remote_is_running(
    session: &RemoteConnection,
    profile: &ServerProfile,
) -> Result<Option<bool>, String> {
    let command = format!(
        "tmux display-message -p -t {} '#{{pane_dead}}'",
        shell_quote(&session_name(profile.id))
    );
    let result = execute(session, &command)?;
    if result.exit_code == 0 {
        return Ok(Some(String::from_utf8_lossy(&result.stdout).trim() == "0"));
    }
    let details = String::from_utf8_lossy(&result.stderr);
    if details.contains("command not found") || result.exit_code == 127 {
        return Ok(None);
    }
    Ok(Some(false))
}

fn remote_directory_has_java_process(
    session: &RemoteConnection,
    root: &str,
) -> Result<bool, String> {
    let command = format!(
        "root={}; [ -d /proc ] || exit 2; command -v readlink >/dev/null 2>&1 || exit 127; for process in /proc/[0-9]*; do [ -r \"$process/comm\" ] || continue; [ \"$(cat \"$process/comm\" 2>/dev/null)\" = java ] || continue; cwd=$(readlink -f -- \"$process/cwd\" 2>/dev/null) || continue; case \"$cwd\" in \"$root\"|\"$root\"/*) printf '%s\\n' \"$process\"; exit 0;; esac; done; exit 1",
        shell_quote(root),
    );
    let result = execute(session, &command)?;
    match result.exit_code {
        0 => Ok(true),
        1 => Ok(false),
        2 => Err("Remote process information is unavailable, so scerite cannot confirm the server is stopped.".into()),
        127 => Err("The remote Linux server needs readlink to verify whether a Java server process is using this folder.".into()),
        _ => Err(remote_command_error("Could not check remote server processes", result)),
    }
}

fn execute(session: &RemoteConnection, command: &str) -> Result<RemoteCommandOutput, String> {
    let command = command.to_owned();
    session.runtime.block_on(async {
        let mut channel = session
            .handle
            .channel_open_session()
            .await
            .map_err(|error| format!("Could not open an SSH channel: {error}"))?;
        channel
            .exec(true, command)
            .await
            .map_err(|error| format!("Could not run the remote command: {error}"))?;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_code = 0;
        while let Some(message) = channel.wait().await {
            match message {
                ChannelMsg::Data { data } => append_limited(&mut stdout, &data)?,
                ChannelMsg::ExtendedData { data, .. } => append_limited(&mut stderr, &data)?,
                ChannelMsg::ExitStatus { exit_status } => exit_code = exit_status as i32,
                ChannelMsg::Close => break,
                _ => {}
            }
        }
        Ok(RemoteCommandOutput {
            stdout,
            stderr,
            exit_code,
        })
    })
}

fn append_limited(output: &mut Vec<u8>, data: &[u8]) -> Result<(), String> {
    if output.len() + data.len() > MAX_CONSOLE_BYTES as usize {
        return Err("Remote command output exceeded the 512 KB safety limit.".into());
    }
    output.extend_from_slice(data);
    Ok(())
}

impl RemoteConnection {
    fn sftp(&self) -> Result<RemoteSftp, String> {
        let channel = self.runtime.block_on(async {
            let channel = self
                .handle
                .channel_open_session()
                .await
                .map_err(|error| format!("Could not open SFTP channel: {error}"))?;
            channel
                .request_subsystem(true, "sftp")
                .await
                .map_err(|error| format!("Could not start SFTP subsystem: {error}"))?;
            Ok::<_, String>(channel.into_stream())
        })?;
        let inner = self
            .runtime
            .block_on(SftpSession::new(channel))
            .map_err(|error| format!("Could not initialize SFTP: {error}"))?;
        inner.set_timeout(CONNECT_TIMEOUT.as_secs());
        Ok(RemoteSftp {
            runtime: Arc::clone(&self.runtime),
            handle: Arc::clone(&self.handle),
            inner,
        })
    }
}

impl RemoteSftp {
    fn copy_file_to_writer<W: std::io::Write>(&self, path: &Path, writer: &mut W) -> Result<(), String> {
        let path = path.to_string_lossy().to_string();
        self.runtime.block_on(async {
            let mut file = self.inner.open(path).await.map_err(|error| error.to_string())?;
            let mut buffer = [0_u8; 65536];
            loop {
                let count = file.read(&mut buffer).await.map_err(|error| error.to_string())?;
                if count == 0 { break; }
                writer.write_all(&buffer[..count]).map_err(|error| error.to_string())?;
            }
            file.close().await.map_err(|error| error.to_string())
        })
    }

    fn write_file_from_reader<R: std::io::Read>(&self, path: &Path, reader: &mut R) -> Result<(), String> {
        let path = path.to_string_lossy().to_string();
        self.runtime.block_on(async {
            let mut attrs = FileAttributes::empty();
            attrs.set_regular(true);
            attrs.permissions = Some(0o100600);
            let mut file = self.inner.open_with_flags_and_attributes(
                path.clone(), OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUDE, attrs,
            ).await.map_err(|error| error.to_string())?;
            let transfer = async {
                let mut buffer = [0_u8; 65536];
                loop {
                    let count = reader.read(&mut buffer).map_err(|error| error.to_string())?;
                    if count == 0 { break; }
                    file.write_all(&buffer[..count]).await.map_err(|error| error.to_string())?;
                }
                Ok::<_, String>(())
            }.await;
            let close = file.close().await.map_err(|error| error.to_string());
            if transfer.is_err() || close.is_err() {
                let _ = self.inner.remove_file(path).await;
            }
            transfer.and(close)
        })
    }

    fn mkdir(&self, path: &Path) -> Result<(), String> {
        self.runtime.block_on(self.inner.create_dir(path.to_string_lossy().to_string()))
            .map_err(|error| error.to_string())
    }

    fn upload_file_exclusive(&self, path: &Path, source: &Path) -> Result<(), String> {
        let path = path.to_string_lossy().to_string();
        self.runtime.block_on(async {
            let mut attrs = FileAttributes::empty();
            attrs.set_regular(true);
            attrs.permissions = Some(0o100600);
            let mut remote = self
                .inner
                .open_with_flags_and_attributes(
                    path,
                    OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUDE,
                    attrs,
                )
                .await
                .map_err(|error| error.to_string())?;
            let mut local = tokio::fs::File::open(source)
                .await
                .map_err(|error| error.to_string())?;
            let mut buffer = [0_u8; 65536];
            loop {
                let count = local
                    .read(&mut buffer)
                    .await
                    .map_err(|error| error.to_string())?;
                if count == 0 {
                    break;
                }
                remote
                    .write_all(&buffer[..count])
                    .await
                    .map_err(|error| error.to_string())?;
            }
            remote.close().await.map_err(|error| error.to_string())
        })
    }
    fn realpath(&self, path: &Path) -> Result<String, String> {
        self.runtime
            .block_on(self.inner.canonicalize(path.to_string_lossy().to_string()))
            .map_err(|e| e.to_string())
    }
    fn stat(&self, path: &Path) -> Result<FileAttributes, String> {
        self.runtime
            .block_on(self.inner.metadata(path.to_string_lossy().to_string()))
            .map_err(|e| e.to_string())
    }
    fn lstat(&self, path: &Path) -> Result<FileAttributes, String> {
        self.runtime
            .block_on(
                self.inner
                    .symlink_metadata(path.to_string_lossy().to_string()),
            )
            .map_err(|e| e.to_string())
    }
    fn lstat_optional(&self, path: &Path) -> Result<FileAttributes, SftpError> {
        self.runtime.block_on(self.inner.symlink_metadata(path.to_string_lossy().to_string()))
    }
    fn read_player_uuids(&self, path: &Path) -> Result<Vec<String>, String> {
        let entries = self.runtime.block_on(self.inner.read_dir(path.to_string_lossy().to_string()))
            .map_err(|error| error.to_string())?;
        let mut uuids = Vec::new();
        for entry in entries {
            if entry.file_type() != FileType::File { continue; }
            let name = entry.file_name();
            let stem = name.strip_suffix(".dat").or_else(|| name.strip_suffix(".json"));
            if let Some(Ok(uuid)) = stem.map(Uuid::parse_str) { uuids.push(uuid.to_string()); }
        }
        Ok(uuids)
    }
    fn read_dir(&self, path: &Path) -> Result<Vec<(String, FileAttributes)>, String> {
        let path = path.to_string_lossy().into_owned();
        self.runtime.block_on(async {
            let channel = self
                .handle
                .channel_open_session()
                .await
                .map_err(|e| e.to_string())?;
            channel
                .request_subsystem(true, "sftp")
                .await
                .map_err(|e| e.to_string())?;
            let raw = RawSftpSession::new(channel.into_stream());
            raw.init().await.map_err(|e| e.to_string())?;
            let handle = raw.opendir(path).await.map_err(|e| e.to_string())?;
            let mut entries = Vec::new();
            loop {
                let batch = match raw.readdir(&handle.handle).await {
                    Ok(batch) => batch,
                    Err(SftpError::Status(status)) if status.status_code == StatusCode::Eof => {
                        break;
                    }
                    Err(error) => {
                        let _ = raw.close(&handle.handle).await;
                        return Err(error.to_string());
                    }
                };
                for entry in batch.files {
                    entries.push((entry.filename, entry.attrs));
                    if entries.len() > MAX_DIRECTORY_ENTRIES {
                        break;
                    }
                }
                if entries.len() > MAX_DIRECTORY_ENTRIES {
                    break;
                }
            }
            raw.close(&handle.handle).await.map_err(|e| e.to_string())?;
            Ok(entries)
        })
    }
    fn read_file(&self, path: &Path) -> Result<Vec<u8>, String> {
        self.read_prefix(path, MAX_FILE_BYTES as usize + 1)
    }
    fn read_terrain_region(&self, path: &Path, requests: &[TerrainRequest], resolution: u8) -> Result<Vec<TerrainTile>, String> {
        let path = path.to_string_lossy().to_string();
        self.runtime.block_on(async {
            let mut file = self.inner.open(path).await.map_err(|error| error.to_string())?;
            let result = async {
                let mut header = [0_u8; terrain::REGION_HEADER_BYTES];
                file.read_exact(&mut header).await.map_err(|error| error.to_string())?;
                let mut tiles = Vec::with_capacity(requests.len());
                for request in requests {
                    let tile = async {
                        let Some(entry) = terrain::region_entry(&header, terrain::region_index(request.x, request.z))? else {
                            return Ok::<TerrainTile, String>(terrain::missing(request, resolution));
                        };
                        if request.known_timestamp == Some(entry.timestamp) {
                            return Ok(terrain::unchanged(request, resolution, entry.timestamp));
                        }
                        file.seek(SeekFrom::Start(entry.offset)).await.map_err(|error| error.to_string())?;
                        let mut length = [0_u8; 4];
                        file.read_exact(&mut length).await.map_err(|error| error.to_string())?;
                        let length = u32::from_be_bytes(length) as usize;
                        if length == 0 || length > terrain::MAX_CHUNK_BYTES || length + 4 > entry.sectors as usize * 4096 {
                            return Err("The chunk length is invalid.".into());
                        }
                        let mut data = vec![0_u8; length];
                        file.read_exact(&mut data).await.map_err(|error| error.to_string())?;
                        Ok(terrain::decode_region_payload(request, resolution, entry.timestamp, &data))
                    }.await.unwrap_or_else(|_| terrain::failed(request, resolution));
                    tiles.push(tile);
                }
                Ok::<Vec<TerrainTile>, String>(tiles)
            }.await;
            let _ = file.close().await;
            result
        })
    }
    fn read_prefix(&self, path: &Path, limit: usize) -> Result<Vec<u8>, String> {
        let path = path.to_string_lossy().to_string();
        self.runtime.block_on(async {
            let mut file = self.inner.open(path).await.map_err(|e| e.to_string())?;
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 8192];
            while bytes.len() < limit {
                let remaining = (limit - bytes.len()).min(buffer.len());
                let count = file
                    .read(&mut buffer[..remaining])
                    .await
                    .map_err(|e| e.to_string())?;
                if count == 0 {
                    break;
                }
                bytes.extend_from_slice(&buffer[..count]);
            }
            file.close().await.map_err(|e| e.to_string())?;
            Ok::<_, String>(bytes)
        })
    }
    fn read_tail(&self, path: &Path, limit: usize) -> Result<(Vec<u8>, bool), String> {
        let path = path.to_string_lossy().to_string();
        self.runtime.block_on(async {
            let mut file = self.inner.open(path).await.map_err(|e| e.to_string())?;
            let size = file.seek(SeekFrom::End(0)).await.map_err(|e| e.to_string())?;
            let truncated = size > limit as u64;
            file.seek(SeekFrom::Start(size.saturating_sub(limit as u64)))
                .await.map_err(|e| e.to_string())?;
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 8192];
            while bytes.len() < limit {
                let remaining = (limit - bytes.len()).min(buffer.len());
                let count = file.read(&mut buffer[..remaining]).await.map_err(|e| e.to_string())?;
                if count == 0 { break; }
                bytes.extend_from_slice(&buffer[..count]);
            }
            file.close().await.map_err(|e| e.to_string())?;
            Ok::<_, String>((bytes, truncated))
        })
    }
    fn write_file_exclusive(&self, path: &Path, content: &[u8]) -> Result<(), String> {
        let path = path.to_string_lossy().to_string();
        self.runtime.block_on(async {
            let mut attrs = FileAttributes::empty();
            attrs.set_regular(true);
            attrs.permissions = Some(0o100600);
            let mut file = self
                .inner
                .open_with_flags_and_attributes(
                    path,
                    OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUDE,
                    attrs,
                )
                .await
                .map_err(|e| e.to_string())?;
            file.write_all(content).await.map_err(|e| e.to_string())?;
            file.close().await.map_err(|e| e.to_string())
        })
    }
    fn unlink(&self, path: &Path) -> Result<(), String> {
        self.runtime
            .block_on(self.inner.remove_file(path.to_string_lossy().to_string()))
            .map_err(|e| e.to_string())
    }
    fn rename(&self, old: &Path, new: &Path) -> Result<(), String> {
        self.runtime
            .block_on(self.inner.rename(
                old.to_string_lossy().to_string(),
                new.to_string_lossy().to_string(),
            ))
            .map_err(|e| e.to_string())
    }
    fn posix_rename(&self, old: &Path, new: &Path) -> Result<(), String> {
        let old = old.to_string_lossy().into_owned();
        let new = new.to_string_lossy().into_owned();
        self.runtime.block_on(async {
            let channel = self.handle.channel_open_session().await.map_err(|e| e.to_string())?;
            channel.request_subsystem(true, "sftp").await.map_err(|e| e.to_string())?;
            let raw = RawSftpSession::new(channel.into_stream());
            raw.init().await.map_err(|e| e.to_string())?;
            let mut data = Vec::with_capacity(old.len() + new.len() + 8);
            for value in [&old, &new] {
                let len = u32::try_from(value.len()).map_err(|_| "Remote path is too long.".to_owned())?;
                data.extend_from_slice(&len.to_be_bytes());
                data.extend_from_slice(value.as_bytes());
            }
            match raw.extended("posix-rename@openssh.com", data).await.map_err(|e| e.to_string())? {
                Packet::Status(status) if status.status_code == StatusCode::Ok => Ok(()),
                Packet::Status(status) => Err(if status.status_code == StatusCode::OpUnsupported {
                    "The remote server does not support atomic file replacement; the existing file was left unchanged.".to_owned()
                } else { status.error_message }),
                _ => Err("The remote server returned an unexpected response to atomic file replacement.".to_owned()),
            }
        })
    }
}

pub(crate) async fn install_remote_jar(
    profile: ServerProfile,
    environment: ServerEnvironment,
    sessions: Arc<RemoteSessionManager>,
    directory: String,
    source: Option<PathBuf>,
    url: String,
    version: String,
    sha1: String,
    sha256: Option<String>,
    cache_kind: String,
    filename: String,
) -> Result<(), String> {
    run_remote(profile, environment.clone(), sessions, move |session, profile| {
        let sftp = session.sftp()?;
        let root = remote_root_with_sftp(&sftp, &environment.root_directory)?;
        if remote_is_running(session, profile)? == Some(true) {
            return Err("Stop this server before installing a version.".into());
        }
        let tools = execute(session, "command -v sha1sum >/dev/null && command -v cp >/dev/null && command -v ln >/dev/null")?;
        if tools.exit_code != 0 {
            return Err("Remote installation needs sha1sum, cp, and ln on the Linux host.".into());
        }
        let servers = format!("{root}/servers");
        let created = execute(session, &format!("mkdir -p -- {}", shell_quote(&servers)))?;
        if created.exit_code != 0 { return Err(remote_command_error("Could not create remote servers folder", created)); }
        let servers = remote_root_with_sftp(&sftp, &servers)?;
        if Path::new(&servers).parent() != Some(Path::new(&root)) {
            return Err("The remote servers folder resolves outside the configured Minecraft root.".into());
        }
        let requested = Path::new(&directory);
        let parent = requested.parent().ok_or_else(|| "Set an absolute remote server folder.".to_owned())?;
        let parent = remote_root_with_sftp(&sftp, &parent.to_string_lossy())?;
        if parent != servers { return Err("The server folder must be directly inside this environment's Minecraft root/servers folder.".into()); }
        let target = match remote_root_with_sftp(&sftp, &directory) {
            Ok(target) => target,
            Err(_) => {
                let create = execute(session, &format!("mkdir -- {}", shell_quote(&directory)))?;
                if create.exit_code != 0 { return Err(remote_command_error("Could not create remote server folder", create)); }
                remote_root_with_sftp(&sftp, &directory)?
            }
        };
        if Path::new(&target).parent() != Some(Path::new(&servers)) {
            return Err("The server folder must be directly inside this environment's Minecraft root/servers folder.".into());
        }
        if remote_directory_has_java_process(session, &target)? {
            return Err("Stop this server before installing a version.".into());
        }
        let destination = format!("{target}/{filename}");
        let absent = execute(session, &format!("test ! -e {} && test ! -L {}", shell_quote(&destination), shell_quote(&destination)))?;
        if absent.exit_code != 0 { return Err(format!("{filename} already exists. Existing server files are not overwritten.")); }
        let cache_dir = format!("{root}/cache/{cache_kind}/{version}");
        let created = execute(session, &format!("mkdir -p -- {}", shell_quote(&cache_dir)))?;
        if created.exit_code != 0 { return Err(remote_command_error("Could not create remote Minecraft cache", created)); }
        let canonical_cache = remote_root_with_sftp(&sftp, &cache_dir)?;
        if !canonical_cache.starts_with(&format!("{root}/cache/")) {
            return Err("The remote cache resolves outside the configured Minecraft root.".into());
        }
        let cached = format!("{canonical_cache}/{filename}");
        let cached_exists = execute(session, &format!("test -e {} || test -L {}", shell_quote(&cached), shell_quote(&cached)))?;
        if cached_exists.exit_code == 0 {
            let valid = if sha1.is_empty() && sha256.is_none() {
                remote_jar_looks_valid(session, &cached)?
            } else {
                let (command, expected) = if let Some(ref expected) = sha256 { ("sha256sum", expected.as_str()) } else { ("sha1sum", sha1.as_str()) };
                let hash = execute(session, &format!("test ! -L {} && {command} -- {}", shell_quote(&cached), shell_quote(&cached)))?;
                remote_hash_matches(&hash, expected)
            };
            if !valid {
                return Err("The remote cached jar has a different checksum. Remove that cache file before retrying.".into());
            }
        } else {
            let upload = format!("{canonical_cache}/.servercrafter-{}.tmp", Uuid::new_v4());
            let result = (|| {
                download_or_upload(session, &sftp, &upload, &url, source.as_deref())?;
                let valid = if sha1.is_empty() && sha256.is_none() {
                    remote_jar_looks_valid(session, &upload)?
                } else {
                    let (command, expected) = if let Some(ref expected) = sha256 { ("sha256sum", expected.as_str()) } else { ("sha1sum", sha1.as_str()) };
                    let hash = execute(session, &format!("{command} -- {}", shell_quote(&upload)))?;
                    remote_hash_matches(&hash, expected)
                };
                if !valid {
                return Err("The uploaded server jar did not match the downloaded file.".into());
                }
                let link = execute(session, &format!("ln -- {} {}", shell_quote(&upload), shell_quote(&cached)))?;
                if link.exit_code != 0 { return Err(remote_command_error("Could not cache remote server jar", link)); }
                Ok(())
            })();
            let _ = sftp.unlink(Path::new(&upload));
            result?;
        }
        let temporary = format!("{target}/.servercrafter-{}.tmp", Uuid::new_v4());
        let install = (|| {
            let copy = execute(session, &format!("cp -- {} {}", shell_quote(&cached), shell_quote(&temporary)))?;
            if copy.exit_code != 0 { return Err(remote_command_error("Could not copy cached jar into server folder", copy)); }
            let link = execute(session, &format!("ln -- {} {}", shell_quote(&temporary), shell_quote(&destination)))?;
            if link.exit_code != 0 { return Err(remote_command_error("Could not install server.jar", link)); }
            Ok(())
        })();
        let _ = sftp.unlink(Path::new(&temporary));
        install?;
        Ok(())
    }).await
}

pub(crate) async fn install_remote_loader(
    profile: ServerProfile,
    environment: ServerEnvironment,
    sessions: Arc<RemoteSessionManager>,
    directory: String,
    source: Option<PathBuf>,
    url: String,
    loader: String,
    version: String,
    build: String,
    quilt_loader: Option<String>,
    java: String,
    sha1: String,
    sha256: Option<String>,
) -> Result<(), String> {
    run_remote(profile, environment.clone(), sessions, move |session, profile| {
        let sftp = session.sftp()?;
        let root = remote_root_with_sftp(&sftp, &environment.root_directory)?;
        if remote_is_running(session, profile)? == Some(true) {
            return Err("Stop this server before installing software.".into());
        }
        let servers = format!("{root}/servers");
        let created = execute(session, &format!("mkdir -p -- {}", shell_quote(&servers)))?;
        if created.exit_code != 0 { return Err(remote_command_error("Could not create servers folder", created)); }
        let servers = remote_root_with_sftp(&sftp, &servers)?;
        if Path::new(&servers).parent() != Some(Path::new(&root)) {
            return Err("The servers folder resolves outside the Minecraft root.".into());
        }
        let requested = Path::new(&directory);
        let parent = requested.parent().ok_or("Set an absolute remote server folder.")?;
        let parent = remote_root_with_sftp(&sftp, &parent.to_string_lossy())?;
        if parent != servers { return Err("The server folder must be directly inside the Minecraft root/servers folder.".into()); }
        let target = match remote_root_with_sftp(&sftp, &directory) {
            Ok(target) => target,
            Err(_) => {
                let created = execute(session, &format!("mkdir -- {}", shell_quote(&directory)))?;
                if created.exit_code != 0 { return Err(remote_command_error("Could not create server folder", created)); }
                remote_root_with_sftp(&sftp, &directory)?
            }
        };
        if Path::new(&target).parent() != Some(Path::new(&servers)) {
            return Err("The server folder must be directly inside the Minecraft root/servers folder.".into());
        }
        if remote_directory_has_java_process(session, &target)? {
            return Err("Stop this server before installing software.".into());
        }
        if sftp.read_dir(Path::new(&target))?.iter().any(|(name, _)| name != "." && name != "..") {
            return Err("Forge, NeoForge, and Quilt installers require an empty server folder. Existing files were not changed.".into());
        }
        let cache = format!("{root}/cache/{loader}/{version}/{build}");
        let created = execute(session, &format!("mkdir -p -- {}", shell_quote(&cache)))?;
        if created.exit_code != 0 { return Err(remote_command_error("Could not create loader cache", created)); }
        let cache = remote_root_with_sftp(&sftp, &cache)?;
        if !cache.starts_with(&format!("{root}/cache/")) { return Err("The loader cache resolves outside the Minecraft root.".into()); }
        let installer = format!("{cache}/installer.jar");
        match sftp.lstat(Path::new(&installer)) {
            Ok(stat) if stat.file_type() != FileType::File => return Err("The cached installer is not a regular file.".into()),
            Ok(_) => {
                let valid = if sha1.is_empty() && sha256.is_none() {
                    remote_jar_looks_valid(session, &installer)?
                } else {
                    let (command, expected) = if let Some(ref expected) = sha256 { ("sha256sum", expected.as_str()) } else { ("sha1sum", sha1.as_str()) };
                    let hash = execute(session, &format!("{command} -- {}", shell_quote(&installer)))?;
                    remote_hash_matches(&hash, expected)
                };
                if !valid {
                    return Err("The cached installer has a different checksum. Remove it before retrying.".into());
                }
            }
            Err(_) => {
                download_or_upload(session, &sftp, &installer, &url, source.as_deref())?;
                let valid = if sha1.is_empty() && sha256.is_none() {
                    remote_jar_looks_valid(session, &installer)?
                } else {
                    let (command, expected) = if let Some(ref expected) = sha256 { ("sha256sum", expected.as_str()) } else { ("sha1sum", sha1.as_str()) };
                    let hash = execute(session, &format!("{command} -- {}", shell_quote(&installer)))?;
                    remote_hash_matches(&hash, expected)
                };
                if !valid {
                    return Err("The uploaded loader installer failed its checksum check.".into());
                }
            }
        }
        let args = if loader == "quilt" {
            format!("install server {} {} --download-server {}", shell_quote(&version), shell_quote(quilt_loader.as_deref().ok_or("Quilt loader version is missing.")?), shell_quote("--install-dir=\".\""))
        } else {
            "--installServer".to_owned()
        };
        let log = format!("{target}/.servercrafter-install.log");
        let command = format!("cd {} && {} -jar {} {} > {} 2>&1", shell_quote(&target), shell_quote(&java), shell_quote(&installer), args, shell_quote(&log));
        let installed = execute(session, &command)?;
        if installed.exit_code != 0 {
            let tail = execute(session, &format!("tail -n 30 -- {}", shell_quote(&log)))?;
            let detail = String::from_utf8_lossy(&tail.stdout);
            return Err(format!("{loader} installer failed. {} See {log} for details.", detail.chars().take(2000).collect::<String>()));
        }
        let expected = if loader == "quilt" {
            format!("{target}/quilt-server-launch.jar")
        } else {
            let family = if loader == "forge" { "net/minecraftforge/forge" } else { "net/neoforged/neoforge" };
            format!("{target}/libraries/{family}/{build}/unix_args.txt")
        };
        if !sftp.lstat(Path::new(&expected)).is_ok_and(|stat| stat.file_type() == FileType::File) {
            return Err(format!("{loader} installer finished without creating its launch files. See {log} for details."));
        }
        let _ = sftp.unlink(Path::new(&log));
        Ok(())
    }).await
}

pub(crate) async fn install_remote_package(
    profile: ServerProfile,
    environment: ServerEnvironment,
    sessions: Arc<RemoteSessionManager>,
    folder: String,
    filename: String,
    source: Option<PathBuf>,
    sha1: String,
    url: String,
    sha512: Option<String>,
) -> Result<(), String> {
    run_remote(profile, environment, sessions, move |session, profile| {
        match remote_is_running(session, profile)? {
            Some(true) => return Err("Stop the server before installing packages.".into()),
            Some(false) => {}
            None => return Err("tmux is required to confirm the remote server is stopped.".into()),
        }
        let sftp = session.sftp()?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        if remote_directory_has_java_process(session, &root)? {
            return Err("A Java process is using the server folder. Stop it before installing packages.".into());
        }
        let directory = format!("{root}/{folder}");
        let created = execute(session, &format!("mkdir -p -- {}", shell_quote(&directory)))?;
        if created.exit_code != 0 { return Err(remote_command_error("Could not create package folder", created)); }
        reject_remote_symlink_path(&sftp, &root, &directory)?;
        if remote_root_with_sftp(&sftp, &directory)? != directory {
            return Err("The package folder resolves outside the server folder.".into());
        }
        let destination = format!("{directory}/{filename}");
        if sftp.lstat(Path::new(&destination)).is_ok() { return Err(format!("{filename} already exists. Existing packages are never replaced.")); }
        let temporary = format!("{directory}/.servercrafter-{}.tmp", Uuid::new_v4());
        let result = (|| {
            download_or_upload(session, &sftp, &temporary, &url, source.as_deref())?;
            let (command, expected) = if let Some(ref expected) = sha512 { ("sha512sum", expected.as_str()) } else { ("sha1sum", sha1.as_str()) };
            let hash = execute(session, &format!("{command} -- {}", shell_quote(&temporary)))?;
            if !remote_hash_matches(&hash, expected) {
                return Err("The package failed its checksum check.".into());
            }
            let linked = execute(session, &format!("ln -- {} {}", shell_quote(&temporary), shell_quote(&destination)))?;
            if linked.exit_code != 0 { return Err(remote_command_error("Could not install package", linked)); }
            Ok(())
        })();
        let _ = sftp.unlink(Path::new(&temporary));
        result
    }).await
}

pub(crate) async fn remove_remote_package(
    profile: ServerProfile,
    environment: ServerEnvironment,
    sessions: Arc<RemoteSessionManager>,
    kind: String,
    filename: String,
) -> Result<(), String> {
    run_remote(profile, environment, sessions, move |session, profile| {
        let folder = match kind.as_str() {
            "mod" => "mods",
            "plugin" => "plugins",
            _ => return Err("Choose a valid package type.".into()),
        };
        match remote_is_running(session, profile)? {
            Some(false) => {}
            Some(true) => return Err("Stop the server before removing packages.".into()),
            None => return Err("tmux is required to confirm the remote server is stopped.".into()),
        }
        let sftp = session.sftp()?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        if remote_directory_has_java_process(session, &root)? {
            return Err("A Java process is using the server folder. Stop it before removing packages.".into());
        }
        let directory = format!("{root}/{folder}");
        reject_remote_symlink_path(&sftp, &root, &directory)?;
        if remote_root_with_sftp(&sftp, &directory)? != directory {
            return Err("The package folder resolves outside the server folder.".into());
        }
        let path = format!("{directory}/{filename}");
        let stat = sftp.lstat(Path::new(&path))
            .map_err(|error| format!("Could not inspect installed package: {error}"))?;
        if stat.file_type() != FileType::File {
            return Err("The installed package is not a regular file.".into());
        }
        sftp.unlink(Path::new(&path))
            .map_err(|error| format!("Could not remove installed package: {error}"))
    }).await
}

pub(crate) async fn remote_java_arch(
    profile: ServerProfile,
    environment: ServerEnvironment,
    sessions: Arc<RemoteSessionManager>,
) -> Result<&'static str, String> {
    run_remote(profile, environment, sessions, |session, profile| {
        if remote_is_running(session, profile)? == Some(true) {
            return Err("Stop this server before installing a version.".into());
        }
        if let ServerTarget::Remote { directory, .. } = &profile.target {
            if !directory.is_empty() {
                let present = execute(session, &format!("test -d {}", shell_quote(directory)))?;
                if present.exit_code == 0 {
                    let root = remote_root(session, directory)?;
                    if remote_directory_has_java_process(session, &root)? {
                        return Err("Stop this server before installing a version.".into());
                    }
                }
            }
        }
        let result = execute(session, "uname -m")?;
        if result.exit_code != 0 { return Err(remote_command_error("Could not detect remote CPU architecture", result)); }
        match String::from_utf8_lossy(&result.stdout).trim() {
            "x86_64" | "amd64" => Ok("x64"),
            "aarch64" | "arm64" => Ok("aarch64"),
            _ => Err("This remote CPU architecture has no managed Temurin JDK build.".into()),
        }
    }).await
}

fn remote_java_paths(session: &RemoteConnection, environment: &ServerEnvironment, package: &JavaPackage) -> Result<(String, String, String), String> {
    let root = remote_root(session, &environment.root_directory)?;
    let parent = format!("{root}/runtimes/java/{}/{}", package.major, package.platform);
    let runtime = format!("{parent}/{}", package.build);
    let java = format!("{runtime}/bin/java");
    Ok((root, runtime, java))
}

fn ensure_remote_managed_dirs(session: &RemoteConnection, sftp: &RemoteSftp, root: &str, parts: &[&str]) -> Result<String, String> {
    let mut current = root.to_owned();
    for part in parts {
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) {
            return Err("The remote Java folder name is invalid.".into());
        }
        let candidate = format!("{current}/{part}");
        let exists = execute(session, &format!("test -e {} || test -L {}", shell_quote(&candidate), shell_quote(&candidate)))?;
        if exists.exit_code == 0 {
            let attrs = sftp.lstat(Path::new(&candidate))?;
            if attrs.file_type() != FileType::Dir { return Err("A remote managed Java folder is a file or symbolic link.".into()); }
        } else {
            let create = execute(session, &format!("mkdir -- {}", shell_quote(&candidate)))?;
            if create.exit_code != 0 { return Err(remote_command_error("Could not create remote Java folder", create)); }
        }
        let canonical = remote_root_with_sftp(sftp, &candidate)?;
        if Path::new(&canonical).parent() != Some(Path::new(&current)) {
            return Err("A remote Java folder resolves outside the configured Minecraft root.".into());
        }
        current = canonical;
    }
    Ok(current)
}

fn existing_remote_java(session: &RemoteConnection, environment: &ServerEnvironment, package: &JavaPackage) -> Result<Option<String>, String> {
    let (root, runtime, java) = remote_java_paths(session, environment, package)?;
    let exists = execute(session, &format!("test -e {} || test -L {}", shell_quote(&runtime), shell_quote(&runtime)))?;
    if exists.exit_code != 0 { return Ok(None); }
    let canonical = remote_root(session, &runtime)?;
    if !canonical.starts_with(&format!("{root}/runtimes/java/")) {
        return Err("The remote Java runtime resolves outside the configured Minecraft root.".into());
    }
    let usable = execute(session, &format!("test -x {}", shell_quote(&java)))?;
    if usable.exit_code != 0 { return Err("The managed remote JDK folder exists but its Java executable is missing. Remove that incomplete folder before retrying.".into()); }
    let actual_java = session.sftp()?.realpath(Path::new(&java))?;
    if !actual_java.starts_with(&format!("{canonical}/")) {
        return Err("The managed remote Java executable resolves outside its runtime folder.".into());
    }
    Ok(Some(java))
}

pub(crate) async fn remote_java_status(
    profile: ServerProfile,
    environment: ServerEnvironment,
    sessions: Arc<RemoteSessionManager>,
    package: JavaPackage,
) -> Result<Option<String>, String> {
    run_remote(profile, environment.clone(), sessions, move |session, _| {
        let root = remote_root(session, &environment.root_directory)?;
        remove_legacy_remote_java_cache(session, &root)?;
        existing_remote_java(session, &environment, &package)
    }).await
}

fn remove_legacy_remote_java_cache(session: &RemoteConnection, root: &str) -> Result<(), String> {
    let sftp = session.sftp()?;
    let cache = format!("{}/cache", root.trim_end_matches('/'));
    let java_cache = format!("{cache}/java");
    for path in [&cache, &java_cache] {
        let exists = execute(session, &format!("test -e {} || test -L {}", shell_quote(path), shell_quote(path)))?;
        if exists.exit_code != 0 {
            return Ok(());
        }
        let metadata = sftp.lstat(Path::new(path))?;
        if metadata.file_type() != FileType::Dir {
            return Err("The old remote Java cache contains a symbolic link or non-folder path. Remove it manually.".into());
        }
        let canonical = remote_root_with_sftp(&sftp, path)?;
        if canonical != *path {
            return Err("The old remote Java cache resolves outside the configured Minecraft root.".into());
        }
    }
    let removed = execute(session, &format!("rm -rf -- {}", shell_quote(&java_cache)))?;
    if removed.exit_code != 0 {
        return Err(remote_command_error("Could not remove the old remote Java cache", removed));
    }
    Ok(())
}

pub(crate) async fn install_remote_java(
    profile: ServerProfile,
    environment: ServerEnvironment,
    sessions: Arc<RemoteSessionManager>,
    package: JavaPackage,
    source: Option<PathBuf>,
    top: String,
) -> Result<String, String> {
    run_remote(profile, environment.clone(), sessions, move |session, _| {
        if let Some(java) = existing_remote_java(session, &environment, &package)? { return Ok(java); }
        let tools = execute(session, "command -v sha256sum >/dev/null && command -v tar >/dev/null && command -v mv >/dev/null && command -v rm >/dev/null")?;
        if tools.exit_code != 0 { return Err("Remote Java installation needs sha256sum, tar, mv, and rm on the Linux host.".into()); }
        let (root, runtime, java) = remote_java_paths(session, &environment, &package)?;
        let sftp = session.sftp()?;
        let major = package.major.to_string();
        let canonical_parent = ensure_remote_managed_dirs(session, &sftp, &root, &["runtimes", "java", &major, &package.platform])?;
        if !canonical_parent.starts_with(&format!("{root}/runtimes/java/")) {
            return Err("The remote Java folder resolves outside the configured Minecraft root.".into());
        }
        let upload = format!("{canonical_parent}/.servercrafter-{}-jdk.tar.gz", Uuid::new_v4());
        let result = (|| {
            download_or_upload(session, &sftp, &upload, &package.link, source.as_deref())?;
            let hash = execute(session, &format!("sha256sum -- {}", shell_quote(&upload)))?;
            if !remote_hash_matches(&hash, &package.sha256) {
                return Err("The JDK archive did not match Adoptium's SHA-256 checksum.".into());
            }
            let archive_top = if top.is_empty() {
                let listed = execute(session, &format!("tar -tzf {} | head -n 20", shell_quote(&upload)))?;
                let listing = String::from_utf8_lossy(&listed.stdout);
                let first = listing
                    .lines()
                    .map(|line| line.trim_start_matches("./").split('/').next().unwrap_or_default())
                    .find(|part| !part.is_empty() && *part != ".")
                    .ok_or("The JDK archive has no top-level folder.")?;
                if first.len() > 80 || !first.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'+')) {
                    return Err("The JDK archive has an unsafe top-level folder.".into());
                }
                first.to_owned()
            } else { top.clone() };
            let staging = format!("{canonical_parent}/.servercrafter-{}.tmp", Uuid::new_v4());
            let create = execute(session, &format!("mkdir -- {}", shell_quote(&staging)))?;
            if create.exit_code != 0 { return Err(remote_command_error("Could not stage remote JDK", create)); }
            let staged_result = (|| {
                let staged = remote_root_with_sftp(&sftp, &staging)?;
                if Path::new(&staged).parent() != Some(Path::new(&canonical_parent)) {
                    return Err("The remote JDK staging folder resolves outside the managed runtime folder.".into());
                }
                let extract = execute(session, &format!("tar -xzf {} -C {}", shell_quote(&upload), shell_quote(&staging)))?;
                if extract.exit_code != 0 { return Err(remote_command_error("Could not extract remote JDK", extract)); }
                let extracted = format!("{staging}/{archive_top}");
                let candidate = format!("{extracted}/bin/java");
                let verify = execute(session, &format!("test -x {} && test ! -e {} && test ! -L {}", shell_quote(&candidate), shell_quote(&runtime), shell_quote(&runtime)))?;
                if verify.exit_code != 0 { return Err("The remote JDK archive is missing Java or the runtime folder already exists.".into()); }
                let move_result = execute(session, &format!("mv -T -- {} {}", shell_quote(&extracted), shell_quote(&runtime)))?;
                if move_result.exit_code != 0 { return Err(remote_command_error("Could not finish installing remote JDK", move_result)); }
                Ok(())
            })();
            let cleanup = execute(session, &format!("rm -rf -- {}", shell_quote(&staging)));
            staged_result?;
            let cleanup = cleanup?;
            if cleanup.exit_code != 0 {
                return Err(remote_command_error("Could not remove remote JDK staging files", cleanup));
            }
            Ok(())
        })();
        let cleanup = sftp.unlink(Path::new(&upload));
        result?;
        cleanup.map_err(|error| format!("Java was installed, but its temporary remote archive could not be removed: {error}"))?;
        Ok(java)
    }).await
}

struct RemoteCommandOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_code: i32,
}

fn remote_command_error(context: &str, output: RemoteCommandOutput) -> String {
    let details = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if details.is_empty() {
        format!("{context} (remote exit code {}).", output.exit_code)
    } else {
        format!("{context}: {details}")
    }
}

fn enable_remote_eula(session: &RemoteConnection, root: &str) -> Result<(), String> {
    let sftp = session.sftp()?;
    let path = format!("{}/eula.txt", root.trim_end_matches('/'));
    let exists = execute(session, &format!("test -e {} || test -L {}", shell_quote(&path), shell_quote(&path)))?;
    let current = if exists.exit_code == 0 {
        let metadata = sftp.lstat(Path::new(&path))?;
        if metadata.file_type() != FileType::File {
            return Err("Remote eula.txt must be a regular file, not a symbolic link.".into());
        }
        if metadata.size.unwrap_or(0) > eula::MAX_EULA_BYTES {
            return Err("Remote eula.txt is unexpectedly large and was not changed.".into());
        }
        let bytes = sftp.read_file(Path::new(&path))?;
        if bytes.len() as u64 > eula::MAX_EULA_BYTES {
            return Err("Remote eula.txt is unexpectedly large and was not changed.".into());
        }
        Some(String::from_utf8(bytes)
            .map_err(|_| "Remote eula.txt is not UTF-8 text and was not changed.".to_owned())?)
    } else {
        None
    };
    let Some(updated) = eula::enabled_content(current.as_deref()) else {
        return Ok(());
    };
    let temporary = format!("{}/.servercrafter-{}.tmp", root.trim_end_matches('/'), Uuid::new_v4());
    if let Err(error) = sftp.write_file_exclusive(Path::new(&temporary), updated.as_bytes()) {
        let _ = sftp.unlink(Path::new(&temporary));
        return Err(format!("Could not prepare remote eula.txt: {error}"));
    }
    let result = if current.is_some() {
        sftp.posix_rename(Path::new(&temporary), Path::new(&path))
    } else {
        sftp.rename(Path::new(&temporary), Path::new(&path))
    };
    if let Err(error) = result {
        let _ = sftp.unlink(Path::new(&temporary));
        return Err(format!("Could not enable remote eula.txt: {error}"));
    }
    Ok(())
}

fn ensure_remote_port(session: &RemoteConnection, root: &str) -> Result<(), String> {
    let sftp = session.sftp()?;
    let path = format!("{}/server.properties", root.trim_end_matches('/'));
    let metadata = match sftp.lstat_optional(Path::new(&path)) {
        Ok(metadata) => Some(metadata),
        Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => None,
        Err(error) => return Err(format!("Could not inspect remote server.properties: {error}")),
    };
    let current = if let Some(metadata) = metadata {
        if metadata.file_type() != FileType::File {
            return Err("Remote server.properties must be a regular file, not a symbolic link.".into());
        }
        if metadata.size.unwrap_or(0) > server_port::MAX_PROPERTIES_BYTES {
            return Err("Remote server.properties is too large to update its port safely.".into());
        }
        let bytes = sftp.read_file(Path::new(&path))?;
        if bytes.len() as u64 > server_port::MAX_PROPERTIES_BYTES {
            return Err("Remote server.properties is too large to update its port safely.".into());
        }
        Some(String::from_utf8(bytes)
            .map_err(|_| "Remote server.properties is not UTF-8 text and was not changed.".to_owned())?)
    } else {
        None
    };
    let requested = server_port::configured_port(current.as_deref())?;
    let sockets = execute(session, "cat /proc/net/tcp /proc/net/tcp6")?;
    if sockets.exit_code != 0 {
        return Err("Could not check remote listening ports through /proc. The server was not started.".into());
    }
    let used: std::collections::HashSet<u16> = String::from_utf8_lossy(&sockets.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            fields.next()?;
            let address = fields.next()?;
            fields.next()?;
            (fields.next()? == "0A").then(|| address.rsplit(':').next())
                .flatten()
                .and_then(|port| u16::from_str_radix(port, 16).ok())
        })
        .collect();
    let Some(port) = server_port::available_port(requested, |candidate| Ok(used.contains(&candidate)))? else {
        return Ok(());
    };
    let updated = server_port::updated_content(current.as_deref(), port);
    if updated.len() as u64 > server_port::MAX_PROPERTIES_BYTES {
        return Err("Remote server.properties is too large to update its port safely.".into());
    }
    let temporary = format!("{}/.scerite-{}.tmp", root.trim_end_matches('/'), Uuid::new_v4());
    if let Err(error) = sftp.write_file_exclusive(Path::new(&temporary), updated.as_bytes()) {
        let _ = sftp.unlink(Path::new(&temporary));
        return Err(format!("Port {requested} is in use, but the remote port change could not be prepared: {error}"));
    }
    let result = if current.is_some() {
        sftp.posix_rename(Path::new(&temporary), Path::new(&path))
    } else {
        sftp.rename(Path::new(&temporary), Path::new(&path))
    };
    if let Err(error) = result {
        let _ = sftp.unlink(Path::new(&temporary));
        return Err(format!("Port {requested} is in use, but remote server.properties could not be updated to port {port}: {error}"));
    }
    Ok(())
}

fn launch_command(launch: &LocalLaunchConfig, managed_java: Option<&str>) -> String {
    let command = std::iter::once(launch.executable.trim())
        .chain(launch.arguments.iter().map(String::as_str))
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ");
    if let Some(java) = managed_java {
        let bin = Path::new(java).parent().expect("validated Java has a bin folder");
        let home = bin.parent().expect("validated Java has a runtime folder");
        format!("export JAVA_HOME={}; export PATH={}:\"$PATH\"; exec {command}",
            shell_quote(&home.to_string_lossy()), shell_quote(&bin.to_string_lossy()))
    } else {
        format!("exec {command}")
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(crate) const REMOTE_DOWNLOAD_UNAVAILABLE: &str = "Remote download unavailable";

fn remote_hash_matches(result: &RemoteCommandOutput, expected: &str) -> bool {
    result.exit_code == 0
        && String::from_utf8_lossy(&result.stdout)
            .split_whitespace()
            .next()
            .is_some_and(|hash| hash.eq_ignore_ascii_case(expected))
}

fn remote_jar_looks_valid(session: &RemoteConnection, path: &str) -> Result<bool, String> {
    let quoted = shell_quote(path);
    let result = execute(session, &format!(
        "test -f {quoted} && test ! -L {quoted} && size=$(wc -c < {quoted}) && test \"$size\" -ge 1024 && test \"$size\" -le 536870912 && test \"$(head -c 2 {quoted})\" = PK"
    ))?;
    Ok(result.exit_code == 0)
}

fn download_or_upload(
    session: &RemoteConnection,
    sftp: &RemoteSftp,
    destination: &str,
    url: &str,
    source: Option<&Path>,
) -> Result<(), String> {
    if let Some(source) = source {
        return sftp.upload_file_exclusive(Path::new(destination), source)
            .map_err(|error| format!("Could not upload file: {error}"));
    }
    let command = format!(
        "if command -v curl >/dev/null 2>&1; then curl --fail --location --silent --show-error --proto '=https' --proto-redir '=https' --max-redirs 5 --connect-timeout 15 --max-time 1800 --output {} -- {}; elif command -v wget >/dev/null 2>&1; then wget --https-only --max-redirect=5 --timeout=30 --output-document={} -- {}; else exit 127; fi",
        shell_quote(destination), shell_quote(url), shell_quote(destination), shell_quote(url)
    );
    let result = execute(session, &command)?;
    if result.exit_code != 0 {
        let _ = sftp.unlink(Path::new(destination));
        return Err(format!("{REMOTE_DOWNLOAD_UNAVAILABLE}: the remote host could not fetch the file over HTTPS."));
    }
    Ok(())
}

fn session_name(profile_id: Uuid) -> String {
    format!("scerite-{}", profile_id.simple())
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordBridgeServer {
    profile_id: Uuid,
    channel_id: String,
    chat_to_discord: bool,
    chat_to_minecraft: bool,
    chat_to_minecraft_template: String,
    online: bool,
    offline: bool,
    join: bool,
    leave: bool,
    kills: bool,
    achievements: bool,
    chat_to_discord_channel_id: String,
    chat_to_minecraft_channel_id: String,
    online_channel_id: String,
    offline_channel_id: String,
    join_channel_id: String,
    leave_channel_id: String,
    kills_channel_id: String,
    achievements_channel_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordBridgeStatus {
    enabled: bool,
    running: bool,
    has_token: bool,
    update_available: bool,
    cpu_percent: Option<f32>,
    memory_bytes: Option<u64>,
    servers: Vec<DiscordBridgeServer>,
}

fn discord_bridge_profile(
    profiles: &ServerProfileStore,
    environment_id: Uuid,
) -> Result<(ServerProfile, ServerEnvironment), String> {
    let environment = profiles.environment(environment_id)?;
    let profile = profiles.list()?.into_iter().find(|profile| {
        matches!(&profile.target, ServerTarget::Remote { environment_id: id, .. } if *id == environment_id)
    }).unwrap_or_else(|| ServerProfile {
        id: environment_id,
        name: environment.name.clone(),
        target: ServerTarget::Remote {
            environment_id,
            directory: environment.root_directory.clone(),
            launch: LocalLaunchConfig::default(),
        },
        installed_software: None,
        installed_version: None,
        installed_packages: Vec::new(),
    });
    Ok((profile, environment))
}

fn bridge_session_name(environment_id: Uuid) -> String {
    format!("scerite-discord-{}", environment_id.simple())
}

fn bridge_directory(session: &RemoteConnection, environment: &ServerEnvironment) -> Result<String, String> {
    let root = remote_root(session, &environment.root_directory)?;
    let sftp = session.sftp()?;
    let environment_folder = environment.id.simple().to_string();
    let bridge_root = format!("{root}/runtimes/discord-bridge/{environment_folder}");
    let mut path = root;
    for part in ["runtimes", "discord-bridge", environment_folder.as_str()] {
        path = format!("{path}/{part}");
        match sftp.lstat_optional(Path::new(&path)) {
            Ok(attrs) if attrs.file_type() == FileType::Dir => {},
            Ok(_) => return Err("A Discord bridge folder is not a regular directory.".into()),
            Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => break,
            Err(error) => return Err(format!("Could not inspect the Discord bridge folder: {error}")),
        }
    }
    Ok(bridge_root)
}

fn bridge_file_exists(sftp: &RemoteSftp, path: &str) -> Result<bool, String> {
    match sftp.lstat_optional(Path::new(path)) {
        Ok(attrs) if attrs.file_type() == FileType::File => Ok(true),
        Ok(_) => Err("A Discord bridge file is not a regular file.".into()),
        Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => Ok(false),
        Err(error) => Err(format!("Could not inspect Discord bridge file: {error}")),
    }
}

fn bridge_write(sftp: &RemoteSftp, path: &str, content: &[u8]) -> Result<(), String> {
    let temporary = format!("{path}.{}.tmp", Uuid::new_v4().simple());
    sftp.write_file_exclusive(Path::new(&temporary), content)?;
    if let Err(error) = sftp.posix_rename(Path::new(&temporary), Path::new(path)) {
        let _ = sftp.unlink(Path::new(&temporary));
        return Err(format!("Could not save Discord bridge setup: {error}"));
    }
    Ok(())
}

#[tauri::command]
pub async fn discord_bridge_status(
    environment_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<DiscordBridgeStatus, String> {
    let (profile, environment) = discord_bridge_profile(&profiles, environment_id)?;
    let session_name = bridge_session_name(environment_id);
    run_remote(profile, environment.clone(), Arc::clone(remote_sessions.inner()), move |session, _| {
        let root = bridge_directory(session, &environment)?;
        let sftp = session.sftp()?;
        let config_path = format!("{root}/servers.json");
        let token_path = format!("{root}/token");
        let enabled = bridge_file_exists(&sftp, &config_path)?;
        let has_token = bridge_file_exists(&sftp, &token_path)?;
        let script_path = format!("{root}/discord_bridge.ts");
        let bundled = include_bytes!("discord_bridge.ts");
        let update_available = enabled && (!bridge_file_exists(&sftp, &script_path)?
            || sftp.read_prefix(Path::new(&script_path), bundled.len() + 1)? != bundled);
        let servers = if enabled {
            let value: serde_json::Value = serde_json::from_slice(&sftp.read_file(Path::new(&config_path))?)
                .map_err(|error| format!("Could not read Discord bridge settings: {error}"))?;
            value.as_array().ok_or_else(|| "Discord bridge settings are invalid.".to_owned())?
                .iter().map(|item| {
                    let mut item = item.clone();
                    let object = item.as_object_mut().ok_or_else(|| "Discord bridge settings are invalid.".to_owned())?;
                    let defaults = [("chatToDiscord", "chat"), ("chatToMinecraft", "chat"), ("online", "status"), ("offline", "status"), ("join", "joins"), ("leave", "joins")];
                    for (field, legacy) in defaults {
                        if !object.contains_key(field) { object.insert(field.into(), object.get(legacy).cloned().unwrap_or(serde_json::Value::Bool(true))); }
                    }
                    for field in ["kills", "achievements"] {
                        if !object.contains_key(field) { object.insert(field.into(), serde_json::Value::Bool(true)); }
                    }
                    for field in ["chatToDiscordChannelId", "chatToMinecraftChannelId", "onlineChannelId", "offlineChannelId", "joinChannelId", "leaveChannelId", "killsChannelId", "achievementsChannelId"] {
                        if !object.contains_key(field) { object.insert(field.into(), serde_json::Value::String(String::new())); }
                    }
                    if object.get("chatToMinecraftTemplate").and_then(serde_json::Value::as_str)
                        .is_none_or(|value| matches!(value, "<🎮{username}> {message}" | "<🎮{color:blue}{name}{color:reset}> {message}" | "<{color:blue}🎮{name}{color:reset}> {message}")) {
                        object.insert("chatToMinecraftTemplate".into(), serde_json::Value::String("<{color:blue}🎮{color:reset}{name}> {message}".into()));
                    }
                    serde_json::from_value(item)
                        .map_err(|error| format!("Discord bridge settings are invalid: {error}"))
                })
                .collect::<Result<Vec<_>, _>>()?
        } else { Vec::new() };
        let pane = execute(session, &format!("tmux display-message -p -t {} '#{{pane_pid}} #{{pane_dead}}' 2>/dev/null", shell_quote(&session_name)))?;
        let pane_output = String::from_utf8_lossy(&pane.stdout);
        let mut pane_parts = pane_output.split_whitespace();
        let pid = pane_parts.next().and_then(|value| value.parse::<u32>().ok());
        let running = pane.exit_code == 0 && pid.is_some() && pane_parts.next() == Some("0");
        let (cpu_percent, memory_bytes) = if running {
            let result = execute(session, &format!("ps -p {} -o %cpu=,rss=", pid.unwrap()))?;
            if result.exit_code == 0 {
                let output = String::from_utf8_lossy(&result.stdout);
                let mut parts = output.split_whitespace();
                (parts.next().and_then(|value| value.parse::<f32>().ok()), parts.next().and_then(|value| value.parse::<u64>().ok()).map(|value| value * 1024))
            } else { (None, None) }
        } else { (None, None) };
        Ok(DiscordBridgeStatus { enabled, running, has_token, update_available, cpu_percent, memory_bytes, servers })
    }).await
}

#[tauri::command]
pub async fn enable_discord_bridge(
    environment_id: Uuid,
    token: String,
    servers: Vec<DiscordBridgeServer>,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<(), String> {
    let (profile, environment) = discord_bridge_profile(&profiles, environment_id)?;
    let candidates = profiles.list()?;
    let mut seen_profiles = std::collections::HashSet::new();
    for server in &servers {
        if !seen_profiles.insert(server.profile_id) {
            return Err("Each server must appear only once.".into());
        }
        for channel in [&server.channel_id, &server.chat_to_discord_channel_id, &server.chat_to_minecraft_channel_id, &server.online_channel_id, &server.offline_channel_id, &server.join_channel_id, &server.leave_channel_id, &server.kills_channel_id, &server.achievements_channel_id] {
            if !channel.is_empty() && (!(15..=22).contains(&channel.len()) || !channel.bytes().all(|byte| byte.is_ascii_digit())) {
                return Err("Enter a valid numeric Discord channel ID or leave it blank.".into());
            }
        }
        if !server.chat_to_minecraft_template.contains("{message}")
            || server.chat_to_minecraft_template.chars().count() > 200
            || server.chat_to_minecraft_template.chars().any(char::is_control) {
            return Err("The Discord-to-game format must include {message}, be at most 200 characters, and stay on one line.".into());
        }
        if !candidates.iter().any(|candidate| candidate.id == server.profile_id && matches!(&candidate.target, ServerTarget::Remote { environment_id: id, .. } if *id == environment_id)) {
            return Err("A selected server does not belong to this environment.".into());
        }
    }
    let token = token.trim().to_owned();
    if !token.is_empty() && (token.len() > 512 || !token.bytes().all(|byte| byte.is_ascii_graphic())) {
        return Err("The Discord bot token has an invalid format.".into());
    }
    let bridge_servers = servers.into_iter().map(|server| {
        let selected = candidates.iter().find(|candidate| candidate.id == server.profile_id).unwrap();
        (server, selected.clone())
    }).collect::<Vec<_>>();
    let bridge_name = bridge_session_name(environment_id);
    run_remote(profile, environment.clone(), Arc::clone(remote_sessions.inner()), move |session, _| {
        let sftp = session.sftp()?;
        let root = remote_root(session, &environment.root_directory)?;
        let environment_folder = environment_id.simple().to_string();
        let bridge_root = ensure_remote_managed_dirs(session, &sftp, &root, &["runtimes", "discord-bridge", &environment_folder])?;
        let token_path = format!("{bridge_root}/token");
        if token.is_empty() && !bridge_file_exists(&sftp, &token_path)? {
            return Err("Enter your Discord bot token to enable the bridge.".into());
        }
        let mut config = Vec::new();
        for (server, selected) in bridge_servers {
            let directory = remote_root(session, configured_remote_root(&selected)?)?;
            config.push(serde_json::json!({
                "id": server.profile_id,
                "name": selected.name,
                "minecraft_version": selected.installed_version,
                "directory": directory,
                "session": session_name(server.profile_id),
                "profileId": server.profile_id,
                "channelId": server.channel_id,
                "channel_id": server.channel_id,
                "chat_to_discord": server.chat_to_discord,
                "chat_to_minecraft": server.chat_to_minecraft,
                "chatToMinecraftTemplate": server.chat_to_minecraft_template,
                "chat_to_minecraft_template": server.chat_to_minecraft_template,
                "online": server.online,
                "offline": server.offline,
                "join": server.join,
                "leave": server.leave,
                "kills": server.kills,
                "achievements": server.achievements,
                "chat_to_discord_channel_id": server.chat_to_discord_channel_id,
                "chat_to_minecraft_channel_id": server.chat_to_minecraft_channel_id,
                "online_channel_id": server.online_channel_id,
                "offline_channel_id": server.offline_channel_id,
                "join_channel_id": server.join_channel_id,
                "leave_channel_id": server.leave_channel_id,
                "kills_channel_id": server.kills_channel_id,
                "achievements_channel_id": server.achievements_channel_id,
            }));
        }
        let architecture = execute(session, "uname -m")?;
        if architecture.exit_code != 0 { return Err(remote_command_error("Could not detect remote CPU architecture", architecture)); }
        let (arch, expected_hash) = match String::from_utf8_lossy(&architecture.stdout).trim() {
            "x86_64" | "amd64" => ("x64", "6c172a9ab6718f7b801fb5a7bd486af2ee5e32758086a04b84fcce2aa77095e6"),
            "aarch64" | "arm64" => ("arm64", "dbabf7a1cbf947e6059ffad2c27b80041db97dea1e553b62afdee7f13fca336e"),
            _ => return Err("The Discord bridge supports remote Linux x64 and arm64 hosts.".into()),
        };
        let folder = format!("node-v24.22.1-linux-{arch}");
        let node = format!("{bridge_root}/{folder}/bin/node");
        if execute(session, &format!("test -x {}", shell_quote(&node)))?.exit_code != 0 {
            let archive = format!("{bridge_root}/{folder}.tar.xz");
            let url = format!("https://nodejs.org/dist/v24.22.1/{folder}.tar.xz");
            download_or_upload(session, &sftp, &archive, &url, None)?;
            let checksum = execute(session, &format!("sha256sum -- {}", shell_quote(&archive)))?;
            if checksum.exit_code != 0 || !String::from_utf8_lossy(&checksum.stdout).starts_with(expected_hash) {
                let _ = sftp.unlink(Path::new(&archive));
                return Err("The downloaded Node.js runtime failed SHA-256 verification.".into());
            }
            let unpack = execute(session, &format!("tar -xJf {} -C {}", shell_quote(&archive), shell_quote(&bridge_root)))?;
            let _ = sftp.unlink(Path::new(&archive));
            if unpack.exit_code != 0 || execute(session, &format!("test -x {}", shell_quote(&node)))?.exit_code != 0 {
                return Err("Could not unpack the Discord bridge's Node.js runtime.".into());
            }
        }
        let npm = format!("{bridge_root}/{folder}/lib/node_modules/npm/bin/npm-cli.js");
        bridge_write(&sftp, &format!("{bridge_root}/package.json"), br#"{"name":"scerite-discord-bridge","private":true,"type":"module","dependencies":{"discord.js":"14.27.0"}}"#)?;
        let install = execute(session, &format!(
            "{} {} install --no-audit --no-fund --ignore-scripts --silent --prefix {} --save-exact 'discord.js@14.27.0'",
            shell_quote(&node), shell_quote(&npm), shell_quote(&bridge_root)
        ))?;
        if install.exit_code != 0 { return Err("Could not install discord.js. The remote host needs outbound HTTPS access to npm.".into()); }
        let script_path = format!("{bridge_root}/discord_bridge.ts");
        bridge_write(&sftp, &script_path, include_str!("discord_bridge.ts").as_bytes())?;
        bridge_write(&sftp, &format!("{bridge_root}/servers.json"), &serde_json::to_vec(&config).map_err(|error| error.to_string())?)?;
        if !token.is_empty() { bridge_write(&sftp, &token_path, token.as_bytes())?; }
        let _ = execute(session, &format!("tmux kill-session -t {} 2>/dev/null", shell_quote(&bridge_name)));
        let launch = format!(
            "umask 077; exec {} {} >> {} 2>&1",
            shell_quote(&node), shell_quote(&script_path), shell_quote(&format!("{bridge_root}/bridge.log"))
        );
        let started = execute(session, &format!(
            "tmux new-session -d -s {} -c {} {}",
            shell_quote(&bridge_name), shell_quote(&bridge_root), shell_quote(&launch)
        ))?;
        if started.exit_code != 0 { return Err(remote_command_error("Could not start the Discord bridge", started)); }
        std::thread::sleep(Duration::from_secs(2));
        if execute(session, &format!("tmux has-session -t {} 2>/dev/null", shell_quote(&bridge_name)))?.exit_code != 0 {
            return Err(format!("The Discord bridge exited during startup. Check {bridge_root}/bridge.log on the remote host."));
        }
        Ok(())
    }).await
}

#[tauri::command]
pub async fn disable_discord_bridge(
    environment_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<(), String> {
    let (profile, environment) = discord_bridge_profile(&profiles, environment_id)?;
    let bridge_name = bridge_session_name(environment_id);
    run_remote(profile, environment.clone(), Arc::clone(remote_sessions.inner()), move |session, _| {
        let root = bridge_directory(session, &environment)?;
        let sftp = session.sftp()?;
        let stopped = execute(session, &format!("tmux kill-session -t {} 2>/dev/null", shell_quote(&bridge_name)))?;
        if stopped.exit_code != 0 && stopped.exit_code != 1 {
            return Err(remote_command_error("Could not stop the Discord bridge", stopped));
        }
        for name in ["token", "servers.json"] {
            let path = format!("{root}/{name}");
            if bridge_file_exists(&sftp, &path)? { sftp.unlink(Path::new(&path))?; }
        }
        Ok(())
    }).await
}

fn configured_remote_root(profile: &ServerProfile) -> Result<&str, String> {
    let ServerTarget::Remote { directory, .. } = &profile.target else {
        return Err("This operation is only available for remote profiles.".into());
    };
    if directory.trim().is_empty() {
        return Err("Set the remote server folder in the profile before opening files.".into());
    }
    Ok(directory)
}

fn optional_remote_root(
    session: &RemoteConnection,
    profile: &ServerProfile,
) -> Result<Option<String>, String> {
    let ServerTarget::Remote { directory, .. } = &profile.target else {
        return Err("This operation is only available for remote profiles.".into());
    };
    if directory.trim().is_empty() {
        Ok(None)
    } else {
        remote_root(session, directory).map(Some)
    }
}

fn remote_root(session: &RemoteConnection, configured: &str) -> Result<String, String> {
    let sftp = session
        .sftp()
        .map_err(|error| format!("Could not open SFTP: {error}"))?;
    remote_root_with_sftp(&sftp, configured)
}

fn remote_root_with_sftp(sftp: &RemoteSftp, configured: &str) -> Result<String, String> {
    let canonical = sftp
        .realpath(Path::new(configured))
        .map_err(|error| format!("Could not open the remote server folder: {error}"))?;
    let root = canonical.replace('\\', "/");
    if !root.starts_with('/') {
        return Err("The remote server folder must resolve to an absolute path.".into());
    }
    if root == "/" {
        return Err("Choose a remote server folder narrower than the filesystem root.".into());
    }
    let metadata = sftp
        .stat(Path::new(&root))
        .map_err(|error| format!("Could not read remote server folder: {error}"))?;
    if metadata.file_type() != FileType::Dir {
        return Err("The configured remote server path is not a folder.".into());
    }
    Ok(root)
}

fn resolve_remote_relative(root: &str, relative: &str, allow_root: bool) -> Result<String, String> {
    if relative.is_empty() && allow_root {
        return Ok(root.to_owned());
    }
    if relative.is_empty() || relative.starts_with('/') || relative.contains('\\') {
        return Err("Choose a path inside the configured server folder.".into());
    }
    let segments: Vec<_> = relative.split('/').collect();
    if segments.iter().any(|segment| {
        segment.is_empty()
            || matches!(*segment, "." | "..")
            || segment.chars().any(char::is_control)
    }) {
        return Err("The remote path contains an invalid file or folder name.".into());
    }
    Ok(format!(
        "{}/{}",
        root.trim_end_matches('/'),
        segments.join("/")
    ))
}

fn canonical_inside(sftp: &RemoteSftp, root: &str, candidate: &str) -> Result<String, String> {
    let canonical = sftp
        .realpath(Path::new(candidate))
        .map_err(|error| format!("Could not resolve the remote path: {error}"))?;
    let canonical = canonical.replace('\\', "/");
    let root_prefix = format!("{}/", root.trim_end_matches('/'));
    if canonical != root && !canonical.starts_with(&root_prefix) {
        return Err("The remote path resolves outside the configured server folder.".into());
    }
    Ok(canonical)
}

fn reject_remote_symlink_path(
    sftp: &RemoteSftp,
    root: &str,
    candidate: &str,
) -> Result<(), String> {
    let prefix = format!("{}/", root.trim_end_matches('/'));
    let relative = candidate.strip_prefix(&prefix).unwrap_or_default();
    let mut current = root.to_owned();
    if relative.is_empty() {
        let metadata = sftp
            .lstat(Path::new(&current))
            .map_err(|error| format!("Could not inspect the remote path: {error}"))?;
        return if metadata.file_type() == FileType::Symlink {
            Err("Remote symbolic links cannot be opened or edited here.".into())
        } else {
            Ok(())
        };
    }
    for segment in relative.split('/') {
        current = format!("{}/{}", current.trim_end_matches('/'), segment);
        let metadata = sftp
            .lstat(Path::new(&current))
            .map_err(|error| format!("Could not inspect the remote path: {error}"))?;
        if metadata.file_type() == FileType::Symlink {
            return Err("Remote symbolic links cannot be opened or edited here.".into());
        }
    }
    Ok(())
}

fn remote_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn checked_remote_world(sftp: &RemoteSftp, root: &str, name: &str) -> Result<String, String> {
    if !world_management::valid_world_name(name) {
        return Err("Choose a valid world folder.".into());
    }
    let world = resolve_remote_relative(root, name, false)?;
    reject_remote_symlink_path(sftp, root, &world)?;
    let world = canonical_inside(sftp, root, &world)?;
    if sftp.lstat(Path::new(&world))?.file_type() != FileType::Dir
        || sftp.lstat(Path::new(&format!("{world}/level.dat")))?.file_type() != FileType::File
    {
        return Err("The selected folder is not a world.".into());
    }
    Ok(world)
}

fn require_remote_world_stopped(session: &RemoteConnection, profile: &ServerProfile, root: &str) -> Result<(), String> {
    match remote_is_running(session, profile)? {
        Some(false) => {}
        Some(true) => return Err("Stop the server before changing or copying a world.".into()),
        None => return Err("tmux is required to confirm the remote server is stopped.".into()),
    }
    if remote_directory_has_java_process(session, root)? {
        return Err("A Java process is using this server folder. Stop it before changing or copying a world.".into());
    }
    Ok(())
}

fn remote_preserved_world(sftp: &RemoteSftp, root: &str, name: &str) -> Result<String, String> {
    let backups = format!("{root}/.scerite-world-backups");
    match sftp.lstat_optional(Path::new(&backups)) {
        Ok(metadata) if metadata.file_type() != FileType::Dir => return Err("The world backup path is not a folder.".into()),
        Ok(_) => reject_remote_symlink_path(sftp, root, &backups)?,
        Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => sftp.mkdir(Path::new(&backups))?,
        Err(error) => return Err(format!("Could not inspect the world backup folder: {error}")),
    }
    let backups = canonical_inside(sftp, root, &backups)?;
    Ok(format!("{backups}/{}", world_management::preserved_backup_filename(name)?))
}

fn restore_remote_worlds(sftp: &RemoteSftp, moved: &[(String, String)]) -> String {
    let unresolved: Vec<_> = moved.iter().rev().filter_map(|(original, backup)| {
        sftp.rename(Path::new(backup), Path::new(original)).err().map(|_| backup.clone())
    }).collect();
    if unresolved.is_empty() { String::new() }
    else { format!(" Recovery needs attention: preserved worlds remain at {}.", unresolved.join(", ")) }
}

fn zip_remote_directory(
    sftp: &RemoteSftp, zip: &mut ZipWriter<File>, directory: &str,
    relative: &str, count: &mut usize, depth: usize,
) -> Result<(), String> {
    if depth > 64 { return Err("The world folder nesting is too deep to archive.".into()); }
    let entries = sftp.read_dir(Path::new(directory)).map_err(|error| format!("Could not list world files: {error}"))?;
    if entries.len() > MAX_DIRECTORY_ENTRIES { return Err("A world folder has too many entries to archive.".into()); }
    for (name, metadata) in entries {
        if matches!(name.as_str(), "." | "..") { continue; }
        if name.is_empty() || name.contains(['/', '\\', ':']) || name.chars().any(char::is_control) {
            return Err("The world has an unsafe filename.".into());
        }
        *count += 1;
        if *count > 200_000 { return Err("The world has too many files to archive.".into()); }
        let child = format!("{directory}/{name}");
        let zip_path = format!("{relative}/{name}");
        match metadata.file_type() {
            FileType::Dir => {
                zip.add_directory(format!("{zip_path}/"), SimpleFileOptions::default()).map_err(|error| error.to_string())?;
                zip_remote_directory(sftp, zip, &child, &zip_path, count, depth + 1)?;
            }
            FileType::File => {
                zip.start_file(zip_path, SimpleFileOptions::default().compression_method(CompressionMethod::Deflated))
                    .map_err(|error| error.to_string())?;
                sftp.copy_file_to_writer(Path::new(&child), zip)
                    .map_err(|error| format!("Could not archive a remote world file: {error}"))?;
            }
            _ => return Err("The world contains a symbolic link or unsupported file.".into()),
        }
    }
    Ok(())
}

fn export_remote_world(sftp: &RemoteSftp, world: &str, name: &str, destination: &Path) -> Result<(), String> {
    let temporary = destination.with_extension(format!("{}.part", Uuid::new_v4()));
    let result = (|| {
        let output = File::create_new(&temporary).map_err(|error| format!("Could not create ZIP: {error}"))?;
        let mut zip = ZipWriter::new(output);
        zip.add_directory(format!("{name}/"), SimpleFileOptions::default()).map_err(|error| error.to_string())?;
        zip_remote_directory(sftp, &mut zip, world, name, &mut 0, 0)?;
        zip.finish().map_err(|error| format!("Could not finish ZIP: {error}"))?;
        std::fs::rename(&temporary, destination).map_err(|error| format!("Could not save ZIP: {error}"))
    })();
    if result.is_err() { let _ = std::fs::remove_file(&temporary); }
    result
}

#[tauri::command]
pub async fn read_remote_world_settings(
    profile_id: Uuid, world_name: String,
    profiles: State<'_, ServerProfileStore>, remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<WorldSettings, String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        let world = checked_remote_world(&sftp, &root, &world_name)?;
        let level = format!("{world}/level.dat");
        let metadata = sftp.lstat(Path::new(&level))?;
        if metadata.size.is_some_and(|size| size > world_management::MAX_LEVEL_BYTES) {
            return Err("level.dat is too large to inspect.".into());
        }
        let bytes = sftp.read_prefix(Path::new(&level), world_management::MAX_LEVEL_BYTES as usize + 1)?;
        if bytes.len() as u64 > world_management::MAX_LEVEL_BYTES { return Err("level.dat is too large to inspect.".into()); }
        let mut settings = world_management::decode_settings(&bytes)?;
        let separate = format!("{world}/data/minecraft/game_rules.dat");
        let separate_metadata = match sftp.lstat_optional(Path::new(&separate)) {
            Ok(metadata) => Some(metadata),
            Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => None,
            Err(error) => return Err(format!("Could not inspect game_rules.dat: {error}")),
        };
        if let Some(metadata) = separate_metadata {
            if metadata.file_type() == FileType::File {
                if metadata.size.is_some_and(|size| size > world_management::MAX_LEVEL_BYTES) {
                    return Err("game_rules.dat is too large to inspect.".into());
                }
                let bytes = sftp.read_prefix(Path::new(&separate), world_management::MAX_LEVEL_BYTES as usize + 1)?;
                settings.gamerules = world_management::decode_separate_rules(&bytes)?;
            }
        }
        Ok(settings)
    }).await
}

#[tauri::command]
pub async fn list_remote_world_backups(
    app: AppHandle, profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>, remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<Vec<WorldBackup>, String> {
    let mut backups = world_management::app_world_backups(&app, profile_id)?;
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    let mut preserved = run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        let folder = format!("{root}/.scerite-world-backups");
        let metadata = match sftp.lstat_optional(Path::new(&folder)) {
            Ok(metadata) => metadata,
            Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => return Ok(Vec::new()),
            Err(error) => return Err(format!("Could not inspect preserved worlds: {error}")),
        };
        if metadata.file_type() != FileType::Dir { return Err("The preserved world path is not a folder.".into()); }
        let folder = canonical_inside(&sftp, &root, &folder)?;
        let entries = sftp.read_dir(Path::new(&folder)).map_err(|error| format!("Could not list preserved worlds: {error}"))?;
        if entries.len() > MAX_DIRECTORY_ENTRIES { return Err("Too many preserved worlds to list.".into()); }
        let mut result = Vec::new();
        for (filename, metadata) in entries {
            if metadata.file_type() != FileType::Dir { continue; }
            let Some(name) = world_management::preserved_backup_name(&filename) else { continue; };
            let path = format!("{folder}/{filename}");
            if !sftp.lstat(Path::new(&format!("{path}/level.dat")))
                .is_ok_and(|level| level.file_type() == FileType::File) { continue; }
            result.push(WorldBackup { name, path, kind: "preserved".into(), modified: world_management::preserved_backup_time(&filename).or_else(|| metadata.mtime.map(u64::from)), automatic: false });
        }
        Ok(result)
    }).await?;
    backups.append(&mut preserved);
    backups.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(backups)
}

#[tauri::command]
pub async fn backup_remote_world(
    app: AppHandle, profile_id: Uuid, world_name: String,
    profiles: State<'_, ServerProfileStore>, remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<String, String> {
    let destination = world_management::backup_destination(&app, profile_id, &world_name)?;
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        require_remote_world_stopped(session, profile, &root)?;
        let world = checked_remote_world(&sftp, &root, &world_name)?;
        export_remote_world(&sftp, &world, &world_name, &destination)?;
        Ok(destination.to_string_lossy().into_owned())
    }).await
}

#[tauri::command]
pub async fn auto_backup_remote_world(
    app: AppHandle, profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>, remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<Option<String>, String> {
    let settings = profiles.auto_backup_settings(profile_id)?;
    if !settings.enabled { return Ok(None); }
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    let app_for_remote = app.clone();
    let (path, name) = run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        require_remote_world_stopped(session, profile, &root)?;
        let properties_path = format!("{root}/server.properties");
        reject_remote_symlink_path(&sftp, &root, &properties_path)?;
        let properties_path = canonical_inside(&sftp, &root, &properties_path)?;
        if sftp.lstat(Path::new(&properties_path))?.file_type() != FileType::File {
            return Err("server.properties is missing or is not a regular file.".into());
        }
        let properties = sftp.read_prefix(Path::new(&properties_path), MAX_FILE_BYTES as usize + 1)?;
        if properties.len() as u64 > MAX_FILE_BYTES { return Err("server.properties is too large to read for automatic backup.".into()); }
        let name = server_players::world_name(Some(&properties))?;
        let world = checked_remote_world(&sftp, &root, &name)?;
        let destination = world_management::backup_destination_for(&app_for_remote, profile_id, &name, true)?;
        export_remote_world(&sftp, &world, &name, &destination)?;
        Ok((destination.to_string_lossy().into_owned(), name))
    }).await?;
    world_management::prune_automatic_backups(&app, profile_id, &name, settings.keep, &path)?;
    Ok(Some(path))
}

#[tauri::command]
pub async fn download_remote_world(
    profile_id: Uuid, world_name: String, destination: String,
    profiles: State<'_, ServerProfileStore>, remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<(), String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        require_remote_world_stopped(session, profile, &root)?;
        let world = checked_remote_world(&sftp, &root, &world_name)?;
        export_remote_world(&sftp, &world, &world_name, Path::new(&destination))
    }).await
}

#[tauri::command]
pub async fn upload_remote_world(
    profile_id: Uuid, world_name: String, source: String,
    profiles: State<'_, ServerProfileStore>, remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<String, String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        let (mut archive, entries) = world_management::open_archive(Path::new(&source))?;
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        require_remote_world_stopped(session, profile, &root)?;
        checked_remote_world(&sftp, &root, &world_name)?;
        install_remote_archive(session, &sftp, &root, &world_name, &mut archive, entries, true, false)
    }).await
}

#[tauri::command]
pub async fn import_remote_world(
    profile_id: Uuid, world_name: String, source: String,
    profiles: State<'_, ServerProfileStore>, remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<String, String> {
    if !world_management::valid_world_name(&world_name) { return Err("Choose a valid world folder name.".into()); }
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        let (mut archive, entries) = world_management::open_archive(Path::new(&source))?;
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        require_remote_world_stopped(session, profile, &root)?;
        install_remote_archive(session, &sftp, &root, &world_name, &mut archive, entries, false, true)
    }).await
}

fn install_remote_archive(
    session: &RemoteConnection, sftp: &RemoteSftp, root: &str, world_name: &str,
    archive: &mut zip::ZipArchive<File>, entries: Vec<world_management::ArchiveEntry>, require_existing: bool, require_absent: bool,
) -> Result<String, String> {
        let world = resolve_remote_relative(root, world_name, false)?;
        let existing = match sftp.lstat_optional(Path::new(&world)) {
            Ok(metadata) if metadata.file_type() == FileType::Dir => {
                if require_absent { return Err("A world folder with this name already exists.".into()); }
                checked_remote_world(sftp, root, world_name)?;
                true
            }
            Ok(_) => return Err("The world path is not a regular folder.".into()),
            Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile && !require_existing => false,
            Err(error) => return Err(format!("Could not inspect the current world: {error}")),
        };
        let staging = format!("{root}/.scerite-world-upload-{}", Uuid::new_v4());
        sftp.mkdir(Path::new(&staging)).map_err(|error| format!("Could not create upload folder: {error}"))?;
        let result = (|| {
            let mut directories = std::collections::HashSet::new();
            for entry in entries {
                let relative = remote_path(&entry.relative);
                let mut parent = staging.clone();
                let parts: Vec<_> = relative.split('/').collect();
                for part in &parts[..parts.len() - if entry.directory { 0 } else { 1 }] {
                    parent.push('/'); parent.push_str(part);
                    if directories.insert(parent.clone()) { sftp.mkdir(Path::new(&parent))?; }
                }
                if !entry.directory {
                    let destination = format!("{staging}/{relative}");
                    sftp.write_file_from_reader(Path::new(&destination),
                        &mut archive.by_index(entry.index).map_err(|error| error.to_string())?)?;
                }
            }
            if sftp.lstat(Path::new(&format!("{staging}/level.dat")))?.file_type() != FileType::File {
                return Err("The ZIP does not contain level.dat.".into());
            }
            let preserved = if existing {
                let preserved = remote_preserved_world(sftp, root, world_name)?;
                sftp.rename(Path::new(&world), Path::new(&preserved))
                    .map_err(|error| format!("Could not preserve the current world: {error}"))?;
                Some(preserved)
            } else { None };
            if let Err(error) = sftp.rename(Path::new(&staging), Path::new(&world)) {
                let recovery = preserved.as_ref().and_then(|path| sftp.rename(Path::new(path), Path::new(&world)).err()
                    .map(|_| format!(" The previous world remains at {path}.")))
                    .unwrap_or_default();
                return Err(format!("Could not install the uploaded world: {error}.{recovery}"));
            }
            Ok(preserved.unwrap_or_default())
        })();
        if result.is_err() {
            // This path is generated under the verified server root and never contains user input.
            let _ = execute(session, &format!("rm -rf -- {}", shell_quote(&staging)));
        }
        result
}

#[tauri::command]
pub async fn restore_remote_world_backup(
    app: AppHandle, profile_id: Uuid, world_name: String, backup_kind: String, backup_path: String,
    profiles: State<'_, ServerProfileStore>, remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<String, String> {
    if !world_management::valid_world_name(&world_name) { return Err("Choose a valid world folder.".into()); }
    let source = if backup_kind == "zip" {
        Some(world_management::checked_app_backup(&app, profile_id, &world_name, &backup_path)?)
    } else if backup_kind == "preserved" { None }
    else { return Err("Choose a valid world backup.".into()); };
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        require_remote_world_stopped(session, profile, &root)?;
        if let Some(source) = source {
            let (mut archive, entries) = world_management::open_archive(&source)?;
            return install_remote_archive(session, &sftp, &root, &world_name, &mut archive, entries, false, false);
        }
        let filename = Path::new(&backup_path).file_name().and_then(|name| name.to_str())
            .ok_or("Choose a valid preserved world.")?;
        let folder = format!("{root}/.scerite-world-backups");
        let expected = format!("{folder}/{filename}");
        if backup_path != expected || world_management::preserved_backup_name(filename).as_deref() != Some(&world_name) {
            return Err("The selected folder is not a backup of this world.".into());
        }
        reject_remote_symlink_path(&sftp, &root, &expected)?;
        let source = canonical_inside(&sftp, &root, &expected)?;
        if sftp.lstat(Path::new(&source))?.file_type() != FileType::Dir
            || sftp.lstat(Path::new(&format!("{source}/level.dat")))?.file_type() != FileType::File {
            return Err("The selected preserved world is missing or invalid.".into());
        }
        let world = resolve_remote_relative(&root, &world_name, false)?;
        let preserved = match sftp.lstat_optional(Path::new(&world)) {
            Ok(metadata) if metadata.file_type() == FileType::Dir => {
                checked_remote_world(&sftp, &root, &world_name)?;
                let preserved = remote_preserved_world(&sftp, &root, &world_name)?;
                sftp.rename(Path::new(&world), Path::new(&preserved))
                    .map_err(|error| format!("Could not preserve the current world: {error}"))?;
                Some(preserved)
            }
            Ok(_) => return Err("The current world path is not a regular folder.".into()),
            Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => None,
            Err(error) => return Err(format!("Could not inspect the current world: {error}")),
        };
        if let Err(error) = sftp.rename(Path::new(&source), Path::new(&world)) {
            let recovery = preserved.as_ref().and_then(|path| sftp.rename(Path::new(path), Path::new(&world)).err()
                .map(|_| format!(" The previous world remains at {path}."))).unwrap_or_default();
            return Err(format!("Could not restore the world: {error}.{recovery}"));
        }
        Ok(preserved.unwrap_or_default())
    }).await
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VelocityManifest {
    public_port: u16,
    backend_port: u16,
    original_properties: String,
    original_paper: String,
    patched_properties: String,
    patched_paper: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VelocityOnDemandStatus {
    enabled: bool,
    public_port: Option<u16>,
    backend_port: Option<u16>,
    proxy_running: bool,
    idle_minutes: u16,
    update_required: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VelocityProxyUsage {
    running: bool,
    cpu_percent: Option<f32>,
    memory_bytes: Option<u64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VelocitySetupProgress {
    profile_id: Uuid,
    step: &'static str,
}

fn velocity_progress(app: &AppHandle, profile_id: Uuid, step: &'static str) {
    let _ = app.emit("velocity-setup-progress", VelocitySetupProgress { profile_id, step });
}

fn velocity_startup_diagnostic(session: &RemoteConnection, proxy_root: &str, proxy_session: &str) -> String {
    let log_path = format!("{proxy_root}/logs/latest.log");
    let command = format!("if [ -f {} ]; then tail -c 6000 -- {}; else tmux capture-pane -p -t {} -S -80 2>/dev/null; fi",
        shell_quote(&log_path), shell_quote(&log_path), shell_quote(proxy_session));
    let Ok(result) = execute(session, &command) else { return String::new(); };
    let output = String::from_utf8_lossy(&result.stdout);
    let lines = output.lines().filter(|line| {
        let lower = line.to_ascii_lowercase();
        !["secret", "password", "token"].iter().any(|word| lower.contains(word))
    }).rev().take(12).collect::<Vec<_>>();
    let message = lines.into_iter().rev().collect::<Vec<_>>().join("\n");
    message.chars().filter(|character| !character.is_control() || *character == '\n').take(1800).collect()
}

fn velocity_root(sftp: &RemoteSftp, profile: &ServerProfile, environment: &ServerEnvironment) -> Result<String, String> {
    let root = remote_root_with_sftp(sftp, &environment.root_directory)?;
    let folder = profile.id.simple().to_string();
    Ok(format!("{root}/runtimes/velocity/{folder}"))
}

fn velocity_read(sftp: &RemoteSftp, path: &str) -> Result<String, String> {
    let metadata = sftp.lstat(Path::new(path)).map_err(|error| format!("Could not read Velocity setup: {error}"))?;
    if metadata.file_type() != FileType::File || metadata.size.is_some_and(|size| size > MAX_FILE_BYTES) {
        return Err("The Velocity setup file is not a regular text file under 1 MB.".into());
    }
    let bytes = sftp.read_prefix(Path::new(path), MAX_FILE_BYTES as usize + 1)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("The Velocity setup file is too large to read.".into());
    }
    String::from_utf8(bytes).map_err(|_| "The Velocity setup file is not UTF-8 text.".into())
}

fn velocity_write(sftp: &RemoteSftp, path: &str, bytes: &[u8]) -> Result<(), String> {
    let temporary = format!("{path}.{}.tmp", Uuid::new_v4().simple());
    sftp.write_file_exclusive(Path::new(&temporary), bytes)?;
    if let Err(error) = sftp.posix_rename(Path::new(&temporary), Path::new(path)) {
        let _ = sftp.unlink(Path::new(&temporary));
        return Err(format!("Could not save Velocity setup: {error}"));
    }
    Ok(())
}

fn remote_listening_ports(session: &RemoteConnection) -> Result<std::collections::HashSet<u16>, String> {
    let sockets = execute(session, "cat /proc/net/tcp /proc/net/tcp6")?;
    if sockets.exit_code != 0 { return Err("Could not inspect remote listening ports.".into()); }
    Ok(String::from_utf8_lossy(&sockets.stdout).lines().filter_map(|line| {
        let mut fields = line.split_whitespace();
        fields.next()?;
        let address = fields.next()?;
        fields.next()?;
        (fields.next()? == "0A").then(|| address.rsplit(':').next())
            .flatten().and_then(|port| u16::from_str_radix(port, 16).ok())
    }).collect())
}

#[tauri::command]
pub async fn velocity_on_demand_status(
    profile_id: Uuid, profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<VelocityOnDemandStatus, String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment.clone(), Arc::clone(remote_sessions.inner()), move |session, profile| {
        let sftp = session.sftp()?;
        let root = velocity_root(&sftp, profile, &environment)?;
        let path = format!("{root}/manifest.json");
        match sftp.lstat_optional(Path::new(&path)) {
            Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => Ok(VelocityOnDemandStatus { enabled: false, public_port: None, backend_port: None, proxy_running: false, idle_minutes: 5, update_required: false }),
            Ok(_) => {
                let manifest: VelocityManifest = serde_json::from_str(&velocity_read(&sftp, &path)?)
                    .map_err(|_| "The saved Velocity setup is invalid.".to_owned())?;
                let proxy_session = format!("scerite-velocity-{}", profile.id.simple());
                let proxy_running = execute(session, &format!("tmux has-session -t {} 2>/dev/null", shell_quote(&proxy_session)))?.exit_code == 0
                    && remote_listening_ports(session)?.contains(&manifest.public_port);
                let plugin_config = velocity_read(&sftp, &format!("{root}/plugins/sceritevelocity/config.properties"))?;
                let idle_minutes = velocity_download::idle_minutes(&plugin_config)?;
                let update_required = !plugin_config.lines().any(|line| line.trim() == "pluginVersion=1.1.1");
                Ok(VelocityOnDemandStatus { enabled: true, public_port: Some(manifest.public_port), backend_port: Some(manifest.backend_port), proxy_running, idle_minutes, update_required })
            }
            Err(error) => Err(format!("Could not check Velocity setup: {error}")),
        }
    }).await
}

#[tauri::command]
pub async fn set_velocity_idle_minutes(
    profile_id: Uuid, idle_minutes: u16, profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<(), String> {
    if !(1..=120).contains(&idle_minutes) { return Err("Choose an idle delay from 1 to 120 minutes.".into()); }
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment.clone(), Arc::clone(remote_sessions.inner()), move |session, profile| {
        let sftp = session.sftp()?;
        let root = velocity_root(&sftp, profile, &environment)?;
        match sftp.lstat_optional(Path::new(&format!("{root}/manifest.json"))) {
            Ok(_) => (),
            Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile =>
                return Err("Enable on-demand startup before changing its idle delay.".into()),
            Err(error) => return Err(format!("Could not check Velocity setup: {error}")),
        }
        let path = format!("{root}/plugins/sceritevelocity/config.properties");
        let config = velocity_read(&sftp, &path)?;
        velocity_write(&sftp, &path, velocity_download::with_idle_minutes(&config, idle_minutes)?.as_bytes())
    }).await
}

#[tauri::command]
pub async fn velocity_proxy_usage(
    profile_id: Uuid, profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<VelocityProxyUsage, String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment.clone(), Arc::clone(remote_sessions.inner()), move |session, profile| {
        let sftp = session.sftp()?;
        let root = velocity_root(&sftp, profile, &environment)?;
        let command = format!(
            "root={}; for process in /proc/[0-9]*; do [ -r \"$process/comm\" ] || continue; [ \"$(cat \"$process/comm\" 2>/dev/null)\" = java ] || continue; cwd=$(readlink -f -- \"$process/cwd\" 2>/dev/null) || continue; [ \"$cwd\" = \"$root\" ] || continue; ps -p \"${{process##*/}}\" -o %cpu=,rss=; exit $?; done; exit 1",
            shell_quote(&root),
        );
        let result = execute(session, &command)?;
        if result.exit_code != 0 {
            return Ok(VelocityProxyUsage { running: false, cpu_percent: None, memory_bytes: None });
        }
        let output = String::from_utf8_lossy(&result.stdout);
        let mut values = output.split_whitespace();
        let cpu_percent = values.next().and_then(|value| value.parse::<f32>().ok());
        let memory_bytes = values.next().and_then(|value| value.parse::<u64>().ok()).map(|value| value * 1024);
        Ok(VelocityProxyUsage { running: true, cpu_percent, memory_bytes })
    }).await
}

#[tauri::command]
pub async fn enable_velocity_on_demand(
    app: AppHandle, profile_id: Uuid, idle_minutes: u16, profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<VelocityOnDemandStatus, String> {
    if !(1..=120).contains(&idle_minutes) { return Err("Choose an idle delay from 1 to 120 minutes.".into()); }
    velocity_progress(&app, profile_id, "Checking the server");
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    if !matches!(profile.installed_software.as_deref(), Some("paper" | "purpur")) {
        return Err("On-demand Velocity start currently supports remote Paper and Purpur servers.".into());
    }
    let sessions = Arc::clone(remote_sessions.inner());
    let preflight_environment = environment.clone();
    run_remote(profile.clone(), environment.clone(), Arc::clone(&sessions), move |session, profile| {
        if remote_is_running(session, profile)? != Some(false) {
            return Err("Stop the server before enabling on-demand startup.".into());
        }
        let root = remote_root(session, configured_remote_root(profile)?)?;
        if remote_directory_has_java_process(session, &root)? {
            return Err("A Java process is still using this server folder. Stop it before enabling on-demand startup.".into());
        }
        let sftp = session.sftp()?;
        let proxy_root = velocity_root(&sftp, profile, &preflight_environment)?;
        match sftp.lstat_optional(Path::new(&format!("{proxy_root}/manifest.json"))) {
            Ok(_) => return Err("On-demand startup is already enabled for this server.".into()),
            Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => {},
            Err(error) => return Err(format!("Could not inspect Velocity setup: {error}")),
        }
        Ok(())
    }).await?;

    velocity_progress(&app, profile_id, "Preparing Java 25");
    let client = crate::vanilla_install::client()?;
    let java = crate::vanilla_install::install_remote_java(&client, &app, &profile, &environment, Arc::clone(&sessions), 25).await?;
    velocity_progress(&app, profile_id, "Downloading Velocity");
    let velocity_jar = velocity_download::stable_velocity_jar(&client).await?;
    let progress_app = app.clone();
    run_remote(profile, environment.clone(), sessions, move |session, profile| {
        velocity_progress(&progress_app, profile.id, "Writing proxy files");
        if remote_is_running(session, profile)? != Some(false) { return Err("Stop the server before enabling on-demand startup.".into()); }
        let sftp = session.sftp()?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        let environment_root = remote_root_with_sftp(&sftp, &environment.root_directory)?;
        let folder = profile.id.simple().to_string();
        let proxy_root = ensure_remote_managed_dirs(session, &sftp, &environment_root, &["runtimes", "velocity", &folder])?;
        let plugin_folder = ensure_remote_managed_dirs(session, &sftp, &proxy_root, &["plugins", "sceritevelocity"])?;
        if [root.as_str(), proxy_root.as_str()].iter().any(|path| path.contains(['\n', '\r'])) {
            return Err("Remote paths with line breaks cannot be used for Velocity setup.".into());
        }
        let properties_path = format!("{root}/server.properties");
        let paper_path = format!("{root}/config/paper-global.yml");
        let original_properties = velocity_read(&sftp, &properties_path)?;
        let original_paper = velocity_read(&sftp, &paper_path)?;
        let spigot = velocity_read(&sftp, &format!("{root}/spigot.yml"))?;
        if spigot.lines().any(|line| line.trim() == "bungeecord: true") {
            return Err("Disable settings.bungeecord in spigot.yml before enabling Velocity forwarding.".into());
        }
        let public_port = server_port::configured_port(Some(&original_properties))?;
        let used = remote_listening_ports(session)?;
        if used.contains(&public_port) { return Err(format!("Port {public_port} is in use. Stop that service before enabling Velocity.")); }
        let backend_port = server_port::available_port(public_port, |port| Ok(port == public_port || used.contains(&port)))?
            .ok_or("Could not choose a backend port.")?;
        let secret = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let patched_properties = velocity_download::backend_properties(&original_properties, backend_port);
        let patched_paper = velocity_download::paper_forwarding(&original_paper, &secret)?;
        let manifest = VelocityManifest { public_port, backend_port, original_properties, original_paper, patched_properties, patched_paper };
        let session_id = session_name(profile.id);
        let proxy_session = format!("scerite-velocity-{}", profile.id.simple());
        let ServerTarget::Remote { launch, .. } = &profile.target else { unreachable!() };
        let managed_java = if let Some(java) = launch.managed_java.as_deref() {
            let actual = sftp.realpath(Path::new(java))?;
            if !Path::new(&actual).starts_with(Path::new(&environment_root).join("runtimes/java")) {
                return Err("The managed Java executable is outside the configured runtime folder.".into());
            }
            if execute(session, &format!("test -x {}", shell_quote(&actual)))?.exit_code != 0 {
                return Err("The managed Java executable is missing or is not executable.".into());
            }
            Some(actual)
        } else { None };
        let launch_command = launch_command(launch, managed_java.as_deref());
        let start = format!("#!/bin/sh\nif tmux has-session -t {} 2>/dev/null; then\n  if [ \"$(tmux display-message -p -t {} '#{{pane_dead}}')\" = 0 ]; then exit 0; fi\n  tmux kill-session -t {}\nfi\ntmux new-session -d -s {} -c {} {}\ntmux set-window-option -t {} remain-on-exit on\n",
            shell_quote(&session_id), shell_quote(&session_id), shell_quote(&session_id), shell_quote(&session_id), shell_quote(&root), shell_quote(&launch_command), shell_quote(&session_id));
        let stop = format!("#!/bin/sh\ntmux send-keys -t {} -l stop\ntmux send-keys -t {} Enter\n", shell_quote(&session_id), shell_quote(&session_id));
        let plugin_config = format!("pluginVersion=1.1.1\nserverName=backend\nbackendPort={backend_port}\nidleSeconds={}\nserverDirectory={root}\nstartScript={proxy_root}/start.sh\nstopScript={proxy_root}/stop.sh\n", u32::from(idle_minutes) * 60);
        let proxy_config = velocity_download::proxy_config(public_port, backend_port);
        velocity_write(&sftp, &format!("{proxy_root}/velocity.jar"), &velocity_jar)?;
        velocity_write(&sftp, &format!("{proxy_root}/plugins/scerite-velocity.jar"), SCERITE_VELOCITY_JAR)?;
        velocity_write(&sftp, &format!("{plugin_folder}/config.properties"), plugin_config.as_bytes())?;
        velocity_write(&sftp, &format!("{proxy_root}/start.sh"), start.as_bytes())?;
        velocity_write(&sftp, &format!("{proxy_root}/stop.sh"), stop.as_bytes())?;
        velocity_write(&sftp, &format!("{proxy_root}/velocity.toml"), proxy_config.as_bytes())?;
        velocity_write(&sftp, &format!("{proxy_root}/forwarding.secret"), secret.as_bytes())?;
        let chmod = execute(session, &format!("chmod 600 -- {}", shell_quote(&format!("{proxy_root}/forwarding.secret"))))?;
        if chmod.exit_code != 0 { return Err(remote_command_error("Could not protect Velocity's forwarding secret", chmod)); }
        velocity_write(&sftp, &properties_path, manifest.patched_properties.as_bytes())?;
        if let Err(error) = velocity_write(&sftp, &paper_path, manifest.patched_paper.as_bytes()) {
            let _ = velocity_write(&sftp, &properties_path, manifest.original_properties.as_bytes());
            return Err(error);
        }
        let start_result = (|| {
            velocity_progress(&progress_app, profile.id, "Starting Velocity");
            let start_proxy = execute(session, &format!("tmux new-session -d -s {} -c {}",
                shell_quote(&proxy_session), shell_quote(&proxy_root)))?;
            if start_proxy.exit_code != 0 { return Err(remote_command_error("Could not start Velocity", start_proxy)); }
            let remain = execute(session, &format!("tmux set-window-option -t {} remain-on-exit on", shell_quote(&proxy_session)))?;
            if remain.exit_code != 0 { return Err(remote_command_error("Could not keep the Velocity console open", remain)); }
            let launch = execute(session, &format!("tmux send-keys -t {} -l {} && tmux send-keys -t {} Enter",
                shell_quote(&proxy_session), shell_quote(&format!("exec {} -jar velocity.jar", shell_quote(&java))), shell_quote(&proxy_session)))?;
            if launch.exit_code != 0 { return Err(remote_command_error("Could not launch Velocity", launch)); }
            velocity_progress(&progress_app, profile.id, "Waiting for the proxy and plugin");
            let mut port_open = false;
            for _ in 0..30 {
                let plugin_log = execute(session, &format!("tmux capture-pane -p -t {} -S -200 2>/dev/null | grep -F 'Scerite on-demand backend ready'",
                    shell_quote(&proxy_session)))?;
                port_open = remote_listening_ports(session)?.contains(&public_port);
                if plugin_log.exit_code == 0 && port_open {
                    return velocity_write(&sftp, &format!("{proxy_root}/manifest.json"),
                        serde_json::to_string(&manifest).map_err(|error| error.to_string())?.as_bytes());
                }
                let pane = execute(session, &format!("tmux display-message -p -t {} '#{{pane_dead}}' 2>/dev/null", shell_quote(&proxy_session)))?;
                if String::from_utf8_lossy(&pane.stdout).trim() == "1" { break; }
                std::thread::sleep(Duration::from_secs(1));
            }
            let reason = if port_open { "Velocity opened its port, but the Scerite plugin did not initialize." }
                else { "Velocity did not open its public port." };
            let diagnostic = velocity_startup_diagnostic(session, &proxy_root, &proxy_session);
            if diagnostic.is_empty() { Err(reason.into()) } else { Err(format!("{reason}\n{diagnostic}")) }
        })();
        if let Err(error) = start_result {
            velocity_progress(&progress_app, profile.id, "Restoring server settings");
            let _ = execute(session, &format!("tmux kill-session -t {} 2>/dev/null", shell_quote(&proxy_session)));
            let properties_restore = velocity_write(&sftp, &properties_path, manifest.original_properties.as_bytes());
            let paper_restore = velocity_write(&sftp, &paper_path, manifest.original_paper.as_bytes());
            if properties_restore.is_err() || paper_restore.is_err() {
                return Err(format!("{error} Could not restore all server settings; check server.properties and config/paper-global.yml before starting the server."));
            }
            return Err(error);
        }
        velocity_progress(&progress_app, profile.id, "Ready");
        Ok(VelocityOnDemandStatus { enabled: true, public_port: Some(public_port), backend_port: Some(backend_port), proxy_running: true, idle_minutes, update_required: false })
    }).await
}

#[tauri::command]
pub async fn disable_velocity_on_demand(
    profile_id: Uuid, profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<VelocityOnDemandStatus, String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment.clone(), Arc::clone(remote_sessions.inner()), move |session, profile| {
        if remote_is_running(session, profile)? != Some(false) {
            return Err("Stop the server before disabling on-demand startup.".into());
        }
        let sftp = session.sftp()?;
        let proxy_root = velocity_root(&sftp, profile, &environment)?;
        let manifest_path = format!("{proxy_root}/manifest.json");
        let manifest: VelocityManifest = serde_json::from_str(&velocity_read(&sftp, &manifest_path)?)
            .map_err(|_| "The saved Velocity setup is invalid.".to_owned())?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        let properties_path = format!("{root}/server.properties");
        let paper_path = format!("{root}/config/paper-global.yml");
        let current_properties = velocity_read(&sftp, &properties_path)?;
        let current_paper = velocity_read(&sftp, &paper_path)?;
        let restored_properties = velocity_download::restore_backend_properties(
            &current_properties, &manifest.original_properties, &manifest.patched_properties)?;
        let restored_paper = velocity_download::restore_paper_forwarding(
            &current_paper, &manifest.original_paper, &manifest.patched_paper)?;
        let proxy_session = format!("scerite-velocity-{}", profile.id.simple());
        let active = execute(session, &format!("tmux has-session -t {} 2>/dev/null", shell_quote(&proxy_session)))?;
        if active.exit_code == 0 {
            let stop = execute(session, &format!("tmux send-keys -t {} -l end && tmux send-keys -t {} Enter",
                shell_quote(&proxy_session), shell_quote(&proxy_session)))?;
            if stop.exit_code != 0 { return Err(remote_command_error("Could not stop Velocity", stop)); }
            for _ in 0..15 {
                if !remote_listening_ports(session)?.contains(&manifest.public_port) { break; }
                std::thread::sleep(Duration::from_secs(1));
            }
            if remote_listening_ports(session)?.contains(&manifest.public_port) {
                return Err("Velocity did not stop. Try again after checking its console.".into());
            }
        }
        if remote_listening_ports(session)?.contains(&manifest.public_port) {
            return Err("The original Minecraft port is still in use. Stop that service before disabling Velocity.".into());
        }
        velocity_write(&sftp, &properties_path, restored_properties.as_bytes())?;
        if let Err(error) = velocity_write(&sftp, &paper_path, restored_paper.as_bytes()) {
            let _ = velocity_write(&sftp, &properties_path, current_properties.as_bytes());
            return Err(error);
        }
        sftp.unlink(Path::new(&manifest_path)).map_err(|error| format!("Could not finish disabling Velocity: {error}"))?;
        Ok(VelocityOnDemandStatus { enabled: false, public_port: None, backend_port: None, proxy_running: false, idle_minutes: 5, update_required: false })
    }).await
}

#[tauri::command]
pub async fn regenerate_remote_world(
    profile_id: Uuid, world_name: String, seed: String, level_type: String, generate_structures: bool,
    profiles: State<'_, ServerProfileStore>, remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<String, String> {
    let (profile, environment) = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, environment, Arc::clone(remote_sessions.inner()), move |session, profile| {
        world_management::generation_values(&world_name, &seed, &level_type)?;
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let root = remote_root_with_sftp(&sftp, configured_remote_root(profile)?)?;
        require_remote_world_stopped(session, profile, &root)?;
        let world = checked_remote_world(&sftp, &root, &world_name)?;
        let mut worlds = vec![(world_name.clone(), world)];
        for suffix in ["_nether", "_the_end"] {
            let name = format!("{world_name}{suffix}");
            let path = resolve_remote_relative(&root, &name, false)?;
            match sftp.lstat_optional(Path::new(&path)) {
                Ok(metadata) if metadata.file_type() == FileType::Dir => {
                    reject_remote_symlink_path(&sftp, &root, &path)?;
                    worlds.push((name, canonical_inside(&sftp, &root, &path)?));
                }
                Ok(_) => return Err(format!("The companion world path {name} is not a regular folder.")),
                Err(SftpError::Status(status)) if status.status_code == StatusCode::NoSuchFile => {}
                Err(error) => return Err(format!("Could not inspect {name}: {error}")),
            }
        }
        let properties_path = format!("{root}/server.properties");
        if sftp.lstat(Path::new(&properties_path))?.file_type() != FileType::File {
            return Err("server.properties is missing or is not a regular file.".into());
        }
        reject_remote_symlink_path(&sftp, &root, &properties_path)?;
        let properties_path = canonical_inside(&sftp, &root, &properties_path)?;
        let content = sftp.read_file(Path::new(&properties_path))?;
        if content.len() as u64 > MAX_FILE_BYTES {
            return Err("server.properties is too large to edit here.".into());
        }
        let content = std::str::from_utf8(&content).map_err(|error| error.to_string())?;
        if server_players::world_name(Some(content.as_bytes()))? != world_name {
            return Err("Only the active world can be generated again.".into());
        }
        let updated = world_management::updated_properties(content, &world_name, &seed, &level_type, generate_structures)?;
        let mut moved = Vec::new();
        for (name, path) in worlds {
            let preserved = match remote_preserved_world(&sftp, &root, &name) {
                Ok(path) => path,
                Err(error) => {
                    let recovery = restore_remote_worlds(&sftp, &moved);
                    return Err(format!("{error}{recovery}"));
                }
            };
            if let Err(error) = sftp.rename(Path::new(&path), Path::new(&preserved)) {
                let recovery = restore_remote_worlds(&sftp, &moved);
                return Err(format!("Could not preserve {name}: {error}.{recovery}"));
            }
            moved.push((path, preserved));
        }
        let temporary = format!("{root}/.scerite-properties-{}.tmp", Uuid::new_v4());
        let saved = sftp.write_file_exclusive(Path::new(&temporary), updated.as_bytes())
            .and_then(|_| sftp.posix_rename(Path::new(&temporary), Path::new(&properties_path)));
        if let Err(error) = saved {
            let _ = sftp.unlink(Path::new(&temporary));
            let recovery = restore_remote_worlds(&sftp, &moved);
            return Err(format!("Could not save world generation settings: {error}.{recovery}"));
        }
        Ok(moved[0].1.clone())
    }).await
}
