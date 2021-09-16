use rocket::{
    http::uri::fmt::{Formatter, FromUriParam, Path, UriDisplay},
    request::FromParam,
};
use std::io::ErrorKind;

pub type Datetime = chrono::DateTime<chrono::Utc>;

#[derive(PartialEq, Clone, Debug)]
pub struct ServerChannel {
    pub server: String,
    pub channel: String,
}

#[derive(PartialEq, Debug, serde::Deserialize, sqlx::Type)]
pub struct Message {
    pub id: i32,
    pub channel: Option<String>,
    pub nick: Option<String>,
    pub line: Option<String>,
    pub opcode: Option<String>,
    pub oper_nick: Option<String>,
    pub payload: Option<String>,
    pub timestamp: Datetime,
}

#[derive(Debug, sqlx::Type)]
pub struct NewMessage {
    pub channel: Option<String>,
    pub nick: Option<String>,
    pub line: Option<String>,
    pub opcode: Option<String>,
    pub oper_nick: Option<String>,
    pub payload: Option<String>,
    pub timestamp: Datetime,
}

impl ServerChannel {
    pub fn new(server: &str, channel: &str) -> Self {
        Self {
            server: server.to_string(),
            channel: channel.to_string(),
        }
    }
}

impl std::fmt::Display for ServerChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.server, self.channel)
    }
}

impl std::str::FromStr for ServerChannel {
    type Err = std::io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (server, channel) = s.split_once('/').ok_or(std::io::ErrorKind::InvalidData)?;
        Ok(Self::new(server, channel))
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
            format!("invalid server/channel: {}", encoded),
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
