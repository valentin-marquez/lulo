//! A user's public profile: the name and picture Steam shows today (the login name never changes).

use std::time::Duration;

use crate::Result;

/// The picture Steam gives everyone who never set one.
const DEFAULT_AVATAR: &str = "fef49e7fa7e1997310d705b2a6158ff8dc1cdfeb";
const AVATAR_CAP: usize = 1 << 20;
const TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Default, PartialEq)]
pub struct Profile {
    /// The display name; `None` when the profile is private or has none.
    pub name: Option<String>,
    /// URL of the 64 px picture; `None` when the user has none of their own.
    pub avatar: Option<String>,
}

pub async fn profile(http: &reqwest::Client, steamid: u64) -> Result<Profile> {
    let xml = http
        .get(format!(
            "https://steamcommunity.com/profiles/{steamid}/?xml=1"
        ))
        .timeout(TIMEOUT)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(parse(&xml))
}

/// The picture at `url`, one of [`Profile::avatar`].
pub async fn avatar(http: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let reply = http.get(url).timeout(TIMEOUT).send().await?;
    crate::read_capped(reply, AVATAR_CAP, "avatar").await
}

fn parse(xml: &str) -> Profile {
    Profile {
        name: cdata(xml, "steamID").map(str::to_owned),
        avatar: cdata(xml, "avatarMedium")
            .filter(|url| is_own_avatar(url))
            .map(str::to_owned),
    }
}

fn cdata<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let rest = xml.split_once(&format!("<{tag}>"))?.1;
    let text = rest.split_once(&format!("</{tag}>"))?.0;
    let text = text.strip_prefix("<![CDATA[")?.strip_suffix("]]>")?.trim();
    (!text.is_empty()).then_some(text)
}

/// Steam's avatar CDN, a plain file name, and not the picture everyone starts with.
fn is_own_avatar(url: &str) -> bool {
    let Some((host, file)) = url
        .strip_prefix("https://avatars.")
        .and_then(|rest| rest.split_once('/'))
    else {
        return false;
    };
    host.ends_with("steamstatic.com")
        && !file.contains(DEFAULT_AVATAR)
        && !file.is_empty()
        && file
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PICTURE: &str = "https://avatars.fastly.steamstatic.com/c5d56249ee5d28a07db4ac9f7f60af961fab5426_medium.jpg";

    #[test]
    fn reads_the_current_name_and_picture() {
        let xml = format!(
            "<profile><steamID><![CDATA[ignaciox.EXEcute]]></steamID><avatarMedium><![CDATA[{PICTURE}]]></avatarMedium></profile>"
        );
        assert_eq!(
            parse(&xml),
            Profile {
                name: Some("ignaciox.EXEcute".into()),
                avatar: Some(PICTURE.into()),
            }
        );
        assert_eq!(
            parse("<response><error>not found</error></response>"),
            Profile::default()
        );
        assert_eq!(parse("<steamID><![CDATA[]]></steamID>").name, None);
    }

    #[test]
    fn only_a_picture_of_their_own_counts() {
        let default = format!("https://avatars.fastly.steamstatic.com/{DEFAULT_AVATAR}_medium.jpg");
        assert!(is_own_avatar(PICTURE));
        assert!(!is_own_avatar(&default));
        for bad in [
            "http://avatars.fastly.steamstatic.com/a_medium.jpg",
            "https://avatars.example.com/a_medium.jpg",
            "https://avatars.fastly.steamstatic.com/../a.jpg",
            "https://avatars.fastly.steamstatic.com/",
            "https://evil.com/avatars.steamstatic.com/a.jpg",
        ] {
            assert!(!is_own_avatar(bad), "{bad}");
        }
    }
}
