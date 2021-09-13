use crate::{
    model::{NewMessage, ServerChannel},
    Logger, ParseResult,
};
use futures::stream::StreamExt;
use std::{
    collections::HashMap,
    marker::PhantomData,
    path::{Path, PathBuf},
};
use tokio::{
    fs::File,
    io::{AsyncBufReadExt, AsyncSeekExt, BufReader, SeekFrom},
    select,
    sync::mpsc,
};
use tokio_stream::wrappers::LinesStream;

#[async_trait]
trait LoggerTailT: Send {
    async fn next(&mut self) -> Option<NewMessage>;
}

struct LoggerTail<L: Logger + Send> {
    sc: ServerChannel,
    line_stream: LinesStream<BufReader<File>>,
    t: PhantomData<L>,
}

impl<L: Logger + Send> LoggerTail<L> {
    async fn new(sc: ServerChannel, path: &Path) -> Option<Self> {
        let f = File::open(path.clone()).await.ok()?;
        let mut reader = BufReader::new(f);
        reader.seek(SeekFrom::End(0)).await.ok()?;
        let line_stream = LinesStream::new(reader.lines());
        Some(Self {
            sc,
            line_stream,
            t: PhantomData,
        })
    }
}

async fn make_tailer(path: &Path) -> Option<Box<dyn LoggerTailT>> {
    // TODO: find a way of writing generic code for each Logger.
    if let Some(sc) = crate::weechat::Weechat::parse_path(path) {
        return Some(Box::new(
            LoggerTail::<crate::weechat::Weechat>::new(sc, path).await?,
        ));
    }
    None
}

#[async_trait]
impl<L: Logger + Send> LoggerTailT for LoggerTail<L> {
    async fn next(&mut self) -> Option<NewMessage> {
        let line = self.line_stream.next().await?.ok()?;
        match L::parse_line(&line) {
            ParseResult::Ok((ts, line)) => crate::line_to_new_message(line, &self.sc, ts),
            _ => None,
        }
    }
}

pub fn watch_for_changes_task(
    logger: slog::Logger,
    new_messages: mpsc::UnboundedSender<NewMessage>,
    mut new_files: mpsc::UnboundedReceiver<PathBuf>,
    mut shutdown: rocket::Shutdown,
) {
    tokio::spawn(async move {
        let mut notify = inotify::Inotify::init().unwrap();
        let mut event_stream = notify.event_stream([0; 32]).unwrap();
        let mut map: HashMap<inotify::WatchDescriptor, Box<dyn LoggerTailT>> = HashMap::new();
        loop {
            select! {
                _ = &mut shutdown => break,
                Some(path) = new_files.recv() => {
                    if let Some(tailer) = make_tailer(&path).await {
                        slog::info!(logger, "Now watching {:?} for changes", &path);
                        let wd = notify.add_watch(path, inotify::WatchMask::MODIFY).expect("add file watch");
                        map.insert(wd, tailer);
                    } else {
                        slog::error!(logger, "Could not watch {:?} for changes", &path);
                    }
                },
                Some(event) = event_stream.next() => {
                    let event = &event.unwrap();
                    let tailer = map.get_mut(&event.wd).unwrap();
                    while let Some(new_message) = tailer.next().await {
                        // It's okay to scream in the void.
                        let _ = new_messages.send(new_message);
                    }
                },
            }
        }
    });
}
