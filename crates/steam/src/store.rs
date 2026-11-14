//! A game's page on the Steam store, read from the public web API: its screenshots.

use std::time::Duration;

use serde::Deserialize;

use crate::Result;

#[derive(Deserialize)]
struct Entry {
    #[serde(default)]
    data: Option<Data>,
}

#[derive(Deserialize)]
struct Data {
    #[serde(default)]
    screenshots: Vec<Screenshot>,
}

#[derive(Deserialize)]
struct Screenshot {
    path_full: String,
}

/// URLs of the full-size screenshots on `app`'s store page, in the page's order.
pub async fn screenshots(http: &reqwest::Client, app: u32) -> Result<Vec<String>> {
    let reply = http
        .get(format!(
            "https://store.steampowered.com/api/appdetails?appids={app}&filters=screenshots"
        ))
        .timeout(Duration::from_secs(10))
        .send()
        .await?;
    let body = crate::read_capped(reply, 1 << 20, "store page").await?;
    parse(&body, app)
}

fn parse(body: &[u8], app: u32) -> Result<Vec<String>> {
    let mut reply: std::collections::HashMap<String, Entry> =
        serde_json::from_slice(body).map_err(|_| crate::Error::Packet("store reply"))?;
    let data = reply.remove(&app.to_string()).and_then(|entry| entry.data);
    Ok(data
        .map(|data| data.screenshots)
        .unwrap_or_default()
        .into_iter()
        .map(|shot| shot.path_full)
        // Fetched and shown by the app: only pictures on Steam's own image hosts.
        .filter(|url| is_steam_image(url))
        .collect())
}

/// An `https` URL on Steam's static hosts, with nothing odd in the host part.
fn is_steam_image(url: &str) -> bool {
    let Some(host) = url
        .strip_prefix("https://")
        .and_then(|rest| rest.split('/').next())
    else {
        return false;
    };
    host.ends_with(".steamstatic.com")
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    // From store.steampowered.com/api/appdetails for app 108600 on 2026-11-13: two of its
    // screenshots, and one given a host that is not Steam's.
    const REPLY: &str = r#"{"108600":{"success":true,"data":{"screenshots":[
        {"id":0,"path_thumbnail":"https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/108600/e0dd/ss_e0dd.600x338.jpg?t=1794338497","path_full":"https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/108600/e0dd/ss_e0dd.1920x1080.jpg?t=1794338497"},
        {"id":1,"path_thumbnail":"https://x","path_full":"https://steamstatic.com.evil.example/ss.jpg"},
        {"id":2,"path_thumbnail":"https://x","path_full":"https://shared.akamai.steamstatic.com/store_item_assets/steam/apps/108600/ss_4c88.1920x1080.jpg?t=1794338497"}
    ]}}}"#;

    #[test]
    fn lists_the_screenshots_on_steam_hosts() {
        let shots = parse(REPLY.as_bytes(), 108600).unwrap();
        assert_eq!(shots.len(), 2);
        assert!(shots[0].contains("ss_e0dd.1920x1080.jpg"));
        assert!(shots[1].contains("ss_4c88.1920x1080.jpg"));
        // Another app's entry, a failed lookup, or junk.
        assert!(parse(REPLY.as_bytes(), 220).unwrap().is_empty());
        assert!(
            parse(br#"{"108600":{"success":false}}"#, 108600)
                .unwrap()
                .is_empty()
        );
        assert!(parse(b"<html>", 108600).is_err());
    }
}
