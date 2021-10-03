use rocket::fairing::AdHoc;
use std::{str::FromStr, time::Duration};
use tokio::sync::broadcast;

use ircjournal::{
    model::{Message, ServerChannel},
    Database, MessageEvent,
};

const CAPACITY: usize = 1024;
const AWAKE_LISTEN_INTERVAL: Duration = Duration::from_secs(60);

pub fn broadcast_message_task(
    db: Database,
    broadcast: broadcast::Sender<MessageEvent>,
    mut shutdown: rocket::Shutdown,
) {
    tokio::spawn(async move {
        let mut listener = sqlx::postgres::PgListener::connect_with(&db)
            .await
            .expect("listener");
        listener.listen("new_message").await.expect("listen");
        let mut wakeup = tokio::time::interval(AWAKE_LISTEN_INTERVAL);
        loop {
            tokio::select! {
                _ = &mut shutdown => break,
                 // Ensures we call recv() from time to time, to resist the connection going away under our feet.
                _ = wakeup.tick() => continue,
                Ok(notification) = listener.recv() => {
                    if let Ok(message) = serde_json::from_str::<Message>(notification.payload()) {
                        let sc = ServerChannel::from_str(message.channel.as_ref().unwrap()).unwrap();
                        let nicks = crate::db::channel_info(&db, &sc, &message.timestamp.into()).await
                            .map(|info| info.nicks).unwrap_or_default();
                        let _ = broadcast.send((sc.clone(), crate::view::formatted_message(&message, &nicks)));
                        debug!("New message for {:?}, id {}", &sc, message.id);
                    }
                },
            }
        }
    });
}

pub fn fairing() -> AdHoc {
    AdHoc::on_ignite("Manage MessageEvent queue", |rocket| async move {
        rocket
            .manage(broadcast::channel::<MessageEvent>(CAPACITY).0)
            .attach(watch_fairing())
    })
}

fn watch_fairing() -> AdHoc {
    AdHoc::on_liftoff("Broadcast live new lines", |rocket| {
        Box::pin(async move {
            broadcast_message_task(
                rocket
                    .state::<Database>()
                    .unwrap() // attached above
                    .clone(),
                rocket
                    .state::<broadcast::Sender<MessageEvent>>()
                    .unwrap() // attached above
                    .clone(),
                rocket.shutdown(),
            );
        })
    })
}
