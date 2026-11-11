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
    steam::workshop::download(&mut client, &http, &item, &dir)
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
