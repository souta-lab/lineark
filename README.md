# lineark

<p align="center">
  <img src="assets/logo.png" alt="lineark logo" width="160">
</p>

A searchable local SQLite archive for LINE chat history.

![Rust](https://img.shields.io/badge/Rust-2021-orange)
![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)
![SQLite](https://img.shields.io/badge/SQLite-FTS5-003B57)

## Contents

- [📖 About](#-about)
- [✨ Features](#-features)
- [📦 Install](#-install)
- [🚀 Usage](#-usage)
- [🗂️ Data sources](#️-data-sources)
- [🔍 Search notes](#-search-notes)
- [🛠️ Development](#️-development)
- [📄 License](#-license)

## 📖 About

LINE redistributes only recent history to new clients. Exported phone histories and linejs snapshots can preserve older messages in one local archive. lineark reads local files only; it makes no network requests and needs no credentials.

## ✨ Features

- Import UTF-8 phone exports and JSON snapshots.
- Deduplicate messages by chat, timestamp, sender, and body.
- Search Japanese and other text with SQLite FTS5.
- List chats and archive statistics. Display timestamps in Japan time (UTC+9).
- Export messages as JSONL or LINE-style text, to stdout or a file.

## 📦 Install

With a Rust toolchain and C compiler installed:

```sh
cargo build --release
# Binary: target/release/lineark
```

SQLite is bundled with the binary.

## 🚀 Usage

```sh
lineark import-export ./fictional-chat.txt
lineark import-snapshot ./snapshots
lineark search '明日は' --chat Alice --limit 20
lineark chats
lineark stats
lineark export
lineark export --format text --chat Alice
lineark export --format jsonl --out ./alice.jsonl
```

The database defaults to `lineark.db`. Set `LINEARK_DB` or use `lineark --db ./archive.db chats` to select another path. Snapshot import reads `.json` files in filename order, without descending into subdirectories; unreadable or invalid files generate warnings and are skipped.

`lineark export` writes all matching messages. JSONL is a single oldest-first stream across chats; text output is grouped by chat (see below). `--chat` filters by substring of chat name or key, like `search --chat`; omit it to export every chat. `--format` is `jsonl` (default) or `text`, and `--out <path>` writes to a file instead of stdout.

- **JSONL:** one object per line with the keys `chat_key`, `chat_name`, `timestamp`, `sender`, and `body`. Timestamps are ISO 8601 in JST, e.g. `"2026-11-12T21:34:00+09:00"`.
- **Text:** grouped by chat, with chats ordered by their earliest message (ties by chat key). Each chat appears once with a `[LINE] <chat name>` header (the chat key when unnamed), then a `YYYY/MM/DD` heading per day and `HH:MM<TAB>sender<TAB>body` lines, oldest first within the chat. Chats are separated by a blank line. Multiline bodies keep their line breaks, as in LINE's own exports.

## 🗂️ Data sources

**Phone exports:** UTF-8 text from LINE's 「トーク履歴を送信」, with a `[LINE] Aliceとのトーク履歴` or group header, date headings, and tab-separated time, sender, and body. BOMs, CRLF endings, multiline messages, and placeholders such as `[写真]` are supported. Chat keys use `export:<chat name>`.

**linejs snapshots:** JSON containing a `chat` object (`mid`, plus `type` and `name` when present — 1:1 chats come with a minimal object without a name) and `messages` array (`id`, `from`, `contentType`, `text`, `createdTime`). Extra fields are ignored. Messages that carry text keep it (including `CALL` entries, whose text is a call summary); media without text becomes a placeholder such as `[image]`; empty NONE messages are skipped. Chat keys use the snapshot chat MID.

Exports and snapshots keep separate chat identifiers; lineark does not infer that two chats represent the same conversation. Attachments are represented by text placeholders, not downloaded or embedded.

## 🔍 Search notes

Queries with at least three Unicode characters use FTS5's trigram tokenizer, supporting Japanese substring search without word segmentation. Queries are quoted as literal FTS phrases. Queries of one or two characters use SQLite `LIKE`; `%` and `_` follow LIKE wildcard rules. The optional chat filter also uses LIKE substring matching on name or key.

Results show the oldest matches first, up to the limit (default 50). Multiline bodies are displayed with ` / ` between lines. All displayed dates use fixed UTC+9.

## 🛠️ Development

```sh
cargo build
cargo test
```

Tests use synthetic messages and an in-memory database. Run with `--offline` when dependencies are already cached and network access is unavailable.

## 📄 License

[MIT](LICENSE), Copyright (c) 2026 s0u7a.
