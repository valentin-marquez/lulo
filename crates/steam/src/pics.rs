//! PICS (product info): an app's KeyValues, and the game branches inside it.

use std::io::Read;

use flate2::read::GzDecoder;

use crate::cm::packet::emsg;
use crate::keyvalues::{self, Map};
use crate::proto::{
    CMsgClientPicsAccessTokenRequest, CMsgClientPicsAccessTokenResponse,
    CMsgClientPicsProductInfoRequest, CMsgClientPicsProductInfoResponse,
    c_msg_client_pics_product_info_request::AppInfo as AppRequest,
};
use crate::{Client, Error, Result};

/// Fetches the `appinfo` KeyValues of `appid`. Large entries come over HTTP instead of inline.
pub async fn app_info(client: &mut Client, http: &reqwest::Client, appid: u32) -> Result<Map> {
    let tokens: CMsgClientPicsAccessTokenResponse = client
        .request(
            emsg::CLIENT_PICS_ACCESS_TOKEN_REQUEST,
            emsg::CLIENT_PICS_ACCESS_TOKEN_RESPONSE,
            &CMsgClientPicsAccessTokenRequest {
                appids: vec![appid],
                ..Default::default()
            },
        )
        .await?;
    let token = tokens
        .app_access_tokens
        .iter()
        .find(|t| t.appid == Some(appid))
        .and_then(|t| t.access_token);

    let request = CMsgClientPicsProductInfoRequest {
        apps: vec![AppRequest {
            appid: Some(appid),
            access_token: token,
            ..Default::default()
        }],
        single_response: Some(true),
        ..Default::default()
    };
    let response: CMsgClientPicsProductInfoResponse = client
        .request_until(
            emsg::CLIENT_PICS_PRODUCT_INFO_REQUEST,
            emsg::CLIENT_PICS_PRODUCT_INFO_RESPONSE,
            &request,
            |r: &CMsgClientPicsProductInfoResponse| !r.apps.is_empty() || !r.response_pending(),
        )
        .await?;

    let app = response.apps.first().ok_or_else(|| Error::Steam {
        code: 9, // EResult::FileNotFound
        context: format!("product info for app {appid}"),
    })?;
    let text = match &app.buffer {
        Some(buffer) if !buffer.is_empty() => buffer.clone(),
        _ => {
            // Path format from steamclient.so: /appinfo/%u/sha/%s.txt.gz
            let sha: String = app.sha().iter().map(|b| format!("{b:02x}")).collect();
            let url = format!(
                "http://{}/appinfo/{appid}/sha/{sha}.txt.gz",
                response.http_host()
            );
            let gz = http
                .get(url)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?;
            let mut out = Vec::new();
            GzDecoder::new(gz.as_ref()).read_to_end(&mut out)?;
            out
        }
    };

    let root = keyvalues::parse(&String::from_utf8_lossy(&text))?;
    root.map("appinfo")
        .cloned()
        .ok_or(Error::Packet("appinfo root missing"))
}

/// A Steam branch of an app. For PZ the branch is the game version.
#[derive(Debug, Clone, PartialEq)]
pub struct Branch {
    pub name: String,
    pub build_id: u32,
    pub description: Option<String>,
    pub password_required: bool,
    /// Unix time of the branch's current build.
    pub updated: Option<u64>,
}

/// Branches listed under `depots.branches`, in appinfo order.
pub fn branches(appinfo: &Map) -> Vec<Branch> {
    let Some(list) = appinfo.map("depots").and_then(|d| d.map("branches")) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|(name, value)| {
            let keyvalues::Value::Map(b) = value else {
                return None;
            };
            Some(Branch {
                name: name.to_owned(),
                build_id: b.str("buildid")?.parse().ok()?,
                description: b.str("description").map(str::to_owned),
                password_required: b.str("pwdrequired") == Some("1"),
                updated: b.str("timebuildupdated").and_then(|t| t.parse().ok()),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_branches_from_appinfo() {
        // Trimmed from PZ's real appinfo (app 108600, 2026-11-10).
        let text = r#"
            "appinfo" { "depots" {
                "108603" { "config" { "oslist" "linux" } }
                "branches" {
                    "public" { "buildid" "26486631" "timebuildupdated" "1793974760" }
                    "legacy41" { "buildid" "24928713" "description" "Build 41.78.21" "timebuildupdated" "1787663102" }
                    "secret" { "buildid" "1" "pwdrequired" "1" }
                    "broken" { "description" "no build id" }
                }
            } }
        "#;
        let info = keyvalues::parse(text)
            .unwrap()
            .map("appinfo")
            .unwrap()
            .clone();
        let branches = branches(&info);

        let names: Vec<_> = branches.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["public", "legacy41", "secret"]);
        assert_eq!(branches[1].build_id, 24928713);
        assert_eq!(branches[1].description.as_deref(), Some("Build 41.78.21"));
        assert_eq!(branches[0].updated, Some(1793974760));
        assert!(branches[2].password_required);
    }
}
