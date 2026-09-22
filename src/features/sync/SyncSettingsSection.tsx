import { useEffect, useState } from "react";
import { getErrorMessage } from "../notes/api";
import {
  connectSync,
  disconnectSync,
  getSyncConnection,
  syncNow,
  syncStatusLabel,
  type SyncConnection,
  type SyncStatus,
} from "./api";
import { useSyncStatus } from "./useSyncStatus";

const buttonClass =
  "h-8 px-3 rounded-lg border border-paper-deep/45 text-[11px] text-ink-soft hover:text-bamboo hover:bg-bamboo-mist/50 disabled:opacity-40 disabled:cursor-not-allowed transition-colors";

export function SyncSettingsSection({
  initialConnection,
  initialStatus,
}: {
  initialConnection?: SyncConnection;
  initialStatus?: SyncStatus;
}) {
  const [saved, setSaved] = useState(initialConnection ?? { url: "", enabled: false });
  const [url, setUrl] = useState(initialConnection?.url ?? "");
  const [loading, setLoading] = useState(!initialConnection);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const status = useSyncStatus(initialStatus);
  useEffect(() => {
    let active = true;
    void getSyncConnection()
      .then((value) => {
        if (active) {
          setSaved(value);
          setUrl(value.url);
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
  const changed = url.trim() !== saved.url;
  const working = busy || loading || status.phase === "syncing";
  async function connect() {
    if (working || !url.trim()) return;
    setBusy(true);
    setMessage("");
    try {
      const value = await connectSync(url.trim());
      setSaved(value);
      setUrl(value.url);
      setMessage("连接已保存，正在首次同步…");
      try {
        setMessage((await syncNow()).message);
      } catch (error) {
        setMessage(`连接已保存；同步未完成：${getErrorMessage(error)}`);
      }
    } catch (error) {
      setMessage(getErrorMessage(error));
    } finally {
      setBusy(false);
    }
  }
  async function run(pause = false) {
    if (working) return;
    setBusy(true);
    setMessage("");
    try {
      if (pause) {
        setSaved(await disconnectSync());
        setMessage("已暂停同步，本地和云端便签均保留。");
      } else {
        setMessage((await syncNow()).message);
      }
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
      <h3 className="text-[13px] font-display text-ink-soft">飞书多维表格同步</h3>
      <p className="text-[11px] leading-relaxed text-ink-faint">
        只需粘贴一个链接。自动识别数据表，使用这台电脑已有的飞书登录。
      </p>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void connect();
        }}
        className="space-y-2"
      >
        <label className="block text-[11px] text-ink-faint" htmlFor="lark-base-link">
          多维表格链接
        </label>
        <input
          id="lark-base-link"
          type="url"
          required
          autoComplete="off"
          spellCheck={false}
          disabled={working}
          value={url}
          onChange={(event) => setUrl(event.target.value)}
          placeholder="https://…/base/…?table=…"
          className="w-full h-9 px-2.5 rounded-lg bg-paper-warm/70 border border-paper-deep/40 text-[12px] text-ink-soft focus:outline-none focus:border-bamboo"
        />
        <p className="text-[10px] leading-relaxed text-ink-faint">
          连接后，当前数据目录中的便签会双向同步到这张表，默认每 60 秒一次。
        </p>
        <button type="submit" className={buttonClass} disabled={working || !url.trim()}>
          {busy ? "处理中…" : "连接并同步"}
        </button>
      </form>
      <div className="flex gap-2">
        <button
          type="button"
          className={buttonClass}
          disabled={working || !saved.enabled || changed}
          onClick={() => void run()}
        >
          {status.phase === "syncing" ? "正在同步…" : "立即同步"}
        </button>
        {saved.enabled && (
          <button
            type="button"
            className={buttonClass}
            disabled={working}
            onClick={() => void run(true)}
          >
            暂停同步
          </button>
        )}
      </div>
      {saved.enabled && !saved.url && (
        <p className="text-[10px] text-ink-faint">
          已保留原有连接，仍可同步；修改连接时只需粘贴链接。
        </p>
      )}
      {changed && (
        <p className="text-[10px] text-ink-faint">链接尚未保存，点击“连接并同步”后生效。</p>
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
        首次使用需在本机安装并登录 Lark
        CLI，程序会自动查找，无需填写路径或凭证。删除请使用表里的“已删除”复选框；图片附件不跨设备同步。
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
      onMouseDown={(event) => event.stopPropagation()}
      className="px-2 py-1 rounded-md text-[10px] text-bamboo hover:bg-bamboo-mist/60 max-w-[170px] truncate"
      title={status.error || "打开飞书同步设置"}
    >
      {syncStatusLabel(status)}
    </button>
  );
}
