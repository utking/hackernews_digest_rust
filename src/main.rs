#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
mod arg_parse;
mod common;
mod config;
mod feeds;
mod hackernews;
mod schemas;

use crate::hackernews::prelude::*;
use arg_parse::CmdArgs;
use common::{FetchOperation, FetcherType};
use config::AppConfig;
use feeds::prelude::RssFetcher;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = &CmdArgs::parse(std::env::args().collect())?;
    let config = AppConfig::from_file(&args.config.clone())?;
    let mut fetchers = vec![];

    // HNFetcher is used only if feeds_only is not set to true
    {
        let mut skip_hackernews = false;
        match &args.feeds_only {
            Some(feeds_only) => skip_hackernews = *feeds_only,
            None => {}
        }

        if !skip_hackernews {
            fetchers.push(FetcherType::HNFetcher(HNFetcher::new(&config)));
        }
    }
    // RssFetcher is optional, if the config has rss_sources then add it to the fetchers
    match &config.rss_sources {
        Some(sources) => {
            if !sources.is_empty() {
                fetchers.push(FetcherType::RssFetcher(RssFetcher::new(&config)));
            }
        }
        None => {}
    }

    // Run the fetchers
    for fetcher in fetchers {
        let fetched_items = match fetcher {
            FetcherType::HNFetcher(f) => f.run(&args.get_action()).await?,
            FetcherType::RssFetcher(f) => f.run(&args.get_action()).await?,
        };

        match args.get_action() {
            FetchOperation::Fetch(_) => println!("Fetched new items: {fetched_items}"),
            FetchOperation::Vacuum => println!("Vacuumed the database"),
        }
    }

    Ok(())
}
