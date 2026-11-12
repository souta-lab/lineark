use chrono::{FixedOffset, TimeZone};

#[derive(Debug, Clone)]
pub struct ChatInfo {
    pub chat_key: String,
    pub chat_name: String,
    pub chat_type: String,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub chat_key: String,
    pub chat_name: String,
    pub ts_ms: i64,
    pub sender: String,
    pub body: String,
    pub source: String,
    pub source_id: Option<String>,
}

pub fn tokyo() -> FixedOffset {
    FixedOffset::east_opt(9 * 3600).unwrap()
}

pub fn format_timestamp(ts: i64) -> String {
    tokyo()
        .timestamp_millis_opt(ts)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| format!("invalid timestamp ({ts})"))
}
