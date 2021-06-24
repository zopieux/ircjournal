use std::{fmt, io::ErrorKind};

use rocket::{
    http::uri::fmt::{Formatter, FromUriParam, Path, UriDisplay},
    request::FromParam,
};

use crate::models::{Day, ServerChannel};

impl UriDisplay<Path> for Day {
    fn fmt(&self, f: &mut Formatter<'_, Path>) -> fmt::Result {
        f.write_raw(self.0.format("%Y-%m-%d").to_string())
    }
}

impl<'r> FromParam<'r> for Day {
    type Error = chrono::ParseError;

    fn from_param(param: &'r str) -> Result<Self, Self::Error> {
        let date = chrono::NaiveDate::parse_from_str(param, "%Y-%m-%d")?;
        Ok(Day(date))
    }
}

impl<'r> FromUriParam<Path, Day> for Day {
    type Target = Day;

    fn from_uri_param(param: Day) -> Self::Target {
        param
    }
}

impl UriDisplay<Path> for ServerChannel {
    fn fmt(&self, f: &mut Formatter<'_, Path>) -> std::fmt::Result {
        f.write_value(format!("{}:{}", &self.server, &self.channel))
    }
}

impl<'r> FromParam<'r> for ServerChannel {
    type Error = std::io::Error;

    fn from_param(encoded: &'r str) -> Result<Self, Self::Error> {
        let (server, channel) = encoded.split_once(':').ok_or(Self::Error::new(
            ErrorKind::InvalidInput,
            "invalid server/channel",
        ))?;
        Ok(Self::new(server, channel))
    }
}

impl<'r> FromUriParam<Path, &'r ServerChannel> for ServerChannel {
    type Target = &'r ServerChannel;

    fn from_uri_param(param: &'r ServerChannel) -> Self::Target {
        param
    }
}
