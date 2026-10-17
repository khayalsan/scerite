use std::{
    fs, io,
    path::PathBuf,
    sync::{Mutex, MutexGuard},
};

use uuid::Uuid;

use super::model::{ServerProfile, ServerTarget, validate_name};

pub struct ServerProfileStore {
    path: PathBuf,
    profiles: Mutex<Vec<ServerProfile>>,
}

impl ServerProfileStore {
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let backup_path = path.with_extension("json.bak");
        let profiles = match fs::read(&path) {
            Ok(contents) => serde_json::from_slice(&contents)
                .map_err(|error| format!("Could not read saved server profiles: {error}"))?,
            Err(error) if error.kind() == io::ErrorKind::NotFound && backup_path.exists() => {
                fs::rename(&backup_path, &path)
                    .map_err(|error| format!("Could not recover saved server profiles: {error}"))?;
                let contents = fs::read(&path).map_err(|error| {
                    format!("Could not open recovered server profiles: {error}")
                })?;
                serde_json::from_slice(&contents)
                    .map_err(|error| format!("Could not read recovered server profiles: {error}"))?
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(format!("Could not open saved server profiles: {error}")),
        };

        Ok(Self {
            path,
            profiles: Mutex::new(profiles),
        })
    }

    pub fn list(&self) -> Result<Vec<ServerProfile>, String> {
        Ok(self.lock()?.clone())
    }

    pub fn create(&self, name: String, target: ServerTarget) -> Result<ServerProfile, String> {
        validate_name(&name)?;
        target.validate()?;

        let profile = ServerProfile {
            id: Uuid::new_v4(),
            name: name.trim().to_owned(),
            target: target.normalized(),
        };
        let mut current = self.lock()?;
        let mut profiles = current.clone();
        profiles.push(profile.clone());
        self.save(&profiles)?;
        *current = profiles;
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
        let mut profiles = current.clone();
        let profile = profiles
            .iter_mut()
            .find(|profile| profile.id == id)
            .ok_or_else(|| "Server profile was not found.".to_owned())?;
        profile.name = name.trim().to_owned();
        profile.target = target.normalized();
        let updated = profile.clone();
        self.save(&profiles)?;
        *current = profiles;
        Ok(updated)
    }

    pub fn delete(&self, id: Uuid) -> Result<(), String> {
        let mut current = self.lock()?;
        let mut profiles = current.clone();
        let original_count = profiles.len();
        profiles.retain(|profile| profile.id != id);
        if profiles.len() == original_count {
            return Err("Server profile was not found.".into());
        }
        self.save(&profiles)?;
        *current = profiles;
        Ok(())
    }

    fn save(&self, profiles: &[ServerProfile]) -> Result<(), String> {
        let contents = serde_json::to_vec_pretty(profiles)
            .map_err(|error| format!("Could not encode server profiles: {error}"))?;
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

    fn lock(&self) -> Result<MutexGuard<'_, Vec<ServerProfile>>, String> {
        self.profiles
            .lock()
            .map_err(|_| "Server profile storage is unavailable.".to_owned())
    }
}
