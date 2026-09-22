import { invoke } from "@tauri-apps/api/core";

export interface SyncConnection {
  url: string;
  enabled: boolean;
}
export const getSyncConnection = () => invoke<SyncConnection>("lark_sync_connection_get");
export const connectSync = (url: string) => invoke<SyncConnection>("lark_sync_connect", { url });
export const disconnectSync = () => invoke<SyncConnection>("lark_sync_disconnect");

export interface SyncSettings {
  enabled: boolean;
  baseToken: string;
  tableId: string;
  profile: string;
  cliPath: string;
  intervalSeconds: number;
}
export interface SyncReport {
  lastSyncedAt: string | null;
  uploaded: number;
  downloaded: number;
  conflicts: number;
  deferred: number;
  message: string;
}
export interface SyncStatus {
  phase: "idle" | "syncing" | "error";
  enabled: boolean;
  report: SyncReport;
  error: string | null;
}
export const DEFAULT_SYNC_SETTINGS: SyncSettings = {
  enabled: false,
  baseToken: "",
  tableId: "",
  profile: "",
  cliPath: "lark-cli",
  intervalSeconds: 60,
};
export const EMPTY_SYNC_STATUS: SyncStatus = {
  phase: "idle",
  enabled: false,
  error: null,
  report: {
    lastSyncedAt: null,
    uploaded: 0,
    downloaded: 0,
    conflicts: 0,
    deferred: 0,
    message: "尚未同步",
  },
};
export const getSyncSettings = () => invoke<SyncSettings>("lark_sync_settings_get");
export const saveSyncSettings = (settings: SyncSettings) =>
  invoke<SyncSettings>("lark_sync_settings_save", { settings });
export const getSyncStatus = () => invoke<SyncStatus>("lark_sync_status");
export const syncNow = () => invoke<SyncReport>("lark_sync_now");
export const markSyncEditor = (noteId: string | null) =>
  invoke<void>("lark_sync_editor_state", { noteId });

export function syncStatusLabel(status: SyncStatus): string {
  if (!status.enabled) return "飞书同步未启用";
  if (status.phase === "syncing") return "正在同步…";
  if (status.phase === "error") return "同步失败 · 本地已保留";
  if (status.report.deferred) return "等待编辑完成后同步";
  if (status.report.conflicts) return "已保留冲突副本";
  return status.report.lastSyncedAt ? "飞书已同步" : "等待首次同步";
}
