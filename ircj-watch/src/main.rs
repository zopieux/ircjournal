use figment::{providers::Format, Figment};
use futures::StreamExt;
use itertools::{Either, Itertools};
use log::{debug, error, info};
use pin_project_lite::pin_project;
use std::{collections::HashMap, marker::PhantomData, path::PathBuf, pin::Pin};
use tokio::{
    fs::File,
    io::{AsyncBufReadExt, BufReader},
};
use tokio_stream::wrappers::LinesStream;

use ircj_watch::backfill;
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
            backfill_concurrency: 4,
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
    let do_backfill = config.backfill;
    let batch_size = config.backfill_batch_size;
    let concurrency = config.backfill_concurrency;
    let results: Vec<_> = futures::stream::iter(config.paths)
        .zip(futures::stream::repeat(pool.clone()))
        .map(|(path, pool)| async move {
            info!("Attempting to backfill {:?}", &path);
            (
                path.clone(),
                // TODO: generify.
                backfill::<ircjournal::weechat::Weechat>(
                    &path,
                    &pool,
                    do_backfill,
                    batch_size,
                    concurrency,
                )
                .await,
            )
        })
        .buffer_unordered(2)
        .collect()
        .await;

    // Display some backfill stats.
    let (successes, failures): (Vec<_>, Vec<_>) =
        results.into_iter().partition_map(|(path, res)| match res {
            Ok(el) => Either::Left((path, el)),
            Err(err) => Either::Right((path, err)),
        });

    if !successes.is_empty() {
        info!(
            "Backfill successful:\n{}",
            successes
                .iter()
                .format_with("\n", |(path, (sc, inserted, _, _)), f| f(&format_args!(
                    "\tPath {:?}: channel is {}; backfilled {} messages",
                    path, sc, inserted
                )))
        );
    }
    if !failures.is_empty() {
        error!(
            "Backfill failed:\n{}",
            failures
                .iter()
                .format_with(", ", |(path, err), f| f(&format_args!(
                    "\tPath {:?}: {:?}",
                    path, err
                )))
        );
    }

    // Now watch for changes and save new messages as they come.
    let mut notifier = inotify::Inotify::init().unwrap();

    let mut tailer_of_wd: HashMap<_, _> = successes
        .into_iter()
        .map(|(path, (sc, _, buf_reader, type_mark))| {
            (
                notifier
                    .add_watch(path, inotify::WatchMask::CLOSE_WRITE)
                    .unwrap(),
                Tailer::new(type_mark, sc, buf_reader),
            )
        })
        .collect();

    let mut notify_stream = notifier.event_stream([0; 32]).unwrap().fuse();

    loop {
        tokio::select! {
            Some(Ok(event)) = notify_stream.next() => {
                let tailer = tailer_of_wd.get_mut(&event.wd).unwrap();
                let sc = tailer.sc.clone();
                let new_messages = Pin::new(tailer).read_all_new_lines().await;
                let inserted = ircjournal::db::batch_insert_messages(&pool, &new_messages).await;
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
