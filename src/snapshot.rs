use crate::model::{ChatInfo, Message};
use anyhow::Result;
use serde::Deserialize;

#[derive(Deserialize)]
struct Snapshot {
    chat: Chat,
    messages: Vec<SnapshotMessage>,
}
#[derive(Deserialize)]
struct Chat {
    mid: String,
    name: String,
    #[serde(rename = "type")]
    chat_type: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotMessage {
    id: String,
    from: String,
    content_type: String,
    text: Option<String>,
    created_time: i64,
}

pub fn parse_snapshot(text: &str) -> Result<(ChatInfo, Vec<Message>)> {
    let snapshot: Snapshot = serde_json::from_str(text)?;
    let chat = ChatInfo {
        chat_key: snapshot.chat.mid,
        chat_name: snapshot.chat.name,
        chat_type: snapshot.chat.chat_type,
    };
    let messages = snapshot
        .messages
        .into_iter()
        .filter_map(|m| {
            let body = if m.content_type == "NONE" {
                m.text.unwrap_or_default()
            } else {
                format!("[{}]", m.content_type.to_lowercase())
            };
            if body.is_empty() {
                return None;
            }
            Some(Message {
                chat_key: chat.chat_key.clone(),
                chat_name: chat.chat_name.clone(),
                ts_ms: m.created_time,
                sender: m.from,
                body,
                source: "snapshot".into(),
                source_id: Some(m.id),
            })
        })
        .collect();
    Ok((chat, messages))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn minimal_and_unknown_fields() {
        let (chat, m) = parse_snapshot(r#"{"chat":{"mid":"c0123abcd","name":"","type":"GROUP","memberCount":{"anything":true}},"extra":true,"messages":[{"id":"1","from":"u0123abcd","contentType":"NONE","text":"hello","createdTime":1700000000000,"raw":{}},{"id":"2","from":"u0123abcd","contentType":"IMAGE","text":null,"createdTime":1700000000001},{"id":"3","from":"u0123abcd","contentType":"NONE","text":null,"createdTime":1700000000002}]}"#).unwrap();
        assert_eq!(chat.chat_key, "c0123abcd");
        assert_eq!(chat.chat_type, "GROUP");
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].body, "hello");
        assert_eq!(m[1].body, "[image]");
    }
}
