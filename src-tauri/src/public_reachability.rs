use std::time::Duration;

use serde::Deserialize;

const CHECKER: &str = "https://pong.khayal.dev";
pub(crate) const RATE_LIMIT_MESSAGE: &str = "The connectivity checker is rate limiting requests. Wait and try again.";

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

pub(crate) fn remote_rate_limited(stderr: &[u8]) -> bool {
    let message = String::from_utf8_lossy(stderr);
    message.contains("returned error: 429") || message.contains("ERROR 429:") || message.contains(" 429 Too Many Requests")
}

#[tauri::command]
pub async fn check_local_public_port(port: u16) -> Result<bool, String> {
    let url = checker_url(port)?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| format!("Could not prepare the connectivity check: {error}"))?;
    let response = client.get(url).send().await
        .map_err(|error| format!("Could not reach the connectivity checker: {error}"))?;
    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(RATE_LIMIT_MESSAGE.into());
    }
    let mut response = response.error_for_status()
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

    #[test]
    fn recognizes_remote_rate_limit() {
        assert!(remote_rate_limited(b"curl: (22) The requested URL returned error: 429"));
        assert!(remote_rate_limited(b"ERROR 429: Too Many Requests."));
        assert!(!remote_rate_limited(b"curl: (7) Failed to connect"));
    }
}
