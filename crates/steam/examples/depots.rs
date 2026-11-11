//! Lists the depots of an app branch for an OS: `cargo run -p steam --example depots -- 380870 public linux`.

#[tokio::main]
async fn main() -> steam::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let app: u32 = args.first().and_then(|a| a.parse().ok()).unwrap_or(108600);
    let branch = args.get(1).map_or("public", String::as_str);
    let os = args.get(2).map_or("linux", String::as_str);

    let http = steam::http();
    let mut client = steam::Client::anonymous(&http).await?;
    let info = steam::pics::app_info(&mut client, &http, app).await?;
    for depot in steam::pics::depots(&info, branch, os) {
        let mb = depot.size.unwrap_or(0) as f64 / 1e6;
        println!(
            "{:>8}  manifest {:>20}  {mb:>9.1} MB",
            depot.id, depot.manifest
        );
    }
    Ok(())
}
