use std::process::Command;

use tauri::State;
use uuid::Uuid;

use crate::server_profiles::{ServerProfileStore, ServerTarget};

#[tauri::command]
pub async fn test_remote_connection(
    profile_id: Uuid,
    profiles: State<'_, ServerProfileStore>,
) -> Result<(), String> {
    let profile = profiles.list()?.into_iter().find(|profile| profile.id == profile_id)
        .ok_or_else(|| "Server profile was not found.".to_owned())?;
    let ServerTarget::Remote { host, port, username, identity_file } = profile.target else {
        return Err("SSH connection checks are only available for remote profiles.".into());
    };
    tauri::async_runtime::spawn_blocking(move || check_remote_connection(host, port, username, identity_file))
        .await
        .map_err(|error| format!("SSH check failed to finish: {error}"))?
}

fn check_remote_connection(
    host: String,
    port: u16,
    username: String,
    identity_file: Option<String>,
) -> Result<(), String> {
    let destination = if host.contains(':') && !host.starts_with('[') {
        format!("{username}@[{host}]")
    } else {
        format!("{username}@{host}")
    };

    let mut command = Command::new("ssh");
    command
        .arg("-T")
        .arg("-p").arg(port.to_string())
        .arg("-o").arg("BatchMode=yes")
        .arg("-o").arg("StrictHostKeyChecking=yes")
        .arg("-o").arg("ConnectTimeout=10")
        .arg("-o").arg("ServerAliveInterval=10")
        .arg("-o").arg("ServerAliveCountMax=2")
        .arg("-o").arg("PreferredAuthentications=publickey");
    if let Some(identity_file) = identity_file {
        command.arg("-i").arg(identity_file)
            .arg("-o").arg("IdentitiesOnly=yes");
    }
    let output = command.arg(destination).arg("true").output().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "OpenSSH client was not found. Install OpenSSH and make sure ssh is on PATH.".to_owned()
        } else {
            format!("Could not start OpenSSH: {error}")
        }
    })?;
    if output.status.success() {
        return Ok(());
    }

    let details = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if details.contains("REMOTE HOST IDENTIFICATION HAS CHANGED") {
        return Err(format!("The SSH host key changed. Connection refused. Verify the server's new fingerprint before updating known_hosts.\n{details}"));
    }
    if details.contains("Host key verification failed") || details.contains("Offending ") {
        return Err(format!("SSH host-key verification failed. Verify the fingerprint before changing known_hosts.\n{details}"));
    }
    if details.contains("Permission denied") {
        return Err("SSH authentication failed. Check the username and make sure the selected key is loaded in your SSH agent or accepted by the server.".into());
    }
    if details.is_empty() {
        Err("SSH connection failed without an error message.".into())
    } else {
        Err(format!("SSH connection failed: {details}"))
    }
}
