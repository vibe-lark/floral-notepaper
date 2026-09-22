import { describe, expect, test } from "vitest";
import { matchAppShortcut, shortcutHelp } from "./appShortcuts";

const keyEvent = (key: string, overrides: Partial<KeyboardEvent> = {}) => ({
  key,
  code: "",
  metaKey: true,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  isComposing: false,
  repeat: false,
  defaultPrevented: false,
  ...overrides,
});

describe("platform-safe application shortcuts", () => {
  test.each([
    ["n", "new"],
    ["s", "save"],
    ["f", "search"],
    ["o", "import"],
    [",", "settings"],
    ["1", "edit"],
    ["2", "split"],
    ["3", "preview"],
    ["w", "close"],
  ])("Command+%s triggers %s on Mac", (key, action) => {
    expect(matchAppShortcut(keyEvent(key), "mac")).toBe(action);
  });
  test.each([
    "a",
    "e",
    "k",
    "u",
    "w",
    "c",
    "d",
    "z",
    "s",
    "n",
    "p",
    "f",
    "b",
    "r",
    "t",
    "h",
    "l",
    "v",
  ])("never intercepts Unix Control+%s on Mac", (key) => {
    expect(matchAppShortcut(keyEvent(key, { metaKey: false, ctrlKey: true }), "mac")).toBeNull();
  });
  test("honors exact modifiers and ignores composition, held keys and consumed events", () => {
    expect(matchAppShortcut(keyEvent("R", { shiftKey: true }), "mac")).toBe("sync");
    expect(matchAppShortcut(keyEvent("E", { shiftKey: true }), "mac")).toBe("export");
    expect(matchAppShortcut(keyEvent("N", { shiftKey: true }), "mac")).toBe("quickNote");
    for (const modifiers of [
      { altKey: true },
      { ctrlKey: true },
      { isComposing: true },
      { repeat: true },
      { defaultPrevented: true },
      { shiftKey: true },
    ]) {
      expect(matchAppShortcut(keyEvent("s", modifiers), "mac")).toBeNull();
    }
  });
  test("uses physical code when layout changes and preserves native edit shortcuts", () => {
    expect(matchAppShortcut(keyEvent("ы", { code: "KeyS" }), "mac")).toBe("save");
    for (const key of ["c", "v", "x", "a", "z", "q", "h", "m", " "])
      expect(matchAppShortcut(keyEvent(key), "mac")).toBeNull();
  });
  test("keeps Control conventions on non-Mac platforms", () => {
    expect(matchAppShortcut(keyEvent("s", { ctrlKey: true, metaKey: false }), "windows")).toBe(
      "save",
    );
    expect(matchAppShortcut(keyEvent("s"), "windows")).toBeNull();
  });
  test("help shows Command keys without advertising Unix Control bindings on Mac", () => {
    expect(shortcutHelp("mac")).toHaveLength(12);
    expect(
      shortcutHelp("mac").every((item) => item.keys.startsWith("⌘") && !item.keys.includes("Ctrl")),
    ).toBe(true);
  });
});
