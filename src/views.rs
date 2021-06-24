use maud::{html, Markup, PreEscaped, DOCTYPE};
use rocket::uri;

use crate::{
    models::{ChannelInfo, Day, Message, ServerChannel},
    routes,
};
use chrono::Datelike;
use core::iter;
use regex::Regex;
use std::collections::HashSet;

const VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");
const LINK_TRUNCATE_LENGTH: usize = 64;

fn some_or_empty(s: &Option<String>) -> String {
    s.as_ref().unwrap_or(&"".to_string()).to_string()
}

fn channel_link(sc: &ServerChannel, day: &Day, content: Markup) -> Markup {
    html! { a href=(uri!(routes::channel(sc, day.clone()))) { (content) } }
}

fn message_link(m: &Message, content: Markup) -> Markup {
    let day = Day::new(&m.timestamp);
    html! { a href={(uri!(routes::channel(&m.sc(), day))) "#" (m.id_str())} { (content) } }
}

macro_rules! format_some {
    ($s: expr, $fmt: literal) => {
        if let Some(ss) = $s {
            format!($fmt, ss)
        } else {
            "".to_string()
        }
    };
}

fn base<T>(title: Option<&str>, content: PreEscaped<T>) -> Markup
where
    T: AsRef<str>,
{
    let version = match VERSION {
        Some(v) => v,
        _ => "unknown",
    };
    html! {
        (DOCTYPE)
        head {
            meta charset="utf-8";
            link rel="stylesheet" href="/static/css/ircjournal.css";
            title {
                @if let Some(t) = title { (t) " — " }
                "ircjournal"
            }
        }
        body {
            (content)
            script type="text/javascript" src="/static/js/ircjournal.js" {}
        }
    }
}

pub(crate) fn home(channels: &[ServerChannel]) -> Markup {
    base(
        Some("Home"),
        html! {
            h1 { "Hello world." }
            ul {
                @for sc in channels {
                    li {
                        a.server-channel href=(uri!(routes::channel_redirect(sc))) {
                            span.server { (sc.server) }
                            span.channel { (sc.channel) }
                        }
                    }
                }
            }
        },
    )
}

pub(crate) fn channel(
    info: &ChannelInfo,
    day: &Day,
    messages: &[Message],
    days_with_messages: &HashSet<u32>,
    truncated: bool,
) -> Markup {
    let sc = &info.sc;
    let cal = render_calendar(day, &info.sc, days_with_messages);

    let date_sel = |from, to, jump| {
        let link_date = |day: &Day, target| channel_link(sc, day, html! { (day.ymd()) });
        html! {
            p.days {
                @if info.first_day != *day {
                    span."day-first" { (link_date(&info.first_day, "")) }
                    @if info.first_day != day.pred() { span."day-prev" { (link_date(&day.pred(), to)) } }
                } @else { span."day-nope" { "logs start here" } }
                span."day-today" {
                    span { (day.ymd()) }
                    a.jump #(from) href={"#" (to)} { (jump) }
                }
                @if info.last_day != *day {
                    @if info.last_day != day.succ() { span."day-next" { (link_date(&day.succ(), to)) } }
                    span."day-last" { (link_date(&info.last_day, "")) }
                } @else { span."day-nope" { "logs ends here" } }
            }
        }
    };
    base(
        Some(&format!("{} — Channel", sc.db_encode())),
        html! {
            h1 { (sc.server) " — " (sc.channel) }
            aside {
                (cal)
                @if let Some(topic) = info.topic.as_ref() {
                    blockquote.last-topic {
                        (some_or_empty(&topic.payload))
                        cite { "Set by " (format_nick(topic.nick.as_ref().unwrap())) " on " (message_link(topic, html!{ (topic.timestamp.format("%Y-%m-%d at %H:%M")) })) }
                    }
                }
            }
            (date_sel("", "bottom", "\u{22ce}"))
            table.messages {
                @for msg in messages { (message(msg, info)) }
            }
            @if messages.is_empty() {
                p.empty { "No messages for " (day.ymd()) "." }
            }
            @if truncated { div.warning { "Only displaying the first " (messages.len()) " lines to prevent browser slowness." } }
            (date_sel("bottom", "", "\u{22cf}"))
        },
    )
}

fn format_nick(nick: &str) -> Markup {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(nick.as_ref());
    let color = hasher.finalize() % 16;
    html! { span.nick.{ "nick-" (color) } { (nick) } }
}

fn format_hl_nick(content: &str, nicks: &HashSet<String>) -> Markup {
    lazy_static! {
        static ref NICK: Regex = Regex::new(r#"^([A-Za-z_0-9|.`-]+)"#).unwrap();
    }
    if !nicks.is_empty() {
        if NICK.is_match(content) {
            let cap = NICK.captures(content).unwrap().get(1).unwrap();
            let nick = cap.as_str().to_string();
            if nicks.contains(&nick) {
                return html! { (format_nick(&nick)) (content[cap.end()..]) };
            }
        }
    }
    html! { (content) }
}

fn format_content(content: &Option<String>, nicks: &HashSet<String>) -> Markup {
    use linkify::{LinkFinder, LinkKind};
    lazy_static! {
        static ref LINK_FINDER: LinkFinder = {
            let mut lf = LinkFinder::new();
            lf.kinds(&[LinkKind::Url]);
            lf
        };
    }
    let markup: Vec<Markup> = if let Some(body) = content {
        LINK_FINDER.spans(body).map(|span| {
            let s = span.as_str();
            match span.kind() {
                Some(LinkKind::Url) => {
                    let truncated = s.len() > LINK_TRUNCATE_LENGTH;
                    let st = if truncated { s.chars().take(LINK_TRUNCATE_LENGTH).collect::<String>() } else { s.to_string() };
                    html! { a.link.trunc[truncated] ref="noreferrer nofollow external" href=(s) title=(s) { (st) } }
                }
                _ => format_hl_nick(s, nicks),
            }
        }).collect()
    } else {
        vec![]
    };
    html! { @for m in markup { (m) } }
}

fn format_message(m: &Message, info: &ChannelInfo) -> Markup {
    html! {
        @match m.opcode.as_deref() {
            None | Some("me") => (format_content(&m.line, &info.nicks)),
            Some("joined") => (format_nick(m.nick.as_ref().unwrap())) " has joined",
            Some("left") => (format_nick(m.nick.as_ref().unwrap())) " has left" (format_some!(&m.payload, " ({})")),
            Some("quit") => (format_nick(m.nick.as_ref().unwrap())) " has quit" (format_some!(&m.payload, " ({})")),
            Some("topic") => (format_nick(m.nick.as_ref().unwrap())) " changed the topic to " span."new-topic" { (some_or_empty(&m.payload)) },
            Some("nick") => (format_nick(m.nick.as_ref().unwrap())) " is now known as " (format_nick(m.nick.as_ref().unwrap())),
            _ => "UNIMPLEMENTED: " (some_or_empty(&m.opcode)),
        }
    }
}

fn message(m: &Message, info: &ChannelInfo) -> Markup {
    html! {
        tbody#(m.id_str()).msg data-timestamp=(m.epoch()) data-oper=(some_or_empty(&m.opcode)) {
            tr {
                td.ts { a.tslink href={"#" (m.id_str())} { (m.timestamp.format("%H:%M")) } }
                @if m.is_talk() {
                    td.nick."me-tell"[m.is_me_tell()] { (format_nick(m.nick.as_ref().unwrap())) }
                } @else {
                    td.nick.operation { "*" }
                }
                td.line { (format_message(m, info)) }
            }
        }
    }
}

fn render_calendar(day: &Day, sc: &ServerChannel, days_with_messages: &HashSet<u32>) -> Markup {
    let weeks = &calendar(day, days_with_messages).weeks;
    html! {
        section.calendar {
            nav {
                span { "<" }
                span { (day.month()) }
                span { ">" }
            }
            table {
                thead {
                    tr {
                        th { "Mo" } th { "Tu" } th { "We" } th { "Th" } th { "Fr" } th { "Sa" } th { "Su" }
                    }
                }
                tbody {
                    @for week in weeks {
                        tr {
                            @for opt in week {
                                td {
                                    @if let Some((day, linked)) = opt.as_ref() {
                                        @if *linked { a href=(uri!(routes::channel(sc, day.clone()))) { (day.day()) } }
                                        @else { (day.day()) }
                                    } @else { span {} }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

type OneWeek = Vec<Option<(Day, bool)>>;

struct OneMonth {
    first_day: Day,
    weeks: Vec<OneWeek>,
}

fn calendar(day: &Day, days_with_message: &HashSet<u32>) -> OneMonth {
    use chrono::{Datelike, NaiveDate, Weekday};

    // Start of week.
    let sow = NaiveDate::from_ymd(day.0.year(), day.0.month(), 1);

    let num_days = NaiveDate::from_ymd(
        sow.year() + sow.month() as i32 / 12,
        1 + sow.month() % 12,
        1,
    )
    .signed_duration_since(sow)
    .num_days();

    let gen = |d: u32| {
        Some((
            Day(sow.with_day(d).unwrap()),
            days_with_message.contains(&d),
        ))
    };

    let offset_monday = sow.weekday().num_days_from_monday() as usize;

    let mut days = (1..=num_days).into_iter();
    let first_week: OneWeek = iter::repeat(None)
        .take(offset_monday)
        .chain((1..=(7 - offset_monday)).map(|_| gen(days.next().unwrap() as u32)))
        .collect();
    let days = days.collect::<Vec<i64>>();
    let mut weeks: Vec<OneWeek> = iter::once(first_week)
        .chain(
            days.chunks(7)
                .map(|chunk| chunk.iter().map(|d| gen(*d as u32)).collect()),
        )
        .collect();
    let last_week = weeks.last_mut().unwrap();
    last_week.extend((0..7 - last_week.len()).map(|_| None));
    OneMonth {
        first_day: Day(sow),
        weeks,
    }
}

#[test]
fn test_lol() {
    let day = &Day(chrono::NaiveDate::from_ymd(2021, 6, 22));
    let present = HashSet::<u32>::from(vec![1, 3, 9, 24].iter().map(|u| *u as u32).collect());
    let sc = Some((ServerChannel::db_decode("a/b").unwrap(), present));
    calendar(day, &sc);
}
