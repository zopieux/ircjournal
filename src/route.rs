use maud::Markup;
use rocket::{response::Redirect, uri, Route, State};

use crate::{
    db::{self, Database, OnePage},
    model::{ChannelInfo, Day, Message, MessagesPerDay, ServerChannel},
    view, MessageEvent,
};
use chrono::Datelike;
use itertools::Itertools;
use rocket::{
    http::Status,
    response::stream::{Event, EventStream},
};
use tokio::{
    select,
    sync::broadcast::{error::RecvError, Sender},
};

#[get("/")]
async fn home(db: Database) -> Option<Markup> {
    let channels = db.run(|c| db::channels(&c)).await;
    Some(view::home(&channels))
}

#[get("/<sc>")]
async fn channel_redirect(db: Database, sc: ServerChannel) -> Redirect {
    let sc2 = sc.clone();
    Redirect::temporary(
        if let Some(last) = db.run(move |c| db::last_message(&c, &sc2)).await {
            let day = Day::new(&last.timestamp);
            uri!(channel(&sc, day))
        } else {
            uri!("/")
        },
    )
}

#[get("/<sc>/stream")]
async fn channel_stream(
    sc: ServerChannel,
    db: Database,
    queue: &State<Sender<MessageEvent>>,
    mut end: rocket::Shutdown,
) -> Option<EventStream![]> {
    let sc_ = sc.clone();
    if !db.run(move |c| db::channel_exists(&c, &sc_)).await {
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
async fn channel(db: Database, sc: ServerChannel, day: Day) -> Option<Markup> {
    let (messages, info, active_days) = {
        let sc = sc.clone();
        let day = day.clone();
        db.run(move |c| {
            (
                db::messages_channel_day(&c, &sc, &day),
                db::channel_info(&c, &sc, &day),
                db::channel_month_index(&c, &sc.clone(), day.0.year(), day.0.month()),
            )
        })
        .await
    };
    let truncated = messages.len() == db::HARD_MESSAGE_LIMIT;
    Some(view::channel(
        &info?,
        &day,
        &messages,
        &active_days,
        truncated,
    ))
}

#[get("/<sc>/search?<query>&<page>")]
async fn search(db: Database, sc: ServerChannel, query: &str, page: Option<u64>) -> Option<Markup> {
    let page = page.unwrap_or(1);
    let (result_page, info): (OnePage<Message>, Option<ChannelInfo>) = {
        let query = query.to_string();
        db.run(move |c| {
            (
                db::channel_search(&c, &sc, &query, page as i64),
                // FIXME: we don't need most stuff
                db::channel_info(&c, &sc, &Day::today()),
            )
        })
        .await
    };
    let messages: Vec<MessagesPerDay> = result_page
        .records
        .into_iter()
        .group_by(|msg| msg.timestamp.date().naive_utc())
        .into_iter()
        .map(|(day, group)| (Day(day), group.collect()))
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

#[get("/favicon")]
fn favicon() -> Status {
    Status::NotFound
}

pub fn routes() -> Vec<Route> {
    routes![
        favicon,
        home,
        channel_redirect,
        channel,
        search,
        channel_stream
    ]
}
