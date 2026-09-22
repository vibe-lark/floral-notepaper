import { shortcutPlatform } from "../settings/shortcutRecorder";
import { shortcutHelp } from "./appShortcuts";

export function ShortcutHelp() {
  return (
    <details className="text-[11px] text-ink-faint">
      <summary className="cursor-pointer py-1">常用快捷键</summary>
      <dl className="grid grid-cols-[1fr_auto] gap-x-3 gap-y-2 py-3">
        {shortcutHelp(shortcutPlatform()).map(({ action, label, keys }) => (
          <div key={action} className="contents">
            <dt>{label}</dt>
            <dd>
              <kbd className="font-mono text-ink-soft">{keys}</kbd>
            </dd>
          </div>
        ))}
      </dl>
      <p className="leading-relaxed">
        仅在应用内生效。Mac 使用 Command，不接管 Control
        文本编辑键；复制、粘贴、撤销等沿用系统行为。小窗支持新建、保存、打开便签列表和关闭。
      </p>
    </details>
  );
}
