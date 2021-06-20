#![feature(decl_macro)]

#[macro_use]
extern crate rocket;

use std::{path::Path, time::Instant};

use rocket::{fairing::AdHoc, Build, Orbit, Rocket};

use ircjournal::{backfill, db, routes::routes, weechat::Weechat, Config};

async fn run_backfills(rocket: &Rocket<Orbit>) {
    let conn = db::DbConn::get_one(&rocket)
        .await
        .expect("database connection for migrating");
    // let cfg = rocket
    //     .state::<rocket_sync_db_pools::Config>()
    //     .expect("config");
    let now = Instant::now();
    let (sc, inserted) = backfill::<Weechat>(
        Path::new("/home/alex/dev/ircjournal/irc.freenode.#test.weechatlog"),
        conn,
        2000,
        4,
    )
    .await
    .expect("backfill");
    println!(
        "backfilled {} messages for {:?} in {:?}",
        inserted,
        sc,
        Instant::now() - now
    );
}

#[launch]
fn get_rocket() -> Rocket<Build> {
    let figment = rocket::Config::figment()
        .merge(figment::providers::Serialized::defaults(Config::default()))
        .merge(figment::providers::Env::prefixed("IRCJ_"));
    let rocket = rocket::custom(figment);
    rocket
        .attach(db::DbConn::fairing())
        .attach(AdHoc::on_ignite("Disel migrations", db::run_migrations))
        .attach(AdHoc::config::<rocket::Config>())
        .attach(AdHoc::on_ignite("Manage db config", |r| async {
            let config = rocket_sync_db_pools::Config::from("ircjournal", &r).unwrap();
            r.manage(config)
        }))
        .attach(AdHoc::on_liftoff("Backfill logs", |r| {
            Box::pin(async move {
                run_backfills(r).await;
            })
        }))
        .mount("/static", rocket::fs::FileServer::from("static"))
        .mount("/", routes())
}
