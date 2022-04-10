use lazy_static::lazy_static;
use std::{collections::HashSet, str::FromStr};

use crate::{ChannelInfo, Day};
use ircjournal::{
    model::{Message, ServerChannel},
    Database,
};

pub(crate) type MessagesPerDay = (Day, Vec<Message>);

const SEARCH_PAGE_SIZE: u64 = 100;
const HARD_NICK_LIMIT: u64 = 1_000;
pub(crate) const HARD_MESSAGE_LIMIT: usize = 10_000;

pub(crate) struct Paginated<U> {
    pub(crate) records: Vec<U>,
    pub(crate) total: i64,
    pub(crate) page_count: i64,
}

pub(crate) async fn channels(db: &Database) -> Vec<ServerChannel> {
    // language=sql
    sqlx::query!(r#"SELECT "channel" FROM all_channels()"#)
        .fetch_all(db)
        .await
        .unwrap_or_default()
        .iter()
        .filter_map(|s| ServerChannel::from_str(s.channel.as_ref().unwrap()).ok())
        .collect()
}

pub(crate) async fn channel_exists(db: &Database, sc: &ServerChannel) -> bool {
    // language=sql
    sqlx::query!(
        r#"SELECT FROM "message" WHERE "channel" = $1 LIMIT 1"#,
        sc.to_string()
    )
    .fetch_optional(db)
    .await
    .unwrap()
    .is_some()
}

pub(crate) async fn channel_info(
    db: &Database,
    sc: &ServerChannel,
    before: &Day,
) -> Option<ChannelInfo> {
    let channel = sc.to_string();
    // language=sql
    sqlx::query!(r#"
        WITH "ts" AS (SELECT min("timestamp") "first!", max("timestamp") "last!" FROM "message" WHERE "channel" = $1)
        SELECT "first!", "last!", array(SELECT "nick" FROM all_nicks($1, $2)) "nicks!",
               (SELECT row("message".*) FROM "message"
                WHERE "channel" = $1 AND "opcode" = 'topic' AND coalesce("payload", '') != '' AND "timestamp" < $3
                ORDER BY "timestamp" DESC LIMIT 1) "topic?:Message"
        FROM "ts" GROUP BY 1, 2, 3 LIMIT 1
    "#, &channel, HARD_NICK_LIMIT as i64, before.succ().midnight())
        .fetch_optional(db)
        .await
        .unwrap()
        .map(|r| ChannelInfo {
            sc: sc.clone(),
            first_day: r.first.into(),
            last_day: r.last.into(),
            topic: r.topic,
            nicks: r.nicks.into_iter().collect(),
        })
}

pub(crate) async fn messages_channel_day(
    db: &Database,
    sc: &ServerChannel,
    day: &Day,
) -> Vec<Message> {
    // language=sql
    sqlx::query_as!(
        Message,
        r#"
        SELECT * FROM "message"
        WHERE "channel" = $1 AND "timestamp" >= $2 AND "timestamp" < $3
        ORDER BY "timestamp"
        LIMIT $4
    "#,
        sc.to_string(),
        day.midnight(),
        day.succ().midnight(),
        HARD_MESSAGE_LIMIT as i64
    )
    .fetch_all(db)
    .await
    .unwrap()
}

pub(crate) async fn channel_month_index(
    db: &Database,
    sc: &ServerChannel,
    year: i32,
    month: u32,
) -> HashSet<u32> {
    let from: Day = chrono::NaiveDate::from_ymd(year, month, 1).into();
    let to: Day = chrono::NaiveDate::from_ymd(year + month as i32 / 12, 1 + month % 12, 1).into();
    // language=sql
    sqlx::query!(
        r#"
        SELECT DISTINCT EXTRACT(DAY FROM "timestamp")::smallint "day!"
        FROM "message"
        WHERE "channel" = $1 AND ("opcode" IS NULL OR "opcode" = 'me')
        AND "timestamp" >= $2 AND "timestamp" < $3
        "#,
        sc.to_string(),
        from.midnight(),
        to.midnight()
    )
    .fetch_all(db)
    .await
    .unwrap_or_default()
    .iter()
    .map(|r| r.day as u32)
    .collect()
}

pub(crate) async fn channel_search(
    db: &Database,
    sc: &ServerChannel,
    query: &str,
    page: i64,
) -> Paginated<Message> {
    // Try to find nick:<something> to build a non-empty nick filter.
    lazy_static! {
        static ref NICK: regex::Regex =
            regex::Regex::new(r#"\b(nick:[A-Za-z_0-9|.`\*-]+)"#).unwrap();
    }
    let (query, nick_filter) = if let Some(mat) = NICK.find(query) {
        let mut query = query.to_owned();
        query.replace_range(mat.range(), "");
        (
            query.trim().to_string(),
            mat.as_str()[5..].replace('*', "%"),
        )
    } else {
        (query.trim().to_string(), "".to_string())
    };
    if query.is_empty() && nick_filter.is_empty() {
        return Paginated {
            page_count: 0,
            records: vec![],
            total: 0,
        };
    }
    let per_page = SEARCH_PAGE_SIZE as i64;
    let offset = (page - 1) * per_page;
    struct Record {
        message: Message,
        headline: String,
        total: i64,
    }
    // language=sql
    let rows = sqlx::query_as!(Record, r#"
        WITH "query" AS (
            SELECT row("message".*) "message!:Message",
                   ts_headline('english', "line", plainto_tsquery('english', $2), U&'StartSel=\E000, StopSel=\E001') "headline!"
            FROM "message"
            WHERE "channel" || '' = $1
              AND coalesce("opcode", '') = ''
              AND CASE WHEN $2 = '' THEN TRUE ELSE to_tsvector('english', "nick" || ' ' || "line") @@ plainto_tsquery('english', $2) END
              AND CASE WHEN $5 = '' THEN TRUE ELSE "nick" LIKE $5 END
            ORDER BY "timestamp" DESC
        )
        SELECT *, COUNT(*) OVER () "total!"
        FROM "query" t LIMIT $3 OFFSET $4
"#, sc.to_string(), &query, per_page, offset, nick_filter)
        .fetch_all(db)
        .await
        .unwrap();
    let total = rows.first().map(|r| r.total).unwrap_or(0);
    let records = rows
        .into_iter()
        .map(|r| Message {
            line: Some(r.headline),
            ..r.message
        })
        .collect();
    Paginated {
        page_count: (total as f64 / per_page as f64).ceil() as i64,
        records,
        total,
    }
}
