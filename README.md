## Rust version of HackerNews Digest

### Requirements

* Sqlite 3

### Configuration

There is a default config-file name - `config.json`. Note that it can be overwritten in the comman line (-c|--config). The path can be relative or absolute.

To create a config file, copy `config.example.json` to `config.json` (or any other name that seems right for you) and adjust what you think should be adjusted. For `sqlite3`, the Database DSN string must contain the name of the DB file.

#### Digest output

There are 3 options to output the collected digest

* Telegram bot - use the corresponding `telegram` part of the config. Each news item will be a separate message in the configured channel.
* Email - use the `smtp` part. All news items will come listed in one email.
* CLI Console - remove both - `smtp` and `telegram` sections of the config. The output will look like the plain-text version of the email.

If you have both `smtp` and `telegram` sections in your config file, `smtp` will be used of the two.

### Arguments

* -r|--reverse - to reverse the filtering
* -v|--vacuum - to remove old records, without running news updates (retention period is set set in the config file)
* -c|--config - to set a config file
* -h|--help - to show this help
