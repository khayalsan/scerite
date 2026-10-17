use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProfile {
    pub id: Uuid,
    pub name: String,
    pub target: ServerTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerTarget {
    Local {
        directory: String,
        #[serde(default)]
        launch: LocalLaunchConfig,
    },
    Remote {
        host: String,
        port: u16,
        username: String,
        #[serde(rename = "identityFile", default)]
        identity_file: Option<String>,
        #[serde(default)]
        directory: String,
        #[serde(default)]
        launch: LocalLaunchConfig,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalLaunchConfig {
    pub executable: String,
    pub arguments: Vec<String>,
}

impl ServerTarget {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Local { directory, launch } => {
                if directory.trim().is_empty() {
                    return Err("Choose a server directory.".into());
                }
                if launch.executable.trim().is_empty() {
                    return Err("Enter a launch executable.".into());
                }
                if launch
                    .arguments
                    .iter()
                    .any(|argument| argument.contains('\0'))
                {
                    return Err("Launch arguments cannot contain null characters.".into());
                }
                Ok(())
            }
            Self::Remote {
                host,
                port,
                username,
                identity_file,
                directory,
                launch,
            } => {
                if host.trim().is_empty()
                    || host.starts_with('-')
                    || host.chars().any(|character| {
                        character.is_whitespace() || matches!(character, '@' | '/' | '\\' | '\0')
                    })
                {
                    return Err("Enter a remote host name or address.".into());
                }
                if *port == 0 {
                    return Err("The SSH port must be between 1 and 65535.".into());
                }
                if username.trim().is_empty()
                    || username.starts_with('-')
                    || !username.chars().all(|character| {
                        character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')
                    })
                {
                    return Err("Enter an SSH user name.".into());
                }
                if identity_file
                    .as_ref()
                    .is_some_and(|path| path.contains('\0'))
                {
                    return Err("The SSH identity path is invalid.".into());
                }
                let trimmed_directory = directory.trim().trim_end_matches('/');
                if !directory.trim().is_empty()
                    && (trimmed_directory.is_empty()
                        || !directory.starts_with('/')
                        || directory.chars().any(char::is_control)
                        || trimmed_directory[1..]
                            .split('/')
                            .any(|part| part.is_empty() || matches!(part, "." | "..")))
                {
                    return Err(
                        "Enter an absolute remote server directory without parent traversal."
                            .into(),
                    );
                }
                if launch.executable.contains('\0')
                    || launch
                        .arguments
                        .iter()
                        .any(|argument| argument.contains('\0'))
                {
                    return Err("Remote launch details cannot contain null characters.".into());
                }
                Ok(())
            }
        }
    }

    pub fn normalized(self) -> Self {
        match self {
            Self::Local { directory, launch } => Self::Local {
                directory: directory.trim().to_owned(),
                launch: LocalLaunchConfig {
                    executable: launch.executable.trim().to_owned(),
                    arguments: launch.arguments,
                },
            },
            Self::Remote {
                host,
                port,
                username,
                identity_file,
                directory,
                launch,
            } => Self::Remote {
                host: host.trim().to_owned(),
                port,
                username: username.trim().to_owned(),
                identity_file: identity_file.and_then(|path| {
                    let path = path.trim().to_owned();
                    (!path.is_empty()).then_some(path)
                }),
                directory: {
                    let directory = directory.trim();
                    if directory.len() > 1 {
                        directory.trim_end_matches('/').to_owned()
                    } else {
                        directory.to_owned()
                    }
                },
                launch: LocalLaunchConfig {
                    executable: launch.executable.trim().to_owned(),
                    arguments: launch.arguments,
                },
            },
        }
    }
}

pub fn validate_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Enter a server name.".into());
    }
    if trimmed.chars().count() > 80 {
        return Err("Server names must be 80 characters or fewer.".into());
    }
    Ok(())
}
