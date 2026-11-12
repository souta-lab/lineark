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

/// ISO 8601 with the fixed JST offset, e.g. `2026-11-12T21:34:00+09:00`.
pub fn format_timestamp_iso(ts: i64) -> String {
    tokyo()
        .timestamp_millis_opt(ts)
        .single()
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%:z").to_string())
        .unwrap_or_else(|| format!("invalid timestamp ({ts})"))
}

/// JST calendar day (`YYYY/MM/DD`) and wall-clock time (`HH:MM`) of `ts`.
pub fn format_day_time(ts: i64) -> Option<(String, String)> {
    tokyo().timestamp_millis_opt(ts).single().map(|dt| {
        (
            dt.format("%Y/%m/%d").to_string(),
            dt.format("%H:%M").to_string(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn iso_timestamp_uses_jst_offset() {
        // 2025-10-31 09:12 JST
        assert_eq!(
            format_timestamp_iso(1_761_869_520_000),
            "2025-10-31T09:12:00+09:00"
        );
        // 2026-11-12 21:34 JST
        assert_eq!(
            format_timestamp_iso(1_794_486_840_000),
            "2026-11-12T21:34:00+09:00"
        );
    }
}
