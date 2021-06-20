use diesel::prelude::*;
use diesel::insert_into;
use rocket::{Build, Rocket};
use rocket_sync_db_pools::database;

use crate::{
    models::{Day, Message, NewMessage, ServerChannel, ChannelInfo},
};
use crate::models::Datetime;
use std::collections::HashSet;

pub(crate) static HARD_MESSAGE_LIMIT: usize = 10_000;

#[database("ircjournal")]
pub struct DbConn(PgConnection);

pub async fn run_migrations(rocket: Rocket<Build>) -> Rocket<Build> {
    embed_migrations!("migrations");
    let conn = DbConn::get_one(&rocket)
        .await
        .expect("database connection for diesel migrations");
    conn.run(|c| embedded_migrations::run(c).expect("diesel migrations"))
        .await;
    rocket
}

pub(crate) fn last_message(conn: &PgConnection, sc: &ServerChannel) -> Option<Message> {
    use crate::schema::message::dsl::*;
    message
        .filter(channel.eq(sc.db_encode()))
        .order(timestamp.desc())
        .first::<Message>(conn)
        .ok()
}

pub(crate) fn channels(conn: &PgConnection) -> Vec<ServerChannel> {
    use crate::schema::message::dsl::*;
    message
        .select(channel)
        .distinct()
        .load::<String>(conn)
        .unwrap_or_default()
        .iter()
        .filter_map(|s| ServerChannel::db_decode(s))
        .collect()
}

pub(crate) fn messages_channel_day(
    conn: &PgConnection,
    sc: &ServerChannel,
    day: &Day,
) -> Vec<Message> {
    use crate::schema::message::dsl::*;
    let next_day = day.succ();
    message
        .filter(channel.eq(sc.db_encode()))
        .filter(timestamp.ge(day.midnight()))
        .filter(timestamp.lt(next_day.midnight()))
        .order(timestamp.asc())
        .limit(HARD_MESSAGE_LIMIT as i64)
        .load::<Message>(conn)
        .unwrap_or_default()
}

pub(crate) fn channel_info(conn: &PgConnection, sc: ServerChannel, before: &Day) -> Option<ChannelInfo> {
    use crate::schema::message::dsl::*;
    use diesel::dsl::sql;
    let (min_ts, max_ts) = message
        .select( (sql("min(timestamp)"), sql("max(timestamp)")))
        .filter(channel.eq(sc.db_encode()))
        .get_result::<(Datetime, Datetime)>(conn)
        .ok()?;
    let topic: Option<(Datetime, String, String)> = message
        .select( (timestamp, nick, payload))
        .filter(payload.is_not_null())
        .filter(payload.ne(""))
        .filter(opcode.eq("topic"))
        .filter(channel.eq(sc.db_encode()))
        .filter(timestamp.lt(before.succ().midnight()))
        .order(timestamp.desc())
        .first::<(Datetime, Option<String>, Option<String>)>(conn)
        .optional()
        .ok()?
        .map(|(ts, n, topic)| (ts, n.unwrap(), topic.unwrap()));
    let nicks = message
        .select(nick)
        .distinct()
        .filter(nick.is_not_null())
        .filter(nick.ne(""))
        .filter(channel.eq(sc.db_encode()))
        .limit(1000)
        .load::<Option<String>>(conn)
        .ok()?
        .iter()
        .filter_map(|n| n.clone())
        .collect::<HashSet<String>>();
    Some(ChannelInfo{
        sc,
        first_day: Day::new(&min_ts),
        last_day: Day::new(&max_ts),
        topic,
        nicks,
    })
}

pub(crate) fn batch_insert_messages(conn: &PgConnection, vec: &[NewMessage]) -> Option<usize> {
    use crate::schema::message::dsl::*;
    insert_into(message).values(vec).execute(conn).ok()
}
