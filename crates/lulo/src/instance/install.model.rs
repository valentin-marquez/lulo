//! Installing an instance's branch: depots for this OS, downloaded and verified into `game/`.

use steam::{content, pics};
use tokio::sync::mpsc::UnboundedSender;

use super::{APP, Instance, store};
use crate::account::Account;
use crate::runtime;

pub enum Event {
    Progress {
        done: u64,
        total: u64,
    },
    /// The build is recorded in the instance; reload it from disk.
    Done,
    Failed(String),
}

/// The OS name appinfo uses in `oslist`.
fn os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        _ => "linux",
    }
}

/// Runs the install on the background runtime, reporting through `events`. Aborting the
/// returned handle cancels it; the files written so far are kept for the next install to reuse.
pub fn start(
    account: Account,
    instance: Instance,
    events: UnboundedSender<Event>,
) -> tokio::task::AbortHandle {
    runtime::spawn(async move {
        let event = match install(&account, &instance, &events).await {
            Ok(build) => match store::set_build(&instance.dir, build) {
                Ok(()) => Event::Done,
                Err(err) => Event::Failed(format!("installed, but couldn't record it: {err}")),
            },
            Err(err) => Event::Failed(err),
        };
        let _ = events.send(event);
    })
    .abort_handle()
}

async fn install(
    account: &Account,
    instance: &Instance,
    events: &UnboundedSender<Event>,
) -> Result<u32, String> {
    let http = steam::http();
    let mut client = account.client(&http).await.map_err(|e| e.to_string())?;
    let info = pics::app_info(&mut client, &http, APP)
        .await
        .map_err(|e| e.to_string())?;
    let branch = instance.branch.as_ref();
    let build = pics::branches(&info)
        .into_iter()
        .find(|b| b.name == branch)
        .ok_or_else(|| format!("Steam no longer has the branch {branch}"))?
        .build_id;
    let depots = pics::depots(&info, branch, os());
    if depots.is_empty() {
        return Err(format!("The {branch} branch has no files for {}", os()));
    }

    // Keys and manifests need the CM session; the files themselves come from the CDN.
    let host = content::cdn_host(&mut client)
        .await
        .map_err(|e| e.to_string())?;
    let mut plan = Vec::new();
    for depot in depots {
        let key = content::depot_key(&mut client, APP, depot.id)
            .await
            .map_err(|err| match err {
                steam::Error::Steam { code: 15, .. } => {
                    "This Steam account doesn't own Project Zomboid".to_owned()
                }
                other => other.to_string(),
            })?;
        let manifest = content::manifest(
            &mut client,
            &http,
            &host,
            (APP, depot.id, depot.manifest),
            branch,
            &key,
        )
        .await
        .map_err(|e| e.to_string())?;
        plan.push((depot.id, key, manifest));
    }
    drop(client);

    let total: u64 = plan
        .iter()
        .flat_map(|(_, _, m)| &m.files)
        .map(|f| f.size)
        .sum();
    let game = instance.dir.join("game");
    let mut before = 0;
    for (depot, key, manifest) in &plan {
        content::install(&http, &host, *depot, key, manifest, &game, |done| {
            let _ = events.send(Event::Progress {
                done: before + done,
                total,
            });
        })
        .await
        .map_err(|e| e.to_string())?;
        before += manifest.files.iter().map(|f| f.size).sum::<u64>();
    }
    Ok(build)
}
