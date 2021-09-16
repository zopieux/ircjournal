# ircjournal

A lightweight, fast, standalone IRC log viewer for the web, with real-time log ingestion, written in Rust.

[![CI](https://github.com/Zopieux/ircjournal/actions/workflows/ci.yaml/badge.svg?branch=master)](https://github.com/Zopieux/ircjournal/actions/workflows/ci.yaml)

### Features

* Simple, no-dependency, JavaScript-optional web front-end to browse and full-text search IRC logs.
* Standalone IRC ingestion binary that observes log files for updates and save new lines.

<p align="center">
  <a target="_blank" rel="noopener noreferrer" href=".github/screenshot.png"><img src=".github/screenshot.png" alt="Screenshot" style="max-width: 100%;"></a>
  <em>Screenshot of the web frontend.</em>
</p>

### Usage

Double-line boxes can be on the same machine or behind network boundaries, making for a pretty flexible setup.

```text
                                           ╔═════════════════╗
         ┌─────────────┐                   ║ ┌─────────────┐ ║
User ◄───► Browser     │                   ║ │ IRC client  │ ║
         └──────▲──────┘                   ║ │ (log files) │ ║
                │                          ║ └──────┬──────┘ ║
       ╔════════╪════════╗                 ║        │        ║
       ║ ┌──────▼──────┐ ║                 ║ ┌──────▼──────┐ ║
       ║ │ Frontend    │ ║                 ║ │ Ingestor    │ ║
       ║ │ ircj-serve  │ ║                 ║ │ ircj-watch  │ ║
       ║ └──────▲──────┘ ║                 ║ └──────┬──────┘ ║
       ╚════════╪════════╝                 ╚════════╪════════╝
                │        ╔═════════════════╗        │
                │        ║ ┌─────────────┐ ║        │
                └────────╫─► PostgreSQL  ◄─╫────────┘
                         ║ └─────────────┘ ║
                         ╚═════════════════╝
```

#### PostgreSQL database

ircjournal relies on a PostgreSQL database for full-text search indexing and the notification mechanism (live updates).
This is the only runtime dependency.

Please create an empty database and associated user. ircjournal will set up the rest on the first run.

#### ircj-serve

Run `ircj-serve` to expose the web interface, directly or behind a reverse-proxy server such as nginx.
You need to set the `IRCJ_DB` environment variable to a valid database URI:

    IRCJ_DB=postgresql://[username:pswd]@[host]/[db name]

#### ircj-watch

Run `ircj-watch` on the machine with the IRC log files. It will watch for changes and save new lines to the database.
You can run `irc-watcher` on multiple machines. Each instance can ingest any number of log files.

You need to set:

* the `IRCJ_DB` environment variable to a valid database URI, like above
* the `IRCJ_PATHS` environment variable to a list of file paths to ingest:

      IRCJ_PATHS=[/data/logs/irc.libera.#foo.weechatlog,/data/logs/irc.libera.#bar.weechatlog]

There are [other `IRCJ_` variables]() you can set to tweak the backfill mechanism.

##### The backfill mechanism

The first time you run `ircj-watch` on an empty database, or whenever you add a new log file, or if new lines were added
in a channel while `ircj-watch` was *not* running, the program will attempt to find the last recorded line in the file
and backfill (save) the missing new lines in the database. It will then continue watching for changes, as usual.

It is therefore safe to restart the `ircj-watch` binary at any time.

#### Logging level

ircjournal uses the popular `env_logger` crate. You can [customize log levels](https://docs.rs/env_logger/*/env_logger/#enabling-logging)
a per-module granularity with the `RUST_LOG` environment variable.
For instance, use `RUST_LOG=warn,ircj_serve=info` to warn by default, and have info-level logs for `ircj-serve`.

### Acknowledgments

[whitequark's irclogger](https://github.com/whitequark/irclogger/) is what motivated me to develop a similar IRC log 
browser to try out the Rust language. ircjournal UI is heavily inspired by irclogger.   

### License

[GNU General Public License v3.0 only](https://spdx.org/licenses/GPL-3.0.html).
