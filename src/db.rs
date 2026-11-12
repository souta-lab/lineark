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
        "m.id IN (SELECT rowid FROM messages_fts WHERE messages_fts MATCH ?1)"
    } else {
        "m.body LIKE '%' || ?1 || '%'"
    };
    let sql = format!("SELECT m.chat_key,c.chat_name,m.ts_ms,m.sender,m.body,m.source,m.source_id FROM messages m JOIN chats c USING(chat_key) WHERE {predicate} AND (?2 IS NULL OR c.chat_name LIKE '%' || ?2 || '%' OR c.chat_key LIKE '%' || ?2 || '%') ORDER BY m.ts_ms ASC,m.id ASC LIMIT ?3");
    let term = if long {
        format!("\"{}\"", query.replace('"', "\"\""))
    } else {
        query.to_string()
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![term, chat, i64::try_from(limit).unwrap_or(i64::MAX)],
        |r| {
            Ok(Message {
                chat_key: r.get(0)?,
                chat_name: r.get(1)?,
                ts_ms: r.get(2)?,
                sender: r.get(3)?,
                body: r.get(4)?,
                source: r.get(5)?,
                source_id: r.get(6)?,
            })
        },
    )?;
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
}
