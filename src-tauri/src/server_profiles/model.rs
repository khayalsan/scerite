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
                if launch.arguments.iter().any(|argument| argument.contains('\0')) {
                    return Err("Launch arguments cannot contain null characters.".into());
                }
                Ok(())
            }
            Self::Remote {
                host,
                port,
                username,
            } => {
                if host.trim().is_empty() {
                    return Err("Enter a remote host name or address.".into());
                }
                if *port == 0 {
                    return Err("The SSH port must be between 1 and 65535.".into());
                }
                if username.trim().is_empty() {
                    return Err("Enter an SSH user name.".into());
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
            } => Self::Remote {
                host: host.trim().to_owned(),
                port,
                username: username.trim().to_owned(),
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
