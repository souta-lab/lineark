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
