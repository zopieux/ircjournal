use crate::{
    model::{Datetime, NewMessage, ServerChannel},
    Database,
};
use std::time::Duration;

pub async fn create_db(uri: &str) -> Result<Database, sqlx::Error> {
    // TODO: configurable options.
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect_timeout(Duration::from_secs(4))
        .connect(uri)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

pub async fn last_message_ts(db: &Database, sc: &ServerChannel) -> Option<Datetime> {
    // language=sql
    sqlx::query!(
        r#"
        SELECT max("timestamp") "timestamp" FROM "message" WHERE "channel" = $1
    "#,
        sc.to_string()
    )
    .fetch_one(db)
    .await
    .unwrap()
    .timestamp
}

macro_rules! batch_messages {
    ($db:ident, $messages:ident, $body:literal) => { {
        if $messages.is_empty() {
            return Some(0);
        }
        // TODO: https://github.com/launchbadge/sqlx/issues/294, https://github.com/launchbadge/sqlx/issues/1240.
        let mut v_channel: Vec<&str> = Vec::with_capacity($messages.len());
        let mut v_nick: Vec<Option<String>> = Vec::with_capacity($messages.len());
        let mut v_line: Vec<Option<String>> = Vec::with_capacity($messages.len());
        let mut v_opcode: Vec<Option<String>> = Vec::with_capacity($messages.len());
        let mut v_oper_nick: Vec<Option<String>> = Vec::with_capacity($messages.len());
        let mut v_payload: Vec<Option<String>> = Vec::with_capacity($messages.len());
        let mut v_timestamp: Vec<Datetime> = Vec::with_capacity($messages.len());
        $messages.iter().for_each(|m| {
            v_channel.push(m.channel.as_ref().unwrap());
            v_nick.push(m.nick.clone());
            v_line.push(m.line.clone());
            v_opcode.push(m.opcode.clone());
            v_oper_nick.push(m.oper_nick.clone());
            v_payload.push(m.payload.clone());
            v_timestamp.push(m.timestamp);
        });
        // language=sql
        sqlx::query($body)
            .bind(v_channel)
            .bind(v_nick)
            .bind(v_line)
            .bind(v_opcode)
            .bind(v_oper_nick)
            .bind(v_payload)
            .bind(v_timestamp)
            .execute($db)
            .await
            .ok()
            .map(|info| info.rows_affected())
    } }
}

pub async fn batch_insert_messages(db: &Database, messages: &[NewMessage]) -> Option<u64> {
    // language=sql
    batch_messages!(
        db,
        messages,
        r#"
        INSERT INTO message ("channel", "nick", "line", "opcode", "oper_nick", "payload", "timestamp")
        SELECT * FROM UNNEST($1, $2, $3, $4, $5, $6, $7)
    "#
    )
}

pub async fn batch_insert_messages_and_notify(
    db: &Database,
    messages: &[NewMessage],
) -> Option<u64> {
    // language=sql
    batch_messages!(
        db,
        messages,
        r#"
        WITH new_rows AS (
            INSERT INTO message ("channel", "nick", "line", "opcode", "oper_nick", "payload", "timestamp")
            SELECT * FROM UNNEST($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
        )
        SELECT pg_notify('new_message', row_to_json(row)::text) FROM new_rows row
    "#
    )
}
