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
