import type { ShortcutPlatform } from "../settings/shortcutRecorder";

export type AppShortcut =
  | "new"
  | "save"
  | "search"
  | "import"
  | "export"
  | "settings"
  | "edit"
  | "split"
  | "preview"
  | "sync"
  | "quickNote"
  | "close";

const bindings: Array<{
  action: AppShortcut;
  code: string;
  key: string;
  shift?: boolean;
  label: string;
}> = [
  { action: "new", code: "KeyN", key: "n", label: "新建便签" },
  { action: "save", code: "KeyS", key: "s", label: "保存便签" },
  { action: "search", code: "KeyF", key: "f", label: "搜索便签" },
  { action: "import", code: "KeyO", key: "o", label: "导入 Markdown" },
  { action: "export", code: "KeyE", key: "e", shift: true, label: "导出当前便签" },
  { action: "settings", code: "Comma", key: ",", label: "打开设置" },
  { action: "edit", code: "Digit1", key: "1", label: "编辑模式" },
  { action: "split", code: "Digit2", key: "2", label: "分栏模式" },
  { action: "preview", code: "Digit3", key: "3", label: "预览模式" },
  { action: "sync", code: "KeyR", key: "r", shift: true, label: "立即同步" },
  { action: "quickNote", code: "KeyN", key: "n", shift: true, label: "打开便签小窗" },
  { action: "close", code: "KeyW", key: "w", label: "保存后关闭窗口" },
];

type KeyEvent = Pick<
  KeyboardEvent,
  | "key"
  | "code"
  | "metaKey"
  | "ctrlKey"
  | "altKey"
  | "shiftKey"
  | "isComposing"
  | "repeat"
  | "defaultPrevented"
>;

export function matchAppShortcut(event: KeyEvent, platform: ShortcutPlatform): AppShortcut | null {
  if (event.defaultPrevented || event.isComposing || event.repeat || event.altKey) return null;
  // Never treat Control as Command on macOS: Ctrl+A/E/K/U/W etc. belong to
  // native text editing / Unix conventions, even when this window has focus.
  const primary =
    platform === "mac" ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
  if (!primary) return null;
  return (
    bindings.find(
      (binding) =>
        Boolean(binding.shift) === event.shiftKey &&
        (event.code ? binding.code === event.code : binding.key === event.key.toLowerCase()),
    )?.action ?? null
  );
}

export function shortcutHelp(platform: ShortcutPlatform) {
  return bindings.map(({ action, key, shift, label }) => ({
    action,
    label,
    keys:
      platform === "mac"
        ? `⌘${shift ? "⇧" : ""}${key.toUpperCase()}`
        : `Ctrl+${shift ? "Shift+" : ""}${key.toUpperCase()}`,
  }));
}
