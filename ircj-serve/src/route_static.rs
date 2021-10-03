use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use core::{
    convert::From,
    option::{
        Option,
        Option::{None, Some},
    },
};
use hex::ToHex;
use rocket::{
    http::{
        hyper::header,
        uri::{fmt::Path, Segments},
        Header, Status,
    },
    request::Request,
    route::Outcome,
};

#[derive(rust_embed::RustEmbed)]
#[folder = "$OUT_DIR/static/"]
#[prefix = ""]
#[include = "**/*.js"]
#[include = "**/*.css"]
#[include = "**/*.png"]
struct StaticAsset;

#[derive(Clone)]
pub struct StaticFiles;

struct StaticFile {
    extension: String,
    file: rust_embed::EmbeddedFile,
}

impl StaticFile {
    fn etag(&self) -> String {
        self.file.metadata.sha256_hash().encode_hex::<String>()
    }
}

impl<'r> rocket::response::Responder<'r, 'static> for StaticFile {
    fn respond_to(self, _: &'r Request<'_>) -> rocket::response::Result<'static> {
        let content_type = rocket::http::ContentType::from_extension(&self.extension)
            .ok_or_else(|| Status::new(400))?;
        let dt =
            self.file.metadata.last_modified().map(|lm| {
                DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(lm as i64, 0), Utc)
            });
        let mut builder = rocket::response::Response::build();
        builder
            .header(content_type)
            .header(Header::new(
                header::CACHE_CONTROL.as_str(),
                "public, no-transform, max-age=86400",
            ))
            .header(Header::new(header::ETAG.as_str(), self.etag()))
            .sized_body(self.file.data.len(), std::io::Cursor::new(self.file.data));
        if let Some(dt) = dt {
            builder
                .header(Header::new(
                    header::LAST_MODIFIED.as_str(),
                    dt.to_rfc2822().replace("+0000", "GMT"),
                ))
                // .header(Header::new(
                //     header::EXPIRES.as_str(),
                //     (dt + Duration::days(7))
                //         .to_rfc2822()
                //         .replace("+0000", "GMT"),
                // ))
                .ok()
        } else {
            builder.ok()
        }
    }
}

fn get_static_file(req: &Request) -> Option<StaticFile> {
    let path = req
        .segments::<Segments<'_, Path>>(0..)
        .ok()
        .and_then(|segments| segments.to_path_buf(false).ok())?;
    let extension = path
        .extension()
        .and_then(std::ffi::OsStr::to_str)?
        .to_owned();
    StaticAsset::get(path.to_str()?).map(|file| StaticFile { extension, file })
}

#[async_trait]
impl rocket::route::Handler for StaticFiles {
    async fn handle<'r>(&self, req: &'r Request<'_>, _: rocket::Data<'r>) -> Outcome<'r> {
        let etag = req
            .headers()
            .get_one(rocket::http::hyper::header::IF_NONE_MATCH.as_str())
            .unwrap_or_default();
        match get_static_file(req) {
            None => Outcome::failure(Status::NotFound),
            Some(file) if file.etag() == etag => Outcome::failure(Status::NotModified),
            Some(file) => Outcome::from(req, file),
        }
    }
}

impl From<StaticFiles> for Vec<rocket::Route> {
    fn from(sf: StaticFiles) -> Self {
        let mut route = rocket::Route::ranked(30, rocket::http::Method::Get, "/<path..>", sf);
        route.name = Some("StaticFiles".into());
        vec![route]
    }
}
