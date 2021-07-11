use maud::Markup;
use rocket::{response::Redirect, uri, Route};

use crate::{
    db::{self, DbConn, OnePage},
    models::{ChannelInfo, Day, Message, MessagesPerDay, ServerChannel},
    views,
};
use chrono::Datelike;
use itertools::Itertools;

#[get("/")]
async fn home(db: DbConn) -> Option<Markup> {
    let channels = db.run(|c| db::channels(&c)).await;
    println!("{:?}", channels);
    Some(views::home(&channels))
}

#[get("/<sc>")]
async fn channel_redirect(db: DbConn, sc: ServerChannel) -> Redirect {
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

#[get("/<sc>/<day>")]
async fn channel(db: DbConn, sc: ServerChannel, day: Day) -> Option<Markup> {
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
    Some(views::channel(
        &info?,
        &day,
        &messages,
        &active_days,
        truncated,
    ))
}

#[get("/<sc>/search?<query>&<page>")]
async fn search(db: DbConn, sc: ServerChannel, query: &str, page: Option<u64>) -> Option<Markup> {
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
    Some(views::search(
        &info?,
        query,
        &messages,
        page,
        result_page.page_count,
        result_page.total,
    ))
}

pub fn routes() -> Vec<Route> {
    routes![home, channel_redirect, channel, search,]
}
