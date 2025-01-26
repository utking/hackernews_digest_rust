#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
mod arg_parse;
mod common;
mod config;
mod feeds;
mod hackernews;
mod sender;
mod storage;

use std::fs::OpenOptions;

use crate::hackernews::prelude::*;
use arg_parse::CmdArgs;
use common::FetcherType;
use config::AppConfig;
use feeds::prelude::RssFetcher;
use storage::FileStorage;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = &CmdArgs::parse(std::env::args().collect())?;
    let config = AppConfig::from_file(&args.config.clone())?;

    // Run the vacuum operation separately if requested
    if args.vacuum {
        let mut storage = FileStorage::from_fs(
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&config.db_dsn)?,
        );
        match vacuum(&mut storage, config.purge_after_days) {
            Ok(num_deleted) => {
                println!("Vacuumed {num_deleted} items");
                return Ok(());
            }
            Err(e) => eprintln!("Error vacuuming the database: {e}"),
        }
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
            let storage = FileStorage::from_fs(
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .open(&config.db_dsn)?,
            );
            let hn_fetcher = HNFetcher::new(&config, storage);
            fetchers.push(FetcherType::HNFetcher(hn_fetcher));
        }
    }
    // RssFetcher is optional, if the config has rss_sources then add it to the fetchers
    if let Some(sources) = &config.rss_sources {
        if !sources.is_empty() {
            let storage = FileStorage::from_fs(
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .open(&config.db_dsn)?,
            );
            let rss_fetcher = RssFetcher::new(&config, storage);
            fetchers.push(FetcherType::RssFetcher(rss_fetcher));
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
