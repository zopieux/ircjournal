#[macro_use]
extern crate rocket;

use chrono::{Datelike, NaiveDate};
use ircjournal::model::{Datetime, Message, ServerChannel};
use std::{collections::HashSet, str::FromStr};

mod db;
pub mod route;
mod route_adapt;
mod route_static;
mod view;
pub mod watch;

pub(crate) type Nicks = HashSet<String>;

#[derive(Debug)]
pub struct ChannelInfo {
    pub(crate) sc: ServerChannel,
    pub(crate) first_day: Day,
    pub(crate) last_day: Day,
    pub(crate) topic: Option<Message>,
    pub(crate) nicks: Nicks,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Day(pub(crate) chrono::NaiveDate);

impl Day {
    pub(crate) fn today() -> Self {
        Self(chrono::Local::now().date_naive())
    }

    pub(crate) fn is_today_or_future(&self) -> bool {
        Self::today().0 <= self.0
    }

    pub(crate) fn succ(&self) -> Self {
        Self(self.0.succ_opt().unwrap_or_default())
    }

    pub(crate) fn pred(&self) -> Self {
        Self(self.0.pred_opt().unwrap_or_default())
    }

    pub(crate) fn midnight(&self) -> Datetime {
        Datetime::from_naive_utc_and_offset(
            self.0.and_hms_opt(0, 0, 0).unwrap_or_default(),
            chrono::Utc,
        )
    }

    pub(crate) fn ymd(&self) -> String {
        self.0.format("%Y-%m-%d").to_string()
    }

    pub(crate) fn day(&self) -> u32 {
        self.0.day()
    }

    pub(crate) fn day_str(&self) -> String {
        self.0.format("%d").to_string()
    }

    pub(crate) fn month(&self) -> String {
        self.0.format("%B").to_string()
    }

    pub(crate) fn month_short(&self) -> String {
        self.0.format("%b").to_string()
    }

    pub(crate) fn year(&self) -> String {
        self.0.format("%Y").to_string()
    }
}

impl From<Datetime> for Day {
    fn from(ts: Datetime) -> Self {
        Self(ts.date_naive())
    }
}

impl From<chrono::NaiveDate> for Day {
    fn from(ts: NaiveDate) -> Self {
        Datetime::from_naive_utc_and_offset(
            ts.and_hms_opt(0, 0, 0).unwrap_or_default(),
            chrono::Utc,
        )
        .into()
    }
}

trait MessageExt {
    fn sc(&self) -> ServerChannel;
    fn is_talk(&self) -> bool;
    fn is_me_tell(&self) -> bool;
    fn id_str(&self) -> String;
    fn epoch(&self) -> i64;
}

impl MessageExt for Message {
    fn sc(&self) -> ServerChannel {
        ServerChannel::from_str(self.channel.as_ref().unwrap()).unwrap()
    }

    fn is_talk(&self) -> bool {
        matches!((&self.opcode, &self.nick), (None, Some(nick)) if !nick.is_empty())
    }

    fn is_me_tell(&self) -> bool {
        matches!(self.opcode.as_deref(), Some("me"))
    }

    fn id_str(&self) -> String {
        self.id.to_string()
    }

    fn epoch(&self) -> i64 {
        self.timestamp.timestamp()
    }
}
