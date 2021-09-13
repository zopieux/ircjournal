use std::collections::HashSet;

use chrono::NaiveDate;
use diesel::{
    dsl::sql,
    insert_into,
    pg::Pg,
    prelude::*,
    query_builder::{AstPass, Query, QueryFragment},
    query_dsl::LoadQuery,
    sql_types::{BigInt, Text},
};
use rocket::{Build, Rocket};
use rocket_sync_db_pools::database;

use crate::model::{ChannelInfo, Datetime, Day, Message, NewMessage, Nicks, ServerChannel};

const HARD_NICK_LIMIT: usize = 1_000;
pub(crate) const HARD_MESSAGE_LIMIT: usize = 10_000;

#[database("ircjournal")]
pub struct Database(PgConnection);

pub async fn run_migrations(rocket: Rocket<Build>) -> Rocket<Build> {
    embed_migrations!("migrations");
    let conn = Database::get_one(&rocket)
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
        .order(channel.asc())
        .distinct()
        .load::<String>(conn)
        .unwrap_or_default()
        .iter()
        .filter_map(|s| ServerChannel::db_decode(s))
        .collect()
}

pub(crate) fn channel_exists(conn: &PgConnection, sc: &ServerChannel) -> bool {
    use crate::schema::message::dsl::*;
    diesel::select(diesel::dsl::exists(
        message.filter(channel.eq(sc.db_encode())),
    ))
    .get_result(conn)
    .unwrap_or(false)
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

pub(crate) fn channel_info(
    conn: &PgConnection,
    sc: &ServerChannel,
    before: &Day,
) -> Option<ChannelInfo> {
    use crate::schema::message::dsl::*;
    let (min_ts, max_ts) = message
        .select((sql("min(timestamp)"), sql("max(timestamp)")))
        .filter(channel.eq(sc.db_encode()))
        .get_result::<(Datetime, Datetime)>(conn)
        .ok()?;
    let topic: Option<Message> = message
        .filter(payload.is_not_null())
        .filter(payload.ne(""))
        .filter(opcode.eq("topic"))
        .filter(channel.eq(sc.db_encode()))
        .filter(timestamp.lt(before.succ().midnight()))
        .order(timestamp.desc())
        .first::<Message>(conn)
        .optional()
        .ok()?;
    let nicks = message
        .select(nick)
        .distinct()
        .filter(nick.is_not_null())
        .filter(nick.ne(""))
        .filter(channel.eq(sc.db_encode()))
        .limit(HARD_NICK_LIMIT as i64)
        .load::<Option<String>>(conn)
        .ok()?
        .into_iter()
        .filter_map(|n| n)
        .collect::<Nicks>();
    Some(ChannelInfo {
        sc: sc.clone(),
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

pub fn insert_message(conn: &PgConnection, new_message: &NewMessage) -> Option<Message> {
    use crate::schema::message::dsl::*;
    insert_into(message)
        .values(new_message)
        .get_result(conn)
        .ok()
}

pub(crate) fn channel_month_index(
    conn: &PgConnection,
    sc: &ServerChannel,
    year: i32,
    month: u32,
) -> HashSet<u32> {
    use crate::schema::message::dsl::*;
    let from = NaiveDate::from_ymd(year, month, 1);
    let to = NaiveDate::from_ymd(year + month as i32 / 12, 1 + month % 12, 1);
    message
        .select(sql("EXTRACT(DAY FROM timestamp)::smallint"))
        .distinct()
        .filter(channel.eq(sc.db_encode()))
        .filter(opcode.is_null().or(opcode.eq("me")))
        .filter(timestamp.ge(from.and_hms(0, 0, 0)))
        .filter(timestamp.lt(to.and_hms(0, 0, 0)))
        .load::<i16>(conn)
        .unwrap_or_default()
        .into_iter()
        .map(|x| x as u32)
        .collect()
}

pub(crate) fn channel_search(
    conn: &PgConnection,
    sc: &ServerChannel,
    query: &str,
    page: i64,
) -> OnePage<Message> {
    use crate::schema::message::dsl::*;
    use regex::Regex;
    lazy_static! {
        static ref NICK: Regex = Regex::new(r#"\b(nick:[A-Za-z_0-9|.`\*-]+)"#).unwrap();
    }
    let headline = sql("ts_headline('english', line, plainto_tsquery('english', ")
        .bind::<Text, _>(query)
        .sql("), 'StartSel = \u{e000}, StopSel = \u{e001}') AS line");
    let (query, nick_filter) = if let Some(mat) = NICK.find(query) {
        let mut query = query.to_owned();
        query.replace_range(mat.range(), "");
        (
            query.trim().to_owned(),
            Some(mat.as_str()[5..].replace('*', "%")),
        )
    } else {
        (query.to_owned(), None)
    };
    let mut q = message
        .select((
            id, channel, nick, headline, opcode, oper_nick, payload, timestamp,
        ))
        .filter(opcode.is_null())
        // From whitequark's irclogger:
        // postgres' query planner is dumb as a brick and will use any index except the btree_gin
        // one if channel is specified as-is.
        .filter(sql("channel || '' = ").bind::<Text, _>(sc.db_encode()))
        .into_boxed();
    if let Some(n) = nick_filter {
        q = q.filter(nick.like(n))
    }
    if !query.is_empty() {
        q = q.filter(
            sql("to_tsvector('english', nick || ' ' || line) @@ plainto_tsquery('english', ")
                .bind::<Text, _>(query)
                .sql(")"),
        );
    }
    q.order(timestamp.desc())
        .paginate(100, page)
        .load_and_count_pages::<Message>(conn)
        .unwrap()
}

pub(crate) trait Paginate: Sized {
    fn paginate(self, per_page: i64, page: i64) -> Paginated<Self>;
}

impl<T> Paginate for T {
    fn paginate(self, per_page: i64, page: i64) -> Paginated<Self> {
        Paginated {
            query: self,
            per_page,
            page,
        }
    }
}

#[derive(Debug, Clone, Copy, QueryId)]
pub(crate) struct Paginated<T> {
    query: T,
    per_page: i64,
    page: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct OnePage<U> {
    pub(crate) records: Vec<U>,
    pub(crate) total: i64,
    pub(crate) page_count: i64,
}

impl<T> Paginated<T> {
    pub fn load_and_count_pages<U>(self, conn: &PgConnection) -> QueryResult<OnePage<U>>
    where
        Self: LoadQuery<PgConnection, (U, i64)>,
    {
        let per_page = self.per_page;
        let results = self.load::<(U, i64)>(conn)?;
        let total = results.get(0).map(|x| x.1).unwrap_or(0);
        let records = results.into_iter().map(|x| x.0).collect();
        let page_count = (total as f64 / per_page as f64).ceil() as i64;
        Ok(OnePage {
            records,
            total,
            page_count,
        })
    }
}

impl<T: Query> Query for Paginated<T> {
    type SqlType = (T::SqlType, BigInt);
}

impl<T> RunQueryDsl<PgConnection> for Paginated<T> {}

impl<T> QueryFragment<Pg> for Paginated<T>
where
    T: QueryFragment<Pg>,
{
    fn walk_ast(&self, mut out: AstPass<Pg>) -> QueryResult<()> {
        out.push_sql("SELECT *, COUNT(*) OVER () FROM (");
        self.query.walk_ast(out.reborrow())?;
        out.push_sql(") t LIMIT ");
        out.push_bind_param::<BigInt, _>(&self.per_page)?;
        out.push_sql(" OFFSET ");
        let offset = (self.page - 1) * self.per_page;
        out.push_bind_param::<BigInt, _>(&offset)?;
        Ok(())
    }
}
