use std::io::Error;

#[derive(Clone)]
pub struct CmdArgs {
    pub config: String,
    pub reverse: bool,
    pub vacuum: bool,
    pub feeds_only: Option<bool>,
}

impl CmdArgs {
    pub fn parse(args: Vec<String>) -> Result<Self, Error> {
        let mut config = String::from("./config.json");
        let mut reverse = false;
        let mut vacuum = false;
        let mut feeds_only = false;
        {
            let mut ap = argparse::ArgumentParser::new();
            ap.set_description("Hackernews CLI");
            ap.refer(&mut config).add_option(
                &["-c", "--config"],
                argparse::Store,
                "Config file path; default is config.json",
            );
            ap.refer(&mut reverse).add_option(
                &["-r", "--reverse"],
                argparse::StoreTrue,
                "Reverse the filters results - exclude instead of include",
            );
            ap.refer(&mut vacuum).add_option(
                &["-v", "--vacuum"],
                argparse::StoreTrue,
                "Vacuum the database of older items",
            );
            ap.refer(&mut feeds_only).add_option(
                &["-f", "--feeds-only"],
                argparse::StoreTrue,
                "Fetch only RSS feeds",
            );

            match ap.parse(args, &mut std::io::stdout(), &mut std::io::stderr()) {
                Ok(()) => {}
                Err(_) => {
                    return Err(Error::from(std::io::ErrorKind::InvalidInput));
                }
            }
        }

        Ok(CmdArgs {
            config,
            reverse,
            vacuum,
            feeds_only: Some(feeds_only),
        })
    }
}
