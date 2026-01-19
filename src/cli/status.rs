use anyhow::Result;

pub async fn run(json: bool, provider: Option<String>) -> Result<()> {
    tracing::info!(?json, ?provider, "Fetching usage status");

    // TODO: Implement status fetching from provider APIs
    if json {
        println!("{{}}");
    } else {
        println!("Claude Bar - Usage Status");
        println!("(Not yet implemented)");
    }

    Ok(())
}
