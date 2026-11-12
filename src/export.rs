use crate::model::{tokyo, Message};
use anyhow::{bail, Context, Result};
use chrono::{NaiveDate, NaiveTime, TimeZone};

fn date_header(line: &str) -> Option<&str> {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() == 13
        && chars[4] == '/'
        && chars[7] == '/'
        && chars[10] == '('
        && chars[12] == ')'
        && [0, 1, 2, 3, 5, 6, 8, 9]
            .iter()
            .all(|&i| chars[i].is_ascii_digit())
    {
        Some(&line[..10])
    } else {
        None
    }
}

pub fn parse_export(text: &str) -> Result<(String, Vec<Message>)> {
    let mut lines = text.trim_start_matches('\u{feff}').lines();
    let header = lines.next().context("not a LINE export")?;
    let name = header
        .strip_prefix("[LINE]")
        .context("not a LINE export")?
        .trim();
    let name = name
        .strip_suffix("とのトーク履歴")
        .or_else(|| name.strip_suffix("のトーク履歴"))
        .unwrap_or(name)
        .to_string();
    let mut date = None;
    let mut messages: Vec<Message> = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if let Some(value) = date_header(trimmed) {
            date =
                Some(NaiveDate::parse_from_str(value, "%Y/%m/%d").context("invalid date header")?);
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with("保存日時：") {
            continue;
        }
        let mut fields = line.splitn(3, '\t');
        let time = fields.next().unwrap();
        let sender = fields.next();
        let body = fields.next();
        let valid_shape = time.len() == 5
            && time.as_bytes()[2] == b':'
            && time
                .bytes()
                .enumerate()
                .all(|(i, b)| i == 2 || b.is_ascii_digit());
        if let (true, Some(sender), Some(body)) = (valid_shape, sender, body) {
            let time = NaiveTime::parse_from_str(time, "%H:%M").context("invalid message time")?;
            let Some(date) = date else {
                bail!("message before date header");
            };
            let ts_ms = tokyo()
                .from_local_datetime(&date.and_time(time))
                .single()
                .context("invalid timestamp")?
                .timestamp_millis();
            messages.push(Message {
                chat_key: format!("export:{name}"),
                chat_name: name.clone(),
                ts_ms,
                sender: sender.into(),
                body: body.into(),
                source: "export".into(),
                source_id: None,
            });
        } else if let Some(previous) = messages.last_mut() {
            previous.body.push('\n');
            previous.body.push_str(line);
        }
    }
    Ok((name, messages))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn export_formats_and_content() {
        let (name, m) = parse_export("\u{feff}[LINE] Aliceとのトーク履歴\r\n保存日時：2025/11/01 10:00\r\n\r\n 2025/10/31(金) \r\n09:12\tAlice\tおはよう\r\n続き\r\n21:40\tBob\t[スタンプ]\r\n2025/11/01(土)\r\n08:00\tAlice\t[写真]\r\n").unwrap();
        assert_eq!(name, "Alice");
        assert_eq!(m.len(), 3);
        assert_eq!(m[0].body, "おはよう\n続き");
        assert_eq!(m[0].ts_ms, 1761869520000);
        assert_eq!(m[1].body, "[スタンプ]");
        assert_eq!(m[2].body, "[写真]");
        assert_eq!(m[2].ts_ms - m[0].ts_ms, 82_080_000);
        assert_eq!(
            parse_export("[LINE] Fictional Groupのトーク履歴")
                .unwrap()
                .0,
            "Fictional Group"
        );
    }
    #[test]
    fn rejects_missing_header_or_date() {
        assert!(parse_export("hello")
            .unwrap_err()
            .to_string()
            .contains("not a LINE export"));
        assert!(parse_export("[LINE] Bobとのトーク履歴\n09:12\tBob\thello").is_err());
    }
}
