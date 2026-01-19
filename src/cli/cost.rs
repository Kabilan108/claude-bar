use anyhow::Result;

pub async fn run(json: bool, days: u32) -> Result<()> {
    tracing::info!(?json, ?days, "Scanning cost data");

    // TODO: Implement cost scanning from local logs
    if json {
        println!("{{}}");
    } else {
        println!("Claude Bar - Cost Summary (last {} days)", days);
        println!("(Not yet implemented)");
    }

    Ok(())
}
