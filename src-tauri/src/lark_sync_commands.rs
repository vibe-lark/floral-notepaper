use crate::services::{
    lark_sync::{self, SyncReport, SyncSettings},
    notes::{default_store, AppError},
};
use serde::Serialize;
use std::{
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub phase: String,
    pub enabled: bool,
    pub report: SyncReport,
    pub error: Option<String>,
}
static STATUS: LazyLock<Mutex<SyncStatus>> = LazyLock::new(|| Mutex::new(SyncStatus::default()));
static RUNNING: Mutex<()> = Mutex::new(());

#[tauri::command]
pub fn lark_sync_editor_state(
    window: tauri::Window,
    note_id: Option<String>,
) -> Result<(), AppError> {
    lark_sync::mark_editor(window.label(), note_id)
}

fn publish(app: &AppHandle, status: SyncStatus) {
    if let Ok(mut current) = STATUS.lock() {
        *current = status.clone();
    }
    let _ = app.emit("lark-sync-status", &status);
}

#[tauri::command]
pub fn lark_sync_settings_get() -> Result<SyncSettings, AppError> {
    lark_sync::get_settings(&default_store()?)
}

#[tauri::command]
pub fn lark_sync_status() -> Result<SyncStatus, AppError> {
    let store = default_store()?;
    let mut status = STATUS
        .lock()
        .map_err(|_| lark_sync::error("syncLock", "同步状态不可用"))?
        .clone();
    status.enabled = lark_sync::get_settings(&store)?.enabled;
    if status.phase.is_empty() {
        status.phase = "idle".into();
        status.report = lark_sync::get_report(&store)?;
    }
    Ok(status)
}

#[tauri::command]
pub async fn lark_sync_settings_save(
    app: AppHandle,
    settings: SyncSettings,
) -> Result<SyncSettings, AppError> {
    let saved = tauri::async_runtime::spawn_blocking(move || {
        lark_sync::save_settings(&default_store()?, settings)
    })
    .await
    .map_err(|_| lark_sync::error("syncWorker", "保存同步设置失败"))??;
    if let Ok(status) = lark_sync_status() {
        publish(&app, status);
    }
    Ok(saved)
}

fn run(app: &AppHandle) -> Result<SyncReport, AppError> {
    let _running = RUNNING
        .try_lock()
        .map_err(|_| lark_sync::error("syncBusy", "正在同步，请稍后重试"))?;
    let mut status = lark_sync_status()?;
    status.phase = "syncing".into();
    status.error = None;
    publish(app, status.clone());
    match lark_sync::sync() {
        Ok(report) => {
            status.phase = "idle".into();
            status.report = report.clone();
            publish(app, status);
            // Also publish after a partial failure below; a completed pull may
            // precede an unrelated record error in the same synchronization.
            let _ = app.emit("notes-changed", ());
            Ok(report)
        }
        Err(error) => {
            status.phase = "error".into();
            status.error = Some(error.message.clone());
            publish(app, status);
            let _ = app.emit("notes-changed", ());
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn lark_sync_now(app: AppHandle) -> Result<SyncReport, AppError> {
    tauri::async_runtime::spawn_blocking(move || run(&app))
        .await
        .map_err(|_| lark_sync::error("syncWorker", "同步工作线程失败"))?
}

/// Native scheduler keeps working when every window is hidden in the tray.
pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let mut next = Instant::now() + Duration::from_secs(5);
        loop {
            std::thread::sleep(Duration::from_secs(2));
            if Instant::now() < next {
                continue;
            }
            match default_store().and_then(|s| lark_sync::get_settings(&s)) {
                Ok(settings) if settings.enabled => {
                    let _ = run(&app);
                    next = Instant::now()
                        + Duration::from_secs(settings.interval_seconds.clamp(30, 3600));
                }
                _ => next = Instant::now() + Duration::from_secs(5),
            }
        }
    });
}
