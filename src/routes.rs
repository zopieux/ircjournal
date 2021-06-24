use maud::Markup;
use rocket::{response::Redirect, uri, Route};

use crate::{
    db::{self, DbConn},
    models::{Day, ServerChannel},
    views,
};
use chrono::Datelike;

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
    let (messages, info, days_with_messages) = {
        let sc = sc.clone();
        let day = day.clone();
        db.run(move |c| {
            (
                db::messages_channel_day(&c, &sc, &day),
                db::channel_info(&c, sc.clone(), &day),
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
        &days_with_messages,
        truncated,
    ))
}

pub fn routes() -> Vec<Route> {
    routes![home, channel_redirect, channel,]
}
