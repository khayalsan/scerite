use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
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
use serde::Serialize;
use tauri::State;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    runtime::{Builder, Runtime},
};
use uuid::Uuid;

use crate::eula;
use crate::server_profiles::{
    LocalLaunchConfig, ServerEnvironment, ServerProfile, ServerProfileStore, ServerTarget,
};
use crate::java_runtime::JavaPackage;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_CONSOLE_BYTES: u64 = 512 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 1000;
const CONNECTION_IDLE_TIMEOUT: Duration = Duration::from_secs(30);

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
        let tmux_command = format!(
            "tmux new-session -d -s {} -c {} {}",
            shell_quote(&session_name(profile.id)),
            shell_quote(&root),
            shell_quote(&command),
        );
        let result = execute(session, &tmux_command)?;
        if result.exit_code != 0 {
            return Err(remote_command_error("Could not start the remote server", result));
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
        if remote_is_running(session, profile)? != Some(true) {
            if let Some(root) = optional_remote_root(session, profile)? {
                if remote_directory_has_java_process(session, &root)? {
                    return Err("The server is running outside scerite's tmux session; its console is not attached.".into());
                }
            }
            return Ok(RemoteConsoleSnapshot { lines: Vec::new() });
        }
        let command = format!(
            "tmux capture-pane -p -S -1000 -t {}",
            shell_quote(&session_name(profile.id))
        );
        let result = execute(session, &command)?;
        if result.exit_code != 0 {
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
            inactivity_timeout: Some(CONNECTION_IDLE_TIMEOUT),
            ..Default::default()
        });
        match tokio::time::timeout(CONNECT_TIMEOUT, client::connect(config, address, handler)).await
        {
            Ok(Ok(mut handle)) => {
                if authenticate(&mut handle, username, identity_file).await? {
                    let _ = handle.send_keepalive(false).await;
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
        "tmux has-session -t {}",
        shell_quote(&session_name(profile.id))
    );
    let result = execute(session, &command)?;
    if result.exit_code == 0 {
        return Ok(Some(true));
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
