use futures::StreamExt;
use indicatif::ProgressBar;
use std::{marker::PhantomData, path::Path};
use tokio::{
    fs::File,
    io::{AsyncBufReadExt, AsyncSeekExt, BufReader, SeekFrom},
};
use tokio_stream::wrappers::LinesStream;

use ircjournal::{
    db::batch_insert_messages, line_to_new_message, model::ServerChannel, seek_past_line, Database,
    Logger, NewMessage, ParseResult,
};

fn invalid_input(msg: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, msg)
}

pub async fn inserter_task(
    chunk_size: usize,
    db: Database,
    mut message_queue: tokio::sync::mpsc::Receiver<NewMessage>,
) -> u64 {
    let mut total = 0u64;
    let mut batch = Vec::with_capacity(chunk_size);
    while let Some(message) = message_queue.recv().await {
        batch.push(message);
        if (batch.len()) == chunk_size {
            total += batch_insert_messages(&db, &batch).await.unwrap_or(0);
            batch.clear();
        }
    }
    total += batch_insert_messages(&db, &batch).await.unwrap_or(0);
    total
}

pub async fn backfill<L: Logger>(
    path: &Path,
    db: &Database,
    backfill: bool,
    tx: tokio::sync::mpsc::Sender<NewMessage>,
    progress: ProgressBar,
) -> std::io::Result<(ServerChannel, BufReader<File>, PhantomData<L>)> {
    let sc = L::parse_path(path).ok_or_else(|| invalid_input("not a valid filename"))?;
    let f = File::open(path).await?;
    let mut reader = tokio::io::BufReader::new(f);

    if !backfill {
        reader.seek(SeekFrom::End(0)).await?;
        return Ok((sc, reader, PhantomData));
    }

    // Do we have a last message in the DB already?
    let mut last_ts = None;
    let sc_ = sc.clone();
    if let Some(ts) = ircjournal::db::last_message_ts(db, &sc_).await {
        // If so, before reading further, seek past it.
        last_ts = seek_past_line::<L, _>(&mut reader, &ts)
            .await
            .map(|(ts, _)| ts);
    }

    let from_str = match last_ts {
        None => "from scratch".to_owned(),
        Some(m) => format!("from {:?}", m),
    };
    progress.set_message(from_str.clone());

    // Read lines, create batch, insert and return inserted size.
    let mut line_stream = LinesStream::new(reader.lines());
    line_stream
        .by_ref() // This is key: we need to grab the inner BufRead to tell position afterwards.
        .zip(futures::stream::repeat((sc.clone(), tx, progress.clone())))
        .filter_map(|(line, (sc, tx, p))| async move {
            let line = line.ok()?;
            match L::parse_line(&line) {
                ParseResult::Ok((ts, line)) => {
                    line_to_new_message(line, &sc, ts).map(|nm| (nm, tx, p))
                }
                _ => None,
            }
        })
        .for_each(|(message, tx, p)| async move {
            tx.send(message).await.expect("channel closed");
            p.inc_length(1);
            p.inc(1);
        })
        .await;
    progress.finish_with_message(format!("{} (done)", from_str));
    Ok((sc, line_stream.into_inner().into_inner(), PhantomData))
}
