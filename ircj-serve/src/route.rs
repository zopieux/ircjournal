use chrono::{Datelike, Duration};
use itertools::Itertools;
use maud::Markup;
use rocket::{
    http::Status,
    response::{
        stream::{Event, EventStream},
        Redirect,
    },
    uri, Request, Route, State,
};
use tokio::{
    select,
    sync::broadcast::{error::RecvError, Sender},
};

use ircjournal::{model::ServerChannel, Database, MessageEvent};

use crate::{view, Day};

#[get("/")]
async fn home(db: &State<Database>) -> Option<Markup> {
    let channels = crate::db::channels(db).await;
    Some(view::home(&channels))
}

#[get("/<sc>")]
async fn channel_redirect(db: &State<Database>, sc: ServerChannel) -> Redirect {
    Redirect::temporary(
        if let Some(ts) = ircjournal::db::last_message_ts(db, &sc).await {
            uri!(channel(&sc, ts.into()))
        } else {
            uri!("/")
        },
    )
}

#[get("/<sc>/stream")]
async fn channel_stream(
    sc: ServerChannel,
    db: &State<Database>,
    queue: &State<Sender<MessageEvent>>,
    mut end: rocket::Shutdown,
) -> Option<EventStream![]> {
    if !crate::db::channel_exists(db, &sc).await {
        return None;
    }
    let mut rx = queue.subscribe();
    Some(EventStream! {
        loop {
            let message_html = select! {
                msg = rx.recv() => match msg {
                    Ok((for_sc, message_html)) if for_sc == sc => message_html,
                    Err(RecvError::Closed) => break,
                    _ => continue,
                },
                _ = &mut end => break,
            };
            yield Event::data(message_html);
        }
    })
}

#[get("/<sc>/<day>")]
async fn channel(db: &State<Database>, sc: ServerChannel, day: Day) -> Option<Markup> {
    let (messages, info, active_days) = {
        tokio::join!(
            crate::db::messages_channel_day(db, &sc, &day),
            crate::db::channel_info(db, &sc, &day),
            crate::db::channel_month_index(db, &sc, day.0.year(), day.0.month()),
        )
    };
    let truncated = messages.len() == crate::db::HARD_MESSAGE_LIMIT;
    Some(view::channel(
        &info?,
        &day,
        &messages,
        &active_days,
        truncated,
    ))
}

#[get("/<sc>/search?<query>&<page>")]
async fn channel_search(
    db: &State<Database>,
    sc: ServerChannel,
    query: &str,
    page: Option<u64>,
) -> Option<Markup> {
    let page = page.unwrap_or(1);
    let (result_page, info) = {
        let query = query.to_string();
        let today = Day::today();
        tokio::join!(
            crate::db::channel_search(db, &sc, &query, page as i64),
            crate::db::channel_info(db, &sc, &today),
        )
    };
    let messages: Vec<_> = result_page
        .records
        .into_iter()
        .group_by(|msg| msg.timestamp.date().naive_utc())
        .into_iter()
        .map(|(day, group)| (day.into(), group.collect()))
        .collect();
    Some(view::search(
        &info?,
        query,
        &messages,
        page,
        result_page.page_count,
        result_page.total,
    ))
}

#[derive(rust_embed::RustEmbed)]
#[folder = "static/"]
#[exclude = "**/*.sass"]
#[exclude = "**/*.ts"]
struct StaticAsset;

#[derive(Clone)]
pub struct StaticFiles;

struct StaticFile {
    extension: String,
    file: rust_embed::EmbeddedFile,
}

impl<'r> rocket::response::Responder<'r, 'static> for StaticFile {
    fn respond_to(self, _: &'r Request<'_>) -> rocket::response::Result<'static> {
        let content_type = rocket::http::ContentType::from_extension(&self.extension)
            .ok_or_else(|| Status::new(400))?;
        use chrono::{DateTime, NaiveDateTime, Utc};
        use hex::ToHex;
        use rocket::http::{hyper::header, Header};
        let dt =
            self.file.metadata.last_modified().map(|lm| {
                DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(lm as i64, 0), Utc)
            });
        let mut builder = rocket::response::Response::build();
        builder
            .header(content_type)
            .header(Header::new(
                header::CACHE_CONTROL.as_str(),
                "private, max-age=86400, stale-while-revalidate=604800",
            ))
            .header(Header::new(
                header::ETAG.as_str(),
                self.file.metadata.sha256_hash().encode_hex::<String>(),
            ))
            .sized_body(self.file.data.len(), std::io::Cursor::new(self.file.data));
        if let Some(dt) = dt {
            builder
                .header(Header::new(
                    header::LAST_MODIFIED.as_str(),
                    dt.to_rfc2822().replace("+0000", "GMT"),
                ))
                .header(Header::new(
                    header::EXPIRES.as_str(),
                    (dt + Duration::days(7))
                        .to_rfc2822()
                        .replace("+0000", "GMT"),
                ))
                .ok()
        } else {
            builder.ok()
        }
    }
}

#[async_trait]
impl rocket::route::Handler for StaticFiles {
    async fn handle<'r>(
        &self,
        req: &'r Request<'_>,
        _: rocket::Data<'r>,
    ) -> rocket::route::Outcome<'r> {
        use rocket::http::uri::{fmt::Path, Segments};
        rocket::route::Outcome::from(
            req,
            match (|req: &'r Request<'_>| -> Option<_> {
                let path = req
                    .segments::<Segments<'_, Path>>(0..)
                    .ok()
                    .and_then(|segments| segments.to_path_buf(false).ok())?;
                let extension = path
                    .extension()
                    .and_then(std::ffi::OsStr::to_str)?
                    .to_owned();
                StaticAsset::get(path.to_str()?).map(|file| StaticFile { extension, file })
            })(req)
            {
                Some(resp) => Ok(resp),
                None => Err(Status::NotFound),
            },
        )
    }
}

impl From<StaticFiles> for Vec<rocket::Route> {
    fn from(sf: StaticFiles) -> Self {
        let mut route = rocket::Route::ranked(100, rocket::http::Method::Get, "/<path..>", sf);
        route.name = Some("StaticFiles".into());
        vec![route]
    }
}

pub fn routes() -> Vec<Route> {
    routes![
        home,
        channel_redirect,
        channel_stream,
        channel_search,
        channel,
    ]
}

#[catch(default)]
fn catch_default(status: Status, _: &Request) -> String {
    format!("{}", status)
}

pub fn catchers() -> Vec<rocket::Catcher> {
    rocket::catchers![catch_default]
}
