use serde_json::Value;
use sha2::{Digest, Sha256};

const MAX_PROXY_BYTES: u64 = 100 * 1024 * 1024;

async fn json(client: &reqwest::Client, url: &str) -> Result<Value, String> {
    client.get(url).send().await
        .map_err(|error| format!("Could not reach PaperMC: {error}"))?
        .error_for_status().map_err(|error| format!("PaperMC rejected the request: {error}"))?
        .json().await.map_err(|error| format!("Could not read PaperMC metadata: {error}"))
}

pub async fn stable_velocity_jar(client: &reqwest::Client) -> Result<Vec<u8>, String> {
    let project = json(client, "https://fill.papermc.io/v3/projects/velocity").await?;
    let groups = project["versions"].as_object().ok_or("Velocity returned an invalid version list.")?;
    let mut groups = groups.iter().collect::<Vec<_>>();
    groups.sort_by(|left, right| right.0.cmp(left.0));
    let versions = groups.into_iter().flat_map(|(_, group)| group.as_array().into_iter().flatten())
        .filter_map(Value::as_str);
    for version in versions {
        if !version.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')) { continue; }
        let builds = json(client, &format!("https://fill.papermc.io/v3/projects/velocity/versions/{version}/builds")).await?;
        let Some(items) = builds.as_array().or_else(|| builds["builds"].as_array()) else { continue; };
        let Some(download) = items.iter().find(|build| build["channel"] == "STABLE")
            .and_then(|build| build["downloads"]["server:default"].as_object()) else { continue; };
        let url = download.get("url").and_then(Value::as_str).ok_or("Velocity has no JAR URL.")?;
        let parsed = reqwest::Url::parse(url).map_err(|_| "Velocity returned an invalid JAR URL.")?;
        if parsed.scheme() != "https" || parsed.host_str() != Some("fill-data.papermc.io") || parsed.port().is_some() {
            return Err("Velocity returned an unapproved JAR URL.".into());
        }
        let size = download.get("size").and_then(Value::as_u64).ok_or("Velocity has no JAR size.")?;
        if size == 0 || size > MAX_PROXY_BYTES { return Err("Velocity JAR exceeds the download limit.".into()); }
        let hash = download.get("checksums").and_then(|checksums| checksums.get("sha256"))
            .and_then(Value::as_str).ok_or("Velocity has no SHA-256 checksum.")?;
        let bytes = client.get(parsed).send().await
            .map_err(|error| format!("Could not download Velocity: {error}"))?
            .error_for_status().map_err(|error| format!("Velocity download failed: {error}"))?
            .bytes().await.map_err(|error| format!("Could not read Velocity JAR: {error}"))?;
        if bytes.len() as u64 != size || format!("{:x}", Sha256::digest(&bytes)) != hash.to_ascii_lowercase() {
            return Err("Velocity JAR did not match PaperMC's size and checksum.".into());
        }
        return Ok(bytes.to_vec());
    }
    Err("PaperMC has no stable Velocity build available.".into())
}

fn set_property(content: &str, key: &str, value: &str) -> String {
    let mut found = false;
    let mut result = String::new();
    for line in content.split_inclusive('\n') {
        if line.split_once('=').is_some_and(|(name, _)| name.trim() == key) {
            result.push_str(&format!("{key}={value}"));
            if line.ends_with("\r\n") { result.push_str("\r\n"); }
            else if line.ends_with('\n') { result.push('\n'); }
            found = true;
        } else { result.push_str(line); }
    }
    if !found {
        if !result.is_empty() && !result.ends_with('\n') { result.push('\n'); }
        result.push_str(&format!("{key}={value}\n"));
    }
    result
}

pub fn backend_properties(original: &str, port: u16) -> String {
    let updated = crate::server_port::updated_content(Some(original), port);
    let updated = set_property(&updated, "server-ip", "127.0.0.1");
    set_property(&updated, "online-mode", "false")
}

pub fn proxy_config(public_port: u16, backend_port: u16) -> String {
    format!("config-version = \"2.9\"\nbind = \"0.0.0.0:{public_port}\"\nonline-mode = true\nforce-key-authentication = true\nplayer-info-forwarding-mode = \"modern\"\nforwarding-secret-file = \"forwarding.secret\"\n[servers]\nbackend = \"127.0.0.1:{backend_port}\"\ntry = [\"backend\"]\n[forced-hosts]\n")
}

pub fn restore_backend_properties(current: &str, original: &str, patched: &str) -> Result<String, String> {
    let mut restored = current.to_owned();
    for key in ["server-port", "server-ip", "online-mode"] {
        let values = |content: &str| content.lines().filter_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == key).then_some(value.trim().to_owned())
        }).collect::<Vec<_>>();
        let expected = values(patched);
        if values(&restored) != expected {
            return Err(format!("{key} changed after Velocity was enabled. Restore that value before disabling it."));
        }
        let original_values = values(original);
        if let Some(value) = original_values.last() {
            restored = set_property(&restored, key, value);
        } else {
            restored = restored.split_inclusive('\n').filter(|line| {
                line.split_once('=').is_none_or(|(name, _)| name.trim() != key)
            }).collect();
        }
    }
    Ok(restored)
}

fn velocity_yaml_line(content: &str, key: &str) -> Option<String> {
    let mut in_proxies = false;
    let mut in_velocity = false;
    for line in content.lines() {
        let trimmed = line.trim();
        let indent = line.chars().take_while(|c| *c == ' ').count();
        if indent == 0 && !trimmed.is_empty() && !trimmed.starts_with('#') {
            in_proxies = trimmed == "proxies:";
            in_velocity = false;
        } else if in_proxies && indent == 2 && !trimmed.is_empty() && !trimmed.starts_with('#') {
            in_velocity = trimmed == "velocity:";
        } else if in_velocity && indent == 4 && trimmed.starts_with(&format!("{key}:")) {
            return Some(trimmed.to_owned());
        }
    }
    None
}

fn replace_velocity_yaml_line(content: &str, key: &str, expected: &str, previous: &str) -> Result<String, String> {
    let mut in_proxies = false;
    let mut in_velocity = false;
    let mut found = false;
    let mut result = String::new();
    for line in content.split_inclusive('\n') {
        let trimmed = line.trim();
        let indent = line.chars().take_while(|c| *c == ' ').count();
        if indent == 0 && !trimmed.is_empty() && !trimmed.starts_with('#') {
            in_proxies = trimmed == "proxies:";
            in_velocity = false;
        } else if in_proxies && indent == 2 && !trimmed.is_empty() && !trimmed.starts_with('#') {
            in_velocity = trimmed == "velocity:";
        }
        if in_velocity && indent == 4 && trimmed.starts_with(&format!("{key}:")) {
            if trimmed != expected { return Err(format!("Paper's proxies.velocity.{key} changed after Velocity was enabled.")); }
            result.push_str("    ");
            result.push_str(previous);
            if line.ends_with("\r\n") { result.push_str("\r\n"); }
            else if line.ends_with('\n') { result.push('\n'); }
            found = true;
        } else {
            result.push_str(line);
        }
    }
    if !found { return Err(format!("Paper's proxies.velocity.{key} is missing.")); }
    Ok(result)
}

pub fn restore_paper_forwarding(current: &str, original: &str, patched: &str) -> Result<String, String> {
    let mut restored = current.to_owned();
    for key in ["enabled", "online-mode", "secret"] {
        let expected = velocity_yaml_line(patched, key).ok_or("The saved Velocity settings are incomplete.")?;
        let previous = velocity_yaml_line(original, key).ok_or("The original Velocity settings are incomplete.")?;
        restored = replace_velocity_yaml_line(&restored, key, &expected, &previous)?;
    }
    Ok(restored)
}

pub fn paper_forwarding(original: &str, secret: &str) -> Result<String, String> {
    if secret.len() < 32 || !secret.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Velocity forwarding secret is invalid.".into());
    }
    let mut in_proxies = false;
    let mut in_velocity = false;
    let mut found = [false; 3];
    let mut result = String::new();
    for line in original.split_inclusive('\n') {
        let trimmed = line.trim();
        let indent = line.chars().take_while(|c| *c == ' ').count();
        if indent == 0 && !trimmed.is_empty() && !trimmed.starts_with('#') {
            in_proxies = trimmed == "proxies:";
            in_velocity = false;
        } else if in_proxies && indent == 2 && !trimmed.is_empty() && !trimmed.starts_with('#') {
            in_velocity = trimmed == "velocity:";
        }
        let replacement = if in_velocity && indent == 4 {
            if trimmed.starts_with("enabled:") { found[0] = true; Some("    enabled: true") }
            else if trimmed.starts_with("online-mode:") { found[1] = true; Some("    online-mode: true") }
            else if trimmed.starts_with("secret:") { found[2] = true; None }
            else { None }
        } else { None };
        if found[2] && in_velocity && indent == 4 && trimmed.starts_with("secret:") {
            result.push_str(&format!("    secret: \"{secret}\""));
        } else if let Some(value) = replacement { result.push_str(value); }
        else { result.push_str(line); continue; }
        if line.ends_with("\r\n") { result.push_str("\r\n"); }
        else if line.ends_with('\n') { result.push('\n'); }
    }
    if found.iter().any(|found| !found) {
        return Err("Paper's proxies.velocity settings are missing. Start this Paper or Purpur server once before enabling Velocity.".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwarding_patch_changes_only_velocity_keys() {
        let original = "proxies:\n  bungee-cord:\n    online-mode: false\n  velocity:\n    enabled: false\n    online-mode: false\n    secret: old\nother: true\n";
        let patched = paper_forwarding(original, &"a".repeat(32)).unwrap();
        assert!(patched.contains("bungee-cord:\n    online-mode: false"));
        assert!(patched.contains("velocity:\n    enabled: true\n    online-mode: true\n    secret: \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\""));
        assert!(patched.ends_with("other: true\n"));
    }

    #[test]
    fn backend_patch_binds_locally_and_disables_direct_auth() {
        let patched = backend_properties("server-port=25565\nonline-mode=true\n", 25566);
        assert!(patched.contains("server-port=25566\n"));
        assert!(patched.contains("server-ip=127.0.0.1\n"));
        assert!(patched.contains("online-mode=false\n"));
    }

    #[test]
    fn proxy_config_clears_velocity_sample_hosts() {
        let config = proxy_config(25565, 25566);
        assert!(config.contains("[servers]\nbackend = \"127.0.0.1:25566\"\ntry = [\"backend\"]\n"));
        assert!(config.ends_with("[forced-hosts]\n"));
        assert!(!config.contains("lobby.example.com"));
    }

    #[test]
    fn disable_restores_only_managed_values() {
        let original = "server-port=25565\nonline-mode=true\nmotd=old\n";
        let patched = backend_properties(original, 25566);
        let current = patched.replace("motd=old", "motd=new");
        let restored = restore_backend_properties(&current, original, &patched).unwrap();
        assert_eq!(restored, "server-port=25565\nonline-mode=true\nmotd=new\n");

        let original = "proxies:\n  velocity:\n    enabled: false\n    online-mode: false\n    secret: old\nother: old\n";
        let patched = paper_forwarding(original, &"a".repeat(32)).unwrap();
        let current = patched.replace("other: old", "other: new");
        let restored = restore_paper_forwarding(&current, original, &patched).unwrap();
        assert_eq!(restored, original.replace("other: old", "other: new"));
    }
}
