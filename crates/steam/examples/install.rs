//! Installs one depot of an app branch into a folder, anonymously:
//! `cargo run -p steam --release --example install -- 380870 public 1006 /tmp/out`.

use std::time::Instant;

#[tokio::main]
async fn main() -> steam::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [app, branch, depot, dir] = args.as_slice() else {
        eprintln!("usage: install <app> <branch> <depot> <dir>");
        std::process::exit(2);
    };
    let (app, depot): (u32, u32) = (
        app.parse().expect("app id"),
        depot.parse().expect("depot id"),
    );

    let http = steam::http();
    let mut client = steam::Client::anonymous(&http).await?;
    let info = steam::pics::app_info(&mut client, &http, app).await?;
    let manifest_id = info
        .map("depots")
        .and_then(|d| d.map(&depot.to_string()))
        .and_then(|d| d.map("manifests"))
        .and_then(|m| m.map(branch))
        .and_then(|m| m.str("gid"))
        .and_then(|gid| gid.parse().ok())
        .expect("depot has a manifest on that branch");

    let key = steam::content::depot_key(&mut client, app, depot).await?;
    let hosts = steam::content::cdn_hosts(&mut client).await?;
    let manifest = steam::content::manifest(
        &mut client,
        &http,
        &hosts[0],
        (app, depot, manifest_id),
        branch,
        &key,
    )
    .await?;
    let total: u64 = manifest.files.iter().map(|f| f.size).sum();

    let start = Instant::now();
    let timings = steam::content::install(
        &http,
        &hosts,
        (depot, &key),
        &manifest,
        std::path::Path::new(dir),
        None,
        |_| {},
    )
    .await?;
    let secs = start.elapsed().as_secs_f64();
    println!(
        "{} files, {:.1} MB in {secs:.1} s ({:.1} MB/s) from {}",
        manifest.files.len(),
        total as f64 / 1e6,
        total as f64 / 1e6 / secs,
        hosts.join(", ")
    );
    println!("{timings:?}");
    Ok(())
}
