import { describe, expect, test, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import {
  DEFAULT_SYNC_SETTINGS,
  EMPTY_SYNC_STATUS,
  getSyncSettings,
  saveSyncSettings,
  syncNow,
  markSyncEditor,
  syncStatusLabel,
} from "./api";
import { SyncSettingsSection } from "./SyncSettingsSection";
import { invoke } from "@tauri-apps/api/core";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

describe("飞书同步", () => {
  test("uses narrow native commands and never accepts credentials", async () => {
    await getSyncSettings();
    await saveSyncSettings(DEFAULT_SYNC_SETTINGS);
    await syncNow();
    await markSyncEditor("note1");
    expect(invoke).toHaveBeenCalledWith("lark_sync_settings_get");
    expect(invoke).toHaveBeenCalledWith("lark_sync_settings_save", {
      settings: DEFAULT_SYNC_SETTINGS,
    });
    expect(invoke).toHaveBeenCalledWith("lark_sync_now");
    expect(invoke).toHaveBeenCalledWith("lark_sync_editor_state", { noteId: "note1" });
  });
  test("distinguishes errors, pending edits and first sync", () => {
    expect(syncStatusLabel(EMPTY_SYNC_STATUS)).toContain("未启用");
    const enabled = { ...EMPTY_SYNC_STATUS, enabled: true };
    expect(syncStatusLabel(enabled)).toBe("等待首次同步");
    expect(syncStatusLabel({ ...enabled, phase: "error" })).toContain("本地已保留");
    expect(syncStatusLabel({ ...enabled, phase: "syncing" })).toBe("正在同步…");
    expect(syncStatusLabel({ ...enabled, report: { ...enabled.report, deferred: 1 } })).toContain(
      "等待编辑",
    );
    expect(syncStatusLabel({ ...enabled, report: { ...enabled.report, conflicts: 1 } })).toContain(
      "冲突副本",
    );
  });
  test("explains upload scope, privacy and unsupported attachments", () => {
    const markup = renderToStaticMarkup(
      <SyncSettingsSection initialSettings={DEFAULT_SYNC_SETTINGS} />,
    );
    expect(markup).toContain("当前数据目录中的便签");
    expect(markup).toContain("图片附件不跨设备同步");
    expect(markup).toContain("保存并校验");
    expect(markup).toContain("已删除");
    expect(markup).not.toContain('type="password"');
    expect(markup).toMatch(/disabled=""[^>]*>立即同步/);
  });
  test("shows last success and failure without claiming synced", () => {
    const markup = renderToStaticMarkup(
      <SyncSettingsSection
        initialSettings={{ ...DEFAULT_SYNC_SETTINGS, enabled: true }}
        initialStatus={{ ...EMPTY_SYNC_STATUS, enabled: true, phase: "error", error: "权限不足" }}
      />,
    );
    expect(markup).toContain("权限不足");
    expect(markup).toContain("同步失败");
    expect(markup).not.toContain("飞书已同步");
  });
});
