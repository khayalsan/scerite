use std::{
    fs, io,
    path::PathBuf,
    sync::{Mutex, MutexGuard},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::model::{
    LocalLaunchConfig, ServerEnvironment, ServerProfile, ServerTarget, validate_name,
};

#[derive(Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProfileData {
    #[serde(default)]
    environments: Vec<ServerEnvironment>,
    #[serde(default)]
    profiles: Vec<ServerProfile>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyServerProfile {
    id: Uuid,
    name: String,
    target: LegacyServerTarget,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LegacyServerTarget {
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

pub struct ServerProfileStore {
    path: PathBuf,
    data: Mutex<ProfileData>,
}

impl ServerProfileStore {
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let backup_path = path.with_extension("json.bak");
        let (data, migrated) = match fs::read(&path) {
            Ok(contents) => decode_data(&contents)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound && backup_path.exists() => {
                fs::rename(&backup_path, &path)
                    .map_err(|error| format!("Could not recover saved server profiles: {error}"))?;
                let contents = fs::read(&path).map_err(|error| {
                    format!("Could not open recovered server profiles: {error}")
                })?;
                decode_data(&contents)?
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                (ProfileData::default(), false)
            }
            Err(error) => return Err(format!("Could not open saved server profiles: {error}")),
        };

        let store = Self {
            path,
            data: Mutex::new(data),
        };
        if migrated {
            let data = store.lock()?.clone();
            store.save(&data)?;
        }
        Ok(store)
    }

    pub fn list(&self) -> Result<Vec<ServerProfile>, String> {
        Ok(self.lock()?.profiles.clone())
    }

    pub fn list_environments(&self) -> Result<Vec<ServerEnvironment>, String> {
        Ok(self.lock()?.environments.clone())
    }

    pub fn environment(&self, id: Uuid) -> Result<ServerEnvironment, String> {
        self.lock()?
            .environments
            .iter()
            .find(|environment| environment.id == id)
            .cloned()
            .ok_or_else(|| "Server environment was not found.".to_owned())
    }

    pub fn create_environment(
        &self,
        name: String,
        host: String,
        port: u16,
        username: String,
        identity_file: Option<String>,
    ) -> Result<ServerEnvironment, String> {
        let environment = ServerEnvironment {
            id: Uuid::new_v4(),
            name,
            host,
            port,
            username,
            identity_file,
        }
        .normalized();
        environment.validate()?;

        let mut current = self.lock()?;
        let mut data = current.clone();
        data.environments.push(environment.clone());
        self.save(&data)?;
        *current = data;
        Ok(environment)
    }

    pub fn update_environment(
        &self,
        id: Uuid,
        name: String,
        host: String,
        port: u16,
        username: String,
        identity_file: Option<String>,
    ) -> Result<ServerEnvironment, String> {
        let environment = ServerEnvironment {
            id,
            name,
            host,
            port,
            username,
            identity_file,
        }
        .normalized();
        environment.validate()?;

        let mut current = self.lock()?;
        let mut data = current.clone();
        let existing = data
            .environments
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or_else(|| "Server environment was not found.".to_owned())?;
        *existing = environment.clone();
        self.save(&data)?;
        *current = data;
        Ok(environment)
    }

    pub fn delete_environment(&self, id: Uuid) -> Result<(), String> {
        let mut current = self.lock()?;
        let mut data = current.clone();
        if data.profiles.iter().any(|profile| {
            matches!(profile.target, ServerTarget::Remote { environment_id, .. } if environment_id == id)
        }) {
            return Err("Remove the servers in this environment before deleting it.".into());
        }
        let original_count = data.environments.len();
        data.environments.retain(|environment| environment.id != id);
        if data.environments.len() == original_count {
            return Err("Server environment was not found.".into());
        }
        self.save(&data)?;
        *current = data;
        Ok(())
    }

    pub fn create(&self, name: String, target: ServerTarget) -> Result<ServerProfile, String> {
        validate_name(&name)?;
        target.validate()?;

        let mut current = self.lock()?;
        validate_environment_reference(&current, &target)?;
        let profile = ServerProfile {
            id: Uuid::new_v4(),
            name: name.trim().to_owned(),
            target: target.normalized(),
        };
        let mut data = current.clone();
        data.profiles.push(profile.clone());
        self.save(&data)?;
        *current = data;
        Ok(profile)
    }

    pub fn update(
        &self,
        id: Uuid,
        name: String,
        target: ServerTarget,
    ) -> Result<ServerProfile, String> {
        validate_name(&name)?;
        target.validate()?;

        let mut current = self.lock()?;
        validate_environment_reference(&current, &target)?;
        let mut data = current.clone();
        let profile = data
            .profiles
            .iter_mut()
            .find(|profile| profile.id == id)
            .ok_or_else(|| "Server profile was not found.".to_owned())?;
        profile.name = name.trim().to_owned();
        profile.target = target.normalized();
        let updated = profile.clone();
        self.save(&data)?;
        *current = data;
        Ok(updated)
    }

    pub fn delete(&self, id: Uuid) -> Result<(), String> {
        let mut current = self.lock()?;
        let mut data = current.clone();
        let original_count = data.profiles.len();
        data.profiles.retain(|profile| profile.id != id);
        if data.profiles.len() == original_count {
            return Err("Server profile was not found.".into());
        }
        self.save(&data)?;
        *current = data;
        Ok(())
    }

    fn save(&self, data: &ProfileData) -> Result<(), String> {
        let contents = serde_json::to_vec_pretty(data).map_err(|error| {
            format!("Could not encode server profiles and environments: {error}")
        })?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create the profile directory: {error}"))?;
        }
        let temp_path = self.path.with_extension("json.tmp");
        let backup_path = self.path.with_extension("json.bak");
        fs::write(&temp_path, contents)
            .map_err(|error| format!("Could not prepare server profiles: {error}"))?;

        if self.path.exists() {
            if backup_path.exists() {
                fs::remove_file(&backup_path)
                    .map_err(|error| format!("Could not rotate profile backup: {error}"))?;
            }
            fs::rename(&self.path, &backup_path)
                .map_err(|error| format!("Could not preserve existing server profiles: {error}"))?;
        }

        if let Err(error) = fs::rename(&temp_path, &self.path) {
            if backup_path.exists() {
                let _ = fs::rename(&backup_path, &self.path);
            }
            return Err(format!("Could not save server profiles: {error}"));
        }

        if backup_path.exists() {
            let _ = fs::remove_file(backup_path);
        }
        Ok(())
    }

    fn lock(&self) -> Result<MutexGuard<'_, ProfileData>, String> {
        self.data
            .lock()
            .map_err(|_| "Server profile storage is unavailable.".to_owned())
    }
}

fn validate_environment_reference(data: &ProfileData, target: &ServerTarget) -> Result<(), String> {
    if let ServerTarget::Remote { environment_id, .. } = target {
        if !data
            .environments
            .iter()
            .any(|environment| environment.id == *environment_id)
        {
            return Err("Choose a saved server environment.".into());
        }
    }
    Ok(())
}

fn decode_data(contents: &[u8]) -> Result<(ProfileData, bool), String> {
    let value: serde_json::Value = serde_json::from_slice(contents)
        .map_err(|error| format!("Could not read saved server profiles: {error}"))?;
    if value.is_array() {
        let legacy_profiles: Vec<LegacyServerProfile> = serde_json::from_value(value)
            .map_err(|error| format!("Could not read saved server profiles: {error}"))?;
        return Ok((migrate_legacy_profiles(legacy_profiles), true));
    }
    serde_json::from_value(value)
        .map(|data| (data, false))
        .map_err(|error| format!("Could not read saved server profiles and environments: {error}"))
}

fn migrate_legacy_profiles(profiles: Vec<LegacyServerProfile>) -> ProfileData {
    let mut data = ProfileData::default();
    for legacy in profiles {
        let target = match legacy.target {
            LegacyServerTarget::Local { directory, launch } => {
                ServerTarget::Local { directory, launch }
            }
            LegacyServerTarget::Remote {
                host,
                port,
                username,
                identity_file,
                directory,
                launch,
            } => {
                let environment_id = data
                    .environments
                    .iter()
                    .find(|environment| {
                        environment.host == host
                            && environment.port == port
                            && environment.username == username
                            && environment.identity_file == identity_file
                    })
                    .map(|environment| environment.id)
                    .unwrap_or_else(|| {
                        let environment = ServerEnvironment {
                            id: Uuid::new_v4(),
                            name: host.clone(),
                            host,
                            port,
                            username,
                            identity_file,
                        }
                        .normalized();
                        let id = environment.id;
                        data.environments.push(environment);
                        id
                    });
                ServerTarget::Remote {
                    environment_id,
                    directory,
                    launch,
                }
            }
        };
        data.profiles.push(ServerProfile {
            id: legacy.id,
            name: legacy.name,
            target,
        });
    }
    data
}
