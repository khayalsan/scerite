use std::time::Duration;

use serde::Deserialize;

const CHECKER: &str = "https://pong.khayal.dev";

#[derive(Deserialize)]
struct CheckerResponse {
    reachable: bool,
}

pub(crate) fn checker_url(port: u16) -> Result<String, String> {
    if port == 0 {
        return Err("Choose a port between 1 and 65535.".into());
    }
    Ok(format!("{CHECKER}/{port}"))
}

pub(crate) fn parse_response(bytes: &[u8]) -> Result<bool, String> {
    if bytes.len() > 1024 {
        return Err("The connectivity checker returned too much data.".into());
    }
    serde_json::from_slice::<CheckerResponse>(bytes)
        .map(|response| response.reachable)
        .map_err(|_| "The connectivity checker returned an invalid response.".into())
}

#[tauri::command]
pub async fn check_local_public_port(port: u16) -> Result<bool, String> {
    let url = checker_url(port)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| format!("Could not prepare the connectivity check: {error}"))?;
    let mut response = client.get(url).send().await
        .map_err(|error| format!("Could not reach the connectivity checker: {error}"))?
        .error_for_status()
        .map_err(|error| format!("The connectivity checker rejected the request: {error}"))?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await
        .map_err(|error| format!("Could not read the connectivity check result: {error}"))? {
        if bytes.len() + chunk.len() > 1024 {
            return Err("The connectivity checker returned too much data.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    parse_response(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_checker_contract() {
        assert_eq!(parse_response(br#"{"reachable":true}"#).unwrap(), true);
        assert_eq!(parse_response(br#"{"reachable":false}"#).unwrap(), false);
        assert!(parse_response(br#"{"open":true}"#).is_err());
    }

    #[test]
    fn rejects_zero_port() {
        assert!(checker_url(0).is_err());
    }
}
