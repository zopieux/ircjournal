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
use ircjournal::{
    model::{Message, ServerChannel},
    Database, MessageEvent,
};

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
        .group_by(|msg| msg.timestamp.date_naive())
        .into_iter()
        .map(|(day, group)| {
            (day.into(), {
                // By now all messages are still in descending chronological order.
                // For a given day to make sense, reverse order, within each day.
                let mut messages: Vec<Message> = group.collect();
                messages.reverse();
                messages
            })
        })
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
