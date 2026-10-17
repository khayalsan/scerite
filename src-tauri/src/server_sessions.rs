use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}},
    thread,
    time::Duration,
};

use interprocess::local_socket::{
    ConnectOptions, GenericFilePath, GenericNamespaced, ListenerOptions,
    prelude::*,
};
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::server_profiles::{ServerProfileStore, ServerTarget};

const MANAGER_ARGUMENT: &str = "--servercrafter-session-manager";
const TOKEN_FILE: &str = "session-manager.token";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalServerStatus {
    pub state: LocalServerState,
    pub process_id: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalServerState {
    Stopped,
    Running,
    Stopping,
}

pub struct LocalSessionManager {
    app_data_dir: PathBuf,
    started: AtomicBool,
}

impl LocalSessionManager {
    pub fn new(app_data_dir: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&app_data_dir)
            .map_err(|error| format!("Could not prepare local session storage: {error}"))?;
        load_or_create_token(&app_data_dir)?;
        Ok(Self {
            app_data_dir,
            started: AtomicBool::new(false),
        })
    }

    fn request(&self, request: ManagerRequest) -> Result<ManagerResponse, String> {
        if !self.started.load(Ordering::Acquire) {
            self.start_helper()?;
        }

        match self.request_once(request.clone()) {
            Ok(response) => Ok(response),
            Err(_) => {
                self.started.store(false, Ordering::Release);
                self.start_helper()?;
                self.request_once(request)
            }
        }
    }

    fn start_helper(&self) -> Result<(), String> {
        let executable = std::env::current_exe()
            .map_err(|error| format!("Could not locate ServerCrafter: {error}"))?;
        let mut command = Command::new(executable);
        command
            .arg(MANAGER_ARGUMENT)
            .arg(&self.app_data_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        detach_helper(&mut command);
        command
            .spawn()
            .map_err(|error| format!("Could not start the local session manager: {error}"))?;

        for _ in 0..60 {
            if self.request_once(ManagerRequest::Ping).is_ok() {
                self.started.store(true, Ordering::Release);
                return Ok(());
            }
            thread::sleep(Duration::from_millis(50));
        }

        Err("The local session manager did not become available.".into())
    }

    fn request_once(&self, request: ManagerRequest) -> Result<ManagerResponse, String> {
        let token = load_or_create_token(&self.app_data_dir)?;
        let name = socket_name(&self.app_data_dir, &token)?;
        let mut stream = ConnectOptions::new()
            .name(name)
            .connect_sync()
            .map_err(|error| format!("Could not connect to the local session manager: {error}"))?;

        serde_json::to_writer(
            &mut stream,
            &ManagerEnvelope { token, request },
        )
        .map_err(|error| format!("Could not prepare the session request: {error}"))?;
        stream
            .write_all(b"\n")
            .and_then(|()| stream.flush())
            .map_err(|error| format!("Could not send the session request: {error}"))?;

        let mut response = String::new();
        BufReader::new(stream)
            .read_line(&mut response)
            .map_err(|error| format!("Could not read the session response: {error}"))?;
        let response: ManagerResponse = serde_json::from_str(&response)
            .map_err(|error| format!("The session manager returned an invalid response: {error}"))?;
        if let Some(error) = response.error.as_ref() {
            return Err(error.clone());
        }
        Ok(response)
    }

    pub fn status(&self, profile_id: Uuid) -> Result<LocalServerStatus, String> {
        self.request(ManagerRequest::Status { profile_id })?
            .status
            .ok_or_else(|| "The session manager returned no server status.".to_owned())
    }

    pub fn start(
        &self,
        profile_id: Uuid,
        directory: String,
        executable: String,
        arguments: Vec<String>,
    ) -> Result<LocalServerStatus, String> {
        self.request(ManagerRequest::Start {
            profile_id,
            directory,
            executable,
            arguments,
        })?
        .status
        .ok_or_else(|| "The session manager returned no server status.".to_owned())
    }

    pub fn stop(&self, profile_id: Uuid) -> Result<LocalServerStatus, String> {
        self.request(ManagerRequest::Stop { profile_id })?
            .status
            .ok_or_else(|| "The session manager returned no server status.".to_owned())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum ManagerRequest {
    Ping,
    Status {
        profile_id: Uuid,
    },
    Start {
        profile_id: Uuid,
        directory: String,
        executable: String,
        arguments: Vec<String>,
    },
    Stop {
        profile_id: Uuid,
    },
}

#[derive(Serialize, Deserialize)]
struct ManagerEnvelope {
    token: String,
    request: ManagerRequest,
}

#[derive(Serialize, Deserialize)]
struct ManagerResponse {
    status: Option<LocalServerStatus>,
    error: Option<String>,
}

impl ManagerResponse {
    fn status(status: LocalServerStatus) -> Self {
        Self {
            status: Some(status),
            error: None,
        }
    }

    fn error(error: String) -> Self {
        Self {
            status: None,
            error: Some(error),
        }
    }
}

struct ManagedSession {
    child: Child,
    stdin: ChildStdin,
    stopping: bool,
}

pub fn run_session_manager(app_data_dir: PathBuf) -> Result<(), String> {
    let token = load_or_create_token(&app_data_dir)?;
    let name = socket_name(&app_data_dir, &token)?;
    let listener = ListenerOptions::new()
        .name(name)
        .create_sync()
        .map_err(|error| format!("Could not listen for local session requests: {error}"))?;
    let sessions = Arc::new(Mutex::new(HashMap::<Uuid, ManagedSession>::new()));

    for connection in listener.incoming() {
        let Ok(stream) = connection else {
            continue;
        };
        let token = token.clone();
        let sessions = Arc::clone(&sessions);
        thread::spawn(move || handle_connection(stream, &token, sessions));
    }

    Ok(())
}

fn handle_connection(
    stream: interprocess::local_socket::Stream,
    token: &str,
    sessions: Arc<Mutex<HashMap<Uuid, ManagedSession>>>,
) {
    let mut reader = BufReader::new(stream);
    let mut request = String::new();
    let response = match reader.read_line(&mut request) {
        Ok(_) => match serde_json::from_str::<ManagerEnvelope>(&request) {
            Ok(envelope) if envelope.token == token => {
                handle_request(envelope.request, &sessions)
            }
            Ok(_) => ManagerResponse::error("Unauthorized local session request.".into()),
            Err(error) => ManagerResponse::error(format!("Invalid session request: {error}")),
        },
        Err(error) => ManagerResponse::error(format!("Could not read session request: {error}")),
    };

    if let Ok(response) = serde_json::to_vec(&response) {
        let _ = reader.get_mut().write_all(&response);
        let _ = reader.get_mut().write_all(b"\n");
        let _ = reader.get_mut().flush();
    }
}

fn handle_request(
    request: ManagerRequest,
    sessions: &Arc<Mutex<HashMap<Uuid, ManagedSession>>>,
) -> ManagerResponse {
    match request {
        ManagerRequest::Ping => ManagerResponse::status(stopped_status()),
        ManagerRequest::Status { profile_id } => {
            let mut sessions = match sessions.lock() {
                Ok(sessions) => sessions,
                Err(_) => return ManagerResponse::error("Local session state is unavailable.".into()),
            };
            let Some(session) = sessions.get_mut(&profile_id) else {
                return ManagerResponse::status(stopped_status());
            };
            match session.child.try_wait() {
                Ok(Some(_)) => ManagerResponse::status(stopped_status()),
                Ok(None) => ManagerResponse::status(LocalServerStatus {
                    state: if session.stopping {
                        LocalServerState::Stopping
                    } else {
                        LocalServerState::Running
                    },
                    process_id: Some(session.child.id()),
                }),
                Err(error) => ManagerResponse::error(format!("Could not read server status: {error}")),
            }
        }
        ManagerRequest::Start {
            profile_id,
            directory,
            executable,
            arguments,
        } => start_session(profile_id, directory, executable, arguments, sessions),
        ManagerRequest::Stop { profile_id } => stop_session(profile_id, sessions),
    }
}

fn start_session(
    profile_id: Uuid,
    directory: String,
    executable: String,
    arguments: Vec<String>,
    sessions: &Arc<Mutex<HashMap<Uuid, ManagedSession>>>,
) -> ManagerResponse {
    let directory = match Path::new(&directory).canonicalize() {
        Ok(directory) if directory.is_dir() => directory,
        Ok(_) => return ManagerResponse::error("The server path is not a folder.".into()),
        Err(_) => return ManagerResponse::error("The server folder could not be found or opened.".into()),
    };
    if executable.trim().is_empty() || executable.contains('\0') {
        return ManagerResponse::error("Enter a valid launch executable.".into());
    }

    let mut sessions = match sessions.lock() {
        Ok(sessions) => sessions,
        Err(_) => return ManagerResponse::error("Local session state is unavailable.".into()),
    };
    if let Some(session) = sessions.get_mut(&profile_id) {
        match session.child.try_wait() {
            Ok(None) => return ManagerResponse::status(LocalServerStatus {
                state: if session.stopping { LocalServerState::Stopping } else { LocalServerState::Running },
                process_id: Some(session.child.id()),
            }),
            Ok(Some(_)) => {
                sessions.remove(&profile_id);
            }
            Err(error) => return ManagerResponse::error(format!("Could not check the existing server process: {error}")),
        }
    }

    let mut command = Command::new(executable);
    command
        .args(arguments)
        .current_dir(&directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return ManagerResponse::error(format!("Could not start the server: {error}")),
    };
    let Some(stdin) = child.stdin.take() else {
        let _ = child.kill();
        return ManagerResponse::error("Could not connect to the server console.".into());
    };
    drain(child.stdout.take());
    drain(child.stderr.take());
    let status = LocalServerStatus {
        state: LocalServerState::Running,
        process_id: Some(child.id()),
    };
    sessions.insert(
        profile_id,
        ManagedSession {
            child,
            stdin,
            stopping: false,
        },
    );
    ManagerResponse::status(status)
}

fn stop_session(
    profile_id: Uuid,
    sessions: &Arc<Mutex<HashMap<Uuid, ManagedSession>>>,
) -> ManagerResponse {
    let mut sessions = match sessions.lock() {
        Ok(sessions) => sessions,
        Err(_) => return ManagerResponse::error("Local session state is unavailable.".into()),
    };
    let Some(session) = sessions.get_mut(&profile_id) else {
        return ManagerResponse::status(stopped_status());
    };
    match session.child.try_wait() {
        Ok(Some(_)) => ManagerResponse::status(stopped_status()),
        Ok(None) if session.stopping => ManagerResponse::status(LocalServerStatus {
            state: LocalServerState::Stopping,
            process_id: Some(session.child.id()),
        }),
        Ok(None) => match session.stdin.write_all(b"stop\n").and_then(|()| session.stdin.flush()) {
            Ok(()) => {
                session.stopping = true;
                ManagerResponse::status(LocalServerStatus {
                    state: LocalServerState::Stopping,
                    process_id: Some(session.child.id()),
                })
            }
            Err(error) => ManagerResponse::error(format!("Could not send the Minecraft stop command: {error}")),
        },
        Err(error) => ManagerResponse::error(format!("Could not check the server process: {error}")),
    }
}

fn drain<R: Read + Send + 'static>(reader: Option<R>) {
    if let Some(mut reader) = reader {
        thread::spawn(move || {
            let _ = io::copy(&mut reader, &mut io::sink());
        });
    }
}

fn stopped_status() -> LocalServerStatus {
    LocalServerStatus {
        state: LocalServerState::Stopped,
        process_id: None,
    }
}

fn load_or_create_token(app_data_dir: &Path) -> Result<String, String> {
    let token_path = app_data_dir.join(TOKEN_FILE);
    match fs::read_to_string(&token_path) {
        Ok(token) => {
            let token = token.trim().to_owned();
            Uuid::parse_str(&token).map_err(|_| "The local session token is invalid.".to_owned())?;
            Ok(token)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let token = Uuid::new_v4().to_string();
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&token_path) {
                Ok(mut file) => {
                    file.write_all(token.as_bytes())
                        .map_err(|error| format!("Could not save the local session token: {error}"))?;
                    Ok(token)
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    fs::read_to_string(&token_path)
                        .map(|token| token.trim().to_owned())
                        .map_err(|error| format!("Could not read the local session token: {error}"))
                }
                Err(error) => Err(format!("Could not create the local session token: {error}")),
            }
        }
        Err(error) => Err(format!("Could not read the local session token: {error}")),
    }
}

fn socket_name(app_data_dir: &Path, token: &str) -> Result<interprocess::local_socket::Name<'static>, String> {
    if GenericNamespaced::is_supported() {
        format!("servercrafter-{token}")
            .to_ns_name::<GenericNamespaced>()
            .map_err(|error| format!("Could not create the local session address: {error}"))
    } else {
        app_data_dir
            .join("session-manager.sock")
            .to_fs_name::<GenericFilePath>()
            .map_err(|error| format!("Could not create the local session address: {error}"))
    }
}

fn detach_helper(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
}

pub mod commands {
    use super::*;

    #[tauri::command]
    pub fn local_server_status(
        profile_id: Uuid,
        profiles: State<'_, ServerProfileStore>,
        manager: State<'_, LocalSessionManager>,
    ) -> Result<LocalServerStatus, String> {
        require_local_profile(profile_id, &profiles)?;
        manager.status(profile_id)
    }

    #[tauri::command]
    pub fn start_local_server(
        profile_id: Uuid,
        profiles: State<'_, ServerProfileStore>,
        manager: State<'_, LocalSessionManager>,
    ) -> Result<LocalServerStatus, String> {
        let profile = require_local_profile(profile_id, &profiles)?;
        let ServerTarget::Local { directory, launch } = profile.target else {
            unreachable!("require_local_profile only returns local targets")
        };
        manager.start(profile.id, directory, launch.executable, launch.arguments)
    }

    #[tauri::command]
    pub fn stop_local_server(
        profile_id: Uuid,
        profiles: State<'_, ServerProfileStore>,
        manager: State<'_, LocalSessionManager>,
    ) -> Result<LocalServerStatus, String> {
        require_local_profile(profile_id, &profiles)?;
        manager.stop(profile_id)
    }

    fn require_local_profile(
        profile_id: Uuid,
        profiles: &State<'_, ServerProfileStore>,
    ) -> Result<crate::server_profiles::ServerProfile, String> {
        let profile = profiles
            .list()?
            .into_iter()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| "Server profile was not found.".to_owned())?;
        if !matches!(profile.target, ServerTarget::Local { .. }) {
            return Err("Local process controls are only available for local profiles.".into());
        }
        Ok(profile)
    }
}
