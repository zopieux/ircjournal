extern crate lazy_static;

use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncSeekExt, BufReader, SeekFrom};

pub use crate::model::{Datetime, NewMessage, ServerChannel};
pub type Database = sqlx::postgres::PgPool;
pub type MessageEvent = (ServerChannel, String);

pub mod db;
pub mod model;
pub mod weechat;

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
        channel: Some(sc.to_string()),
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

async fn find_last_line<L: Logger, F>(
    line: &mut String,
    reader: &mut BufReader<F>,
    end: u64,
) -> Option<String>
where
    F: AsyncRead + AsyncSeekExt + Unpin,
{
    let mut offset = 64u64;
    loop {
        line.clear();
        reader.seek(SeekFrom::Start(end - offset)).await.ok()?;
        match reader.read_to_string(line).await {
            Ok(0) => return None,
            Ok(_) => {
                let lasts: Vec<_> = line[0..line.len() - 1].rsplitn(2, '\n').collect();
                match lasts.len() {
                    1 => offset *= 2,
                    2 => return Some(lasts[0].to_string()),
                    _ => unreachable!(),
                };
            }
            Err(_) => return None,
        }
    }
}

pub async fn seek_past_line<L: Logger, F>(
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
                Ok(0) => L::parse_line(&find_last_line::<L, F>(&mut line, reader, end).await?),
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
            (_, _) /* ts == needle */ => {
                // In the (unlikely) case there are multiple messages with the same timestamp,
                // continue reading.
                let mut dated_line = dated_line;
                let mut last_pos = reader.seek(SeekFrom::Current(0)).await.ok()?;
                loop {
                    line.clear();
                    let parsed = match reader.read_line(&mut line).await {
                        Ok(0) => break,
                        Ok(_) => L::parse_line(&line[0..line.len() - 1]),
                        Err(_) => break,
                    };
                    match parsed {
                        ParseResult::Invalid => break,
                        ParseResult::Noise => break,
                        ParseResult::Ok((mts, _)) if mts != dated_line.0 => break,
                        ParseResult::Ok(new_dated_line) => /* mts == ts */ {
                            // Same timestamp. Advance the cursor.
                            dated_line = new_dated_line;
                            last_pos = reader.seek(SeekFrom::Current(0)).await.ok()?;
                            continue;
                        }
                    }
                }
                // Reset to wherever we were before the ts changed.
                reader.seek(SeekFrom::Start(last_pos)).await.ok()?;
                return Some(dated_line);
            }
        }
    }
    None
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
