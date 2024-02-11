use crate::{
    model::{Datetime, NewMessage, ServerChannel},
    Database,
};
use sqlx::{Postgres, QueryBuilder};
use std::time::Duration;

pub async fn create_db(uri: &str) -> Result<Database, sqlx::Error> {
    // TODO: configurable options.
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(4))
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

fn push_message_values<'a>(builder: &mut QueryBuilder<'a, Postgres>, messages: &'a [NewMessage]) {
    builder.push_values(messages, |mut b, message| {
        b /**/
            .push_bind(message.channel.as_ref().expect("no channel"))
            .push_bind(message.nick.clone())
            .push_bind(message.line.clone())
            .push_bind(message.opcode.clone())
            .push_bind(message.oper_nick.clone())
            .push_bind(message.payload.clone())
            .push_bind(message.timestamp);
    });
}

async fn execute_batch_insert_messages(
    mut builder: QueryBuilder<'_, Postgres>,
    db: &Database,
) -> Option<u64> {
    builder
        .build()
        .execute(db)
        .await
        .ok()
        .map(|info| info.rows_affected())
}

pub async fn batch_insert_messages(db: &Database, messages: &[NewMessage]) -> Option<u64> {
    // language=sql
    let mut builder = QueryBuilder::new(
        r#"
        INSERT INTO message ("channel", "nick", "line", "opcode", "oper_nick", "payload", "timestamp")
        "#,
    );
    push_message_values(&mut builder, messages);
    execute_batch_insert_messages(builder, db).await
}

pub async fn batch_insert_messages_and_notify(
    db: &Database,
    messages: &[NewMessage],
) -> Option<u64> {
    // language=sql
    let mut builder = QueryBuilder::new(
        r#"
        WITH new_rows AS (
            INSERT INTO message ("channel", "nick", "line", "opcode", "oper_nick", "payload", "timestamp")
        "#,
    );
    push_message_values(&mut builder, messages);
    // language=sql
    builder.push(
        r#"
            RETURNING *
        )
        SELECT pg_notify('new_message', row_to_json(row)::text) FROM new_rows row
        "#,
    );
    execute_batch_insert_messages(builder, db).await
}
