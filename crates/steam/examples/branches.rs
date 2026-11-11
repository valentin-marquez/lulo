//! Prints Project Zomboid's branches straight from Steam: `cargo run -p steam --example branches`.

const PROJECT_ZOMBOID: u32 = 108600;

#[tokio::main]
async fn main() -> steam::Result<()> {
    let http = steam::http();
    let mut client = steam::Client::anonymous(&http).await?;
    let info = steam::pics::app_info(&mut client, &http, PROJECT_ZOMBOID).await?;
    for branch in steam::pics::branches(&info) {
        println!(
            "{:<14} build {:>9}  {}",
            branch.name,
            branch.build_id,
            branch.description.unwrap_or_default()
        );
    }
    Ok(())
}
