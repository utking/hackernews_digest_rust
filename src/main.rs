#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
mod arg_parse;
mod common;
mod config;
mod habr;
mod hackernews;
mod schemas;

use crate::hackernews::prelude::*;
use arg_parse::CmdArgs;
use common::FetchOperation;
use config::AppConfig;
use habr::prelude::HabrFetcher;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = &CmdArgs::parse(std::env::args().collect())?;
    let config = AppConfig::from_file(&args.config.clone())?;

    let fetcher = HNFetcher::new(&config);
    let mut fetched_items = fetcher.run(&args.get_action()).await?;

    match args.get_action() {
        FetchOperation::Fetch(_) => println!("Fetched new items: {fetched_items}"),
        FetchOperation::Vacuum => println!("Vacuumed the database"),
    }

    let habr_fetcher = HabrFetcher::new(&config);
    fetched_items = habr_fetcher.run(&args.get_action()).await?;

    match args.get_action() {
        FetchOperation::Fetch(_) => println!("Fetched new items: {fetched_items}"),
        FetchOperation::Vacuum => println!("Vacuumed the database"),
    }

    Ok(())
}
