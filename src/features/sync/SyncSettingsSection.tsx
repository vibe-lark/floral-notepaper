import { useEffect, useState } from "react";
import { getErrorMessage } from "../notes/api";
import {
  DEFAULT_SYNC_SETTINGS,
  getSyncSettings,
  saveSyncSettings,
  syncNow,
  syncStatusLabel,
  type SyncSettings,
  type SyncStatus,
} from "./api";
import { useSyncStatus } from "./useSyncStatus";

const inputClass =
  "w-full h-8 px-2.5 rounded-lg bg-paper-warm/70 border border-paper-deep/40 text-[11px] text-ink-soft focus:outline-none focus:border-bamboo";
const buttonClass =
  "h-8 px-3 rounded-lg border border-paper-deep/45 text-[11px] text-ink-soft hover:text-bamboo hover:bg-bamboo-mist/50 disabled:opacity-40 disabled:cursor-not-allowed transition-colors";

export function SyncSettingsSection({
  initialSettings,
  initialStatus,
}: {
  initialSettings?: SyncSettings;
  initialStatus?: SyncStatus;
}) {
  const [settings, setSettings] = useState(initialSettings ?? DEFAULT_SYNC_SETTINGS);
  const [saved, setSaved] = useState(initialSettings ?? DEFAULT_SYNC_SETTINGS);
  const [loading, setLoading] = useState(!initialSettings);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const status = useSyncStatus(initialStatus);
  useEffect(() => {
    let active = true;
    void getSyncSettings()
      .then((value) => {
        if (active) {
          setSettings(value);
          setSaved(value);
        }
      })
      .catch((error) => {
        if (active) setMessage(getErrorMessage(error));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);
  const changed = JSON.stringify(settings) !== JSON.stringify(saved);
  const working = busy || status.phase === "syncing" || loading;
  const set = <K extends keyof SyncSettings>(key: K, value: SyncSettings[K]) =>
    setSettings((s) => ({ ...s, [key]: value }));

  async function save() {
    setBusy(true);
    setMessage("");
    try {
      const value = await saveSyncSettings(settings);
      setSaved(value);
      setSettings(value);
      setMessage(
        value.enabled
          ? "连接校验通过，设置已保存。点击立即同步开始。"
          : "已暂停同步，本地和云端便签均保留。",
      );
    } catch (error) {
      setMessage(getErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }
  async function run() {
    setBusy(true);
    setMessage("");
    try {
      setMessage((await syncNow()).message);
    } catch (error) {
      setMessage(getErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section
      aria-label="飞书多维表格同步"
      className="space-y-3 rounded-xl border border-bamboo/20 bg-bamboo-mist/20 p-3"
    >
      <div className="flex items-center justify-between">
        <h3 className="text-[13px] font-display text-ink-soft">飞书多维表格同步</h3>
        <span className="text-[10px] text-bamboo">LarkNote</span>
      </div>
      <p className="text-[11px] leading-relaxed text-ink-faint">
        保留花笺的编辑、小窗与磁贴。多维表格保存云端便签，本地保留离线副本。
      </p>
      <fieldset disabled={working} className="space-y-2 disabled:opacity-60">
        <label className="flex items-start gap-2 text-[11px] text-ink-soft leading-relaxed">
          <input
            type="checkbox"
            checked={settings.enabled}
            onChange={(e) => set("enabled", e.target.checked)}
            className="mt-0.5 accent-bamboo"
          />
          启用双向同步，将当前数据目录中的便签同步到指定表
        </label>
        <label className="block text-[11px] text-ink-faint">
          Base token
          <input
            className={inputClass}
            autoComplete="off"
            value={settings.baseToken}
            onChange={(e) => set("baseToken", e.target.value.trim())}
            placeholder="多维表格链接 /base/ 后的 token"
          />
        </label>
        <label className="block text-[11px] text-ink-faint">
          数据表 ID
          <input
            className={inputClass}
            autoComplete="off"
            value={settings.tableId}
            onChange={(e) => set("tableId", e.target.value.trim())}
            placeholder="tbl…"
          />
        </label>
        <details className="text-[11px] text-ink-faint">
          <summary className="cursor-pointer py-1">账号与同步选项</summary>
          <div className="space-y-2 pt-2">
            <label className="block">
              Lark CLI 路径
              <input
                className={inputClass}
                value={settings.cliPath}
                onChange={(e) => set("cliPath", e.target.value)}
              />
            </label>
            <label className="block">
              CLI profile（留空使用当前默认账号）
              <input
                className={inputClass}
                value={settings.profile}
                onChange={(e) => set("profile", e.target.value.trim())}
              />
            </label>
            <label className="block">
              自动同步间隔（秒）
              <input
                type="number"
                min={30}
                max={3600}
                className={inputClass}
                value={settings.intervalSeconds}
                onChange={(e) => set("intervalSeconds", Number(e.target.value))}
              />
            </label>
          </div>
        </details>
      </fieldset>
      <div className="flex gap-2">
        <button
          type="button"
          className={buttonClass}
          disabled={working}
          onClick={() => void save()}
        >
          保存并校验
        </button>
        <button
          type="button"
          className={buttonClass}
          disabled={working || !saved.enabled || changed}
          onClick={() => void run()}
        >
          {status.phase === "syncing" ? "正在同步…" : "立即同步"}
        </button>
      </div>
      {changed && (
        <p className="text-[10px] text-ink-faint">设置尚未保存；立即同步仅使用已保存配置。</p>
      )}
      <div
        role="status"
        aria-live="polite"
        className="space-y-1 text-[11px] leading-relaxed text-ink-faint"
      >
        <p>{syncStatusLabel(status)}</p>
        {status.report.lastSyncedAt && (
          <p>
            上次完成：{new Date(status.report.lastSyncedAt).toLocaleString()}
            <br />
            上传 {status.report.uploaded} · 下载 {status.report.downloaded} · 冲突{" "}
            {status.report.conflicts} · 待处理 {status.report.deferred}
          </p>
        )}
        {(message || status.error) && (
          <p className="text-ink-soft break-words">{message || status.error}</p>
        )}
      </div>
      <p className="text-[10px] leading-relaxed text-ink-faint">
        使用当前飞书用户身份，不保存密码或 access
        token。适合个人多设备同步；表的分享权限由飞书控制。删除请使用“已删除”复选框；空分类、窗口布局、图片附件不跨设备同步。
      </p>
    </section>
  );
}

export function SyncBadge({ onOpenSettings }: { onOpenSettings: () => void }) {
  const status = useSyncStatus();
  return (
    <button
      type="button"
      onClick={onOpenSettings}
      onMouseDown={(e) => e.stopPropagation()}
      className="px-2 py-1 rounded-md text-[10px] text-bamboo hover:bg-bamboo-mist/60 max-w-[170px] truncate"
      title={status.error || "打开飞书同步设置"}
    >
      {syncStatusLabel(status)}
    </button>
  );
}
