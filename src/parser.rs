use std::collections::HashSet;
use std::iter::FromIterator;

use chrono::Weekday;
use crate::feed::{FeedEvent, FeedInfo, FilterType, UpdateSpec};
use regex::Regex;

use crate::error::ParseError;
use crate::parse_util::{Buffer, ParseResult};

pub fn parse_command(input: &str) -> Result<Vec<String>, ParseError> {
    let buf = Buffer {
        row: 0,
        col: 0,
        text: input,
    };

    let (_, output) = parse_command_internal(&buf)?;
    Ok(output)
}

fn parse_command_internal<'a>(buf: &Buffer<'a>) -> ParseResult<'a, Vec<String>> {
    let mut output = Vec::new();
    let mut buf = buf.trim();
    while !buf.text.is_empty() {
        let (new_buf, part) = parse_command_part(&buf)?;
        output.push(part);
        buf = new_buf.trim_start();
    }
    Ok((buf, output.into_iter().map(String::from).collect()))
}

fn parse_command_part<'a>(buf: &Buffer<'a>) -> ParseResult<'a, &'a str> {
    let buf = buf.trim_start();
    match buf.peek() {
        Some('\'') => buf.read_between('\'', '\''),
        Some('"') => buf.read_between('"', '"'),
        _ => match buf.text.find(|x: char| x.is_whitespace()) {
            Some(offset) => Ok((buf.advance(offset), &buf.text[..offset])),
            None => Ok((buf.advance(buf.text.len()), buf.text)),
        },
    }
}

pub fn parse_config(input: &str) -> Result<Vec<FeedInfo>, ParseError> {
    let mut out = Vec::new();
    let mut root_path = None;
    let mut command = None;
    for (row, line) in input.lines().enumerate() {
        let buf = Buffer {
            row: row + 1,
            col: 0,
            text: line,
        }
        .trim();

        if buf.starts_with("#") || buf.text.is_empty() {
            continue;
        }

        if buf.starts_with("root") {
            let buf = buf.token_no_case("root")?;
            if buf.trim().text.is_empty() {
                root_path = None;
            } else {
                root_path = Some(buf.space()?.trim().text);
            }
        } else if buf.starts_with("command") {
            let buf = buf.token_no_case("command")?;
            if buf.trim().text.is_empty() {
                command = None;
            } else {
                command = Some(parse_command(buf.text)?);
            }
        } else {
            let (_, mut feed) = parse_line(&buf)?;
            feed.root = root_path.map(From::from);
            feed.command = command.clone();
            out.push(feed);
        }
    }
    Ok(out)
}

fn parse_line<'a>(buf: &Buffer<'a>) -> ParseResult<'a, FeedInfo> {
    let (buf, name) = parse_name(buf)?;
    let buf = buf.trim_start();
    let (buf, url) = parse_url(&buf)?;
    let buf = buf.trim_start();
    let (buf, policies) = parse_policies(&buf)?;
    Ok((
        buf,
        FeedInfo {
            name: name.into(),
            url: url.into(),
            update_policies: HashSet::from_iter(policies),
            root: None,
            command: None,
        },
    ))
}

fn parse_name<'a>(buf: &Buffer<'a>) -> ParseResult<'a, &'a str> {
    buf.trim_start().read_between('"', '"')
}

fn parse_url<'a>(buf: &Buffer<'a>) -> ParseResult<'a, &'a str> {
    buf.trim_start().read_between('<', '>')
}

fn parse_policies<'a>(buf: &Buffer<'a>) -> ParseResult<'a, Vec<UpdateSpec>> {
    let mut policies = Vec::new();
    let mut buf = buf.trim_start();
    while buf.starts_with("@") {
        let (inp, policy) = parse_policy(&buf)?;
        policies.push(policy);
        buf = inp.trim_start();
    }
    Ok((buf, policies))
}

fn parse_policy<'a>(buf: &Buffer<'a>) -> Result<(Buffer<'a>, UpdateSpec), ParseError> {
    let buf = buf.trim_start().token("@")?.space()?;

    if buf.starts_with_no_case("on") {
        let buf = buf.token_no_case("on")?.space()?;
        let (buf, weekday) = parse_weekday(&buf)?;
        let buf = buf.space_or_end()?;
        Ok((buf, UpdateSpec::On(weekday)))
    } else if buf.starts_with_no_case("every") {
        let buf = buf.token_no_case("every")?.space()?;
        let (buf, count) = parse_number(&buf)?;
        let buf = buf
            .space()?
            .first_token_of_no_case(&["days", "day"])?
            .0
            .space_or_end()?;
        Ok((buf, UpdateSpec::Every(count)))
    } else if buf.starts_with_no_case("overlap") {
        let buf = buf.token_no_case("overlap")?.space()?;
        let (buf, count) = parse_number(&buf)?;
        let buf = buf
            .space()?
            .first_token_of_no_case(&["comics", "comic"])?
            .0
            .space_or_end()?;
        Ok((buf, UpdateSpec::Overlap(count)))
    } else if buf.starts_with_no_case("keep") || buf.starts_with_no_case("ignore") {
        let (buf, act_kind) = buf.first_token_of_no_case(&["keep", "ignore"])?;
        let buf = buf.space()?;
        let (buf, act_target) = buf.first_token_of_no_case(&["url", "title"])?;
        let buf = buf.space()?;
        let c = buf.text.chars().next().ok_or(buf.expected("a pattern"))?;
        let (buf, pat) = buf.read_between(c, c)?;
        if let Err(err) = Regex::new(pat) {
            // @Todo: Get the span right
            return Err(buf.expected(format!("/{}/ to be a valid pattern: {}", pat, err)));
        }
        Ok((
            buf,
            UpdateSpec::Filter(
                match (act_kind, act_target) {
                    ("keep", "title") => FilterType::KeepTitle,
                    ("keep", "url") => FilterType::KeepUrl,
                    ("ignore", "title") => FilterType::IgnoreTitle,
                    ("ignore", "url") => FilterType::IgnoreUrl,
                    _ => unreachable!("invalid filter type"),
                },
                pat.into(),
            ),
        ))
    } else if buf.starts_with_no_case("open") {
        let buf = buf
            .token_no_case("open")?
            .space()?
            .token_no_case("all")?
            .space_or_end()?;
        Ok((buf, UpdateSpec::OpenAll))
    } else if buf
        .text
        .chars()
        .next()
        .map(|x| x.is_digit(10))
        .unwrap_or_default()
    {
        let (buf, count) = parse_number(&buf)?;
        let buf = buf
            .trim_start()
            .token_no_case("new")?
            .space()?
            .first_token_of_no_case(&["comics", "comic"])?
            .0;
        Ok((buf, UpdateSpec::Comics(count)))
    } else {
        let error = ParseError::expected(
            r#"a policy definition. One of:
 - "@ on WEEKDAY"
 - "@ every # day(s)"
 - "@ # new comic(s)"
 - "@ overlap # comic(s)"
 - "@ keep pattern /pattern/"
 - "@ ignore pattern /pattern/"
 - "@ open all""#,
            buf.row,
            (buf.col, buf.col + buf.text.len()),
        );
        Err(error)
    }
}

fn parse_number<'a>(buf: &Buffer<'a>) -> ParseResult<'a, usize> {
    let buf = buf.trim_start();
    let end = buf
        .text
        .find(|c: char| !c.is_digit(10))
        .unwrap_or_else(|| buf.text.len());
    if end == 0 {
        return Err(buf.expected("digit"));
    }
    let value = buf.text[..end].parse().expect("Should only contain digits");
    let buf = buf.advance(end);
    Ok((buf, value))
}

fn parse_weekday<'a>(buf: &Buffer<'a>) -> ParseResult<'a, Weekday> {
    if buf.starts_with_no_case("sunday") {
        let buf = buf.advance("sunday".len());
        Ok((buf, Weekday::Sun))
    } else if buf.starts_with_no_case("monday") {
        let buf = buf.advance("monday".len());
        Ok((buf, Weekday::Mon))
    } else if buf.starts_with_no_case("tuesday") {
        let buf = buf.advance("tuesday".len());
        Ok((buf, Weekday::Tue))
    } else if buf.starts_with_no_case("wednesday") {
        let buf = buf.advance("wednesday".len());
        Ok((buf, Weekday::Wed))
    } else if buf.starts_with_no_case("thursday") {
        let buf = buf.advance("thursday".len());
        Ok((buf, Weekday::Thu))
    } else if buf.starts_with_no_case("friday") {
        let buf = buf.advance("friday".len());
        Ok((buf, Weekday::Fri))
    } else if buf.starts_with_no_case("saturday") {
        let buf = buf.advance("saturday".len());
        Ok((buf, Weekday::Sat))
    } else {
        Err(buf.expected("a weekday"))
    }
}

pub fn parse_events(input: &str) -> Result<Vec<FeedEvent>, ParseError> {
    let mut result = Vec::new();
    for (row, line) in input.lines().enumerate() {
        let line = Buffer {
            row: row + 1,
            col: 0,
            text: line,
        }
        .trim();
        if line.text.is_empty() {
            continue;
        }

        if line.starts_with_no_case("read") {
            let line = line.token_no_case("read")?.space()?;
            let date = match line.text.parse() {
                Ok(date) => date,
                Err(_) => {
                    return Err(line.expected("a valid date"));
                }
            };
            result.push(FeedEvent::Read(date))
        } else if line.starts_with("<") {
            let (line, url) = line.read_between('<', '>')?;
            line.space_or_end()?;
            result.push(FeedEvent::ComicUrl(url.into()));
        } else {
            return Err(ParseError::expected(
                r#"a feed event. One of:
 - "<url>"
 - "read DATE""#,
                row,
                None,
            ));
        }
    }
    Ok(result)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_config_parser() {
        let buf = r#"
"Questionable Content" <http://questionablecontent.net/QCRSS.xml> @ on Saturday @ every 10 days
"#;
        assert_eq!(
            parse_config(buf),
            Ok(vec![FeedInfo {
                name: "Questionable Content".into(),
                url: "http://questionablecontent.net/QCRSS.xml".into(),
                update_policies: HashSet::from_iter(vec![
                    UpdateSpec::On(Weekday::Sat),
                    UpdateSpec::Every(10),
                ]),
                root: None,
                command: None,
            }])
        );
    }

    #[test]
    fn test_multi_feeds() {
        let buf = r#"

# Good and cute
"Goodbye To Halos" <http://goodbyetohalos.com/feed/> @ 3 new comics @ on Monday @ overlap 2 comics
# pe'i xamgu
"Electrum" <https://electrum.cubemelon.net/feed> @ On Thursday @ 5 new Comics

"Gunnerkrigg Court" <http://gunnerkrigg.com/rss.xml> @ 4 new comics @ on tuesday

# A tumblr comic that doesn't have forward/backward buttons on individual comics
"GQutie!" <http://gqutiecomics.com/rss> @ Open all
"#;
        assert_eq!(
            parse_config(buf),
            Ok(vec![
                FeedInfo {
                    name: "Goodbye To Halos".into(),
                    url: "http://goodbyetohalos.com/feed/".into(),
                    update_policies: HashSet::from_iter(vec![
                        UpdateSpec::Comics(3),
                        UpdateSpec::On(Weekday::Mon),
                        UpdateSpec::Overlap(2),
                    ]),
                    root: None,
                    command: None,
                },
                FeedInfo {
                    name: "Electrum".into(),
                    url: "https://electrum.cubemelon.net/feed".into(),
                    update_policies: HashSet::from_iter(vec![
                        UpdateSpec::Comics(5),
                        UpdateSpec::On(Weekday::Thu),
                    ]),
                    root: None,
                    command: None,
                },
                FeedInfo {
                    name: "Gunnerkrigg Court".into(),
                    url: "http://gunnerkrigg.com/rss.xml".into(),
                    update_policies: HashSet::from_iter(vec![
                        UpdateSpec::Comics(4),
                        UpdateSpec::On(Weekday::Tue),
                    ]),
                    root: None,
                    command: None,
                },
                FeedInfo {
                    name: "GQutie!".into(),
                    url: "http://gqutiecomics.com/rss".into(),
                    update_policies: HashSet::from_iter(vec![UpdateSpec::OpenAll]),
                    root: None,
                    command: None,
                },
            ])
        )
    }

    #[test]
    fn test_feed_root() {
        let buf = concat!(
            r#"

"Eth's Skin" <http://www.eths-skin.com/rss>

root /hello/world
"Witchy" <http://feeds.feedburner.com/WitchyComic?format=xml> @ on Wednesday
"Cucumber Quest" <http://cucumber.gigidigi.com/feed/> @ on Sunday
root /oops/this/is/another/path
"Imogen Quest" <http://imogenquest.net/?feed=rss2> @ on Friday
root
root "#,
            r#"

"Balderdash" <http://www.balderdashcomic.com/rss.php>
"#
        );

        assert_eq!(
            parse_config(buf),
            Ok(vec![
                FeedInfo {
                    name: "Eth's Skin".into(),
                    url: "http://www.eths-skin.com/rss".into(),
                    update_policies: HashSet::new(),
                    root: None,
                    command: None,
                },
                FeedInfo {
                    name: "Witchy".into(),
                    url: "http://feeds.feedburner.com/WitchyComic?format=xml".into(),
                    update_policies: HashSet::from_iter(vec![UpdateSpec::On(Weekday::Wed)]),
                    root: Some("/hello/world".into()),
                    command: None,
                },
                FeedInfo {
                    name: "Cucumber Quest".into(),
                    url: "http://cucumber.gigidigi.com/feed/".into(),
                    update_policies: HashSet::from_iter(vec![UpdateSpec::On(Weekday::Sun)]),
                    root: Some("/hello/world".into()),
                    command: None,
                },
                FeedInfo {
                    name: "Imogen Quest".into(),
                    url: "http://imogenquest.net/?feed=rss2".into(),
                    update_policies: HashSet::from_iter(vec![UpdateSpec::On(Weekday::Fri)]),
                    root: Some("/oops/this/is/another/path".into()),
                    command: None,
                },
                FeedInfo {
                    name: "Balderdash".into(),
                    url: "http://www.balderdashcomic.com/rss.php".into(),
                    update_policies: HashSet::new(),
                    root: None,
                    command: None,
                },
            ])
        )
    }

    #[test]
    fn test_invalid_configs() {
        let bad_weekday = r#"
"Boozle" <http://boozle.sgoetter.com/feed/> @ on wendsday
"#;
        assert_eq!(
            parse_config(bad_weekday),
            Err(ParseError::expected("a weekday", 2, 49))
        );

        let bad_policy = r#"
"Boozle" <http://boozle.sgoetter.com/feed/> @ foo
"#;

        let ParseError::Expected { msg, row, .. } = parse_config(bad_policy).unwrap_err();
        assert!(msg.starts_with("a policy definition"));
        assert_eq!(row, 2);
    }

    #[test]
    fn test_feed_commands() {
        let input = r#"
"Eth's Skin" <http://www.eths-skin.com/rss>

command example "command here" 'single quotes' then-something
"Witchy" <http://feeds.feedburner.com/WitchyComic?format=xml>
"Cucumber Quest" <http://cucumber.gigidigi.com/feed/>
command
"Imogen Quest" <http://imogenquest.net/?feed=rss2>
"#;

        let command_vec = Some(vec![
            "example".into(),
            "command here".into(),
            "single quotes".into(),
            "then-something".into(),
        ]);

        assert_eq!(
            parse_config(input),
            Ok(vec![
                FeedInfo {
                    name: "Eth's Skin".into(),
                    url: "http://www.eths-skin.com/rss".into(),
                    update_policies: HashSet::new(),
                    root: None,
                    command: None,
                },
                FeedInfo {
                    name: "Witchy".into(),
                    url: "http://feeds.feedburner.com/WitchyComic?format=xml".into(),
                    update_policies: HashSet::new(),
                    root: None,
                    command: command_vec.clone(),
                },
                FeedInfo {
                    name: "Cucumber Quest".into(),
                    url: "http://cucumber.gigidigi.com/feed/".into(),
                    update_policies: HashSet::new(),
                    root: None,
                    command: command_vec,
                },
                FeedInfo {
                    name: "Imogen Quest".into(),
                    url: "http://imogenquest.net/?feed=rss2".into(),
                    update_policies: HashSet::new(),
                    root: None,
                    command: None,
                },
            ])
        )
    }

    #[test]
    fn test_parse_events() {
        use chrono::{TimeZone, Utc};
        let input = r#"
<http://www.goodbyetohalos.com/comic/01137>

<http://www.goodbyetohalos.com/comic/01138-139>
 read 2017-07-17T03:21:21.492180+00:00
 <http://www.goodbyetohalos.com/comic/01140>
read 2017-07-18T23:41:58.130248+00:00
"#;
        assert_eq!(
            parse_events(input),
            Ok(vec![
                FeedEvent::ComicUrl("http://www.goodbyetohalos.com/comic/01137".into()),
                FeedEvent::ComicUrl("http://www.goodbyetohalos.com/comic/01138-139".into()),
                FeedEvent::Read(Utc.ymd(2017, 07, 17).and_hms_micro(03, 21, 21, 492180)),
                FeedEvent::ComicUrl("http://www.goodbyetohalos.com/comic/01140".into()),
                FeedEvent::Read(Utc.ymd(2017, 07, 18).and_hms_micro(23, 41, 58, 130248)),
            ])
        );

        assert!(parse_events("invalid").is_err());
    }

    #[test]
    fn test_patterns() {
        let pattern_text = "
\"El Goonish Shive\" <http://www.egscomics.com/rss.php> @ ignore title /EGS:NP/ @ keep title \"\\d{4}-\\d{2}-\\d{2}\" @ keep url \u{1f49c}.\u{1f49c} @ ignore url /egsnp/
";
        assert_eq!(
            parse_config(pattern_text),
            Ok(vec![FeedInfo {
                name: "El Goonish Shive".into(),
                url: "http://www.egscomics.com/rss.php".into(),
                update_policies: HashSet::from_iter(vec![
                    UpdateSpec::Filter(FilterType::IgnoreTitle, "EGS:NP".into()),
                    UpdateSpec::Filter(FilterType::KeepTitle, "\\d{4}-\\d{2}-\\d{2}".into()),
                    UpdateSpec::Filter(FilterType::KeepUrl, ".".into()),
                    UpdateSpec::Filter(FilterType::IgnoreUrl, "egsnp".into()),
                ]),
                root: None,
                command: None,
            }])
        );
    }
}
