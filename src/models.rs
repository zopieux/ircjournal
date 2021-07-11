use super::schema::message;
use chrono::Datelike;
use std::collections::HashSet;

pub type Datetime = chrono::DateTime<chrono::Utc>;

#[derive(Debug, Clone, PartialEq)]
pub struct Day(pub(crate) chrono::NaiveDate);

pub type MessagesPerDay = (Day, Vec<Message>);

#[derive(PartialEq, Clone, Debug)]
pub struct ServerChannel {
    pub server: String,
    pub channel: String,
}

pub(crate) type Nicks = HashSet<String>;

#[derive(Debug)]
pub(crate) struct ChannelInfo {
    pub(crate) sc: ServerChannel,
    pub(crate) first_day: Day,
    pub(crate) last_day: Day,
    pub(crate) topic: Option<Message>,
    pub(crate) nicks: Nicks,
}

#[derive(Queryable, PartialEq, Debug)]
pub struct Message {
    pub id: i32,
    pub channel: String,
    pub nick: Option<String>,
    pub line: Option<String>,
    pub opcode: Option<String>,
    pub oper_nick: Option<String>,
    pub payload: Option<String>,
    pub timestamp: Datetime,
}

#[derive(Insertable, Debug)]
#[table_name = "message"]
pub struct NewMessage {
    pub channel: String,
    pub nick: Option<String>,
    pub line: Option<String>,
    pub opcode: Option<String>,
    pub oper_nick: Option<String>,
    pub payload: Option<String>,
    pub timestamp: Datetime,
}

impl Day {
    pub(crate) fn today() -> Self {
        Self(chrono::Local::today().naive_local())
    }

    pub(crate) fn new(ts: &Datetime) -> Self {
        Self(ts.date().naive_utc())
    }

    pub(crate) fn succ(&self) -> Self {
        Self(self.0.succ())
    }

    pub(crate) fn pred(&self) -> Self {
        Self(self.0.pred())
    }

    pub(crate) fn midnight(&self) -> Datetime {
        Datetime::from_utc(self.0.and_hms(0, 0, 0), chrono::Utc)
    }

    pub(crate) fn ymd(&self) -> String {
        self.0.format("%Y-%m-%d").to_string()
    }

    pub(crate) fn day(&self) -> u32 {
        self.0.day()
    }

    pub(crate) fn month(&self) -> String {
        self.0.format("%B").to_string()
    }
}

impl ServerChannel {
    pub fn new(server: &str, channel: &str) -> Self {
        Self {
            server: server.to_string(),
            channel: channel.to_string(),
        }
    }

    pub fn db_encode(&self) -> String {
        format!("{}/{}", self.server, self.channel)
    }

    pub fn db_decode(encoded: &str) -> Option<Self> {
        let (server, channel) = encoded.split_once('/')?;
        Some(Self::new(server, channel))
    }
}

impl Message {
    pub(crate) fn sc(&self) -> ServerChannel {
        ServerChannel::db_decode(&*self.channel).unwrap()
    }

    pub(crate) fn is_talk(&self) -> bool {
        match (&self.opcode, &self.nick) {
            (None, Some(nick)) if !nick.is_empty() => true,
            _ => false,
        }
    }

    pub(crate) fn is_me_tell(&self) -> bool {
        match self.opcode.as_deref() {
            Some("me") => true,
            _ => false,
        }
    }

    pub(crate) fn id_str(&self) -> String {
        self.id.to_string()
    }

    pub(crate) fn epoch(&self) -> i64 {
        self.timestamp.timestamp()
    }
}
