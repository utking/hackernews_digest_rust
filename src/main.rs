#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
mod arg_parse;
mod common;
mod config;
mod feeds;
mod hackernews;
mod schemas;
mod sender;

use crate::hackernews::prelude::*;
use arg_parse::CmdArgs;
use common::FetcherType;
use config::AppConfig;
use feeds::prelude::RssFetcher;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = &CmdArgs::parse(std::env::args().collect())?;
    let config = AppConfig::from_file(&args.config.clone())?;

    // Run the vacuum operation separately if requested
    if args.vacuum {
        let num_deleted = Storage::new(Storage::establish_connection(&config.get_db_file()))
            .vacuum(config.purge_after_days)?;
        println!("Vacuumed {num_deleted} items");
        return Ok(());
    }

    // Create a list of fetchers to run
    let mut fetchers = vec![];
    // HNFetcher is used only if feeds_only is not set to true
    {
        let mut skip_hackernews = false;
        if let Some(feeds_only) = &args.feeds_only {
            skip_hackernews = *feeds_only;
        }

        if !skip_hackernews {
            let storage = Storage::new(Storage::establish_connection(&config.get_db_file()));
            fetchers.push(FetcherType::HNFetcher(HNFetcher::new(&config, storage)));
        }
    }
    // RssFetcher is optional, if the config has rss_sources then add it to the fetchers
    if let Some(sources) = &config.rss_sources {
        if !sources.is_empty() {
            let storage = Storage::new(Storage::establish_connection(&config.get_db_file()));
            fetchers.push(FetcherType::RssFetcher(RssFetcher::new(&config, storage)));
        }
    }

    // Run the fetchers if there are any
    for fetcher in &mut fetchers {
        let fetched_items = match fetcher {
            FetcherType::HNFetcher(f) => f.run(args.reverse).await?,
            FetcherType::RssFetcher(f) => f.run(args.reverse).await?,
        };

        println!("Fetched new items: {fetched_items}");
    }

    Ok(())
}
