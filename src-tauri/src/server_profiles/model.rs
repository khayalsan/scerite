use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProfile {
    pub id: Uuid,
    pub name: String,
    pub target: ServerTarget,
    #[serde(default)]
    pub installed_software: Option<String>,
    #[serde(default)]
    pub installed_version: Option<String>,
    #[serde(default)]
    pub installed_packages: Vec<InstalledPackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPackage {
    pub kind: String,
    pub project_id: String,
    pub version_id: String,
    pub title: String,
    pub filename: String,
    #[serde(default)]
    pub required_by: Vec<String>,
    #[serde(default)]
    pub direct: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerEnvironment {
    pub id: Uuid,
    pub name: String,
    #[serde(default)]
    pub root_directory: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(default)]
    pub identity_file: Option<String>,
}

impl ServerEnvironment {
    pub fn validate(&self) -> Result<(), String> {
        validate_environment_name(&self.name)?;
        if !valid_remote_directory(&self.root_directory) {
            return Err("Enter an absolute Minecraft root folder for this environment.".into());
        }
        if self.host.trim().is_empty()
            || self.host.starts_with('-')
            || self.host.chars().any(|character| {
                character.is_whitespace() || matches!(character, '@' | '/' | '\\' | '\0')
            })
        {
            return Err("Enter a remote host name or address.".into());
        }
        if self.port == 0 {
            return Err("The SSH port must be between 1 and 65535.".into());
        }
        if self.username.trim().is_empty()
            || self.username.starts_with('-')
            || !self.username.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')
            })
        {
            return Err("Enter an SSH user name.".into());
        }
        if self
            .identity_file
            .as_ref()
            .is_some_and(|path| path.contains('\0'))
        {
            return Err("The SSH identity path is invalid.".into());
        }
        Ok(())
    }

    pub fn normalized(mut self) -> Self {
        self.name = self.name.trim().to_owned();
        self.root_directory = self.root_directory.trim().trim_end_matches('/').to_owned();
        self.host = self.host.trim().to_owned();
        self.username = self.username.trim().to_owned();
        self.identity_file = self.identity_file.and_then(|path| {
            let path = path.trim().to_owned();
            (!path.is_empty()).then_some(path)
        });
        self
    }
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
        #[serde(rename = "environmentId")]
        environment_id: Uuid,
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
    #[serde(default)]
    pub managed_java: Option<String>,
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
                environment_id: _,
                directory,
                launch,
            } => {
                if !directory.trim().is_empty() && !valid_remote_directory(directory) {
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
                    managed_java: launch.managed_java,
                },
            },
            Self::Remote {
                environment_id,
                directory,
                launch,
            } => Self::Remote {
                environment_id,
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
                    managed_java: launch.managed_java,
                },
            },
        }
    }
}

fn valid_remote_directory(directory: &str) -> bool {
    let trimmed = directory.trim().trim_end_matches('/');
    !trimmed.is_empty()
        && trimmed.starts_with('/')
        && !trimmed.chars().any(char::is_control)
        && trimmed[1..]
            .split('/')
            .all(|part| !part.is_empty() && !matches!(part, "." | ".."))
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

pub fn validate_environment_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Enter an environment name.".into());
    }
    if trimmed.chars().count() > 80 {
        return Err("Environment names must be 80 characters or fewer.".into());
    }
    Ok(())
}
