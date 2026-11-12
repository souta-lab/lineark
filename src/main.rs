mod db;
mod export;
mod model;
mod snapshot;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use model::{format_day_time, format_timestamp, format_timestamp_iso, ChatInfo, Message};
use rusqlite::Connection;
use serde::Serialize;
use std::{collections::BTreeMap, fs, io::Write, path::PathBuf};

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
    Export {
        #[arg(long)]
        chat: Option<String>,
        #[arg(long, value_enum, default_value_t = ExportFormat::Jsonl)]
        format: ExportFormat,
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum ExportFormat {
    Jsonl,
    Text,
}

#[derive(Serialize)]
struct JsonlRecord<'a> {
    chat_key: &'a str,
    chat_name: &'a str,
    timestamp: String,
    sender: &'a str,
    body: &'a str,
}

fn render_jsonl(messages: &[Message]) -> Result<String> {
    let mut out = String::new();
    for m in messages {
        let record = JsonlRecord {
            chat_key: &m.chat_key,
            chat_name: &m.chat_name,
            timestamp: format_timestamp_iso(m.ts_ms),
            sender: &m.sender,
            body: &m.body,
        };
        out.push_str(&serde_json::to_string(&record)?);
        out.push('\n');
    }
    Ok(out)
}

/// LINE-export-like text: a `[LINE] <name>` header per chat, `YYYY/MM/DD`
/// headings per day, then `HH:MM<TAB>sender<TAB>body`. Bodies keep their
/// newlines, as in LINE's own exports. Chats are separated by a blank line.
///
/// Output is grouped by chat: each chat appears once, ordered by its earliest
/// message (ties by `chat_key`), with its messages oldest first inside the
/// group. `messages` is expected oldest-first, so per-chat order is stable.
fn render_text(messages: &[Message]) -> String {
    let mut groups: BTreeMap<&str, Vec<&Message>> = BTreeMap::new();
    for m in messages {
        groups.entry(m.chat_key.as_str()).or_default().push(m);
    }
    let mut groups: Vec<Vec<&Message>> = groups.into_values().collect();
    fn first<'a>(g: &'a [&'a Message]) -> Option<(i64, &'a str)> {
        g.first().map(|m| (m.ts_ms, m.chat_key.as_str()))
    }
    groups.sort_by(|a, b| first(a).cmp(&first(b)));

    let mut out = String::new();
    for (i, group) in groups.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        // A chat with no renderable timestamp still gets its header.
        let label = group
            .first()
            .map(|m| {
                if m.chat_name.is_empty() {
                    m.chat_key.as_str()
                } else {
                    m.chat_name.as_str()
                }
            })
            .unwrap_or_default();
        out.push_str(&format!("[LINE] {label}\n"));
        let mut current_day: Option<String> = None;
        for m in group {
            let Some((day, time)) = format_day_time(m.ts_ms) else {
                continue;
            };
            if current_day.as_deref() != Some(day.as_str()) {
                out.push_str(&day);
                out.push('\n');
                current_day = Some(day);
            }
            out.push_str(&format!("{time}\t{}\t{}\n", m.sender, m.body));
        }
    }
    out
}

fn write_output(out: Option<&PathBuf>, content: &str) -> Result<()> {
    match out {
        Some(path) => {
            fs::write(path, content).with_context(|| format!("writing {}", path.display()))
        }
        None => {
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(content.as_bytes())?;
            stdout.flush()?;
            Ok(())
        }
    }
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
        Command::Export { chat, format, out } => {
            let messages = db::export_messages(&conn, chat.as_deref())?;
            let content = match format {
                ExportFormat::Jsonl => render_jsonl(&messages)?,
                ExportFormat::Text => render_text(&messages),
            };
            write_output(out.as_ref(), &content)?;
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
