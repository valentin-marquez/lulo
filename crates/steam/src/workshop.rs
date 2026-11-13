//! Steam Workshop items. A PZ mod's files live in the game's own depot (108600), under the
//! manifest named by `hcontent_file`.

use std::collections::HashSet;
use std::path::Path;

use crate::proto::{
    CPublishedFileGetDetailsRequest, CPublishedFileGetDetailsResponse, PublishedFileDetails,
};
use crate::{Client, Error, Result, content};

/// `EWorkshopFileType::Collection`.
const COLLECTION: u32 = 2;
/// Items asked for in one request.
const BATCH: usize = 50;
/// A collection with its dependencies can't go past this many items.
const MAX_ITEMS: usize = 2000;
/// `EResult::OK`.
const OK: u32 = 1;

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

impl From<PublishedFileDetails> for Item {
    fn from(d: PublishedFileDetails) -> Self {
        Self {
            id: d.publishedfileid(),
            app: d.consumer_appid(),
            title: d.title().to_owned(),
            manifest: d.hcontent_file(),
            size: d.file_size(),
            updated: d.time_updated,
            // Last: it moves out of `d`.
            file_url: d.file_url.filter(|u| !u.is_empty()),
        }
    }
}

async fn fetch(
    client: &mut Client,
    ids: &[u64],
    children: bool,
) -> Result<Vec<PublishedFileDetails>> {
    let request = CPublishedFileGetDetailsRequest {
        publishedfileids: ids.to_vec(),
        includechildren: children.then_some(true),
    };
    let reply: CPublishedFileGetDetailsResponse =
        client.call("PublishedFile.GetDetails", &request).await?;
    Ok(reply.publishedfiledetails)
}

pub async fn details(client: &mut Client, ids: &[u64]) -> Result<Vec<Item>> {
    Ok(fetch(client, ids, false)
        .await?
        .into_iter()
        .map(Item::from)
        .collect())
}

/// A collection's mods, with the ones they require.
#[derive(Debug)]
pub struct Collection {
    pub id: u64,
    pub title: String,
    /// Every mod once, in the collection's order and then the order dependencies were found.
    /// Items that are gone or private are left out.
    pub mods: Vec<Item>,
}

/// The mods of collection `id`: the ones it lists, the collections inside it, and what each
/// mod requires (`children`), each once.
pub async fn collection(client: &mut Client, id: u64) -> Result<Collection> {
    let root = fetch(client, &[id], true)
        .await?
        .into_iter()
        .next()
        .filter(|d| d.result() == OK && d.file_type() == COLLECTION)
        .ok_or(Error::NotCollection)?;
    let mut seen = HashSet::from([id]);
    let mut queue = new_children(&root, &mut seen);
    let mut mods = Vec::new();
    while !queue.is_empty() {
        let take = queue.len().min(BATCH);
        let batch: Vec<u64> = queue.drain(..take).collect();
        for item in fetch(client, &batch, true).await? {
            queue.extend(new_children(&item, &mut seen));
            if item.result() == OK && item.file_type() != COLLECTION {
                mods.push(Item::from(item));
            }
        }
        if seen.len() > MAX_ITEMS {
            return Err(Error::TooLarge("collection"));
        }
    }
    Ok(Collection {
        id,
        title: root.title().to_owned(),
        mods,
    })
}

/// The ids `item` lists that haven't been seen yet, by their sort order; marks them seen.
fn new_children(item: &PublishedFileDetails, seen: &mut HashSet<u64>) -> Vec<u64> {
    let mut children: Vec<_> = item.children.iter().collect();
    children.sort_by_key(|child| child.sortorder());
    children
        .into_iter()
        .map(|child| child.publishedfileid())
        .filter(|id| seen.insert(*id))
        .collect()
}

/// The item id in what a user pastes: a Workshop or Steam link, or the number itself.
pub fn parse_id(text: &str) -> Option<u64> {
    let text = text.trim();
    if let Ok(id) = text.parse() {
        return Some(id);
    }
    if let Some(id) = text.strip_prefix("steam://url/CommunityFilePage/") {
        return id.trim_end_matches('/').parse().ok();
    }
    let (host, path) = text
        .strip_prefix("https://")
        .or_else(|| text.strip_prefix("http://"))?
        .split_once('/')?;
    let (path, query) = path.split_once('?')?;
    let page = path.trim_end_matches('/');
    let ours = matches!(host, "steamcommunity.com" | "www.steamcommunity.com")
        && matches!(page, "sharedfiles/filedetails" | "workshop/filedetails");
    if !ours {
        return None;
    }
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("id=")?.parse().ok())
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
    let hosts = content::cdn_hosts(client).await?;
    let manifest = content::manifest(
        client,
        http,
        &hosts[0],
        (item.app, item.app, item.manifest),
        "public",
        &key,
    )
    .await?;
    content::install(http, &hosts, (item.app, &key), &manifest, dir, None, |_| {})
        .await
        .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::PublishedFileChild;

    #[test]
    fn finds_the_id_in_what_people_paste() {
        let id = Some(3832274571);
        assert_eq!(parse_id("3832274571"), id);
        assert_eq!(parse_id("  3832274571\n"), id);
        for link in [
            "https://steamcommunity.com/sharedfiles/filedetails/?id=3832274571",
            "https://steamcommunity.com/sharedfiles/filedetails?id=3832274571",
            "https://www.steamcommunity.com/workshop/filedetails/?id=3832274571&searchtext=x",
            "http://steamcommunity.com/sharedfiles/filedetails/?l=spanish&id=3832274571",
            "steam://url/CommunityFilePage/3832274571",
        ] {
            assert_eq!(parse_id(link), id, "{link}");
        }
        for not in [
            "",
            "mods",
            "https://evil.example/sharedfiles/filedetails/?id=3832274571",
            "https://steamcommunity.com/id/someone/",
            "https://steamcommunity.com/sharedfiles/filedetails/?id=abc",
            "https://steamcommunity.com/sharedfiles/filedetails/",
        ] {
            assert_eq!(parse_id(not), None, "{not}");
        }
    }

    #[test]
    fn children_come_in_sort_order_and_only_once() {
        let child = |id, sortorder| PublishedFileChild {
            publishedfileid: Some(id),
            sortorder: Some(sortorder),
        };
        let item = PublishedFileDetails {
            children: vec![child(30, 2), child(10, 0), child(20, 1), child(10, 3)],
            ..Default::default()
        };
        let mut seen = HashSet::from([20]);
        assert_eq!(new_children(&item, &mut seen), [10, 30]);
        // A second pass over the same item finds nothing new, so shared dependencies don't loop.
        assert!(new_children(&item, &mut seen).is_empty());
    }
}
