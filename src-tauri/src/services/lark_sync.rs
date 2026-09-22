//! Personal desktop sync. Lark CLI owns credentials; Base owns cloud records.
//! No network operation holds NOTE_IO, so offline editing stays responsive.
use super::notes::{AppError, Note, NoteStore};
use crate::json_io::write_json_atomic;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{LazyLock, Mutex, MutexGuard},
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

pub static NOTE_IO: Mutex<()> = Mutex::new(());
static SYNC_RUN: Mutex<()> = Mutex::new(());
static DIRTY_EDITORS: LazyLock<Mutex<BTreeMap<String, String>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

pub fn mark_editor(window: &str, note_id: Option<String>) -> Result<(), AppError> {
    let mut editors = DIRTY_EDITORS
        .lock()
        .map_err(|_| error("syncLock", "编辑状态不可用"))?;
    editors.remove(window);
    if let Some(id) = note_id {
        editors.insert(window.into(), id);
    }
    Ok(())
}

pub fn sync_guard() -> Result<MutexGuard<'static, ()>, AppError> {
    SYNC_RUN
        .try_lock()
        .map_err(|_| error("syncBusy", "正在同步，请稍后重试"))
}

pub fn error(code: &str, message: impl Into<String>) -> AppError {
    AppError {
        code: code.into(),
        message: message.into(),
        details: Default::default(),
    }
}

pub fn notes_guard() -> Result<MutexGuard<'static, ()>, AppError> {
    NOTE_IO
        .lock()
        .map_err(|_| error("syncLock", "便签存储锁不可用，请重启应用"))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncSettings {
    pub enabled: bool,
    pub base_url: String,
    pub base_token: String,
    pub table_id: String,
    pub profile: String,
    pub cli_path: String,
    pub interval_seconds: u64,
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: String::new(),
            base_token: String::new(),
            table_id: String::new(),
            profile: String::new(),
            cli_path: "lark-cli".into(),
            interval_seconds: 60,
        }
    }
}

impl SyncSettings {
    pub fn validate(&self) -> Result<(), AppError> {
        let valid_id = |s: &str| !s.is_empty() && s.bytes().all(|c| c.is_ascii_alphanumeric());
        if !valid_id(&self.base_token)
            || !valid_id(&self.table_id)
            || !self.table_id.starts_with("tbl")
        {
            return Err(error(
                "syncConfig",
                "请填写有效的 Base token 与 tbl 开头的数据表 ID（不是完整链接）",
            ));
        }
        if self.cli_path.trim().is_empty()
            || self.cli_path.contains('\0')
            || !(30..=3600).contains(&self.interval_seconds)
        {
            return Err(error(
                "syncConfig",
                "CLI 路径不能为空；同步间隔为 30–3600 秒",
            ));
        }
        Ok(())
    }
    fn target(&self) -> String {
        format!("{}:{}:{}", self.profile, self.base_token, self.table_id)
    }
}

pub fn get_settings(store: &NoteStore) -> Result<SyncSettings, AppError> {
    read_or_default(&store.config_dir().join("lark-sync.json"))
}

pub fn save_settings(store: &NoteStore, settings: SyncSettings) -> Result<SyncSettings, AppError> {
    let _run = sync_guard()?;
    if settings.enabled {
        settings.validate()?;
        let state = load_state(store)?;
        if !state.entries.is_empty() && state.target != settings.target() {
            return Err(error(
                "syncTargetChanged",
                "当前数据目录已绑定另一张表，请使用独立数据目录连接新表，防止混入旧便签",
            ));
        }
        CliRemote::new(settings.clone(), store.data_dir())?.check_schema()?;
    }
    write_json_atomic(&store.config_dir().join("lark-sync.json"), &settings)?;
    Ok(settings)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncNote {
    pub id: String,
    pub title: String,
    pub content: String,
    pub category: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted: bool,
}

impl From<Note> for SyncNote {
    fn from(n: Note) -> Self {
        Self {
            id: n.id,
            title: n.title,
            content: n.content,
            category: n.category,
            created_at: n.created_at,
            updated_at: n.updated_at,
            deleted: false,
        }
    }
}

impl SyncNote {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.id.is_empty()
            || self.id.len() > 120
            || !self
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(error(
                "syncInvalidRecord",
                "便签ID只能包含字母、数字、连字符或下划线，长度不超过120",
            ));
        }
        if self.category.contains(['/', '\\', ':', '\0'])
            || self.category.contains("..")
            || self.category.len() > 200
        {
            return Err(error(
                "syncUnsafePath",
                "云端分类包含不安全的路径字符，请在多维表格中修正",
            ));
        }
        if self.content.chars().count() > 90_000 || self.title.chars().count() > 1000 {
            return Err(error(
                "syncTooLarge",
                "便签超过同步限制（正文9万字、标题1000字）；本地内容仍然保留",
            ));
        }
        Ok(())
    }
    fn fingerprint(&self) -> String {
        if self.deleted {
            return "deleted".into();
        }
        format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&(&self.title, &self.content, &self.category))
                    .expect("strings serialize")
            )
        )
    }
    fn conflict_copy(&self) -> Self {
        let mut copy = self.clone();
        copy.id = format!(
            "conflict-{:x}",
            Sha256::digest(format!("{}:{}", self.id, self.fingerprint()))
        );
        copy.title = format!(
            "{}（冲突副本）",
            self.title.chars().take(990).collect::<String>()
        );
        copy
    }
}

#[derive(Clone, Debug)]
pub struct RemoteNote {
    pub record_id: String,
    pub note: SyncNote,
}

pub trait Remote {
    fn list(&mut self) -> Result<Vec<RemoteNote>, AppError>;
    fn get(&mut self, record_id: &str) -> Result<RemoteNote, AppError>;
    fn put(&mut self, note: &SyncNote, record_id: Option<&str>) -> Result<RemoteNote, AppError>;
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub last_synced_at: Option<DateTime<Utc>>,
    pub uploaded: usize,
    pub downloaded: usize,
    pub conflicts: usize,
    pub deferred: usize,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    fingerprint: String,
    record_id: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncState {
    target: String,
    entries: BTreeMap<String, Entry>,
    report: SyncReport,
}

fn read_or_default<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T, AppError> {
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e.into()),
    }
}
fn state_path(store: &NoteStore) -> PathBuf {
    store.data_dir().join(".lark-sync/state.json")
}
fn load_state(store: &NoteStore) -> Result<SyncState, AppError> {
    read_or_default(&state_path(store))
}
pub fn get_report(store: &NoteStore) -> Result<SyncReport, AppError> {
    Ok(load_state(store)?.report)
}

fn local_note(store: &NoteStore, id: &str) -> Result<Option<SyncNote>, AppError> {
    match store.read_note(id) {
        Ok(note) => Ok(Some(note.into())),
        Err(e) if e.code == "noteNotFound" => Ok(None),
        Err(e) => Err(e),
    }
}
fn local_hash(note: Option<&SyncNote>) -> String {
    note.map(|n| n.fingerprint())
        .unwrap_or_else(|| "deleted".into())
}

pub fn sync() -> Result<SyncReport, AppError> {
    let _run = sync_guard()?;
    // Resolve after acquiring the migration/sync lock, not before it.
    let store = super::notes::default_store()?;
    let settings = get_settings(&store)?;
    if !settings.enabled {
        return Err(error("syncDisabled", "请先在设置中启用飞书同步"));
    }
    settings.validate()?;
    let mut remote = CliRemote::new(settings.clone(), store.data_dir())?;
    remote.check_schema()?;
    sync_inner(&store, &settings, &mut remote)
}

/// Three-way merge against the last acknowledged content, not device clocks.
/// All remote reads must complete before any changes are made locally.
pub fn sync_with(
    store: &NoteStore,
    settings: &SyncSettings,
    remote: &mut dyn Remote,
) -> Result<SyncReport, AppError> {
    let _run = sync_guard()?;
    sync_inner(store, settings, remote)
}

fn sync_inner(
    store: &NoteStore,
    settings: &SyncSettings,
    remote: &mut dyn Remote,
) -> Result<SyncReport, AppError> {
    let mut state = load_state(store)?;
    if !state.entries.is_empty() && state.target != settings.target() {
        return Err(error(
            "syncTargetChanged",
            "同步目标与本地状态不一致，已停止以避免混入其他数据库",
        ));
    }
    state.target = settings.target();
    let mut cloud = BTreeMap::new();
    for record in remote.list()? {
        record.note.validate()?;
        if cloud.insert(record.note.id.clone(), record).is_some() {
            return Err(error(
                "syncDuplicateId",
                "多维表格存在重复便签ID，请处理重复记录后重试；未覆盖本地内容",
            ));
        }
    }
    // Physical deletion or changed IDs are ambiguous. Never infer a mass delete
    // from missing rows, permissions, or an incomplete query.
    for (id, entry) in &state.entries {
        if !cloud.contains_key(id) {
            return Err(error(
                "syncRemoteMissing",
                format!(
                    "云端记录 {} 缺失。请恢复该行/便签ID，删除请使用“已删除”复选框",
                    entry.record_id
                ),
            ));
        }
    }
    let local: BTreeMap<String, SyncNote> = {
        let _io = notes_guard()?;
        // The original store can rebuild IDs after metadata loss. During sync
        // that would look like deletes + creates, so fail before such recovery.
        let before = match fs::read(store.metadata_path()) {
            Ok(bytes) => {
                let metadata: Value = serde_json::from_slice(&bytes)?;
                Some(
                    metadata["notes"]
                        .as_array()
                        .ok_or_else(|| error("syncMetadata", "本地元数据格式损坏，请先恢复备份"))?
                        .len(),
                )
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && state.entries.is_empty() => None,
            Err(_) => {
                return Err(error(
                    "syncMetadata",
                    "本地元数据缺失，暂停同步以避免错误传播删除",
                ))
            }
        };
        let notes = store.list_notes()?;
        if before.is_some_and(|count| count != notes.len()) {
            return Err(error(
                "syncMetadata",
                "本地笔记文件与元数据不一致，请恢复文件；不会将缺失文件传播为云端删除",
            ));
        }
        notes
            .into_iter()
            .map(|m| store.read_note(&m.id).map(|n| (n.id.clone(), n.into())))
            .collect::<Result<_, _>>()?
    };
    let ids: BTreeSet<String> = local.keys().chain(cloud.keys()).cloned().collect();
    let mut report = SyncReport::default();
    for id in ids {
        let l = local.get(&id);
        let r = cloud.get(&id);
        let baseline = state.entries.get(&id).map(|e| e.fingerprint.clone());
        let lh = local_hash(l);
        let rh = r.map(|r| r.note.fingerprint());
        // Equal content also recovers a write that succeeded before a crash.
        if rh.as_ref() == Some(&lh) {
            let r = r.expect("remote hash exists");
            state.entries.insert(
                id,
                Entry {
                    fingerprint: lh,
                    record_id: r.record_id.clone(),
                },
            );
            write_json_atomic(&state_path(store), &state)?;
            continue;
        }
        let local_changed = if baseline.is_some() {
            baseline.as_ref() != Some(&lh)
        } else {
            l.is_some()
        };
        let remote_changed = r.is_some() && baseline.as_ref() != rh.as_ref();

        if !remote_changed && local_changed || r.is_none() {
            let mut upload = match (l, r) {
                (Some(n), _) => n.clone(),
                (None, Some(r)) => {
                    let mut n = r.note.clone();
                    n.deleted = true;
                    n.updated_at = Utc::now();
                    n
                }
                _ => continue,
            };
            upload.validate()?;
            if let Some(r) = r {
                let latest = remote.get(&r.record_id)?;
                if latest.note.id != id
                    || latest.note.fingerprint() != rh.clone().unwrap_or_default()
                {
                    report.deferred += 1;
                    continue;
                }
            }
            upload.updated_at = Utc::now();
            let written = remote.put(&upload, r.map(|r| r.record_id.as_str()))?;
            if written.note.fingerprint() != upload.fingerprint() {
                return Err(error(
                    "syncReadback",
                    "云端写入回读不一致，本地同步进度未确认，请重试",
                ));
            }
            state.entries.insert(
                id,
                Entry {
                    fingerprint: upload.fingerprint(),
                    record_id: written.record_id,
                },
            );
            report.uploaded += 1;
        } else if let Some(r) = r {
            // Re-check while holding the desktop write lock. Edits made during
            // slow network reads are left pending, never replaced by the pull.
            let _io = notes_guard()?;
            if DIRTY_EDITORS
                .lock()
                .map_err(|_| error("syncLock", "编辑状态不可用"))?
                .values()
                .any(|value| value == &id)
            {
                report.deferred += 1;
                continue;
            }
            if local_hash(local_note(store, &id)?.as_ref()) != lh {
                report.deferred += 1;
                continue;
            }
            if local_changed {
                if let Some(l) = l {
                    let copy = l.conflict_copy();
                    if local_note(store, &copy.id)?.is_none() {
                        store.apply_synced_note(&copy)?;
                    }
                    // The copy is durable locally; upload it on the next pass.
                    // No remote write is needed while the note lock is held.
                }
                report.conflicts += 1;
            }
            store.apply_synced_note(&r.note)?;
            state.entries.insert(
                id,
                Entry {
                    fingerprint: r.note.fingerprint(),
                    record_id: r.record_id.clone(),
                },
            );
            report.downloaded += 1;
        }
        write_json_atomic(&state_path(store), &state)?;
    }
    report.last_synced_at = Some(Utc::now());
    report.message = if report.conflicts > 0 {
        "已保留冲突副本；副本将在下次同步上传"
    } else if report.deferred > 0 {
        "部分便签同步期间发生修改，将在下次同步处理"
    } else {
        "已与飞书多维表格同步"
    }
    .into();
    state.report = report.clone();
    write_json_atomic(&state_path(store), &state)?;
    Ok(report)
}

const FIELDS: [(&str, &str); 7] = [
    ("标题", "text"),
    ("便签ID", "text"),
    ("正文", "text"),
    ("分类", "text"),
    ("创建时间", "datetime"),
    ("修改时间", "datetime"),
    ("已删除", "checkbox"),
];

pub struct CliRemote {
    settings: SyncSettings,
    work_dir: PathBuf,
}
impl CliRemote {
    pub fn new(settings: SyncSettings, data_dir: &Path) -> Result<Self, AppError> {
        settings.validate()?;
        let work_dir = data_dir.join(".lark-sync/requests");
        fs::create_dir_all(&work_dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&work_dir, fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self { settings, work_dir })
    }

    fn call(&self, action: &str, extra: &[String]) -> Result<Value, AppError> {
        let mut args = vec![
            "base".into(),
            action.into(),
            "--base-token".into(),
            self.settings.base_token.clone(),
            "--table-id".into(),
            self.settings.table_id.clone(),
        ];
        args.extend_from_slice(extra);
        if action == "+record-list" || action == "+record-get" {
            for (name, _) in FIELDS {
                args.extend(["--field-id".into(), name.into()]);
            }
        }
        run_cli(&self.settings, &self.work_dir, &args)
    }

    pub fn check_schema(&self) -> Result<(), AppError> {
        let data = self.call("+field-list", &[])?;
        let fields = data["fields"]
            .as_array()
            .ok_or_else(|| error("syncSchema", "无法读取字段结构"))?;
        for (name, kind) in FIELDS {
            if !fields
                .iter()
                .any(|f| f["name"] == name && f["type"] == kind)
            {
                return Err(error(
                    "syncSchema",
                    format!("数据表缺少 {name}（{kind}）字段；请使用为便签准备的专用表"),
                ));
            }
        }
        Ok(())
    }
}

pub(super) fn run_cli(
    settings: &SyncSettings,
    work_dir: &Path,
    args: &[String],
) -> Result<Value, AppError> {
    let executable = super::lark_connection::find_cli(&settings.cli_path)?;
    let mut command = Command::new(executable);
    if !settings.profile.is_empty() {
        command.args(["--profile", &settings.profile]);
    }
    command
        .args(args)
        .args(["--as", "user", "--format", "json"])
        .current_dir(work_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("LARKSUITE_CLI_NO_UPDATE_NOTIFIER", "1")
        .env("LARKSUITE_CLI_NO_SKILLS_NOTIFIER", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = command.spawn().map_err(|_| {
        error(
            "syncCliMissing",
            "无法启动 Lark CLI；请先在这台电脑安装并登录 Lark CLI，程序会自动查找",
        )
    })?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let output = thread::spawn(move || {
        let mut b = Vec::new();
        stdout.read_to_end(&mut b).map(|_| b)
    });
    let errors = thread::spawn(move || {
        let mut b = Vec::new();
        stderr.read_to_end(&mut b).map(|_| b)
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= Duration::from_secs(45) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = output.join();
            let _ = errors.join();
            return Err(error(
                "syncTimeout",
                "飞书请求超时；本地便签已保留，稍后可重试",
            ));
        }
        thread::sleep(Duration::from_millis(50));
    };
    let out = output
        .join()
        .map_err(|_| error("syncCli", "读取CLI结果失败"))??;
    let err = errors
        .join()
        .map_err(|_| error("syncCli", "读取CLI错误失败"))??;
    let envelope: Value = serde_json::from_slice(if status.success() { &out } else { &err })
        .map_err(|_| {
            error(
                "syncCli",
                "Lark CLI 返回无效结果，请在终端检查登录状态和版本",
            )
        })?;
    if !status.success() || envelope["ok"] != true {
        // Never return raw stderr/request bodies or credentials to the UI.
        let code = envelope
            .pointer("/error/code")
            .map(|v| v.to_string())
            .unwrap_or_default();
        return Err(error(
            "syncApi",
            format!(
                "飞书请求失败（{}）。请检查当前用户登录、表权限和字段结构；不会自动切换为机器人",
                code
            ),
        ));
    }
    envelope
        .get("data")
        .cloned()
        .ok_or_else(|| error("syncProtocol", "CLI结果缺少 data"))
}

fn string_cell(row: &serde_json::Map<String, Value>, name: &str) -> Result<String, AppError> {
    match row.get(name) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(Value::Null) => Ok(String::new()),
        _ => Err(error("syncProtocol", format!("字段 {name} 不是文本"))),
    }
}

fn parse_matrix(data: &Value) -> Result<Vec<RemoteNote>, AppError> {
    let fields = data["fields"]
        .as_array()
        .ok_or_else(|| error("syncProtocol", "缺少 fields"))?;
    for (name, _) in FIELDS {
        if !fields.iter().any(|f| f == name) {
            return Err(error(
                "syncProtocol",
                format!("读取结果缺少 {name}，停止同步"),
            ));
        }
    }
    let rows = data["data"]
        .as_array()
        .ok_or_else(|| error("syncProtocol", "缺少行数据"))?;
    let ids = data["record_id_list"]
        .as_array()
        .ok_or_else(|| error("syncProtocol", "缺少记录ID"))?;
    if rows.len() != ids.len() {
        return Err(error("syncProtocol", "记录ID与行数不一致"));
    }
    rows.iter()
        .zip(ids)
        .map(|(values, record_id)| {
            let values = values
                .as_array()
                .ok_or_else(|| error("syncProtocol", "无效的行数据"))?;
            if values.len() != fields.len() {
                return Err(error("syncProtocol", "字段数与单元格数不一致"));
            }
            let record_id = record_id
                .as_str()
                .ok_or_else(|| error("syncProtocol", "无效的记录ID"))?
                .to_string();
            let row: serde_json::Map<String, Value> = fields
                .iter()
                .zip(values)
                .map(|(f, v)| (f.as_str().unwrap_or_default().to_string(), v.clone()))
                .collect();
            let id = string_cell(&row, "便签ID")?;
            let date = |name: &str| -> Result<DateTime<Utc>, AppError> {
                let s = string_cell(&row, name)?;
                if s.is_empty() {
                    return Ok(Utc::now());
                }
                DateTime::parse_from_rfc3339(&s)
                    .map(|d| d.with_timezone(&Utc))
                    .map_err(|_| error("syncProtocol", format!("无效的{name}")))
            };
            let deleted = match &row["已删除"] {
                Value::Bool(b) => *b,
                Value::Null => false,
                _ => return Err(error("syncProtocol", "已删除字段不是布尔值")),
            };
            let note = SyncNote {
                id: if id.is_empty() {
                    format!("base-{record_id}")
                } else {
                    id
                },
                title: string_cell(&row, "标题")?,
                content: string_cell(&row, "正文")?,
                category: string_cell(&row, "分类")?,
                created_at: date("创建时间")?,
                updated_at: date("修改时间")?,
                deleted,
            };
            note.validate()?;
            Ok(RemoteNote { record_id, note })
        })
        .collect()
}

impl Remote for CliRemote {
    fn list(&mut self) -> Result<Vec<RemoteNote>, AppError> {
        let mut result = Vec::new();
        let mut revision = None;
        loop {
            let data = self.call(
                "+record-list",
                &[
                    "--limit".into(),
                    "200".into(),
                    "--offset".into(),
                    result.len().to_string(),
                ],
            )?;
            let rev = data
                .get("rev")
                .ok_or_else(|| error("syncProtocol", "缺少表版本"))?;
            if let Some(previous) = &revision {
                if previous != rev {
                    return Err(error("syncTableChanged", "分页读取期间表发生变化，请重试"));
                }
            }
            revision = Some(rev.clone());
            let page = parse_matrix(&data)?;
            let more = data["has_more"]
                .as_bool()
                .ok_or_else(|| error("syncProtocol", "缺少分页完成标记"))?;
            if more && page.is_empty() {
                return Err(error("syncProtocol", "分页未前进，停止同步"));
            }
            result.extend(page);
            if !more {
                return Ok(result);
            }
            if result.len() >= 10_000 {
                return Err(error(
                    "syncLimit",
                    "当前同步支持最多10000条便签（含软删除）；未执行任何修改",
                ));
            }
        }
    }
    fn get(&mut self, record_id: &str) -> Result<RemoteNote, AppError> {
        let data = self.call("+record-get", &["--record-id".into(), record_id.into()])?;
        let mut records = parse_matrix(&data)?;
        if records.len() != 1 {
            return Err(error("syncRemoteMissing", "云端记录不存在或不可访问"));
        }
        Ok(records.remove(0))
    }
    fn put(&mut self, note: &SyncNote, record_id: Option<&str>) -> Result<RemoteNote, AppError> {
        note.validate()?;
        // Retry after an uncertain create by stable ID, not by title.
        let existing = if record_id.is_none() {
            let filter = json!({"logic":"and","conditions":[["便签ID","==",note.id]]});
            let data = self.call(
                "+record-list",
                &[
                    "--filter-json".into(),
                    filter.to_string(),
                    "--limit".into(),
                    "2".into(),
                ],
            )?;
            let found = parse_matrix(&data)?;
            if found.len() > 1 || data["has_more"] == true {
                return Err(error("syncDuplicateId", "云端便签ID重复，请先处理重复行"));
            }
            if let Some(found) = found.into_iter().next() {
                if found.note.fingerprint() == note.fingerprint() {
                    return Ok(found);
                }
                return Err(error(
                    "syncConcurrentCreate",
                    "云端出现同ID的新版本，请重新同步以保留两份内容",
                ));
            }
            None
        } else {
            record_id.map(str::to_string)
        };
        let payload = json!({"标题":note.title,"便签ID":note.id,"正文":note.content,"分类":note.category,
            "创建时间":note.created_at.to_rfc3339(),"修改时间":note.updated_at.to_rfc3339(),"已删除":note.deleted});
        let name = format!("write-{}.json", Uuid::new_v4());
        let path = self.work_dir.join(&name);
        write_json_atomic(&path, &payload)?;
        let mut args = vec!["--json".into(), format!("@{name}")];
        if let Some(id) = &existing {
            args.extend(["--record-id".into(), id.clone()]);
        }
        let response = self.call("+record-upsert", &args);
        let _ = fs::remove_file(&path);
        let data = response?;
        let id = existing
            .or_else(|| {
                data.pointer("/record/record_id_list/0")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .ok_or_else(|| {
                error(
                    "syncProtocol",
                    "云端写入未返回记录ID；请重新同步，不要重复创建",
                )
            })?;
        self.get(&id)
    }
}

#[cfg(test)]
#[path = "lark_sync_tests.rs"]
mod tests;
