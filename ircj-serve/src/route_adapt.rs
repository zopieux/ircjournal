use std::fmt;

use rocket::{
    http::uri::fmt::{Formatter, FromUriParam, Path, UriDisplay},
    request::FromParam,
};

use crate::Day;

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

impl FromUriParam<Path, Day> for Day {
    type Target = Day;

    fn from_uri_param(param: Day) -> Self::Target {
        param
    }
}
