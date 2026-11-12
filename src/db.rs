use crate::model::{ChatInfo, Message};
use anyhow::Result;
use rusqlite::{params, Connection};

pub fn initialize(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;
CREATE TABLE IF NOT EXISTS chats (chat_key TEXT PRIMARY KEY, chat_name TEXT NOT NULL DEFAULT '', chat_type TEXT NOT NULL DEFAULT '');
CREATE TABLE IF NOT EXISTS messages (id INTEGER PRIMARY KEY, chat_key TEXT NOT NULL REFERENCES chats(chat_key), ts_ms INTEGER NOT NULL, sender TEXT NOT NULL, body TEXT NOT NULL, source TEXT NOT NULL, source_id TEXT);
CREATE UNIQUE INDEX IF NOT EXISTS idx_messages_dedup ON messages(chat_key, ts_ms, sender, body);
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(body, tokenize='trigram');")?;
    Ok(())
}

pub fn import(conn: &mut Connection, chat: &ChatInfo, messages: &[Message]) -> Result<usize> {
    let tx = conn.transaction()?;
    tx.execute("INSERT INTO chats(chat_key, chat_name, chat_type) VALUES (?1,?2,?3) ON CONFLICT(chat_key) DO UPDATE SET chat_name=excluded.chat_name, chat_type=excluded.chat_type", params![chat.chat_key, chat.chat_name, chat.chat_type])?;
    let mut count = 0;
    for m in messages {
        let inserted = tx.execute("INSERT OR IGNORE INTO messages(chat_key, ts_ms, sender, body, source, source_id) VALUES (?1,?2,?3,?4,?5,?6)", params![m.chat_key, m.ts_ms, m.sender, m.body, m.source, m.source_id])?;
        if inserted == 1 {
            tx.execute(
                "INSERT INTO messages_fts(rowid, body) VALUES (?1,?2)",
                params![tx.last_insert_rowid(), m.body],
            )?;
            count += 1;
        }
    }
    tx.commit()?;
    Ok(count)
}

pub fn search(
    conn: &Connection,
    query: &str,
    chat: Option<&str>,
    limit: usize,
) -> Result<Vec<Message>> {
    let long = query.chars().count() >= 3;
    let predicate = if long {
        "m.id IN (SELECT rowid FROM messages_fts WHERE messages_fts MATCH :term)"
    } else {
        "m.body LIKE '%' || :term || '%'"
    };
    let sql = format!("SELECT m.chat_key,c.chat_name,m.ts_ms,m.sender,m.body,m.source,m.source_id FROM messages m JOIN chats c USING(chat_key) WHERE {predicate} AND {CHAT_FILTER} ORDER BY m.ts_ms ASC,m.id ASC LIMIT :limit");
    let term = if long {
        format!("\"{}\"", query.replace('"', "\"\""))
    } else {
        query.to_string()
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::named_params! {
            ":term": term,
            ":chat": chat,
            ":limit": i64::try_from(limit).unwrap_or(i64::MAX),
        },
        map_message,
    )?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// LIKE substring match on chat name or key; `:chat` NULL matches every chat.
const CHAT_FILTER: &str =
    "(:chat IS NULL OR c.chat_name LIKE '%' || :chat || '%' OR c.chat_key LIKE '%' || :chat || '%')";

fn map_message(r: &rusqlite::Row<'_>) -> rusqlite::Result<Message> {
    Ok(Message {
        chat_key: r.get(0)?,
        chat_name: r.get(1)?,
        ts_ms: r.get(2)?,
        sender: r.get(3)?,
        body: r.get(4)?,
        source: r.get(5)?,
        source_id: r.get(6)?,
    })
}

/// All messages of the chats matching `chat` (substring of name or key; `None`
/// = every chat), oldest first.
pub fn export_messages(conn: &Connection, chat: Option<&str>) -> Result<Vec<Message>> {
    let sql = format!("SELECT m.chat_key,c.chat_name,m.ts_ms,m.sender,m.body,m.source,m.source_id FROM messages m JOIN chats c USING(chat_key) WHERE {CHAT_FILTER} ORDER BY m.ts_ms ASC,m.id ASC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::named_params! { ":chat": chat }, map_message)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dedup_and_search() {
        let mut conn = Connection::open_in_memory().unwrap();
        initialize(&conn).unwrap();
        let (name, messages) = crate::export::parse_export(
            "[LINE] Aliceとのトーク履歴\n2025/10/31(金)\n09:12\tBob\t明日は雨らしい",
        )
        .unwrap();
        let chat = ChatInfo {
            chat_key: format!("export:{name}"),
            chat_name: name,
            chat_type: String::new(),
        };
        assert_eq!(import(&mut conn, &chat, &messages).unwrap(), 1);
        assert_eq!(import(&mut conn, &chat, &messages).unwrap(), 0);
        for query in ["雨ら", "明日は"] {
            assert_eq!(search(&conn, query, None, 50).unwrap().len(), 1);
        }
        assert_eq!(search(&conn, "明日は", Some("Alice"), 50).unwrap().len(), 1);
        assert!(search(&conn, "雨ら", Some("Bob"), 50).unwrap().is_empty());
        for query in ["\" OR *", "hello\"world", "***", ""] {
            search(&conn, query, None, 50).unwrap();
        }
        let count: i64 = conn
            .query_row("SELECT count(*) FROM messages_fts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
    #[test]
    fn failed_import_rolls_back_message_and_fts() {
        let mut conn = Connection::open_in_memory().unwrap();
        initialize(&conn).unwrap();
        let (name, mut messages) = crate::export::parse_export(
            "[LINE] Aliceとのトーク履歴\n2025/10/31(金)\n09:12\tBob\thello",
        )
        .unwrap();
        let chat = ChatInfo {
            chat_key: format!("export:{name}"),
            chat_name: name,
            chat_type: String::new(),
        };
        let mut bad = messages[0].clone();
        bad.chat_key = "missing".into();
        messages.push(bad);
        assert!(import(&mut conn, &chat, &messages).is_err());
        for table in ["messages", "messages_fts", "chats"] {
            let count: i64 = conn
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(count, 0);
        }
    }

    fn message(chat: &ChatInfo, ts_ms: i64, body: &str) -> Message {
        Message {
            chat_key: chat.chat_key.clone(),
            chat_name: chat.chat_name.clone(),
            ts_ms,
            sender: "Bob".into(),
            body: body.into(),
            source: "export".into(),
            source_id: None,
        }
    }

    #[test]
    fn export_messages_orders_and_filters() {
        let mut conn = Connection::open_in_memory().unwrap();
        initialize(&conn).unwrap();
        let alice = ChatInfo {
            chat_key: "export:Alice".into(),
            chat_name: "Alice".into(),
            chat_type: String::new(),
        };
        let group = ChatInfo {
            chat_key: "c0123abcd".into(),
            chat_name: "Fictional Group".into(),
            chat_type: "GROUP".into(),
        };
        let base = 1_761_869_520_000; // 2025-10-31 09:12 JST
                                      // Inserted out of order; the same-ms pair must fall back to id order.
        import(
            &mut conn,
            &alice,
            &[
                message(&alice, base + 60_000, "second"),
                message(&alice, base, "first"),
            ],
        )
        .unwrap();
        import(
            &mut conn,
            &group,
            &[message(&group, base - 86_400_000, "earlier")],
        )
        .unwrap();

        let all = export_messages(&conn, None).unwrap();
        let bodies: Vec<&str> = all.iter().map(|m| m.body.as_str()).collect();
        assert_eq!(bodies, ["earlier", "first", "second"]);
        assert_eq!(all[1].chat_name, "Alice");

        let filtered = export_messages(&conn, Some("Alice")).unwrap();
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|m| m.chat_key == "export:Alice"));
        // Matching also works on the chat key, like the search --chat filter.
        assert_eq!(export_messages(&conn, Some("c0123")).unwrap().len(), 1);
        assert_eq!(export_messages(&conn, Some("Group")).unwrap().len(), 1);
        assert!(export_messages(&conn, Some("Nobody")).unwrap().is_empty());
    }
}
