use figment::{providers::Format, Figment};
use futures::StreamExt;
use itertools::{Either, Itertools};
use log::debug;
use pin_project_lite::pin_project;
use std::{collections::HashMap, marker::PhantomData, path::PathBuf, pin::Pin};
use tokio::{
    fs::File,
    io::{AsyncBufReadExt, BufReader},
};
use tokio_stream::wrappers::LinesStream;

use ircj_watch::{backfill, inserter_task};
use ircjournal::{
    line_to_new_message,
    model::{NewMessage, ServerChannel},
    Logger, ParseResult,
};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Config {
    db: String,
    paths: Vec<PathBuf>,
    backfill: bool,
    backfill_batch_size: usize,
    backfill_concurrency: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            db: "".to_owned(),
            paths: vec![],
            backfill: true,
            backfill_batch_size: 5_000,
            backfill_concurrency: 2,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), figment::Error> {
    env_logger::init();

    let config: Config = Figment::new()
        .merge(figment::providers::Serialized::defaults(Config::default()))
        .merge(figment::providers::Yaml::file("ircj-watch.yml"))
        .merge(figment::providers::Env::prefixed("IRCJ_"))
        .extract()?;

    let pool = ircjournal::db::create_db(&config.db)
        .await
        .expect(&*format!(
            "Connecting and migrating the database at {}",
            &config.db
        ));

    // First, backfill.
    let prog = indicatif::MultiProgress::new();
    let sty = indicatif::ProgressStyle::default_bar()
        .template("{spinner} [{elapsed_precise}] {len:>7} ({per_sec:>6}) {prefix} {wide_msg}");

    let (tx, rx) = tokio::sync::mpsc::channel::<NewMessage>(128);

    let db_for_inserter = pool.clone();
    let batch_size = config.backfill_batch_size;
    let inserter_handle =
        tokio::spawn(async move { inserter_task(batch_size, db_for_inserter, rx).await });

    let do_backfill = config.backfill;
    let prepared: Vec<_> = config
        .paths
        .iter()
        .map(|path| {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let p = prog.add(
                indicatif::ProgressBar::new(0)
                    .with_prefix(name)
                    .with_style(sty.clone()),
            );
            p.tick();
            (path, p, pool.clone(), tx.clone())
        })
        .collect();

    drop(tx);

    let prog_handle = tokio::task::spawn_blocking(move || {
        let _ = prog.join();
    });

    let results: Vec<_> = futures::stream::iter(prepared)
        .map(|(path, progress, pool, tx)| async move {
            // TODO: generify.
            (
                path.clone(),
                backfill::<ircjournal::weechat::Weechat>(
                    path,
                    &pool,
                    do_backfill,
                    tx,
                    progress.clone(),
                )
                .await,
                progress,
            )
        })
        .buffer_unordered(config.backfill_concurrency)
        .collect()
        .await;

    let (successes, failures): (Vec<_>, Vec<_>) =
        results
            .into_iter()
            .partition_map(|(path, res, p)| match res {
                Ok(el) => Either::Left((path, el)),
                Err(err) => Either::Right((err, p)),
            });

    failures.into_iter().for_each(|(err, p)| {
        p.set_message(err.to_string());
    });

    let (ins, prog) = tokio::join!(inserter_handle, prog_handle);
    ins.unwrap();
    prog.unwrap();

    // Now watch for changes and save new messages as they come.
    let mut notifier = inotify::Inotify::init().unwrap();

    let mut tailer_of_wd: HashMap<_, _> = successes
        .into_iter()
        .map(|(path, (sc, buf_reader, type_mark))| {
            (
                notifier
                    .add_watch(path, inotify::WatchMask::CLOSE_WRITE)
                    .unwrap(),
                Tailer::new(type_mark, sc, buf_reader),
            )
        })
        .collect();

    let mut notify_stream = notifier.event_stream([0; 32]).expect("event stream").fuse();

    loop {
        tokio::select! {
            Some(Ok(event)) = notify_stream.next() => {
                // A file has changed. Get its associated tailed, read new lines, save them.
                let tailer = tailer_of_wd.get_mut(&event.wd).unwrap();
                let sc = tailer.sc.clone();
                let new_messages = Pin::new(tailer).read_all_new_lines().await;
                let inserted = ircjournal::db::batch_insert_messages_and_notify(&pool, &new_messages).await;
                debug!("Channel {}: inserted {}", &sc, inserted.unwrap_or_default());
            }
        }
    }
}

pin_project! {
    struct Tailer<L: Logger> {
        type_mark: PhantomData<L>,
        sc: ServerChannel,
        #[pin]
        buf_reader: BufReader<File>,
    }
}

impl<L: Logger> Tailer<L> {
    fn new(type_mark: PhantomData<L>, sc: ServerChannel, buf_reader: BufReader<File>) -> Self {
        Self {
            type_mark,
            sc,
            buf_reader,
        }
    }

    async fn read_all_new_lines(self: Pin<&mut Self>) -> Vec<NewMessage> {
        let this = self.project();
        LinesStream::new(this.buf_reader.lines())
            .by_ref()
            .filter_map(|line| async move { line.ok() })
            .zip(futures::stream::repeat(this.sc.clone()))
            .filter_map(|(line, sc)| async move {
                match L::parse_line(&line) {
                    ParseResult::Ok((ts, line)) => line_to_new_message(line, &sc, ts),
                    _ => None,
                }
            })
            .collect()
            .await
    }
}
