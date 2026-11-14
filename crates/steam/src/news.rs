//! A game's news on Steam: the announcements and patch notes its developers post. Read from
//! the public web API, which needs no session.

use std::time::Duration;

use serde::Deserialize;

use crate::Result;

/// The feed of a game's own announcements, apart from the press articles Steam mixes in.
const ANNOUNCEMENTS: &str = "steam_community_announcements";
/// Characters of each post's text Steam is asked for; the rest is on its page.
const SUMMARY: u32 = 180;

/// One post in a game's news.
#[derive(Debug, Clone, PartialEq)]
pub struct Post {
    pub title: String,
    /// Its page; always `https`.
    pub url: String,
    /// When it was posted, in seconds since the epoch.
    pub date: u64,
    pub author: String,
    /// The start of its text, plain.
    pub summary: String,
}

#[derive(Deserialize)]
struct Reply {
    appnews: AppNews,
}

#[derive(Deserialize)]
struct AppNews {
    #[serde(default)]
    newsitems: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    title: String,
    url: String,
    #[serde(default)]
    date: u64,
    #[serde(default)]
    author: String,
    #[serde(default)]
    contents: String,
}

/// The newest `count` announcements of `app`, newest first.
pub async fn announcements(http: &reqwest::Client, app: u32, count: u32) -> Result<Vec<Post>> {
    let url = format!(
        "https://api.steampowered.com/ISteamNews/GetNewsForApp/v2/?appid={app}&count={count}\
         &maxlength={SUMMARY}&feeds={ANNOUNCEMENTS}&format=json"
    );
    let reply = http
        .get(url)
        .timeout(Duration::from_secs(10))
        .send()
        .await?;
    let body = crate::read_capped(reply, 1 << 20, "news").await?;
    parse(&body)
}

fn parse(body: &[u8]) -> Result<Vec<Post>> {
    let reply: Reply =
        serde_json::from_slice(body).map_err(|_| crate::Error::Packet("news reply"))?;
    Ok(reply
        .appnews
        .newsitems
        .into_iter()
        // The link is opened in the user's browser: only a web page, never another scheme.
        .filter(|item| item.url.starts_with("https://") && !item.title.trim().is_empty())
        .map(|item| Post {
            title: item.title.trim().to_owned(),
            url: item.url,
            date: item.date,
            author: item.author,
            summary: item
                .contents
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    // From ISteamNews/GetNewsForApp for app 108600 on 2026-11-13, two posts kept and their
    // text shortened; the second is given a link that must not be opened.
    const REPLY: &str = r#"{"appnews":{"appid":108600,"newsitems":[
        {"gid":"1848664643355564","title":"42.22.2 STABLE Released","url":"https://steamstore-a.akamaihd.net/news/externalpost/steam_community_announcements/1848664643355564","is_external_url":true,"author":"Higgs","contents":"Updating your game client and previous,\n unmodded saves","feedlabel":"Community Announcements","date":1794233055,"feedname":"steam_community_announcements","feed_type":1,"appid":108600,"tags":["mod_reviewed"]},
        {"gid":"1","title":"Not a web page","url":"steam://run/1","date":1,"feedname":"steam_community_announcements","feed_type":1,"appid":108600},
        {"gid":"1847286114028340","title":" A Note on Java Mods and Modding ","url":"https://steamstore-a.akamaihd.net/news/externalpost/steam_community_announcements/1847286114028340","is_external_url":true,"author":"AmazingSully","contents":"The modding community","feedlabel":"Community Announcements","date":1792578061,"feedname":"steam_community_announcements","feed_type":1,"appid":108600}
    ],"count":681}}"#;

    #[test]
    fn reads_the_posts_and_drops_links_that_are_not_pages() {
        let posts = parse(REPLY.as_bytes()).unwrap();
        assert_eq!(posts.len(), 2);
        assert_eq!(posts[0].title, "42.22.2 STABLE Released");
        assert_eq!(
            (posts[0].date, posts[0].author.as_str()),
            (1794233055, "Higgs")
        );
        assert_eq!(
            posts[0].summary,
            "Updating your game client and previous, unmodded saves"
        );
        assert_eq!(posts[1].title, "A Note on Java Mods and Modding");
        assert!(parse(br#"{"appnews":{"appid":1}}"#).unwrap().is_empty());
        assert!(parse(b"<html>").is_err());
    }
}
