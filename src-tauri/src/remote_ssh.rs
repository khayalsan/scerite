use std::{
    collections::HashMap,
    env,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
    time::Instant,
};

use serde::Serialize;
use ssh2::{
    CheckResult, ErrorCode, FileType, KnownHostFileKind, MethodType, OpenFlags, OpenType,
    RenameFlags, Session,
};
use tauri::State;
use uuid::Uuid;

use crate::server_profiles::{LocalLaunchConfig, ServerProfile, ServerProfileStore, ServerTarget};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_CONSOLE_BYTES: u64 = 512 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 1000;
const CONNECTION_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const SFTP_END_OF_DIRECTORY: ErrorCode = ErrorCode::Session(-16);

#[derive(Default)]
pub struct RemoteSessionManager {
    sessions: Mutex<HashMap<Uuid, CachedConnection>>,
}

struct CachedConnection {
    config: RemoteConnectionConfig,
    session: Session,
    last_used: Instant,
}

#[derive(Clone, PartialEq, Eq)]
struct RemoteConnectionConfig {
    host: String,
    port: u16,
    username: String,
    identity_file: Option<String>,
}

impl RemoteSessionManager {
    fn session(&self, profile: &ServerProfile) -> Result<Session, String> {
        let config = remote_connection_config(profile)?;
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "Remote session manager is unavailable.".to_owned())?;
        sessions.retain(|_, connection| connection.last_used.elapsed() < CONNECTION_IDLE_TIMEOUT);
        if let Some(connection) = sessions.get_mut(&profile.id) {
            if connection.config == config {
                connection.last_used = Instant::now();
                return Ok(connection.session.clone());
            }
            sessions.remove(&profile.id);
        }
        drop(sessions);

        let session = connect_remote(
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
                session: session.clone(),
                last_used: Instant::now(),
            },
        );
        Ok(session)
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
    let profile = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, Arc::clone(remote_sessions.inner()), |_, _| Ok(())).await
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
        connect_remote(&host, port, &username, identity_file.as_deref()).map(|_| ())
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
    let profile = require_remote_profile(&profiles, profile_id)?;
    run_remote(
        profile,
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
    let profile = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, Arc::clone(remote_sessions.inner()), |session, profile| {
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
            return Err("A Java process is already using the remote server folder outside ServerCrafter. Stop it there before starting this profile.".into());
        }
        let command = launch_command(launch);
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
    let profile = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, Arc::clone(remote_sessions.inner()), |session, profile| {
        let running = remote_is_running(session, profile)?;
        if running != Some(true) {
            if let Some(root) = optional_remote_root(session, profile)? {
                if remote_directory_has_java_process(session, &root)? {
                    return Err("The server is running outside ServerCrafter's tmux session. Stop it using its current manager first.".into());
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
    let profile = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, Arc::clone(remote_sessions.inner()), |session, profile| {
        if remote_is_running(session, profile)? != Some(true) {
            if let Some(root) = optional_remote_root(session, profile)? {
                if remote_directory_has_java_process(session, &root)? {
                    return Err("The server is running outside ServerCrafter's tmux session; its console is not attached.".into());
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
    let profile = require_remote_profile(&profiles, profile_id)?;
    if input.len() > 4096
        || input
            .chars()
            .any(|character| matches!(character, '\0' | '\r' | '\n'))
    {
        return Err("Console commands must be one line and 4 KB or less.".into());
    }
    run_remote(profile, Arc::clone(remote_sessions.inner()), move |session, profile| {
        if remote_is_running(session, profile)? != Some(true) {
            if let Some(root) = optional_remote_root(session, profile)? {
                if remote_directory_has_java_process(session, &root)? {
                    return Err("The server is running outside ServerCrafter's tmux session; its console is not attached.".into());
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
    let profile = require_remote_profile(&profiles, profile_id)?;
    run_remote(profile, Arc::clone(remote_sessions.inner()), move |session, profile| {
        let root = configured_remote_root(profile)?;
        let sftp = session.sftp().map_err(|error| format!("Could not open SFTP: {error}"))?;
        let canonical_root = remote_root_with_sftp(&sftp, root)?;
        let requested = resolve_remote_relative(&canonical_root, &relative_path, true)?;
        reject_remote_symlink_path(&sftp, &canonical_root, &requested)?;
        let directory = canonical_inside(&sftp, &canonical_root, &requested)?;
        let directory_stat = sftp.stat(Path::new(&directory)).map_err(|error| format!("Could not read remote folder: {error}"))?;
        if directory_stat.file_type() != FileType::Directory {
            return Err("The remote path is not a folder.".into());
        }

        let prefix = relative_path.trim_matches('/');
        let mut entries = Vec::new();
        let mut directory_handle = sftp.opendir(Path::new(&directory)).map_err(|error| format!("Could not list remote files: {error}"))?;
        loop {
            let (path, stat) = match directory_handle.readdir() {
                Ok(entry) => entry,
                Err(error) if error.code() == SFTP_END_OF_DIRECTORY => break,
                Err(error) => return Err(format!("Could not list remote files: {error}")),
            };
            let Some(name) = path.to_str() else { continue };
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
                directory: kind == FileType::Directory,
                size: stat.size.unwrap_or(0),
            });
        }
        entries.sort_by(|left, right| right.directory.cmp(&left.directory).then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase())));
        Ok(entries)
    })
    .await
}

#[tauri::command]
pub async fn read_remote_server_file(
    profile_id: Uuid,
    relative_path: String,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<String, String> {
    let profile = require_remote_profile(&profiles, profile_id)?;
    run_remote(
        profile,
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
            if metadata.file_type() != FileType::RegularFile {
                return Err("Only regular remote files can be opened.".into());
            }
            if metadata.size.unwrap_or(0) > MAX_FILE_BYTES {
                return Err("Files larger than 1 MB cannot be edited here.".into());
            }
            let file = sftp
                .open(Path::new(&canonical))
                .map_err(|error| format!("Could not open remote file: {error}"))?;
            let mut bytes = Vec::new();
            file.take(MAX_FILE_BYTES + 1)
                .read_to_end(&mut bytes)
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
pub async fn write_remote_server_file(
    profile_id: Uuid,
    relative_path: String,
    content: String,
    overwrite: bool,
    profiles: State<'_, ServerProfileStore>,
    remote_sessions: State<'_, Arc<RemoteSessionManager>>,
) -> Result<(), String> {
    let profile = require_remote_profile(&profiles, profile_id)?;
    if content.len() as u64 > MAX_FILE_BYTES {
        return Err("Files larger than 1 MB cannot be edited here.".into());
    }
    run_remote(profile, Arc::clone(remote_sessions.inner()), move |session, profile| {
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
            if metadata.file_type() != FileType::RegularFile {
                return Err("Only regular remote files can be overwritten.".into());
            }
            if !overwrite {
                return Err("The file already exists. Confirm overwrite before saving.".into());
            }
            canonical_inside(&sftp, &root, &destination)?;
        }

        let temporary = format!("{}/.servercrafter-{}.tmp", canonical_parent.trim_end_matches('/'), Uuid::new_v4());
        let mut file = sftp.open_mode(
            Path::new(&temporary),
            OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::EXCLUSIVE,
            0o600,
            OpenType::File,
        ).map_err(|error| format!("Could not create a temporary remote file: {error}"))?;
        if let Err(error) = file.write_all(content.as_bytes()) {
            drop(file);
            let _ = sftp.unlink(Path::new(&temporary));
            return Err(format!("Could not write remote file: {error}"));
        }
        drop(file);
        let flags = if overwrite {
            RenameFlags::ATOMIC | RenameFlags::OVERWRITE | RenameFlags::NATIVE
        } else {
            RenameFlags::ATOMIC | RenameFlags::NATIVE
        };
        if let Err(error) = sftp.rename(Path::new(&temporary), Path::new(&destination), Some(flags)) {
            let _ = sftp.unlink(Path::new(&temporary));
            return Err(format!("Could not save remote file: {error}"));
        }
        Ok(())
    })
    .await
}

async fn run_remote<T, F>(
    profile: ServerProfile,
    sessions: Arc<RemoteSessionManager>,
    operation: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&Session, &ServerProfile) -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let session = sessions.session(&profile)?;
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
) -> Result<ServerProfile, String> {
    let profile = profiles
        .list()?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| "Server profile was not found.".to_owned())?;
    if !matches!(profile.target, ServerTarget::Remote { .. }) {
        return Err("This operation is only available for remote profiles.".into());
    }
    Ok(profile)
}

fn remote_connection_config(profile: &ServerProfile) -> Result<RemoteConnectionConfig, String> {
    let ServerTarget::Remote {
        host,
        port,
        username,
        identity_file,
        ..
    } = &profile.target
    else {
        return Err("This operation is only available for remote profiles.".into());
    };
    Ok(RemoteConnectionConfig {
        host: host.clone(),
        port: *port,
        username: username.clone(),
        identity_file: identity_file.clone(),
    })
}

fn connect_remote(
    host: &str,
    port: u16,
    username: &str,
    identity_file: Option<&str>,
) -> Result<Session, String> {
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|error| format!("Could not resolve SSH host: {error}"))?;
    let mut last_error = None;
    let mut stream = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
            Ok(connected) => {
                stream = Some(connected);
                break;
            }
            Err(error) => last_error = Some(error),
        }
    }
    let stream = stream.ok_or_else(|| {
        format!(
            "Could not connect to the SSH host: {}",
            last_error.map_or_else(
                || "no addresses were returned".into(),
                |error| error.to_string()
            )
        )
    })?;
    stream
        .set_read_timeout(Some(CONNECT_TIMEOUT))
        .map_err(|error| format!("Could not configure the SSH connection: {error}"))?;
    stream
        .set_write_timeout(Some(CONNECT_TIMEOUT))
        .map_err(|error| format!("Could not configure the SSH connection: {error}"))?;

    let mut session =
        Session::new().map_err(|error| format!("Could not create SSH session: {error}"))?;
    let usable_host_keys = session
        .supported_algs(MethodType::HostKey)
        .map_err(|error| format!("Could not inspect supported SSH host keys: {error}"))?
        .into_iter()
        .filter(|algorithm| !algorithm.contains("-cert-v01@openssh.com"))
        .collect::<Vec<_>>();
    if usable_host_keys.is_empty() {
        return Err("This SSH build has no supported host-key algorithms.".into());
    }
    session
        .method_pref(MethodType::HostKey, &usable_host_keys.join(","))
        .map_err(|error| format!("Could not configure SSH host-key algorithms: {error}"))?;
    session.set_tcp_stream(stream);
    session.set_timeout(CONNECT_TIMEOUT.as_millis() as u32);
    session
        .handshake()
        .map_err(|error| format!("SSH handshake failed during encryption/key negotiation: {error}. Check that the server offers a standard SSH host key and a modern key-exchange and cipher algorithm."))?;
    verify_host_key(&session, host, port)?;
    authenticate(&session, username, identity_file)?;
    session.set_keepalive(true, 15);
    Ok(session)
}

fn verify_host_key(session: &Session, host: &str, port: u16) -> Result<(), String> {
    let (key, _) = session
        .host_key()
        .ok_or_else(|| "The SSH server did not provide a host key.".to_owned())?;
    let path = known_hosts_path()?;
    let mut known_hosts = session
        .known_hosts()
        .map_err(|error| format!("Could not read trusted SSH hosts: {error}"))?;
    known_hosts.read_file(&path, KnownHostFileKind::OpenSSH).map_err(|_| {
        format!("No trusted SSH host keys were found in {}. Verify the server fingerprint and add it to known_hosts before connecting.", path.display())
    })?;
    match known_hosts.check_port(host, port, key) {
        CheckResult::Match => Ok(()),
        CheckResult::Mismatch => Err("The SSH host key changed. Connection refused; verify the new fingerprint before updating known_hosts.".into()),
        CheckResult::NotFound => Err(format!("The SSH host key for {host}:{port} is not trusted. Verify its fingerprint and add it to {}.", path.display())),
        CheckResult::Failure => Err("Could not check the SSH host key against known_hosts.".into()),
    }
}

fn known_hosts_path() -> Result<PathBuf, String> {
    let home = env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .ok_or_else(|| {
            "Could not locate the current user's home directory for SSH known_hosts.".to_owned()
        })?;
    Ok(PathBuf::from(home).join(".ssh").join("known_hosts"))
}

fn authenticate(
    session: &Session,
    username: &str,
    identity_file: Option<&str>,
) -> Result<(), String> {
    if let Some(identity_file) = identity_file {
        if session
            .userauth_pubkey_file(username, None, Path::new(identity_file), None)
            .is_ok()
        {
            return Ok(());
        }
    }

    if let Ok(mut agent) = session.agent() {
        if agent.connect().is_ok() && agent.list_identities().is_ok() {
            if let Ok(identities) = agent.identities() {
                for identity in identities {
                    if agent.userauth(username, &identity).is_ok() {
                        return Ok(());
                    }
                }
            }
        }
    }

    if identity_file.is_none() {
        for identity in default_identity_paths() {
            if session
                .userauth_pubkey_file(username, None, &identity, None)
                .is_ok()
            {
                return Ok(());
            }
        }
    }

    Err("SSH authentication failed. Check the selected key or load the key into your SSH agent; encrypted private keys must be unlocked by the agent.".into())
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

fn remote_is_running(session: &Session, profile: &ServerProfile) -> Result<Option<bool>, String> {
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

fn remote_directory_has_java_process(session: &Session, root: &str) -> Result<bool, String> {
    let command = format!(
        "root={}; [ -d /proc ] || exit 2; command -v readlink >/dev/null 2>&1 || exit 127; for process in /proc/[0-9]*; do [ -r \"$process/comm\" ] || continue; [ \"$(cat \"$process/comm\" 2>/dev/null)\" = java ] || continue; cwd=$(readlink -f -- \"$process/cwd\" 2>/dev/null) || continue; case \"$cwd\" in \"$root\"|\"$root\"/*) printf '%s\\n' \"$process\"; exit 0;; esac; done; exit 1",
        shell_quote(root),
    );
    let result = execute(session, &command)?;
    match result.exit_code {
        0 => Ok(true),
        1 => Ok(false),
        2 => Err("Remote process information is unavailable, so ServerCrafter cannot confirm the server is stopped.".into()),
        127 => Err("The remote Linux server needs readlink to verify whether a Java server process is using this folder.".into()),
        _ => Err(remote_command_error("Could not check remote server processes", result)),
    }
}

fn execute(session: &Session, command: &str) -> Result<RemoteCommandOutput, String> {
    let mut channel = session
        .channel_session()
        .map_err(|error| format!("Could not open an SSH channel: {error}"))?;
    channel
        .exec(command)
        .map_err(|error| format!("Could not run the remote command: {error}"))?;
    let stdout = read_channel_limited(&mut channel)?;
    let stderr = read_channel_limited(&mut channel.stderr())?;
    channel
        .wait_close()
        .map_err(|error| format!("Could not finish the remote command: {error}"))?;
    let exit_code = channel
        .exit_status()
        .map_err(|error| format!("Could not read remote command status: {error}"))?;
    Ok(RemoteCommandOutput {
        stdout,
        stderr,
        exit_code,
    })
}

fn read_channel_limited(reader: &mut impl Read) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    reader
        .take(MAX_CONSOLE_BYTES + 1)
        .read_to_end(&mut output)
        .map_err(|error| format!("Could not read remote command output: {error}"))?;
    if output.len() as u64 > MAX_CONSOLE_BYTES {
        return Err("Remote command output exceeded the 512 KB safety limit.".into());
    }
    Ok(output)
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

fn launch_command(launch: &LocalLaunchConfig) -> String {
    let command = std::iter::once(launch.executable.trim())
        .chain(launch.arguments.iter().map(String::as_str))
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ");
    format!("exec {command}")
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn session_name(profile_id: Uuid) -> String {
    format!("servercrafter-{}", profile_id.simple())
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
    session: &Session,
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

fn remote_root(session: &Session, configured: &str) -> Result<String, String> {
    let sftp = session
        .sftp()
        .map_err(|error| format!("Could not open SFTP: {error}"))?;
    remote_root_with_sftp(&sftp, configured)
}

fn remote_root_with_sftp(sftp: &ssh2::Sftp, configured: &str) -> Result<String, String> {
    let canonical = sftp
        .realpath(Path::new(configured))
        .map_err(|error| format!("Could not open the remote server folder: {error}"))?;
    let root = remote_path(&canonical);
    if !root.starts_with('/') {
        return Err("The remote server folder must resolve to an absolute path.".into());
    }
    if root == "/" {
        return Err("Choose a remote server folder narrower than the filesystem root.".into());
    }
    let metadata = sftp
        .stat(Path::new(&root))
        .map_err(|error| format!("Could not read remote server folder: {error}"))?;
    if metadata.file_type() != FileType::Directory {
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

fn canonical_inside(sftp: &ssh2::Sftp, root: &str, candidate: &str) -> Result<String, String> {
    let canonical = sftp
        .realpath(Path::new(candidate))
        .map_err(|error| format!("Could not resolve the remote path: {error}"))?;
    let canonical = remote_path(&canonical);
    let root_prefix = format!("{}/", root.trim_end_matches('/'));
    if canonical != root && !canonical.starts_with(&root_prefix) {
        return Err("The remote path resolves outside the configured server folder.".into());
    }
    Ok(canonical)
}

fn reject_remote_symlink_path(
    sftp: &ssh2::Sftp,
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
