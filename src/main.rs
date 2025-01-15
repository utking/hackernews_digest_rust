#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
mod hackernews;
mod schemas;

use crate::hackernews::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = &CmdArgs::parse(std::env::args().collect())?;
    let config = AppConfig::from_file(&args.config.clone())?;

    let fetcher = Fetcher::new(&config);
    let fetched_items = fetcher.run(&args.get_action()).await?;

    match args.get_action() {
        FetchOperation::Fetch(_) => println!("Fetched new items: {fetched_items}"),
        FetchOperation::Vacuum => println!("Vacuumed the database"),
    }

    Ok(())
}
