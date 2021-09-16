# ircjournal

A lightweight, fast, standalone IRC log viewer for the web, with real-time log ingestion.

[![CI](https://github.com/Zopieux/ircjournal/actions/workflows/ci.yaml/badge.svg?branch=master)](https://github.com/Zopieux/ircjournal/actions/workflows/ci.yaml)

### Features

* Simple, no-dependency, JavaScript-optional web front-end to browse and full-text search IRC logs.
* Standalone IRC ingestion binary that observes log files for updates and save new lines.

### Dependencies

The tool relies on a PostgresSQL database for full-text search indexing and notification mechanism (live updates).
This is the only runtime dependency.

### Components

* Run `ircj-watch` on the machine with the IRC log files. It will watch for changes and save new lines to the database. You can run `irc-watcher` on multiple machines. Each instance can ingest any number of log files.
* Run `ircj-serve` to expose the web interface, directly or behind a web server.

Double-line boxes can be on the same machine or behind network boundaries, making for a pretty flexible setup.

```text
                                           ╔═════════════════╗
         ┌─────────────┐                   ║ ┌─────────────┐ ║
User ◄───┤ Browser     │                   ║ │ IRC client  │ ║
         └──────▲──────┘                   ║ │ (log files) │ ║
                │                          ║ └──────┬──────┘ ║
       ╔════════╪════════╗                 ║        │        ║
       ║ ┌──────┴──────┐ ║                 ║ ┌──────▼──────┐ ║
       ║ │ Frontend    │ ║                 ║ │ Ingestor    │ ║
       ║ │ ircj-server │ ║                 ║ │ ircj-watch  │ ║
       ║ └──────▲──────┘ ║                 ║ └──────┬──────┘ ║
       ╚════════╪════════╝                 ╚════════╪════════╝
                │        ╔═════════════════╗        │
                │        ║ ┌─────────────┐ ║        │
                └────────╫─┤ PostgreSQL  ◄─╫────────┘
                         ║ └─────────────┘ ║
                         ╚═════════════════╝
```

### License

[GNU General Public License v3.0 only](https://spdx.org/licenses/GPL-3.0.html).
