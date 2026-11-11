//! PICS (product info): an app's KeyValues, and the game branches inside it.

use crate::cm::packet::emsg;
use crate::keyvalues::{self, Map};
use crate::proto::{
    CMsgClientPicsAccessTokenRequest, CMsgClientPicsAccessTokenResponse,
    CMsgClientPicsProductInfoRequest, CMsgClientPicsProductInfoResponse,
    c_msg_client_pics_product_info_request::AppInfo as AppRequest,
};
use crate::{Client, Error, Result};

/// appinfo is KBs; these caps only stop a broken or hostile server.
const COMPRESSED_CAP: usize = 8 << 20;
const TEXT_CAP: u64 = 32 << 20;

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
            // Path format from steamclient.so: /appinfo/%u/sha/%s.txt.gz, fetched over http there.
            // Try https first; `sha` is not a hash of the text, so it can't verify the body.
            // ponytail: untested path (PZ's appinfo currently arrives inline).
            let sha: String = app.sha().iter().map(|b| format!("{b:02x}")).collect();
            let path = format!("{}/appinfo/{appid}/sha/{sha}.txt.gz", response.http_host());
            let reply = match http.get(format!("https://{path}")).send().await {
                Ok(reply) => reply,
                Err(_) => http.get(format!("http://{path}")).send().await?,
            };
            let gz = crate::read_capped(reply, COMPRESSED_CAP, "appinfo download").await?;
            crate::gunzip(&gz, TEXT_CAP, "appinfo")?
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

/// A depot to install and the manifest it uses on the chosen branch.
#[derive(Debug, Clone, PartialEq)]
pub struct Depot {
    pub id: u32,
    pub manifest: u64,
    /// Install size in bytes, when appinfo has it.
    pub size: Option<u64>,
}

/// Depots that make up `branch` on `os` (`windows`, `linux` or `macos`): those with a manifest on
/// that branch whose `oslist` is empty or includes `os`. Depots borrowed from other apps
/// (redistributables) carry no manifest here and are skipped.
pub fn depots(appinfo: &Map, branch: &str, os: &str) -> Vec<Depot> {
    let Some(all) = appinfo.map("depots") else {
        return Vec::new();
    };
    all.iter()
        .filter_map(|(key, value)| {
            let id: u32 = key.parse().ok()?;
            let keyvalues::Value::Map(depot) = value else {
                return None;
            };
            let oslist = depot
                .map("config")
                .and_then(|c| c.str("oslist"))
                .unwrap_or("");
            if !oslist.is_empty() && !oslist.split(',').any(|o| o.trim() == os) {
                return None;
            }
            // Current appinfo nests {gid, size}; older entries are just the gid.
            let (manifest, size) = match depot.map("manifests")?.get(branch)? {
                keyvalues::Value::Map(m) => (m.str("gid")?, m.str("size")),
                keyvalues::Value::Str(gid) => (gid.as_str(), None),
            };
            Some(Depot {
                id,
                manifest: manifest.parse().ok()?,
                size: size.and_then(|s| s.parse().ok()),
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

    #[test]
    fn picks_depots_for_branch_and_os() {
        // Shape of PZ's appinfo (app 108600): per-OS game depots, a manifest-less depot and
        // Windows redistributables borrowed from another app.
        let text = r#"
            "appinfo" { "depots" {
                "108601" { "systemdefined" "1" }
                "108603" { "config" { "oslist" "linux" } "manifests" {
                    "public" { "gid" "111" "size" "4000" }
                    "legacy41" { "gid" "222" "size" "3000" } } }
                "108604" { "config" { "oslist" "windows" } "manifests" { "public" { "gid" "333" } } }
                "228983" { "config" { "oslist" "windows" } "depotfromapp" "228980" }
                "380871" { "manifests" { "public" "444" } }
                "branches" { "public" { "buildid" "1" } }
            } }
        "#;
        let info = keyvalues::parse(text)
            .unwrap()
            .map("appinfo")
            .unwrap()
            .clone();

        let linux: Vec<_> = depots(&info, "public", "linux")
            .iter()
            .map(|d| (d.id, d.manifest))
            .collect();
        assert_eq!(linux, [(108603, 111), (380871, 444)]);
        let old = depots(&info, "legacy41", "linux");
        assert_eq!(
            old,
            [Depot {
                id: 108603,
                manifest: 222,
                size: Some(3000)
            }]
        );
        let windows: Vec<_> = depots(&info, "public", "windows")
            .iter()
            .map(|d| d.id)
            .collect();
        assert_eq!(windows, [108604, 380871]);
    }
}
