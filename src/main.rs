mod db;
mod export;
mod model;
mod snapshot;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use model::{format_timestamp, ChatInfo};
use rusqlite::Connection;
use std::{fs, path::PathBuf};

#[derive(Parser)]
#[command(version, about = "A searchable local archive for LINE chat history")]
struct Cli {
    #[arg(long, global = true, env = "LINEARK_DB", default_value = "lineark.db")]
    db: PathBuf,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    ImportExport {
        file: PathBuf,
    },
    ImportSnapshot {
        dir: PathBuf,
    },
    Search {
        query: String,
        #[arg(long)]
        chat: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    Chats,
    Stats,
}
fn date_range(first: Option<i64>, last: Option<i64>) -> String {
    match (first, last) {
        (Some(a), Some(b)) => format!("{} .. {}", format_timestamp(a), format_timestamp(b)),
        _ => "(empty)".into(),
    }
}
fn run() -> Result<()> {
    let cli = Cli::parse();
    let mut conn =
        Connection::open(&cli.db).with_context(|| format!("opening {}", cli.db.display()))?;
    db::initialize(&conn)?;
    match cli.command {
        Command::ImportExport { file } => {
            let text =
                fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?;
            let (name, messages) = export::parse_export(&text)?;
            let chat = ChatInfo {
                chat_key: format!("export:{name}"),
                chat_name: name.clone(),
                chat_type: String::new(),
            };
            let new = db::import(&mut conn, &chat, &messages)?;
            println!(
                "imported {} messages ({new} new) into chat \"{name}\"",
                messages.len()
            );
        }
        Command::ImportSnapshot { dir } => {
            let mut files = Vec::new();
            for entry in fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
                let entry = entry?;
                if entry.file_type()?.is_file()
                    && entry.path().extension().is_some_and(|e| e == "json")
                {
                    files.push(entry.path());
                }
            }
            files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
            let (mut total, mut added, mut imported, mut skipped) = (0, 0, 0, 0);
            for file in files {
                let parsed = fs::read_to_string(&file)
                    .map_err(anyhow::Error::from)
                    .and_then(|text| snapshot::parse_snapshot(&text));
                match parsed {
                    Ok((chat, messages)) => {
                        let new = db::import(&mut conn, &chat, &messages)
                            .with_context(|| format!("importing {}", file.display()))?;
                        println!(
                            "{}: imported {} messages ({new} new) into chat \"{}\"",
                            file.display(),
                            messages.len(),
                            chat.chat_name
                        );
                        total += messages.len();
                        added += new;
                        imported += 1;
                    }
                    Err(err) => {
                        eprintln!("warning: {}: {err:#}", file.display());
                        skipped += 1;
                    }
                }
            }
            println!(
                "imported {total} messages ({added} new) from {imported} files; {skipped} skipped"
            );
        }
        Command::Search { query, chat, limit } => {
            for m in db::search(&conn, &query, chat.as_deref(), limit)? {
                let label = if m.chat_name.is_empty() {
                    &m.chat_key
                } else {
                    &m.chat_name
                };
                println!(
                    "{}  {}  {}: {}",
                    format_timestamp(m.ts_ms),
                    label,
                    m.sender,
                    m.body.replace('\r', "").replace('\n', " / ")
                );
            }
        }
        Command::Chats => {
            let mut stmt = conn.prepare("SELECT c.chat_key,c.chat_name,count(m.id),min(m.ts_ms),max(m.ts_ms) FROM chats c LEFT JOIN messages m USING(chat_key) GROUP BY c.chat_key ORDER BY max(m.ts_ms) DESC,c.chat_key")?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let key: String = row.get(0)?;
                let name: String = row.get(1)?;
                let count: i64 = row.get(2)?;
                println!(
                    "{}\t{count}\t{}",
                    if name.is_empty() { key } else { name },
                    date_range(row.get(3)?, row.get(4)?)
                );
            }
        }
        Command::Stats => {
            let (total, first, last): (i64, Option<i64>, Option<i64>) = conn.query_row(
                "SELECT count(*),min(ts_ms),max(ts_ms) FROM messages",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            let chats: i64 = conn.query_row("SELECT count(*) FROM chats", [], |r| r.get(0))?;
            println!(
                "messages: {total}\nchats: {chats}\ndate range: {}",
                date_range(first, last)
            );
            for source in ["export", "snapshot"] {
                let count: i64 = conn.query_row(
                    "SELECT count(*) FROM messages WHERE source=?1",
                    [source],
                    |r| r.get(0),
                )?;
                println!("{source}: {count}");
            }
        }
    }
    Ok(())
}
fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}
