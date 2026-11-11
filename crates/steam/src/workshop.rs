//! Steam Workshop items. A PZ mod's files live in the game's own depot (108600), under the
//! manifest named by `hcontent_file`.

use std::path::Path;

use crate::proto::{CPublishedFileGetDetailsRequest, CPublishedFileGetDetailsResponse};
use crate::{Client, Result, content};

#[derive(Debug, Clone)]
pub struct Item {
    pub id: u64,
    pub app: u32,
    pub title: String,
    /// Manifest of the current version in the app's depot.
    pub manifest: u64,
    /// Legacy items are a direct download instead of a manifest.
    pub file_url: Option<String>,
    pub size: u64,
    pub updated: Option<u32>,
}

pub async fn details(client: &mut Client, ids: &[u64]) -> Result<Vec<Item>> {
    let request = CPublishedFileGetDetailsRequest {
        publishedfileids: ids.to_vec(),
        ..Default::default()
    };
    let reply: CPublishedFileGetDetailsResponse =
        client.call("PublishedFile.GetDetails", &request).await?;
    Ok(reply
        .publishedfiledetails
        .into_iter()
        .map(|d| Item {
            id: d.publishedfileid(),
            app: d.consumer_appid(),
            title: d.title().to_owned(),
            manifest: d.hcontent_file(),
            size: d.file_size(),
            updated: d.time_updated,
            // Last: it moves out of `d`.
            file_url: d.file_url.filter(|u| !u.is_empty()),
        })
        .collect())
}

/// Downloads `item`'s current version into `dir`.
// ponytail: legacy `file_url` items aren't handled; none of the PZ mods tested use them.
pub async fn download(
    client: &mut Client,
    http: &reqwest::Client,
    item: &Item,
    dir: &Path,
) -> Result<()> {
    let key = content::depot_key(client, item.app, item.app).await?;
    let host = content::cdn_host(client).await?;
    let manifest = content::manifest(
        client,
        http,
        &host,
        (item.app, item.app, item.manifest),
        "public",
        &key,
    )
    .await?;
    content::install(http, &host, item.app, &key, &manifest, dir, |_| {}).await
}
