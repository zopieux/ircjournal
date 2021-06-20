use maud::{DOCTYPE, html, Markup, PreEscaped};
use rocket::uri;

use crate::{
    models::{Day, Message},
    routes, models::ServerChannel,
};
use crate::models::ChannelInfo;
use regex::Regex;
use std::collections::HashSet;

const VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");
const LINK_TRUNCATE_LENGTH: usize = 64;

fn some_or_empty(s: &Option<String>) -> String {
    s.as_ref().unwrap_or(&"".to_string()).to_string()
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
        })
}

pub(crate) fn channel(info: &ChannelInfo, day: &Day, messages: &[Message], truncated: bool) -> Markup {
    let sc = &info.sc;
    let date_sel = |from, to, jump| {
        let link_date = |day: &Day, target| html! { a href={(uri!(routes::channel(sc, day.clone())))} { (day.ymd()) } };
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
            @if let Some((topic_date, topic_nick, topic)) = &info.topic {
                blockquote.last-topic {
                    (topic)
                    cite { "Set by " (topic_nick) " on " (topic_date.format("%Y-%m-%d at %H:%M")) }
                }
            }
            (date_sel("top", "bottom", "\u{22ce}"))
            table.messages {
                @for msg in messages { (message(msg, info)) }
            }
            @if truncated { div.warning { "Only displaying the first " (messages.len()) " lines to prevent browser slowness." } }
            (date_sel("bottom", "top", "\u{22cf}"))
        })
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
                },
                _ => format_hl_nick(s, nicks),
            }
        }).collect()
    } else { vec![] };
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
        tbody.msg data-timestamp=(m.timestamp.to_rfc3339()) data-oper=(some_or_empty(&m.opcode)) {
            tr#(m.id_str()) {
                td.ts { a href={"#" (m.id_str())} { (m.timestamp.format("%H:%M")) } }
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

fn base<T>(title: Option<&str>, content: PreEscaped<T>) -> Markup where T: AsRef<str> {
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
        body { (content) }
    }
}
