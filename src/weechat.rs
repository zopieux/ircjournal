use std::path::Path;

use lazy_static::lazy_static;
use regex::{Match, Regex};

use crate::{
    models::{Datetime, ServerChannel},
    IrcLine, Logger, ParseResult,
};

lazy_static! {
    static ref FNAME: Regex = Regex::new(r"^irc\.(.+)\.(#.+|.+)\.weechatlog$").unwrap();
    static ref LINE: Regex = Regex::new(r"^([0-9]{4}\-[0-9]{2}\-[0-9]{2} [0-9]{2}:[0-9]{2}:[0-9]{2})\t(.*)$").unwrap();
    // static ref LOG_GARBAGE: Regex = Regex::new(r#"^--\t(?:\[.*?\]|#(.+?): (Unknown|Cannot)|irc|Notice|Mode|Topic (set|for)|Channel|You are|Nicks)"#).unwrap();
    static ref LOG_NICK_CHANGED: Regex = Regex::new(r#"^--\t(\S+) is now known as (\S+)$"#).unwrap();
    static ref LOG_TOPIC_CHANGED: Regex = Regex::new(r#"^--\t(\S+) has changed topic for \S+ from "(.*?)" to "(.*?)"$"#).unwrap();
    static ref LOG_JOINED: Regex = Regex::new(r#"^-->\t(\S+) \(.*?\) has joined (#.+)$"#).unwrap();
    static ref LOG_LEFT: Regex = Regex::new(r#"^<--\t(\S+) \(.*?\) has left (#.+?)(?: \("(.*?)"\))?$"#).unwrap();
    static ref LOG_QUIT: Regex = Regex::new(r#"^<--\t(\S+) \(.*?\) has quit(?: \((.*?)\))?$"#).unwrap();
    static ref LOG_KICKED: Regex = Regex::new(r#"^<--\t(\S+) has kicked (\S+)(?: \((.*?)\))?$"#).unwrap();
    static ref LOG_ME: Regex = Regex::new(r#"^ \*\t(\S+)(?: (.*))?$"#).unwrap();
    static ref LOG_MESSAGE: Regex = Regex::new(r#"^([^\s<-]\S*)\t(.*)$"#).unwrap();
}

pub struct Weechat;

// fn to_int4(input: &str) -> IResult<&str, u32> {
//     map_res(
//         take_while_m_n(4, 4, |c: char| c.is_digit(10)),
//         |o: &str| u32::from_str(o))
//         (input)
// }
//
// fn to_int2(input: &str) -> IResult<&str, u32> {
//     map_res(
//         take_while_m_n(2, 2, |c: char| c.is_digit(10)),
//         |o: &str| u32::from_str(o))
//         (input)
// }
//
// fn parse_timestamp(input: &str) -> IResult<&str, Datetime> {
//     let (input, (y, _, m, _, d, _, h, _, mm, _, s)) = tuple((
//         to_int4, tag("-"), to_int2, tag("-"), to_int2,
//         tag(" "), to_int2, tag(":"), to_int2, tag(":"), to_int2,
//     ))(input)?;
//     let ts = Datetime::from_utc(
//         chrono::NaiveDate::from_ymd(y as i32, m, d)
//             .and_hms(h, mm, s),
//         chrono::Utc);
//     Ok((input, ts))
// }
//
// fn parse_nick(input: &str) -> IResult<&str, &str> {
//     take_till(|c: char| c == '-' || c == '<' || c == '>' || c.is_ascii_whitespace())(input)
// }
//
// fn parse_joined(input: &str) -> IResult<&str, IrcLine> {
//     let (input, (_, nick, _, _, _)) = tuple((
//         tag("-->\t"),
//         parse_nick,
//         tag(" ("),
//         take_until(")"),
//         tag(") has joined"),
//     ))(input)?;
//     Ok((input, IrcLine::Joined { nick: nick.to_string() }))
// }
//
// fn parse_quit(input: &str) -> IResult<&str, IrcLine> {
//     let (input, (_, nick, _, _, _, reason)) = tuple((
//         tag("<--\t"),
//         parse_nick,
//         tag(" ("),
//         take_until(")"),
//         tag(") has quit ("),
//         take_until(")"),
//     ))(input)?;
//     Ok((input, IrcLine::Quit { nick: nick.to_string(), reason: reason.to_string() }))
// }
//
// fn maybe_reason(input: &str) -> IResult<&str, &str> {
//     let (input, (_, _, r)) = tuple((
//         take_until(" "),
//         tag(" ("),
//         rest,
//     ))(input)?;
//     Ok((input, r.strip_suffix(')').unwrap()))
// }
//
// fn parse_left(input: &str) -> IResult<&str, IrcLine> {
//     let (input, (_, nick, _, _, _)) = tuple((
//         tag("<--\t"),
//         parse_nick,
//         tag(" ("),
//         take_until(")"),
//         tag(") has left "),
//     ))(input)?;
//     let (input, reason) = maybe_reason(input).unwrap_or((input, ""));
//     Ok((input, IrcLine::Left { nick: nick.to_string(), reason: reason.to_string() }))
// }
//
// fn parse_kick(input: &str) -> IResult<&str, IrcLine> {
//     let (input, (_, oper_nick, _, nick)) = tuple((
//         tag("<--\t"),
//         parse_nick,
//         tag(" has kicked "),
//         parse_nick,
//     ))(input)?;
//     let (input, reason) = maybe_reason(input).unwrap_or((input, ""));
//     Ok((input, IrcLine::Kicked { oper_nick: oper_nick.to_string(), nick: nick.to_string(), reason: reason.to_string() }))
// }
//
// fn parse_topic_changed(input: &str) -> IResult<&str, IrcLine> {
//     let (input, (_, nick, _, _, _, reg)) = tuple((
//         tag("--\t"),
//         parse_nick,
//         tag(" has changed topic for "),
//         take_until(" "),
//         tag(" from \""),
//         re_capture(Regex::new(r#"(.+?)" to "(.+?)"$"#).unwrap()),
//     ))(input)?;
//     Ok((input, IrcLine::TopicChanged { nick: nick.to_string(), old: reg[1].to_string(), new: reg[2].to_string() }))
// }
//
// fn parse_nick_changed(input: &str) -> IResult<&str, IrcLine> {
//     let (input, (_, old, _, new)) = tuple((
//         tag("--\t"),
//         parse_nick,
//         tag(" is now known as "),
//         rest,
//     ))(input)?;
//     Ok((input, IrcLine::NickChanged { old: old.to_string(), new: new.to_string() }))
// }
//
// fn parse_line(input: &str) -> IResult<&str, IrcLine> {
//     let (input, (nick, _, line)) = tuple((
//         parse_nick,
//         char(' '),
//         rest,
//     ))(input)?;
//     Ok((input, IrcLine::Message { nick: nick.to_string(), line: line.to_string() }))
// }
//
// fn parse_me(input: &str) -> IResult<&str, IrcLine> {
//     let (input, (_, nick, _, line)) = tuple((
//         tag("*\t"),
//         parse_nick,
//         char(' '),
//         rest,
//     ))(input)?;
//     Ok((input, IrcLine::Me { nick: nick.to_string(), line: line.to_string() }))
// }
//
// fn parse_contents(input: &str) -> IResult<&str, IrcLine> {
//     let (input, _) = tag("\t")(input)?;
//     let (input, line) = alt((
//         parse_me,
//         parse_topic_changed,
//         parse_nick_changed, parse_joined, parse_left, parse_quit, parse_kick, parse_line))(input)?;
//     Ok((input, line))
// }

impl Logger for Weechat {
    fn parse_path(path: &Path) -> Option<ServerChannel> {
        let captures = FNAME.captures(path.file_name()?.to_str()?)?;
        Some(ServerChannel {
            server: captures.get(1)?.as_str().to_string(),
            channel: captures.get(2)?.as_str().to_string(),
        })
    }

    fn parse_line(line: &str) -> ParseResult {
        let mstr = |om: Match| om.as_str().to_string();
        let mstr_empty = |om: Option<Match>| {
            match om {
                Some(m) => m.as_str(),
                _ => "",
            }
            .to_string()
        };
        let cap = match LINE.captures(line) {
            Some(cap) => cap,
            None => return ParseResult::Invalid,
        };
        let naive = match chrono::NaiveDateTime::parse_from_str(
            cap.get(1).unwrap().as_str(),
            "%Y-%m-%d %H:%M:%S",
        ) {
            Ok(t) => t,
            Err(_) => return ParseResult::Invalid,
        };
        let timestamp = Datetime::from_utc(naive, chrono::Utc);
        let s = cap.get(2).unwrap().as_str();
        let parsed = (|| {
            if LOG_JOINED.is_match(s) {
                let x = LOG_JOINED.captures(s).unwrap();
                Some(IrcLine::Joined {
                    nick: mstr(x.get(1)?),
                })
            } else if LOG_LEFT.is_match(s) {
                let x = LOG_LEFT.captures(s).unwrap();
                Some(IrcLine::Left {
                    nick: mstr(x.get(1)?),
                    reason: mstr_empty(x.get(3)),
                })
            } else if LOG_QUIT.is_match(s) {
                let x = LOG_QUIT.captures(s).unwrap();
                Some(IrcLine::Quit {
                    nick: mstr(x.get(1)?),
                    reason: mstr_empty(x.get(2)),
                })
            } else if LOG_NICK_CHANGED.is_match(s) {
                let x = LOG_NICK_CHANGED.captures(s).unwrap();
                Some(IrcLine::NickChanged {
                    old: mstr(x.get(1)?),
                    new: mstr(x.get(2)?),
                })
            } else if LOG_TOPIC_CHANGED.is_match(s) {
                let x = LOG_TOPIC_CHANGED.captures(s).unwrap();
                Some(IrcLine::TopicChanged {
                    nick: mstr(x.get(1)?),
                    old: mstr(x.get(2)?),
                    new: mstr(x.get(3)?),
                })
            } else if LOG_KICKED.is_match(s) {
                let x = LOG_KICKED.captures(s).unwrap();
                Some(IrcLine::Kicked {
                    oper_nick: mstr(x.get(1)?),
                    nick: mstr(x.get(2)?),
                    reason: mstr_empty(x.get(3)),
                })
            } else if LOG_ME.is_match(s) {
                let x = LOG_ME.captures(s).unwrap();
                Some(IrcLine::Me {
                    nick: mstr(x.get(1)?),
                    line: mstr(x.get(2)?),
                })
            } else if LOG_MESSAGE.is_match(s) {
                let x = LOG_MESSAGE.captures(s).unwrap();
                Some(IrcLine::Message {
                    nick: mstr(x.get(1)?),
                    line: mstr(x.get(2)?),
                })
            } else {
                None
            }
        })();
        match parsed {
            Some(line) => ParseResult::Ok((timestamp, line)),
            None => ParseResult::Noise,
        }
    }
}

#[test]
fn test_parse_name() {
    use crate::test::ts;
    assert_eq!(Weechat::parse_path(Path::new("garbage")), None);
    assert_eq!(
        Weechat::parse_path(Path::new("irc.serv.foo.#bar.weechatlog")),
        Some(ServerChannel {
            server: "serv.foo".to_string(),
            channel: "#bar".to_string(),
        })
    );
    assert_eq!(
        Weechat::parse_path(Path::new("irc.serv.##dieses.weechatlog")),
        Some(ServerChannel {
            server: "serv".to_string(),
            channel: "##dieses".to_string(),
        })
    );
    assert_eq!(
        Weechat::parse_line(
            "2019-12-14 23:11:17\t-->\tzopieux (~zopieux@unaffiliated/zopieux) has joined ##dieses"
        ),
        ParseResult::Ok((
            ts("2019-12-14 23:11:17"),
            IrcLine::Joined {
                nick: "zopieux".to_string()
            }
        ))
    );
    assert_eq!(Weechat::parse_line("2019-12-14 23:11:38\t<--\tzopiuex (zopieux@unaffiliated/zopieux) has quit (Quit: WeeChat 2.2)"),
               ParseResult::Ok((ts("2019-12-14 23:11:38"),
                     IrcLine::Quit { nick: "zopiuex".to_string(), reason: "Quit: WeeChat 2.2".to_string() })));
    assert_eq!(Weechat::parse_line("2019-12-16 15:51:46\t<--\tTuxkowo (~Tuxkowo@2001:bc8:4400:2800::5d1b) has left ##dieses"),
               ParseResult::Ok((ts("2019-12-16 15:51:46"),
                     IrcLine::Left { nick: "Tuxkowo".to_string(), reason: "".to_string() })));
    assert_eq!(
        Weechat::parse_line(
            "2019-12-31 14:14:18\t<--\tTycale (~Tycale@tycale.be) has left ##dieses (\"Cya\")"
        ),
        ParseResult::Ok((
            ts("2019-12-31 14:14:18"),
            IrcLine::Left {
                nick: "Tycale".to_string(),
                reason: "Cya".to_string(),
            }
        ))
    );
    assert_eq!(
        Weechat::parse_line("2021-04-29 18:46:41\t *\thaileda uploaded an image: (68KiB)"),
        ParseResult::Ok((
            ts("2021-04-29 18:46:41"),
            IrcLine::Me {
                nick: "haileda".to_string(),
                line: "uploaded an image: (68KiB)".to_string(),
            }
        ))
    );
    assert_eq!(
        Weechat::parse_line("2019-12-14 23:12:14\tzopieux\ttest"),
        ParseResult::Ok((
            ts("2019-12-14 23:12:14"),
            IrcLine::Message {
                nick: "zopieux".to_string(),
                line: "test".to_string(),
            }
        ))
    );
    assert_eq!(Weechat::parse_line("2021-04-26 20:09:33\t--\tChanServ has changed topic for ##dieses from \"Bienvenue \"sur ##dieses\" to \"Joyeux \"anniversaire\" zopieux\""),
               ParseResult::Ok((ts("2021-04-26 20:09:33"),
                     IrcLine::TopicChanged { nick: "ChanServ".to_string(), old: "Bienvenue \"sur ##dieses".to_string(), new: "Joyeux \"anniversaire\" zopieux".to_string() })));
    assert_eq!(
        Weechat::parse_line("2021-01-19 00:39:46\t<--\tthizanne has kicked rom1504"),
        ParseResult::Ok((
            ts("2021-01-19 00:39:46"),
            IrcLine::Kicked {
                oper_nick: "thizanne".to_string(),
                nick: "rom1504".to_string(),
                reason: "".to_string(),
            }
        ))
    );
    assert_eq!(
        Weechat::parse_line("2021-01-19 00:39:46\t<--\tthizanne has kicked rom1504 (no u)"),
        ParseResult::Ok((
            ts("2021-01-19 00:39:46"),
            IrcLine::Kicked {
                oper_nick: "thizanne".to_string(),
                nick: "rom1504".to_string(),
                reason: "no u".to_string(),
            }
        ))
    );
    assert_eq!(
        Weechat::parse_line("2021-01-19 12:59:06\t--\tJuanTitor is now known as ordiclic"),
        ParseResult::Ok((
            ts("2021-01-19 12:59:06"),
            IrcLine::NickChanged {
                old: "JuanTitor".to_string(),
                new: "ordiclic".to_string(),
            }
        ))
    );
}
