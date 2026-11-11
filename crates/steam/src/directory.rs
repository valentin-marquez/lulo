//! Steam's public directory of connection managers.

use serde::Deserialize;

use crate::Result;

const URL: &str = "https://api.steampowered.com/ISteamDirectory/GetCMListForConnect/v1/?cellid=0";

#[derive(Deserialize)]
struct Reply {
    response: List,
}

#[derive(Deserialize)]
struct List {
    serverlist: Vec<Server>,
}

#[derive(Deserialize)]
struct Server {
    endpoint: String,
    #[serde(rename = "type")]
    kind: String,
}

/// `host:port` of CMs that speak WebSocket, best first as Steam ranks them.
pub async fn websocket_endpoints(http: &reqwest::Client) -> Result<Vec<String>> {
    let reply: Reply = http
        .get(URL)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(reply
        .response
        .serverlist
        .into_iter()
        .filter(|s| s.kind == "websockets")
        .map(|s| s.endpoint)
        .collect())
}
