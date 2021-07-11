use maud::{html, Markup, PreEscaped, DOCTYPE};
use rocket::uri;

use crate::{
    models::{ChannelInfo, Datetime, Day, Message, MessagesPerDay, Nicks, ServerChannel},
    routes,
};
use chrono::{Datelike, NaiveDate, NaiveDateTime};
use core::iter;
use itertools::Itertools;
use regex::Regex;
use std::collections::HashSet;

const VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");
const LINK_TRUNCATE_LENGTH: usize = 64;

enum LinkType {
    ABSOLUTE,
    RELATIVE,
}

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

fn base<A, C>(title: Option<&str>, aside: PreEscaped<A>, content: PreEscaped<C>) -> Markup
where
    A: AsRef<str>,
    C: AsRef<str>,
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
            aside {
                h1 { (title.unwrap_or("ircjournal")) }
                (aside)
            }
            main {
                (content)
            }
            script type="text/javascript" src="/static/js/ircjournal.js" {}
        }
    }
}

pub(crate) fn home(channels: &[ServerChannel]) -> Markup {
    base(
        Some("Home"),
        html! {},
        html! {
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
    active_days: &HashSet<u32>,
    truncated: bool,
) -> Markup {
    let sc = &info.sc;
    let cal = render_calendar(day, &info.sc, active_days);

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
        Some(&sc.db_encode()),
        html! {
            (cal)
            hr;
            (search_form(sc, ""))
        },
        html! {
            (date_sel("", "bottom", "\u{22ce}"))
            @if let Some(topic) = info.topic.as_ref() {
                blockquote.last-topic {
                    (some_or_empty(&topic.payload))
                    cite { "Set by " (format_nick(topic.nick.as_ref().unwrap())) " on " (message_link(topic, html!{ (topic.timestamp.format("%Y-%m-%d at %H:%M")) })) }
                }
            }
            table.messages {
                @for msg in messages { (message(msg, sc, &info.nicks, LinkType::RELATIVE)) }
            }
            @if messages.is_empty() {
                p.empty { "No messages for " (day.ymd()) "." }
            }
            @if truncated { div.warning { "Only displaying the first " (messages.len()) " lines to prevent browser slowness." } }
            (date_sel("bottom", "", "\u{22cf}"))
        },
    )
}

pub(crate) fn search(
    info: &ChannelInfo,
    query: &str,
    messages: &[MessagesPerDay],
    page: u64,
    page_count: i64,
    result_count: i64,
) -> Markup {
    let sc = &info.sc;
    let pages: Vec<Markup> = (1..=page_count)
        .map(|p| {
            html! {
                @if p as u64 == page {
                    (p)
                } @else {
                    a href=(uri!(routes::search(sc, query, Some(p as u64)))) { (p) }
                }
            }
        })
        .intersperse(html! { " " })
        .collect();
    base(
        Some(&sc.db_encode()),
        search_form(sc, query),
        html! {
            div {
                @if result_count == 0 {
                    "No message found."
                } @else {
                    "Found " (result_count) " results."
                    @if page_count > 1 {
                        " Pages: "
                        @for p in pages { (p) }
                    }
                }
            }
            table.messages {
                @for per_day in messages {
                    tbody.search-date { tr { td colspan="3" { (per_day.0.ymd()) } } }
                    @for msg in &per_day.1 { (message(msg, sc, &info.nicks, LinkType::ABSOLUTE)) }
                }
            }
        },
    )
}

fn search_form(sc: &ServerChannel, query: &str) -> Markup {
    html! {
        form.search action=(uri!(routes::search(sc, "", None as Option<u64>))) method="get" {
            input name="query" value=(query) placeholder="Search…";
            button type="submit" { "Search" }
        }
    }
}

fn format_nick(nick: &str) -> Markup {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(nick.as_ref());
    let color = hasher.finalize() % 16;
    html! { span.nick.{ "nick-" (color) } { (nick) } }
}

fn format_hl_nick(content: &str, nicks: &Nicks) -> Markup {
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

fn format_content(content: &Option<String>, nicks: &Nicks) -> Markup {
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

fn format_message(m: &Message, nicks: &Nicks) -> Markup {
    html! {
        @match m.opcode.as_deref() {
            None | Some("me") => (format_content(&m.line, nicks)),
            Some("joined") => (format_nick(m.nick.as_ref().unwrap())) " has joined",
            Some("left") => (format_nick(m.nick.as_ref().unwrap())) " has left" (format_some!(&m.payload, " ({})")),
            Some("quit") => (format_nick(m.nick.as_ref().unwrap())) " has quit" (format_some!(&m.payload, " ({})")),
            Some("topic") => (format_nick(m.nick.as_ref().unwrap())) " changed the topic to " span."new-topic" { (some_or_empty(&m.payload)) },
            Some("nick") => (format_nick(m.nick.as_ref().unwrap())) " is now known as " (format_nick(m.nick.as_ref().unwrap())),
            _ => "UNIMPLEMENTED: " (some_or_empty(&m.opcode)),
        }
    }
}

fn message(m: &Message, sc: &ServerChannel, nicks: &Nicks, link_type: LinkType) -> Markup {
    let rel = match link_type {
        LinkType::ABSOLUTE => uri!(routes::channel(sc, Day::new(&m.timestamp))).to_string(),
        _ => "".to_owned(),
    };
    html! {
        tbody#(m.id_str()).msg data-timestamp=(m.epoch()) data-oper=(some_or_empty(&m.opcode)) {
            tr {
                td.ts { a.tslink title=(m.timestamp.to_rfc3339()) href={(rel) "#" (m.id_str())} { (m.timestamp.format("%H:%M")) } }
                @if m.is_talk() {
                    td.nick."me-tell"[m.is_me_tell()] { (format_nick(m.nick.as_ref().unwrap())) }
                } @else {
                    td.nick.operation { "*" }
                }
                td.line { (format_message(m, &nicks)) }
            }
        }
    }
}

fn render_calendar(day: &Day, sc: &ServerChannel, active_days: &HashSet<u32>) -> Markup {
    let month = &calendar(day, active_days);
    let today = &Day::today();
    html! {
        section.calendar {
            nav {
                span { a href=(uri!(routes::channel(sc, month.prev.clone()))) { "<" } }
                span { (day.month()) }
                span { a href=(uri!(routes::channel(sc, month.succ.clone()))) { ">" } }
            }
            table {
                thead {
                    tr {
                        th { "Mo" } th { "Tu" } th { "We" } th { "Th" } th { "Fr" } th { "Sa" } th { "Su" }
                    }
                }
                tbody {
                    @for week in &month.weeks {
                        tr {
                            @for opt in week {
                                td {
                                    @if let Some((d, linked)) = opt.as_ref() {
                                        @let fmted = format!("{:\u{00A0}>2}", d.day());
                                        @if *linked { a href=(uri!(routes::channel(sc, d.clone()))) .active[d == day].today[d == today] { (fmted) } }
                                        @else { span.active[d == day].today[d == today] { (fmted) } }
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
    weeks: Vec<OneWeek>,
    prev: Day,
    succ: Day,
}

fn calendar(day: &Day, active_days: &HashSet<u32>) -> OneMonth {
    use chrono::{Datelike, NaiveDate};

    // Start of week.
    let sow = NaiveDate::from_ymd(day.0.year(), day.0.month(), 1);

    let num_days = NaiveDate::from_ymd(
        sow.year() + sow.month() as i32 / 12,
        1 + sow.month() % 12,
        1,
    )
    .signed_duration_since(sow)
    .num_days();

    let closest_day = |hint: &NaiveDate| {
        let mut d = day.day();
        loop {
            let p = hint.with_day(d);
            if let Some(prev) = p {
                break Day(prev);
            }
            d -= 1;
        }
    };

    let prev_month_day = closest_day(&sow.pred());
    let succ_month_day =
        closest_day(&NaiveDate::from_ymd(day.0.year(), day.0.month(), num_days as u32).succ());

    let gen = |d: u32| Some((Day(sow.with_day(d).unwrap()), active_days.contains(&d)));

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
        weeks,
        prev: prev_month_day,
        succ: succ_month_day,
    }
}

#[test]
fn test_lol() {
    let day = &Day(chrono::NaiveDate::from_ymd(2021, 6, 22));
    let present: HashSet<u32> = vec![1, 3, 9, 24].into_iter().collect();
    calendar(day, &present);
}
