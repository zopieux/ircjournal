use futures::StreamExt;
use ircjournal::model::ServerChannel;
use std::{marker::PhantomData, path::Path};
use tokio::{
    fs::File,
    io::{AsyncBufReadExt, BufReader},
};
use tokio_stream::wrappers::LinesStream;

use ircjournal::{
    db::batch_insert_messages, line_to_new_message, seek_past_line, Database, Logger, ParseResult,
};

fn invalid_input(msg: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, msg)
}

pub async fn backfill<L: Logger>(
    path: &Path,
    db: &Database,
    chunk_size: usize,
    concurrency: usize,
) -> std::io::Result<(ServerChannel, u64, BufReader<File>, PhantomData<L>)> {
    let sc = L::parse_path(path).ok_or(invalid_input("not a valid filename"))?;
    let f = File::open(path).await?;
    let mut reader = tokio::io::BufReader::new(f);

    // Do we have a last message in the DB already?
    let sc_ = sc.clone();
    if let Some(ts) = ircjournal::db::last_message_ts(db, &sc_).await {
        // If so, before reading further, seek past it.
        seek_past_line::<L, _>(&mut reader, &ts).await;
    }

    // Read lines, create batch, insert and return inserted size.
    let mut line_stream = LinesStream::new(reader.lines());
    let total_inserted = line_stream
        .by_ref() // This is key: we need to grab the inner BufRead to tell position afterwards.
        .zip(futures::stream::repeat(sc.clone()))
        .filter_map(|(line, sc)| async move {
            let line = line.ok()?;
            match L::parse_line(&line) {
                ParseResult::Ok((ts, line)) => line_to_new_message(line, &sc, ts),
                _ => None,
            }
        })
        .chunks(chunk_size)
        .map(|messages| async move { batch_insert_messages(db, &messages).await.unwrap_or(0) })
        .buffered(concurrency)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .sum();
    Ok((
        sc,
        total_inserted,
        line_stream.into_inner().into_inner(),
        PhantomData,
    ))
}

/*
pub(crate) async fn insert_message(db: &Database, message: &NewMessage) -> Option<Message> {
    sqlx::query_as!(Message, r#"
        INSERT INTO message ("channel", "nick", "line", "opcode", "oper_nick", "payload", "timestamp")
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING "id", "channel", "nick", "line", "opcode", "oper_nick", "payload", "timestamp"
    "#, message.channel, message.nick, message.line, message.opcode, message.oper_nick, message.payload, message.timestamp)
        .fetch_one(db)
        .await
        .ok()
}
*/
