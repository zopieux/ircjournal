use rocket::fairing::AdHoc;
use std::str::FromStr;
use tokio::sync::broadcast;

use ircjournal::{
    model::{Message, ServerChannel},
    Database, MessageEvent,
};

use crate::view;

const CAPACITY: usize = 1024;

pub fn save_broadcast_task(
    db: Database,
    broadcast: broadcast::Sender<MessageEvent>,
    mut shutdown: rocket::Shutdown,
) {
    tokio::spawn(async move {
        let mut listener = sqlx::postgres::PgListener::connect_with(&db)
            .await
            .expect("listener");
        listener.listen("new_message").await.expect("listen");
        loop {
            tokio::select! {
                _ = &mut shutdown => break,
                Ok(notification) = listener.recv() => {
                    if let Ok(message) = serde_json::from_str::<Message>(notification.payload()) {
                        let sc = ServerChannel::from_str(&message.channel.as_ref().unwrap()).unwrap();
                        let _ = broadcast.send((sc.clone(), view::formatted_message(&message)));
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
    AdHoc::on_liftoff("Save and broadcast new lines", |rocket| {
        Box::pin(async move {
            save_broadcast_task(
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
