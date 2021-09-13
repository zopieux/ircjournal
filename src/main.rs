#![feature(decl_macro)]

#[macro_use]
extern crate rocket;

use rocket::fairing::AdHoc;
use sloggers::Build;
use std::path::PathBuf;
use tokio::sync::{broadcast, mpsc};

#[launch]
fn get_rocket() -> rocket::Rocket<rocket::Build> {
    let figment = rocket::Config::figment()
        .merge(figment::providers::Serialized::defaults(
            ircjournal::Config::default(),
        ))
        .merge(figment::providers::Env::prefixed("IRCJ_"));

    use rocket::config::LogLevel;
    let log_level = figment.extract_inner::<LogLevel>("log_level").unwrap();
    let logger = match log_level {
        LogLevel::Off => sloggers::null::NullLoggerBuilder {}.build().unwrap(),
        level => {
            let mut log_builder = sloggers::terminal::TerminalLoggerBuilder::new();
            log_builder.destination(sloggers::terminal::Destination::Stderr);
            log_builder.level(match level {
                LogLevel::Critical => sloggers::types::Severity::Critical,
                LogLevel::Debug => sloggers::types::Severity::Debug,
                LogLevel::Normal => sloggers::types::Severity::Info,
                LogLevel::Off => unreachable!(),
            });
            log_builder.build().unwrap()
        }
    };
    let logger_for_rocket = logger.clone();

    let (new_files_tx, new_files_rx) = mpsc::unbounded_channel::<PathBuf>();
    let (new_messages_tx, new_messages_rx) =
        mpsc::unbounded_channel::<ircjournal::model::NewMessage>();
    rocket::custom(figment)
        .attach(AdHoc::config::<ircjournal::Config>())
        .manage(logger_for_rocket)
        .manage(broadcast::channel::<ircjournal::MessageEvent>(1024).0)
        .attach(ircjournal::Database::fairing())
        .attach(AdHoc::on_ignite(
            "Migrate database",
            ircjournal::run_migrations,
        ))
        .attach(AdHoc::on_liftoff("Backfill existing logs", |rocket| {
            Box::pin(async move {
                ircjournal::run_backfills(
                    rocket.state::<slog::Logger>().unwrap(),
                    rocket.state::<ircjournal::Config>().unwrap(),
                    &ircjournal::Database::get_one(rocket).await.unwrap(),
                )
                .await;
            })
        }))
        .attach(AdHoc::on_liftoff("[bg] Watch for file changes", |rocket| {
            Box::pin(async move {
                ircjournal::watch_for_changes_task(
                    rocket.state::<slog::Logger>().unwrap().clone(),
                    new_messages_tx,
                    new_files_rx,
                    rocket.shutdown(),
                );
                for path in &rocket.state::<ircjournal::Config>().unwrap().logs {
                    let _ = new_files_tx.send(path.clone());
                }
            })
        }))
        .attach(AdHoc::on_liftoff(
            "[bg] Save and broadcast new lines",
            |rocket| {
                Box::pin(async move {
                    ircjournal::save_broadcast_task(
                        rocket.state::<slog::Logger>().unwrap().clone(),
                        ircjournal::Database::get_one(rocket).await.unwrap(),
                        rocket
                            .state::<broadcast::Sender<ircjournal::MessageEvent>>()
                            .unwrap()
                            .clone(),
                        new_messages_rx,
                        rocket.shutdown(),
                    );
                })
            },
        ))
        .mount("/static", rocket::fs::FileServer::from("static"))
        .mount("/", ircjournal::routes())
}
