//! Talks to the real Steam network. Run with `cargo test -p steam -- --ignored`.

const PROJECT_ZOMBOID: u32 = 108600;

#[tokio::test]
#[ignore = "needs network access to Steam"]
async fn lists_project_zomboid_branches_anonymously() {
    let http = steam::http();
    let mut client = steam::Client::anonymous(&http)
        .await
        .expect("anonymous logon");
    let info = steam::pics::app_info(&mut client, &http, PROJECT_ZOMBOID)
        .await
        .expect("app info");
    let branches = steam::pics::branches(&info);
    assert!(branches.iter().any(|b| b.name == "public"), "{branches:?}");
}

/// Steam drops a session that stays silent for `heartbeat_seconds` (about 9), so a request
/// after several intervals only works if the client kept beating.
#[tokio::test]
#[ignore = "needs network access to Steam"]
async fn session_survives_past_the_heartbeat_interval() {
    let http = steam::http();
    let mut client = steam::Client::anonymous(&http)
        .await
        .expect("anonymous logon");
    tokio::time::sleep(std::time::Duration::from_secs(150)).await;
    let info = steam::pics::app_info(&mut client, &http, PROJECT_ZOMBOID)
        .await
        .expect("session still open after 150 s");
    assert!(!steam::pics::branches(&info).is_empty());
}

/// A small PZ mod. Set `LULO_STEAMCMD_MOD` to the folder SteamCMD downloaded for it to also
/// compare every file byte for byte.
const MOD: u64 = 3832274571;

#[tokio::test]
#[ignore = "needs network access to Steam"]
async fn downloads_a_workshop_mod_like_steamcmd() {
    let http = steam::http();
    let mut client = steam::Client::anonymous(&http)
        .await
        .expect("anonymous logon");
    let item = steam::workshop::details(&mut client, &[MOD])
        .await
        .expect("details")
        .remove(0);
    assert_eq!(item.app, PROJECT_ZOMBOID);

    let dir = std::env::temp_dir().join(format!("lulo-live-{}", std::process::id()));
    steam::workshop::download(&mut client, &http, &item, &dir, |_| {})
        .await
        .expect("download");

    if let Ok(reference) = std::env::var("LULO_STEAMCMD_MOD") {
        let mut compared = 0;
        let mut stack = vec![std::path::PathBuf::from(&reference)];
        while let Some(path) = stack.pop() {
            for entry in std::fs::read_dir(&path).unwrap() {
                let entry = entry.unwrap().path();
                if entry.is_dir() {
                    stack.push(entry);
                    continue;
                }
                let ours = dir.join(entry.strip_prefix(&reference).unwrap());
                assert_eq!(
                    std::fs::read(&ours).unwrap(),
                    std::fs::read(&entry).unwrap(),
                    "{ours:?}"
                );
                compared += 1;
            }
        }
        assert!(compared > 0, "reference folder was empty");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

/// QR login can be started anonymously; Steam answers with a challenge URL and a pending poll.
#[tokio::test]
#[ignore = "needs network access to Steam"]
async fn starts_a_qr_login() {
    let http = steam::http();
    let mut client = steam::Client::connect(&http).await.expect("connect");
    let mut session = steam::auth::qr(&mut client).await.expect("begin QR login");
    let url = session.challenge_url.clone().unwrap_or_default();
    assert!(url.starts_with("https://s.team/q/"), "{url}");
    assert!(matches!(
        steam::auth::poll(&mut client, &mut session)
            .await
            .expect("poll"),
        steam::auth::Poll::Pending | steam::auth::Poll::Rotated(_)
    ));
}

/// A public Project Zomboid collection (82 mods when this was written; if its author removes
/// it, any other public one works), and a plain mod, which is not one.
const COLLECTION: u64 = 3834236342;
const PLAIN_MOD: u64 = 3832274571;

#[tokio::test]
#[ignore = "needs network access to Steam"]
async fn resolves_a_workshop_collection_into_its_mods() {
    let http = steam::http();
    let mut client = steam::Client::anonymous(&http)
        .await
        .expect("anonymous logon");
    let collection = steam::workshop::collection(&mut client, COLLECTION)
        .await
        .expect("a public collection resolves");
    eprintln!("{}: {} mods", collection.title, collection.mods.len());
    assert!(!collection.title.is_empty());
    assert!(!collection.mods.is_empty(), "{collection:?}");
    let mut ids: Vec<_> = collection.mods.iter().map(|m| m.id).collect();
    ids.sort_unstable();
    let count = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), count, "every mod is listed once");
    assert!(
        collection
            .mods
            .iter()
            .all(|m| m.app == PROJECT_ZOMBOID && !m.title.is_empty() && m.manifest != 0),
        "mods have what a download needs"
    );

    let not = steam::workshop::collection(&mut client, PLAIN_MOD).await;
    assert!(
        matches!(not, Err(steam::Error::NotCollection)),
        "a mod is not a collection: {not:?}"
    );
}

/// A made-up account still gets an RSA key, and a wrong password is refused with EResult 5.
#[tokio::test]
#[ignore = "needs network access to Steam"]
async fn refuses_a_made_up_password_login() {
    let http = steam::http();
    let mut client = steam::Client::connect(&http).await.expect("connect");
    let result = steam::auth::password(
        &mut client,
        "lulo-made-up-account-0001",
        "not-the-password",
        None,
    )
    .await;
    assert!(
        matches!(result, Err(steam::Error::Steam { code: 5, .. })),
        "{result:?}"
    );
}

/// The news and the trending mods the app's side panel shows.
#[tokio::test]
#[ignore = "needs network access to Steam"]
async fn reads_the_news_and_the_trending_mods() {
    let http = steam::http();
    let posts = steam::news::announcements(&http, PROJECT_ZOMBOID, 5)
        .await
        .expect("the news loads");
    eprintln!(
        "{} posts, first: {:?}",
        posts.len(),
        posts.first().map(|p| &p.title)
    );
    assert!(!posts.is_empty());
    assert!(
        posts
            .iter()
            .all(|p| p.url.starts_with("https://") && p.date > 0)
    );
    assert!(
        posts.windows(2).all(|pair| pair[0].date >= pair[1].date),
        "newest first"
    );

    let mut client = steam::Client::anonymous(&http)
        .await
        .expect("anonymous logon");
    let mods = steam::workshop::trending(&mut client, PROJECT_ZOMBOID, 7, 6)
        .await
        .expect("the trending mods load");
    for item in &mods {
        eprintln!(
            "{} {} subs={} {:?}",
            item.id, item.title, item.subscriptions, item.preview
        );
    }
    assert!(!mods.is_empty());
    assert!(
        mods.iter()
            .all(|m| m.app == PROJECT_ZOMBOID && !m.title.is_empty() && m.manifest != 0)
    );
}
