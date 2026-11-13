//! A user's public profile, for the name Steam shows today (the login name never changes).

use std::time::Duration;

use crate::Result;

/// The profile's display name, or `None` when the profile is private or has none.
pub async fn persona_name(http: &reqwest::Client, steamid: u64) -> Result<Option<String>> {
    let xml = http
        .get(format!(
            "https://steamcommunity.com/profiles/{steamid}/?xml=1"
        ))
        .timeout(Duration::from_secs(8))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(parse_name(&xml))
}

fn parse_name(xml: &str) -> Option<String> {
    let rest = xml.split_once("<steamID>")?.1;
    let name = rest.split_once("</steamID>")?.0;
    let name = name.strip_prefix("<![CDATA[")?.strip_suffix("]]>")?.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::parse_name;

    #[test]
    fn reads_the_current_name() {
        let xml = "<profile><steamID64>1</steamID64><steamID><![CDATA[ignaciox.EXEcute]]></steamID></profile>";
        assert_eq!(parse_name(xml).as_deref(), Some("ignaciox.EXEcute"));
        assert_eq!(
            parse_name("<response><error>not found</error></response>"),
            None
        );
        assert_eq!(parse_name("<steamID><![CDATA[]]></steamID>"), None);
    }
}
