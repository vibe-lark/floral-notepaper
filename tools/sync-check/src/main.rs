use larknote_sync_check::{
    json_io::write_json_atomic,
    services::{
        lark_connection,
        lark_sync::{self, CliRemote, Remote, SyncSettings},
        notes::{NoteStore, SaveNoteRequest},
    },
};
use serde_json::json;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn isolated_store(root: &Path) -> Result<NoteStore, Box<dyn std::error::Error>> {
    let store = NoteStore::new(root.join("config"), root.join("data"));
    if !store.config_path().exists() {
        // Seed config before accessing the upstream NoteStore to skip legacy
        // discovery of the user's existing Floral Notepaper installation.
        write_json_atomic(
            &store.config_path(),
            &json!({"globalShortcut":"Ctrl+Space","closeToTray":true,
            "autostart":false,"defaultViewMode":"split","dataDir":store.data_dir()}),
        )?;
    }
    Ok(store)
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("help");
    if mode == "resolve-link" {
        let link = args.get(2).ok_or("A Base URL is required")?;
        let root = PathBuf::from(
            args.get(3)
                .map(String::as_str)
                .unwrap_or(".local/link-check"),
        );
        fs::create_dir_all(&root)?;
        let store = isolated_store(&fs::canonicalize(root)?)?;
        let resolved = lark_connection::resolve_link(&store, link)?;
        println!(
            "{}",
            json!({"ok":true,"url":resolved.base_url,"baseToken":resolved.base_token,
            "tableId":resolved.table_id,"cliDetected":true,"schemaVerified":true,"remoteWrites":false})
        );
        return Ok(());
    }
    if !["check", "sync", "verify-live"].contains(&mode) {
        println!("Usage: larknote-sync-check <check|sync|verify-live> <connection.json> [isolated-root]\nverify-live creates test notes and soft-deletes its own test notes at completion; it never deletes Base rows.");
        return Ok(());
    }
    let connection = args.get(2).ok_or("connection.json is required")?;
    let settings: SyncSettings = serde_json::from_slice(&fs::read(connection)?)?;
    settings.validate()?;
    let root = PathBuf::from(args.get(3).map(String::as_str).unwrap_or(".local/headless"));
    fs::create_dir_all(&root)?;
    let root = fs::canonicalize(root)?;
    let a = isolated_store(&root.join("device-a"))?;
    let mut remote = CliRemote::new(settings.clone(), a.data_dir())?;
    remote.check_schema()?;
    if mode == "check" {
        println!(
            "{}",
            json!({"ok":true,"schema":"verified","records":remote.list()?.len()})
        );
        return Ok(());
    }
    if !settings.enabled {
        return Err("Sync is disabled in the connection file".into());
    }
    if mode == "sync" {
        println!(
            "{}",
            serde_json::to_string_pretty(&lark_sync::sync_with(&a, &settings, &mut remote)?)?
        );
        return Ok(());
    }

    let b = isolated_store(&root.join("device-b"))?;
    lark_sync::sync_with(&a, &settings, &mut remote)?;
    let note = a.create_note(SaveNoteRequest { title: "[同步验收] Markdown 双向同步".into(),
        content: "# 验收便签\n\n- [ ] 双设备\n- [x] 中文与 emoji 📝\n\n```js\nconst value = '<safe>';\n```".into(), category: "同步验收".into() })?;
    lark_sync::sync_with(&a, &settings, &mut remote)?;
    let unchanged = lark_sync::sync_with(&a, &settings, &mut remote)?;
    assert_eq!(
        unchanged.uploaded, 0,
        "repeat sync must not duplicate writes"
    );
    lark_sync::sync_with(&b, &settings, &mut remote)?;
    assert_eq!(
        b.read_note(&note.id)?.content,
        note.content,
        "Markdown round trip"
    );
    let row = remote
        .list()?
        .into_iter()
        .find(|r| r.note.id == note.id)
        .ok_or("created record missing")?;
    let mut cloud = row.note.clone();
    cloud.content = "# 直接修改多维表格\n云端 → 桌面".into();
    remote.put(&cloud, Some(&row.record_id))?;
    lark_sync::sync_with(&a, &settings, &mut remote)?;
    assert_eq!(a.read_note(&note.id)?.content, cloud.content);
    a.update_note(
        &note.id,
        SaveNoteRequest {
            title: note.title.clone(),
            content: "本地离线修改".into(),
            category: "同步验收".into(),
        },
    )?;
    cloud.content = "云端并发修改".into();
    remote.put(&cloud, Some(&row.record_id))?;
    let before_conflict: Vec<_> = a.list_notes()?.into_iter().map(|n| n.id).collect();
    let conflict = lark_sync::sync_with(&a, &settings, &mut remote)?;
    assert_eq!(conflict.conflicts, 1);
    assert_eq!(a.read_note(&note.id)?.content, cloud.content);
    lark_sync::sync_with(&a, &settings, &mut remote)?;
    let new_copy_ids: Vec<_> = a
        .list_notes()?
        .into_iter()
        .filter(|n| !before_conflict.contains(&n.id))
        .map(|n| n.id)
        .collect();
    let copies: Vec<_> = remote
        .list()?
        .into_iter()
        .filter(|r| new_copy_ids.contains(&r.note.id))
        .collect();
    assert!(!copies.is_empty(), "local conflict copy uploaded");

    // Only this run's original note is deleted; cloud tombstones keep its text.
    a.delete_note(&note.id)?;
    lark_sync::sync_with(&a, &settings, &mut remote)?;
    lark_sync::sync_with(&b, &settings, &mut remote)?;
    assert!(b.read_note(&note.id).is_err());
    let deleted = remote.get(&row.record_id)?;
    assert!(deleted.note.deleted);
    assert_eq!(deleted.note.content, cloud.content);
    // Restore through the same checkbox users can edit in Base.
    let mut restored = deleted.note;
    restored.deleted = false;
    remote.put(&restored, Some(&row.record_id))?;
    lark_sync::sync_with(&a, &settings, &mut remote)?;
    assert_eq!(a.read_note(&note.id)?.content, restored.content);
    // Retain audit records in Base, with no irreversible cleanup.
    restored.deleted = true;
    remote.put(&restored, Some(&row.record_id))?;
    let local_copy_ids: Vec<_> = a
        .list_notes()?
        .into_iter()
        .filter(|n| n.title == format!("{}（冲突副本）", note.title))
        .map(|n| n.id)
        .collect();
    for copy in copies
        .into_iter()
        .filter(|r| local_copy_ids.contains(&r.note.id))
    {
        let mut tombstone = copy.note;
        tombstone.deleted = true;
        remote.put(&tombstone, Some(&copy.record_id))?;
    }
    lark_sync::sync_with(&a, &settings, &mut remote)?;
    lark_sync::sync_with(&b, &settings, &mut remote)?;
    let evidence = json!({"ok":true,"verifiedAt":chrono::Utc::now(),"recordId":row.record_id,
        "checks":["schema","markdown-roundtrip","two-device-pull","idempotence","cloud-to-local","conflict-copy","soft-delete","restore"],
        "testRecords":"soft-deleted, content retained in Base","source":"production Rust NoteStore and sync engine"});
    write_json_atomic(&root.join("verification.json"), &evidence)?;
    println!("{}", serde_json::to_string_pretty(&evidence)?);
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
