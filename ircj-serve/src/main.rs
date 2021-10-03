extern crate rocket;

use figment::providers::Format;
use rocket::fairing::AdHoc;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Config {
    pub db: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            db: "".to_owned(),
        }
    }
}

#[rocket::launch]
async fn get_rocket() -> rocket::Rocket<rocket::Build> {
    env_logger::init();

    let figment = rocket::Config::figment()
        .merge(figment::providers::Serialized::defaults(Config::default()))
        .merge(figment::providers::Toml::file("ircj-serve.toml"))
        .merge(figment::providers::Env::prefixed("IRCJ_"));

    rocket::custom(figment)
        .attach(AdHoc::config::<Config>())
        .attach(AdHoc::on_ignite(
            "Connect to database and migrate",
            |rocket| async move {
                let db = rocket.state::<Config>().unwrap().db.clone(); // attached above.
                rocket.manage(
                    ircjournal::db::create_db(&db)
                        .await
                        .expect("connecting and migrating the DB"),
                )
            },
        ))
        .attach(ircj_serve::watch::fairing())
        .register("/", ircj_serve::route::catchers())
        .mount("/static", ircj_serve::route::StaticFiles {})
        .mount("/", ircj_serve::route::routes())
}
