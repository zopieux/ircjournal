use chrono::Datelike;
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

pub use crate::route_static::StaticFiles;
use ircjournal::{model::ServerChannel, Database, MessageEvent};

use crate::{view, ChannelRemap, Day};

#[get("/")]
async fn home(db: &State<Database>, remap: &State<ChannelRemap>) -> Option<Markup> {
    let channels = crate::db::channels(db, remap).await;
    Some(view::home(&channels))
}

#[get("/<sc>")]
async fn channel_redirect(
    db: &State<Database>,
    remap: &State<ChannelRemap>,
    sc: ServerChannel,
) -> Redirect {
    let sc = remap.canonical(&sc);
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
    db: &State<Database>,
    remap: &State<ChannelRemap>,
    queue: &State<Sender<MessageEvent>>,
    sc: ServerChannel,
    mut end: rocket::Shutdown,
) -> Option<EventStream![]> {
    let sc = remap.canonical(&sc);
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
async fn channel(
    db: &State<Database>,
    remap: &State<ChannelRemap>,
    sc: ServerChannel,
    day: Day,
) -> Option<Markup> {
    let sc = remap.canonical(&sc);
    let (messages, info, active_days) = {
        tokio::join!(
            crate::db::messages_channel_day(db, &sc, remap, &day),
            crate::db::channel_info(db, &sc, remap, &day),
            crate::db::channel_month_index(db, &sc, remap, day.0.year(), day.0.month()),
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
    remap: &State<ChannelRemap>,
    sc: ServerChannel,
    query: &str,
    page: Option<u64>,
) -> Option<Markup> {
    let sc = remap.canonical(&sc);
    let page = page.unwrap_or(1);
    let (result_page, info) = {
        let query = query.to_string();
        let today = Day::today();
        tokio::join!(
            crate::db::channel_search(db, &sc, remap, &query, page as i64),
            crate::db::channel_info(db, &sc, remap, &today),
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
