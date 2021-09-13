#![feature(async_stream, async_closure)]
#![feature(generators)]
#![feature(iter_intersperse)]

#[macro_use]
extern crate diesel;
#[macro_use]
extern crate diesel_migrations;
extern crate inotify;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate rocket;

use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use futures::stream::StreamExt;
use rocket::serde::{Deserialize, Serialize};
use tokio::{
    fs::File,
    io::{AsyncBufReadExt, AsyncRead, AsyncSeekExt, BufReader, SeekFrom},
    select,
    sync::{broadcast, mpsc},
};
use tokio_stream::wrappers::LinesStream;

use crate::{
    db::{batch_insert_messages, last_message},
    model::{Datetime, NewMessage, ServerChannel},
    weechat::Weechat,
};

pub mod db;
pub mod model;
pub mod route;
pub mod route_adapt;
pub mod schema;
pub mod view;
pub mod watch;
pub mod weechat;

pub use db::{run_migrations, Database};
pub use route::routes;
pub use watch::watch_for_changes_task;

pub type MessageEvent = (ServerChannel, String);

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Config {
    pub logs: Vec<PathBuf>,
    pub backfill: bool,
    pub backfill_chunk_size: usize,
    pub backfill_concurrency: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            logs: vec![],
            backfill: true,
            backfill_chunk_size: 4_000,
            backfill_concurrency: 4,
        }
    }
}

#[derive(PartialEq, Debug)]
pub enum IrcLine {
    Garbage,
    NickChanged {
        old: String,
        new: String,
    },
    TopicChanged {
        nick: String,
        old: String,
        new: String,
    },
    Joined {
        nick: String,
    },
    Left {
        nick: String,
        reason: String,
    },
    Quit {
        nick: String,
        reason: String,
    },
    Kicked {
        oper_nick: String,
        nick: String,
        reason: String,
    },
    Me {
        nick: String,
        line: String,
    },
    Message {
        nick: String,
        line: String,
    },
}

pub fn line_to_new_message(
    line: IrcLine,
    sc: &ServerChannel,
    timestamp: Datetime,
) -> Option<NewMessage> {
    let m: NewMessage = NewMessage {
        channel: sc.db_encode(),
        timestamp,
        opcode: None,
        payload: None,
        nick: None,
        oper_nick: None,
        line: None,
    };
    match line {
        IrcLine::NickChanged { old, new } => Some(NewMessage {
            nick: Some(old),
            payload: Some(new),
            opcode: Some("nick".to_owned()),
            ..m
        }),
        IrcLine::TopicChanged {
            old: _old,
            nick,
            new,
        } => Some(NewMessage {
            nick: Some(nick),
            payload: Some(new),
            opcode: Some("topic".to_owned()),
            ..m
        }),
        IrcLine::Joined { nick } => Some(NewMessage {
            nick: Some(nick),
            opcode: Some("joined".to_owned()),
            ..m
        }),
        IrcLine::Left { nick, reason } => Some(NewMessage {
            nick: Some(nick),
            payload: Some(reason),
            opcode: Some("left".to_owned()),
            ..m
        }),
        IrcLine::Quit { nick, reason } => Some(NewMessage {
            nick: Some(nick),
            payload: Some(reason),
            opcode: Some("quit".to_owned()),
            ..m
        }),
        IrcLine::Kicked {
            oper_nick,
            nick,
            reason,
        } => Some(NewMessage {
            nick: Some(nick),
            oper_nick: Some(oper_nick),
            payload: Some(reason),
            opcode: Some("kicked".to_owned()),
            ..m
        }),
        IrcLine::Me { nick, line } => Some(NewMessage {
            nick: Some(nick),
            line: Some(line),
            opcode: Some("me".to_owned()),
            ..m
        }),
        IrcLine::Message { nick, line } => Some(NewMessage {
            nick: Some(nick),
            line: Some(line),
            ..m
        }),
        IrcLine::Garbage => None,
    }
}

async fn seek_past_line<L: Logger, F>(
    reader: &mut BufReader<F>,
    needle: &Datetime,
) -> Option<DatedIrcLine>
where
    F: AsyncRead + AsyncSeekExt + Unpin,
{
    let mut line = String::new();
    let mut start: u64 = 0;
    let mut end: u64 = reader.seek(SeekFrom::End(0)).await.ok()?;
    while start <= end {
        let mid = start + (end - start) / 2;
        reader.seek(SeekFrom::Start(mid)).await.ok()?;
        let mut attempts = 0;
        let dated_line = loop {
            line.clear();
            let parsed = match reader.read_line(&mut line).await {
                Ok(0) => ParseResult::Invalid,
                Ok(_) => L::parse_line(&line[0..line.len() - 1]),
                Err(_) => ParseResult::Invalid,
            };
            match (parsed, attempts) {
                (ParseResult::Invalid, 2) => break None,
                (ParseResult::Invalid, _) => attempts += 1,
                (ParseResult::Noise, _) => attempts = 0,
                (ParseResult::Ok(dated_line), _) => break Some(dated_line),
            }
        }?;
        match (&dated_line.0, mid) {
            (ts, _) if ts < needle => start = mid + 1,
            (ts, 0) if ts > needle => return None,
            (ts, _) if ts > needle => end = mid - 1,
            (_, _) /* if ts == needle */ => {
                // I don't know why this line is necessary, but it is.
                reader.seek(SeekFrom::Current(0)).await.ok()?;
                return Some(dated_line);
            }
        }
    }
    None
}

pub(crate) fn invalid_input(msg: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, msg)
}

type DatedIrcLine = (Datetime, IrcLine);

#[derive(Debug, PartialEq)]
pub enum ParseResult {
    Invalid,
    Noise,
    Ok(DatedIrcLine),
}

pub trait Logger {
    fn parse_path(path: &Path) -> Option<ServerChannel>;
    fn parse_line(line: &str) -> ParseResult;
}

pub async fn backfill<L: Logger>(
    path: &Path,
    conn: &Database,
    chunk_size: usize,
    concurrency: usize,
) -> std::io::Result<(ServerChannel, usize)> {
    let sc = L::parse_path(path).ok_or(invalid_input("not a valid filename"))?;
    let f = File::open(path).await?;
    let mut reader = tokio::io::BufReader::new(f);

    // Do we have a last message in the DB already?
    let sc_ = sc.clone();
    if let Some(last_message) = conn.run(move |c| last_message(c, &sc_)).await {
        // If so, before reading further, seek past it.
        seek_past_line::<L, _>(&mut reader, &last_message.timestamp).await;
    }

    // Stream of valid NewMessages, ignoring errors.
    let lines = LinesStream::new(reader.lines());
    let sc_ = sc.clone();
    let message_stream =
        lines
            .zip(futures::stream::repeat(sc_))
            .filter_map(|(line, sc)| async move {
                let line = line.ok()?;
                match L::parse_line(&line) {
                    ParseResult::Ok((ts, line)) => line_to_new_message(line, &sc, ts),
                    _ => None,
                }
            });
    // Concurrently insert in batches.
    let inserted: usize = message_stream
        .chunks(chunk_size)
        .map(|messages| async {
            conn.run(move |c| batch_insert_messages(c, &messages))
                .await
                .unwrap_or(0)
        })
        .buffered(concurrency)
        .collect::<Vec<usize>>()
        .await
        .iter()
        .sum();
    Ok((sc, inserted))
}

pub fn save_broadcast_task(
    logger: slog::Logger,
    db: Database,
    broadcast: broadcast::Sender<MessageEvent>,
    mut new_messages: mpsc::UnboundedReceiver<NewMessage>,
    mut shutdown: rocket::Shutdown,
) {
    tokio::spawn(async move {
        loop {
            select! {
                _ = &mut shutdown => break,
                Some(new_message) = new_messages.recv() => {
                    let sc = ServerChannel::db_decode(new_message.channel.as_str()).expect("decoding channel");
                    let message = db.run(move |c| db::insert_message(c, &new_message)).await.expect("inserting");
                    let _ = broadcast.send((sc.clone(), view::formatted_message(&message)));
                    slog::debug!(logger, "Observed and saved new message for {:?}, id {}", &sc, message.id);
                },
            }
        }
    });
}

pub async fn run_backfills(logger: &slog::Logger, config: &Config, db: &Database) {
    let now = Instant::now();
    // TODO: consider move concurrency here (distribute within files but also across files).
    for path in &config.logs {
        // TODO: find a way of writing generic code for each Logger.
        if let Some(sc) = Weechat::parse_path(path) {
            if let Ok((_, inserted)) = backfill::<Weechat>(
                path,
                db,
                config.backfill_chunk_size,
                config.backfill_concurrency,
            )
            .await
            {
                if inserted > 0 {
                    slog::info!(logger, "Backfilled {} messages for {:?}", inserted, sc);
                }
            } else {
                slog::error!(logger, "Could not backfill {:?}", sc)
            }
        } else {
            slog::error!(logger, "Could not determine log type for {:?}", path)
        }
    }
    slog::info!(logger, "Backfilled finished in {:?}", Instant::now() - now);
}

#[cfg(test)]
mod test {
    use tempfile::tempdir;

    use crate::{model::Datetime, seek_past_line, weechat::Weechat, IrcLine};

    pub(crate) fn ts(x: &'static str) -> Datetime {
        chrono::DateTime::parse_from_rfc3339(&format!("{}+00:00", x.replace(" ", "T")))
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    #[tokio::test]
    async fn test_seek_paste_line_weechat() {
        use tokio::io::AsyncWriteExt;
        let dir = tempdir().unwrap();
        let fname = dir.path().join("server.#chan.weechatlogs");
        {
            let mut f = tokio::fs::File::create(&fname).await.unwrap();
            f.write(
                "2020-01-25 09:31:14\thaileda\til pleut
2020-01-25 09:31:18\thaileda\til mouille
2020-01-25 09:31:34\thaileda\ty'a une houle de 5m
2020-01-25 09:39:17\tDettorer\tça rime pas
2020-01-25 09:39:26\thaileda\tc'est un haiku
2020-01-25 09:40:31\tDettorer\tça rime pas un haiku ?
2020-01-25 10:02:42\tspider-mario\tDettorer: non
2020-01-25 10:04:13\t<--\thaileda (~lda@hawaii) has quit (Quit: Lost terminal)\n"
                    .as_bytes(),
            )
            .await
            .unwrap();
        };

        macro_rules! seek_past_is {
            ($fname: ident, $ts: literal, $res: expr) => {
                let f = tokio::fs::File::open(&$fname).await.unwrap();
                let mut buff = tokio::io::BufReader::new(f);
                assert_eq!(
                    seek_past_line::<Weechat, _>(&mut buff, &ts($ts)).await,
                    $res
                );
            };
        }
        macro_rules! seek_past_some {
            ($fname: ident, $ts: literal, $some: expr) => {
                seek_past_is!($fname, $ts, Some((ts($ts), $some)))
            };
        }
        seek_past_is!(fname, "2020-01-25 01:01:01", None);
        seek_past_is!(fname, "2020-01-25 09:39:00", None);
        seek_past_is!(fname, "2020-01-25 23:59:59", None);
        seek_past_some!(
            fname,
            "2020-01-25 09:31:14",
            IrcLine::Message {
                nick: "haileda".to_string(),
                line: "il pleut".to_string()
            }
        );
        seek_past_some!(
            fname,
            "2020-01-25 09:39:26",
            IrcLine::Message {
                nick: "haileda".to_string(),
                line: "c'est un haiku".to_string()
            }
        );
        seek_past_some!(
            fname,
            "2020-01-25 10:02:42",
            IrcLine::Message {
                nick: "spider-mario".to_string(),
                line: "Dettorer: non".to_string()
            }
        );
        seek_past_some!(
            fname,
            "2020-01-25 10:04:13",
            IrcLine::Quit {
                nick: "haileda".to_string(),
                reason: "Quit: Lost terminal".to_string()
            }
        );
    }
}
