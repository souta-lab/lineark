use std::{fs, process::Command};

#[test]
fn local_archive_workflow() {
    let dir = std::env::temp_dir().join(format!("lineark-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let db = dir.join("archive.db");
    let export = dir.join("fictional.txt");
    fs::write(
        &export,
        "[LINE] Aliceとのトーク履歴\n2025/10/31(金)\n09:12\tBob\t明日は雨らしい\n続き",
    )
    .unwrap();
    let run = |args: &[&str]| {
        let result = Command::new(env!("CARGO_BIN_EXE_lineark"))
            .arg("--db")
            .arg(&db)
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap()
    };
    assert!(run(&["import-export", export.to_str().unwrap()]).contains("1 messages (1 new)"));
    assert!(run(&["import-export", export.to_str().unwrap()]).contains("(0 new)"));
    assert_eq!(
        run(&["search", "明日は"]),
        "2025-10-31 09:12  Alice  Bob: 明日は雨らしい / 続き\n"
    );
    assert!(run(&["chats"]).starts_with("Alice\t1\t2025-10-31 09:12"));
    let snapshots = dir.join("snapshots");
    fs::create_dir_all(&snapshots).unwrap();
    fs::write(snapshots.join("a.json"), "invalid JSON").unwrap();
    fs::write(snapshots.join("b.json"), r#"{"chat":{"mid":"c0123abcd","name":"Fictional Group","type":"GROUP"},"messages":[{"id":"1","from":"u0123abcd","contentType":"IMAGE","text":null,"createdTime":1761869520000}]}"#).unwrap();
    let summary = run(&["import-snapshot", snapshots.to_str().unwrap()]);
    assert!(summary.contains("from 1 files; 1 skipped"));
    let stats = run(&["stats"]);
    assert!(stats.contains("messages: 2\nchats: 2"));
    assert!(stats.contains("export: 1\nsnapshot: 1"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn export_text_groups_interleaved_chats() {
    let dir = std::env::temp_dir().join(format!("lineark-export-group-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let db = dir.join("archive.db");
    let run = |args: &[&str]| {
        let result = export_output(&db, args);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap()
    };
    // Alice and Bob alternate in time; Bob has the earliest message.
    let alice = dir.join("alice.txt");
    fs::write(
        &alice,
        "[LINE] Aliceとのトーク履歴\n2025/10/31(金)\n09:00\tBob\ta1\n2025/11/01(土)\n12:00\tAlice\ta3",
    )
    .unwrap();
    let bob = dir.join("bob.txt");
    fs::write(
        &bob,
        "[LINE] Bobとのトーク履歴\n2025/10/30(木)\n08:00\tAlice\tb1\n2025/10/31(金)\n10:00\tBob\tb2",
    )
    .unwrap();
    run(&["import-export", alice.to_str().unwrap()]);
    run(&["import-export", bob.to_str().unwrap()]);

    let text = run(&["export", "--format", "text"]);
    assert_eq!(
        text,
        "[LINE] Bob\n2025/10/30\n08:00\tAlice\tb1\n2025/10/31\n10:00\tBob\tb2\n\n[LINE] Alice\n2025/10/31\n09:00\tBob\ta1\n2025/11/01\n12:00\tAlice\ta3\n"
    );
    assert_eq!(text.matches("[LINE] Alice\n").count(), 1);
    assert_eq!(text.matches("[LINE] Bob\n").count(), 1);

    // JSONL stays a single global oldest-first stream.
    let timestamps: Vec<String> = run(&["export"])
        .lines()
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line).unwrap()["timestamp"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        timestamps,
        [
            "2025-10-30T08:00:00+09:00",
            "2025-10-31T09:00:00+09:00",
            "2025-10-31T10:00:00+09:00",
            "2025-11-01T12:00:00+09:00",
        ]
    );

    fs::remove_dir_all(dir).unwrap();
}

fn export_output(db: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lineark"))
        .arg("--db")
        .arg(db)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn export_jsonl_text_and_filters() {
    let dir = std::env::temp_dir().join(format!("lineark-export-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let db = dir.join("archive.db");
    let run = |args: &[&str]| {
        let result = export_output(&db, args);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap()
    };
    let alice = dir.join("alice.txt");
    fs::write(
        &alice,
        "[LINE] Aliceとのトーク履歴\n2025/10/31(金)\n21:40\tBob\tおはよう\n続き\n2025/11/01(土)\n08:00\tAlice\t[写真]",
    )
    .unwrap();
    let group = dir.join("group.txt");
    fs::write(
        &group,
        "[LINE] Fictional Groupのトーク履歴\n2025/10/30(木)\n09:12\tCarol\thello",
    )
    .unwrap();
    run(&["import-export", alice.to_str().unwrap()]);
    run(&["import-export", group.to_str().unwrap()]);

    // (a) JSONL: one object per line, exact keys, oldest first across chats.
    let jsonl = run(&["export"]);
    let records: Vec<serde_json::Value> = jsonl
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 3);
    let keys: Vec<&str> = records[0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        ["body", "chat_key", "chat_name", "sender", "timestamp"]
    );
    assert_eq!(records[0]["chat_key"], "export:Fictional Group");
    assert_eq!(records[0]["chat_name"], "Fictional Group");
    assert_eq!(records[0]["timestamp"], "2025-10-30T09:12:00+09:00");
    assert_eq!(records[0]["sender"], "Carol");
    assert_eq!(records[0]["body"], "hello");
    assert_eq!(records[1]["chat_name"], "Alice");
    assert_eq!(records[1]["timestamp"], "2025-10-31T21:40:00+09:00");
    assert_eq!(records[1]["body"], "おはよう\n続き");
    assert_eq!(records[2]["timestamp"], "2025-11-01T08:00:00+09:00");

    // (b) --chat filters by name or key substring.
    let alice_only: Vec<serde_json::Value> = run(&["export", "--chat", "Alice"])
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(alice_only.len(), 2);
    assert!(alice_only.iter().all(|r| r["chat_key"] == "export:Alice"));
    assert_eq!(run(&["export", "--chat", "Fictional"]).lines().count(), 1);
    assert_eq!(run(&["export", "--chat", "Nobody"]), "");

    // (c) Text format: header, date heading, tab-separated lines.
    let text = run(&["export", "--format", "text", "--chat", "Alice"]);
    assert_eq!(
        text,
        "[LINE] Alice\n2025/10/31\n21:40\tBob\tおはよう\n続き\n2025/11/01\n08:00\tAlice\t[写真]\n"
    );
    // Grouped by chat, chats ordered by earliest message: Fictional Group
    // (10/30) first, then Alice, whose messages stay oldest-first.
    let all_text = run(&["export", "--format", "text"]);
    assert_eq!(
        all_text,
        "[LINE] Fictional Group\n2025/10/30\n09:12\tCarol\thello\n\n[LINE] Alice\n2025/10/31\n21:40\tBob\tおはよう\n続き\n2025/11/01\n08:00\tAlice\t[写真]\n"
    );
    assert_eq!(all_text.matches("[LINE] ").count(), 2);

    // --out writes the file instead of stdout.
    let out = dir.join("out.jsonl");
    assert_eq!(run(&["export", "--out", out.to_str().unwrap()]), "");
    assert_eq!(fs::read_to_string(&out).unwrap(), jsonl);

    // Invalid --format is rejected by clap.
    let bad = export_output(&db, &["export", "--format", "csv"]);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("invalid value 'csv'"));

    fs::remove_dir_all(dir).unwrap();
}
