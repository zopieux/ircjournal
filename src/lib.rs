#![feature(async_stream, async_closure)]

#[macro_use]
extern crate diesel;
#[macro_use]
extern crate diesel_migrations;
extern crate futures;
extern crate inotify;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate rocket;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use async_stream::stream;
use futures_core::Stream;
use futures_util::StreamExt;
use inotify::{WatchDescriptor, WatchMask};
use rocket::serde::{Deserialize, Serialize};
use tokio::{
    fs::File,
    io::{AsyncBufReadExt, AsyncRead, AsyncSeekExt, BufReader, SeekFrom},
};

use crate::{
    db::{batch_insert_messages, last_message, DbConn},
    models::{Datetime, NewMessage, ServerChannel},
};

pub mod db;
pub mod models;
pub mod route_impl;
pub mod routes;
pub mod schema;
pub mod views;
pub mod weechat;

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Config {
    logs: Vec<PathBuf>,
    backfill: bool,
    backfill_chunk_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            backfill: true,
            logs: vec![],
            backfill_chunk_size: 3_000,
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
        timestamp: timestamp,
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
    conn: DbConn,
    chunk_size: usize,
    concurrency: usize,
) -> std::io::Result<(ServerChannel, usize)> {
    let sc = L::parse_path(path).ok_or(invalid_input("not a valid filename"))?;
    let sc_clone = sc.clone();
    let f = File::open(path).await?;
    let mut reader = tokio::io::BufReader::new(f);

    let sc_clone_ = sc.clone();
    if let Some(last_message) = conn.run(move |c| last_message(c, &sc_clone_)).await {
        println!("last message {:?}", last_message);
        let x = seek_past_line::<L, _>(&mut reader, &last_message.timestamp).await;
        println!("seek result {:?}", x);
    }

    let message_stream = stream! {
        let mut lines = reader.lines();
        while let Some(l) = lines.next_line().await.unwrap() {
            match L::parse_line(&l) {
                ParseResult::Ok((ts, line)) => if let Some(m) = line_to_new_message(line, &sc_clone, ts) { yield m },
                _ => continue,
            }
        }
    };
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
        .fold(0, |x, acc| x + acc);
    Ok((sc, inserted))
}

pub struct Watcher {
    notifier: inotify::Inotify,
    paths: HashMap<WatchDescriptor, PathBuf>,
}

impl Watcher {
    pub fn new() -> std::io::Result<Self> {
        Ok(Self {
            notifier: inotify::Inotify::init()?,
            paths: HashMap::new(),
        })
    }

    pub fn watch(&mut self, path: &Path) {
        let w = self
            .notifier
            .add_watch(&path, WatchMask::MODIFY | WatchMask::CLOSE_WRITE)
            .expect("onoes");
        self.paths.insert(w, path.to_path_buf());
    }

    pub fn stream(&mut self) -> impl Stream<Item = PathBuf> + '_ {
        stream! {
            let mut events = self.notifier.event_stream([0; 32]).unwrap();
            while let Some(e) = events.next().await {
                yield self.paths.get(&e.unwrap().wd).unwrap().to_path_buf();
            }
        }
    }
}

#[cfg(test)]
mod test {
    use tempfile::tempdir;

    use crate::{models::Datetime, seek_past_line, IrcLine, Weechat};

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
