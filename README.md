## Rust version of HackerNews Digest

### Requirements

* Sqlite 3
* MySQL

### Configuration

There is a default config-file name - `config.json`. Note that it can be overwritten in the comman line (-c|--config). The path can be relative or absolute.

To create a config file, copy `config.example.json` to `config.json` (or any other name that seems right for you) and adjust what you think should be adjusted.

Database connection agreement:

* For `sqlite3`, the Database DSN string must contain the name of the DB file (i.e., `./news.db`).
* For `mysql`, the DSN must be in the form of `mysql://user:password@host:port/db-name`. See https://docs.diesel.rs/2.1.x/diesel/mysql/struct.MysqlConnection.html for additional options. (!) libmysqlclient must be installed in the systemd for Mysql backend to work.

#### Digest output

There are 3 options to output the collected digest

* Telegram bot - use the corresponding `telegram` part of the config. Each news item will be a separate message in the configured channel.
* Email - use the `smtp` part. All news items will come listed in one email.
* CLI Console - remove both - `smtp` and `telegram` sections of the config. The output will look like the plain-text version of the email.

If you have both `smtp` and `telegram` sections in your config file, `smtp` will be used of the two.

### CLI flags and parameters

* -r|--reverse - to reverse the filtering
* -v|--vacuum - to remove old records, without running news updates (retention period is set set in the config file)
* -c|--config - to set a config file
* -f|--feeds-only - to pull RSS feeds only
* -h|--help - to show this help
