use chrono::{Datelike, NaiveDate};
use lazy_static::lazy_static;
use maud::{html, Markup, PreEscaped, DOCTYPE};
use regex::Regex;
use rocket::uri;
use std::{collections::HashSet, str::FromStr};

use ircjournal::model::{Message, ServerChannel};

use crate::{db::MessagesPerDay, route, ChannelInfo, Day, MessageExt, Nicks};

const VERSION: Option<&'static str> = option_env!("CARGO_PKG_VERSION");
const LINK_TRUNCATE_LENGTH: usize = 40;

enum LinkType {
    Absolute,
    Relative,
}

fn some_or_empty(s: &Option<String>) -> String {
    s.as_ref().unwrap_or(&"".to_string()).to_string()
}

fn channel_link(sc: &ServerChannel, day: &Day, content: Markup) -> Markup {
    html! { a href=(uri!(route::channel(sc, day.clone()))) { (content) } }
}

fn message_link(m: &Message, content: Markup) -> Markup {
    html! { a href={(uri!(route::channel(&m.sc(), m.timestamp.into()))) "#" (m.id_str())} { (content) } }
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

fn base<A, C>(title: &str, aside: PreEscaped<A>, content: PreEscaped<C>) -> Markup
where
    A: AsRef<str>,
    C: AsRef<str>,
{
    html! {
        (DOCTYPE)
        head {
            meta charset="utf-8";
            meta name="viewport" content="width=device-width, initial-scale=1";
            link rel="icon" type="image/png" href="/static/favicon.png";
            link rel="stylesheet" href="/static/css/ircjournal.css";
            title {
                @if !title.is_empty() { (title) " — " }
                "ircjournal"
            }
        }
        body {
            aside {
                div {
                    h1 { (title) }
                    h2.brand { "ircjournal" }
                }
                (aside)
            }
            main {
                (content)
            }
        }
    }
}

pub(crate) fn home(channels: &[ServerChannel]) -> Markup {
    base(
        "Channel list",
        html! {
            @if channels.is_empty() {
                p { em { "No channel. Ingest some logs!" } }
            }
            ul.chanlist {
                @for sc in channels {
                    li {
                        a.server-channel href=(uri!(route::channel_redirect(sc))) {
                            (sc.server) "/" (sc.channel)
                        }
                    }
                }
            }
        },
        html! {
            p {
                "This is "
                a hrefe="https://github.com/zopieux/ircjournal" rel="nofollow" { "ircjournal" }
                " v" (VERSION.unwrap_or("?")) ", brought to you by "
                a href="https://github.com/zopieux" { "zopieux@" } "."
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
    let cal = render_calendar(day, info, active_days);

    let date_sel = |from, to, jump, jump_tip| {
        let _link_date = |day: &Day| channel_link(sc, day, html! { (day.ymd()) });
        html! {
            p.days {
                @if info.first_day != *day {
                    @let pred = day.pred();
                    span."day-prev" { (channel_link(sc, &pred, html! { "< " (pred.day_str()) })) }
                } @else { span."day-nope" { "logs start here" } }
                span."day-today".current {
                    span { (day.ymd()) }
                    a.jump #(from) href={"#" (to)} title=(jump_tip) { (jump) }
                }
                @if info.last_day != *day {
                     @let next = day.succ();
                    span."day-next" { (channel_link(sc, &next, html! { (next.day_str()) " >" })) }
                } @else { span."day-nope" { "logs ends here" } }
            }
        }
    };
    base(
        &sc.to_string(),
        html! {
            (home_link())
            (cal)
            (search_form(sc, ""))
            label for="show-join-part" title="If checked, join, part, quit and nick messages are shown." {
                input#show-join-part name="show-join-part" type="checkbox" checked;
                "Show join / leave"
            }
            div.check-group {
                label for="live" title="Show new messages, as they are logged live." {
                    input#live name="live" type="checkbox" checked;
                    "Live update"
                }
                label for="auto-scroll" title="Automatically scroll the new messages remain visible." {
                    input#auto-scroll name="auto-scroll" type="checkbox" checked;
                    "Auto-scroll"
                }
            }
            form.search {
                input#filter type="search" placeholder="Search this day";
            }
            (clear_selection_button())
        },
        html! {
            (date_sel("", "bottom", "\u{22ce}", "Jump to the bottom"))
            @if let Some(topic) = info.topic.as_ref() {
                blockquote.last-topic {
                    (some_or_empty(&topic.payload))
                    cite { "Set by " (format_nick(topic.nick.as_ref().unwrap())) " on " (message_link(topic, html!{ (topic.timestamp.format("%Y-%m-%d at %H:%M")) })) }
                }
            }
            table.messages data-stream=(uri!(route::channel_stream(sc))) {
                tbody {
                @for msg in messages { (message(msg, sc, &info.nicks, LinkType::Relative)) }
                }
            }
            @if messages.is_empty() {
                p.empty { "No messages for " (day.ymd()) "." }
            }
            @if truncated { div.warning { "Only displaying the first " (messages.len()) " lines to prevent browser slowness." } }
            (date_sel("bottom", "", "\u{22cf}", "Jump to the top"))
            div#bottom {}
            script type="text/javascript" src="/static/js/ircjournal.js" {}
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
                    strong { (p) }
                } @else {
                    a href=(uri!(route::channel_search(sc, query, Some(p as u64)))) title=(format!("Page {}", p)) { (p) }
                }
            }
        })
        .collect();
    let pages = html! { @if pages.len() > 1 { div.pages { @for p in pages { (p) } } } };
    base(
        &sc.to_string(),
        html! {
            (home_link())
            a href=(uri!(route::channel_redirect(sc))) { "Back to channel" }
            (search_form(sc, query))
        },
        html! {
            div {
                @if result_count == 0 {
                    "No message found."
                } @else {
                    "Found " strong { (result_count) } " lines. "
                    (pages)
                }
            }
            table.messages {
                @for per_day in messages {
                    tbody.search-date { tr { td colspan="3" { (per_day.0.ymd()) } } }
                    @for msg in &per_day.1 { (message(msg, sc, &info.nicks, LinkType::Absolute)) }
                }
            }
            (pages)
        },
    )
}

pub(crate) fn formatted_message(m: &Message) -> String {
    message(
        m,
        &ServerChannel::from_str(m.channel.as_ref().unwrap()).unwrap(),
        &HashSet::new(),
        LinkType::Relative,
    )
    .into_string()
}

fn home_link() -> Markup {
    html! { a href=(uri!(route::home)) { "Home" } }
}

fn clear_selection_button() -> Markup {
    html! { button#clear-selection disabled type="button" title="Un-select all selected messages." { "Clear selection" } }
}

fn highlight(line: &str) -> Markup {
    if !line.contains('\u{e000}') {
        // Early exit.
        return html! { (clean(line)) };
    }
    let mut it = line.split('\u{e000}');
    let first = it
        .by_ref()
        .take(1)
        .collect::<Vec<_>>()
        .first()
        .unwrap()
        .to_owned();
    let mut out = vec![html! { (first) }];
    while let Some(full) = it.by_ref().next() {
        if let Some((hl, tail)) = full.split_once('\u{e001}') {
            out.push(html! { b { (hl) } (clean(tail)) });
        } else {
            out.push(html! { b { (clean(full)) } });
        }
    }
    html! { @for s in out { (s) } }
}

fn clean(line: &str) -> String {
    line.replace('\u{e000}', "").replace('\u{e001}', "")
}

fn search_form(sc: &ServerChannel, query: &str) -> Markup {
    html! {
        form.search action=(uri!(route::channel_search(sc, "", None as Option<u64>))) method="get" {
            input type="search" name="query" value=(query) placeholder="Search this channel";
            button type="submit" { "Search" }
        }
    }
}

fn format_nick(nick: &str) -> Markup {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(nick.as_ref());
    let color = hasher.finalize() % 16;
    html! { span.nick.{ "nick-" (color) } { (highlight(nick)) } }
}

fn format_hl_nick(content: &str, nicks: &Nicks) -> Markup {
    lazy_static! {
        static ref NICK: Regex = Regex::new(r#"^([A-Za-z_0-9|.`-]+)"#).unwrap();
    }
    if !nicks.is_empty() && NICK.is_match(content) {
        let cap = NICK.captures(content).unwrap().get(1).unwrap();
        let nick = cap.as_str().to_string();
        if nicks.contains(&nick) {
            return html! { (format_nick(&nick)) (highlight(&content[cap.end()..])) };
        }
    }
    highlight(content)
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
                    let short = match s.split_once("://") {
                        Some((_, url)) => url,
                        None => s,
                    };
                    let truncated = short.len() > LINK_TRUNCATE_LENGTH;
                    let short = if truncated { short.chars().take(LINK_TRUNCATE_LENGTH).collect::<String>() } else { short.to_string() };
                    html! { a.link.trunc[truncated] ref="noreferrer nofollow external" href=(clean(s)) title=(clean(s)) { (highlight(&short)) } }
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
        LinkType::Absolute => uri!(route::channel(sc, m.timestamp.into())).to_string(),
        _ => "".to_string(),
    };
    html! {
        tr#(m.id_str()).msg data-timestamp=(m.epoch()) data-oper=(some_or_empty(&m.opcode)) {
                td.ts { a.tslink title=(m.timestamp.to_rfc3339()) href={(rel) "#" (m.id_str())} { (m.timestamp.format("%H:%M")) } }
                @if m.is_talk() {
                    td.nick."me-tell"[m.is_me_tell()] { (format_nick(m.nick.as_ref().unwrap())) }
                } @else {
                    td.nick.operation { "*" }
                }
                td.line { (format_message(m, nicks)) }
            }
    }
}

fn render_calendar(day: &Day, info: &ChannelInfo, active_days: &HashSet<u32>) -> Markup {
    let sc = &info.sc;
    let month = &calendar(day, active_days);
    let today = &Day::today();
    html! {
        section.calendar {
            nav {
                span { a href=(uri!(route::channel(sc, info.first_day.clone()))) title="Jump to first available logs" { "\u{291a}" } }
                span { a href=(uri!(route::channel(sc, month.prev.clone()))) title="Previous month" { "«" } }
                span.current { (day.month()) }
                span { a href=(uri!(route::channel(sc, month.succ.clone()))) title="Next month" { "»" } }
                span { a href=(uri!(route::channel(sc, info.last_day.clone()))) title="Jump to last available logs" { "\u{2919}" } }
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
                                        @if *linked { a href=(uri!(route::channel(sc, d.clone()))) .active[d == day].today[d == today] { (fmted) } }
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

    let gen =
        |d: u32| Some::<(Day, _)>((sow.with_day(d).unwrap().into(), active_days.contains(&d)));

    let offset_monday = sow.weekday().num_days_from_monday() as usize;

    let mut days = 1..=num_days;
    let first_week: OneWeek = core::iter::repeat(None)
        .take(offset_monday)
        .chain((1..=(7 - offset_monday)).map(|_| gen(days.next().unwrap() as u32)))
        .collect();
    let days = days.collect::<Vec<i64>>();
    let mut weeks: Vec<OneWeek> = core::iter::once(first_week)
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
fn test_calendar() {
    let day = &Day(chrono::NaiveDate::from_ymd(2021, 6, 22));
    let present: HashSet<u32> = vec![1, 3, 9, 24].into_iter().collect();
    calendar(day, &present);
}

#[test]
fn test_hl() {
    assert_eq!(highlight("").into_string(), "");
    assert_eq!(highlight("world").into_string(), "world");
    assert_eq!(
        highlight("\u{e000}world\u{e001}").into_string(),
        "<b>world</b>"
    );
    assert_eq!(
        highlight("\u{e000}world\u{e001}!").into_string(),
        "<b>world</b>!"
    );
    assert_eq!(
        highlight("hello \u{e000}world\u{e001}").into_string(),
        "hello <b>world</b>"
    );
    assert_eq!(
        highlight("hello \u{e000}world\u{e001}!").into_string(),
        "hello <b>world</b>!"
    );
    assert_eq!(
        highlight("\u{e000}hello\u{e001}\u{e000}world\u{e001}").into_string(),
        "<b>hello</b><b>world</b>"
    );
    assert_eq!(
        highlight("a\u{e000}hello\u{e001}b\u{e000}world\u{e001}c").into_string(),
        "a<b>hello</b>b<b>world</b>c"
    );
    assert_eq!(
        highlight("\u{e000}world\u{e000}garbage").into_string(),
        "<b>world</b><b>garbage</b>"
    );
    assert_eq!(
        highlight("\u{e000}world garbage").into_string(),
        "<b>world garbage</b>"
    );
    assert_eq!(
        highlight("\u{e000}hello\u{e001}\u{e001}world").into_string(),
        "<b>hello</b>world"
    );
}
