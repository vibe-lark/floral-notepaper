use super::super::notes::SaveNoteRequest;
use super::*;

#[derive(Default)]
struct FakeRemote {
    rows: BTreeMap<String, RemoteNote>,
    fail: bool,
    writes: usize,
}
impl Remote for FakeRemote {
    fn list(&mut self) -> Result<Vec<RemoteNote>, AppError> {
        if self.fail {
            return Err(error("offline", "offline"));
        }
        Ok(self.rows.values().cloned().collect())
    }
    fn get(&mut self, id: &str) -> Result<RemoteNote, AppError> {
        self.rows
            .values()
            .find(|n| n.record_id == id)
            .cloned()
            .ok_or_else(|| error("missing", "missing"))
    }
    fn put(&mut self, note: &SyncNote, record_id: Option<&str>) -> Result<RemoteNote, AppError> {
        if self.fail {
            return Err(error("offline", "offline"));
        }
        self.writes += 1;
        let record = RemoteNote {
            record_id: record_id
                .map(str::to_string)
                .unwrap_or_else(|| format!("rec-{}", note.id)),
            note: note.clone(),
        };
        self.rows.insert(note.id.clone(), record.clone());
        Ok(record)
    }
}
fn store() -> NoteStore {
    let root = std::env::temp_dir().join(format!("larknote-sync-{}", Uuid::new_v4()));
    let store = NoteStore::new(root.join("config"), root.join("data"));
    write_json_atomic(
        &store.config_path(),
        &json!({"globalShortcut":"Ctrl+Space","closeToTray":true,
        "autostart":false,"defaultViewMode":"split","dataDir":store.data_dir()}),
    )
    .unwrap();
    store
}
fn settings() -> SyncSettings {
    SyncSettings {
        enabled: true,
        base_token: "testBase".into(),
        table_id: "tblTest".into(),
        ..Default::default()
    }
}
fn create(store: &NoteStore, content: &str) -> Note {
    store
        .create_note(SaveNoteRequest {
            title: "便签".into(),
            content: content.into(),
            category: "工作".into(),
        })
        .unwrap()
}
fn edit(store: &NoteStore, id: &str, content: &str) {
    store
        .update_note(
            id,
            SaveNoteRequest {
                title: "便签".into(),
                content: content.into(),
                category: "工作".into(),
            },
        )
        .unwrap();
}

#[test]
fn uploads_pulls_and_second_sync_is_idempotent() {
    let a = store();
    let b = store();
    let mut remote = FakeRemote::default();
    let n = create(&a, "# 中文\n\n- [ ] Markdown 📝");
    assert_eq!(sync_with(&a, &settings(), &mut remote).unwrap().uploaded, 1);
    assert_eq!(
        sync_with(&b, &settings(), &mut remote).unwrap().downloaded,
        1
    );
    assert_eq!(b.read_note(&n.id).unwrap().content, n.content);
    assert_eq!(b.read_note(&n.id).unwrap().created_at, n.created_at);
    let report = sync_with(&a, &settings(), &mut remote).unwrap();
    assert_eq!(
        (report.uploaded, report.downloaded, remote.writes),
        (0, 0, 1)
    );
    edit(&b, &n.id, "另一个设备");
    sync_with(&b, &settings(), &mut remote).unwrap();
    sync_with(&a, &settings(), &mut remote).unwrap();
    assert_eq!(a.read_note(&n.id).unwrap().content, "另一个设备");
}

#[test]
fn conflicts_preserve_both_and_upload_copy_on_next_pass() {
    let a = store();
    let mut remote = FakeRemote::default();
    let n = create(&a, "初稿");
    sync_with(&a, &settings(), &mut remote).unwrap();
    edit(&a, &n.id, "本地修改");
    remote.rows.get_mut(&n.id).unwrap().note.content = "云端修改".into();
    assert_eq!(
        sync_with(&a, &settings(), &mut remote).unwrap().conflicts,
        1
    );
    assert_eq!(a.read_note(&n.id).unwrap().content, "云端修改");
    assert_eq!(a.list_notes().unwrap().len(), 2);
    sync_with(&a, &settings(), &mut remote).unwrap();
    assert_eq!(remote.rows.len(), 2);
    assert!(remote.rows.values().any(|r| r.note.content == "本地修改"));
    sync_with(&a, &settings(), &mut remote).unwrap();
    assert_eq!(remote.rows.len(), 2);
}

#[test]
fn soft_delete_restore_and_edit_vs_delete_keep_content() {
    let a = store();
    let b = store();
    let mut remote = FakeRemote::default();
    let n = create(&a, "保留");
    sync_with(&a, &settings(), &mut remote).unwrap();
    sync_with(&b, &settings(), &mut remote).unwrap();
    a.delete_note(&n.id).unwrap();
    sync_with(&a, &settings(), &mut remote).unwrap();
    assert!(remote.rows[&n.id].note.deleted);
    assert_eq!(remote.rows[&n.id].note.content, "保留");
    edit(&b, &n.id, "离线修改");
    assert_eq!(
        sync_with(&b, &settings(), &mut remote).unwrap().conflicts,
        1
    );
    assert!(b.read_note(&n.id).is_err());
    assert_eq!(b.list_notes().unwrap().len(), 1);
    remote.rows.get_mut(&n.id).unwrap().note.deleted = false;
    sync_with(&a, &settings(), &mut remote).unwrap();
    assert_eq!(a.read_note(&n.id).unwrap().content, "保留");
}

#[test]
fn offline_failure_keeps_local_pending_and_last_success() {
    let a = store();
    let mut remote = FakeRemote::default();
    let n = create(&a, "初稿");
    let first = sync_with(&a, &settings(), &mut remote).unwrap();
    edit(&a, &n.id, "离线草稿");
    remote.fail = true;
    assert!(sync_with(&a, &settings(), &mut remote).is_err());
    assert_eq!(a.read_note(&n.id).unwrap().content, "离线草稿");
    assert_eq!(get_report(&a).unwrap().last_synced_at, first.last_synced_at);
    remote.fail = false;
    assert_eq!(sync_with(&a, &settings(), &mut remote).unwrap().uploaded, 1);
}

#[test]
fn physical_deletion_and_target_switch_fail_closed() {
    let a = store();
    let mut remote = FakeRemote::default();
    let n = create(&a, "不丢失");
    sync_with(&a, &settings(), &mut remote).unwrap();
    remote.rows.clear();
    assert_eq!(
        sync_with(&a, &settings(), &mut remote).unwrap_err().code,
        "syncRemoteMissing"
    );
    assert_eq!(a.read_note(&n.id).unwrap().content, "不丢失");
    let mut changed = settings();
    changed.table_id = "tblOther".into();
    assert_eq!(
        sync_with(&a, &changed, &mut remote).unwrap_err().code,
        "syncTargetChanged"
    );
}

#[test]
fn crash_after_cloud_write_does_not_create_duplicate() {
    let a = store();
    let mut remote = FakeRemote::default();
    let n = create(&a, "持久化");
    remote.put(&n.clone().into(), None).unwrap();
    let report = sync_with(&a, &settings(), &mut remote).unwrap();
    assert_eq!(report.uploaded, 0);
    assert_eq!(a.list_notes().unwrap().len(), 1);
}

#[test]
fn invalid_remote_path_blocks_entire_pull_before_any_change() {
    let a = store();
    let n = create(&a, "本地安全");
    let mut remote = FakeRemote::default();
    let mut bad: SyncNote = n.clone().into();
    bad.category = "../../outside".into();
    remote.put(&bad, None).unwrap();
    assert_eq!(
        sync_with(&a, &settings(), &mut remote).unwrap_err().code,
        "syncUnsafePath"
    );
    assert_eq!(a.read_note(&n.id).unwrap().content, "本地安全");
}

#[test]
fn matrix_rejects_missing_fields_and_accepts_nullable_values() {
    let fields: Vec<_> = FIELDS.iter().map(|(n, _)| *n).collect();
    let good = json!({"fields":fields,"data":[["标题",null,"正文",null,null,null,null]],"record_id_list":["recTest"]});
    let row = parse_matrix(&good).unwrap().remove(0);
    assert_eq!(row.note.id, "base-recTest");
    assert_eq!(row.note.category, "");
    assert!(!row.note.deleted);
    assert!(parse_matrix(&json!({"fields":["标题"],"data":[],"record_id_list":[]})).is_err());
    let mut bad = good.clone();
    bad["record_id_list"] = json!([]);
    assert!(parse_matrix(&bad).is_err());
}

#[test]
fn config_validation_and_corrupt_state_never_reset_silently() {
    let a = store();
    assert!(SyncSettings::default().validate().is_err());
    let mut s = settings();
    s.interval_seconds = 0;
    assert!(s.validate().is_err());
    fs::create_dir_all(state_path(&a).parent().unwrap()).unwrap();
    fs::write(state_path(&a), "broken json").unwrap();
    assert!(sync_with(&a, &settings(), &mut FakeRemote::default()).is_err());
}

#[test]
fn dirty_editor_defers_remote_pull_and_then_keeps_unsaved_revision() {
    let a = store();
    let mut remote = FakeRemote::default();
    let n = create(&a, "原稿");
    sync_with(&a, &settings(), &mut remote).unwrap();
    mark_editor("test-window", Some(n.id.clone())).unwrap();
    remote.rows.get_mut(&n.id).unwrap().note.content = "云端修改".into();
    let report = sync_with(&a, &settings(), &mut remote).unwrap();
    assert_eq!(report.deferred, 1);
    assert_eq!(a.read_note(&n.id).unwrap().content, "原稿");
    edit(&a, &n.id, "刚保存的草稿");
    mark_editor("test-window", None).unwrap();
    let report = sync_with(&a, &settings(), &mut remote).unwrap();
    assert_eq!(report.conflicts, 1);
    assert_eq!(a.list_notes().unwrap().len(), 2);
}

#[test]
fn missing_file_or_corrupt_metadata_does_not_propagate_deletion() {
    let a = store();
    let mut remote = FakeRemote::default();
    let n = create(&a, "保留");
    sync_with(&a, &settings(), &mut remote).unwrap();
    fs::remove_file(
        a.data_dir()
            .join("notes")
            .join(&n.category)
            .join(&n.file_name),
    )
    .unwrap();
    assert_eq!(
        sync_with(&a, &settings(), &mut remote).unwrap_err().code,
        "syncMetadata"
    );
    assert!(!remote.rows[&n.id].note.deleted);
    fs::write(a.metadata_path(), "corrupt").unwrap();
    assert_eq!(a.list_notes().unwrap_err().code, "syncMetadata");
    assert!(sync_with(&a, &settings(), &mut remote).is_err());
    assert!(!remote.rows[&n.id].note.deleted);
}

#[test]
fn migration_keeps_sync_baselines_and_backups_with_notes() {
    let a = store();
    let mut remote = FakeRemote::default();
    create(&a, "迁移");
    sync_with(&a, &settings(), &mut remote).unwrap();
    let path = a.data_dir().parent().unwrap().join("migrated");
    let moved = a.migrate_data_to(&path).unwrap();
    assert!(state_path(&moved).exists());
    assert_eq!(
        sync_with(&moved, &settings(), &mut remote)
            .unwrap()
            .uploaded,
        0
    );
}

#[test]
fn local_edit_during_pull_is_deferred_instead_of_overwritten() {
    struct RacingRemote {
        inner: FakeRemote,
        store: NoteStore,
        id: String,
    }
    impl Remote for RacingRemote {
        fn list(&mut self) -> Result<Vec<RemoteNote>, AppError> {
            self.inner.list()
        }
        fn get(&mut self, id: &str) -> Result<RemoteNote, AppError> {
            edit(&self.store, &self.id, "网络请求期间继续输入");
            self.inner.get(id)
        }
        fn put(&mut self, n: &SyncNote, id: Option<&str>) -> Result<RemoteNote, AppError> {
            self.inner.put(n, id)
        }
    }
    let a = store();
    let n = create(&a, "原稿");
    let mut remote = FakeRemote::default();
    sync_with(&a, &settings(), &mut remote).unwrap();
    edit(&a, &n.id, "待上传版本");
    let mut remote = RacingRemote {
        inner: remote,
        store: a.clone(),
        id: n.id.clone(),
    };
    sync_with(&a, &settings(), &mut remote).unwrap();
    assert_eq!(a.read_note(&n.id).unwrap().content, "网络请求期间继续输入");
    assert_eq!(
        sync_with(&a, &settings(), &mut remote.inner)
            .unwrap()
            .uploaded,
        1
    );
}
