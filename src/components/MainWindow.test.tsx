// @vitest-environment happy-dom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { MainWindow } from "./MainWindow";
import { showToast } from "./Toast";
import type { Note, SaveNoteRequest } from "../features/notes/types";
import { EMPTY_SYNC_STATUS } from "../features/sync/api";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    label: "main",
    onDragDropEvent: async () => () => {},
    onResized: async () => () => {},
    isMaximized: async () => false,
  }),
}));
vi.mock("./Toast", () => ({ showToast: vi.fn() }));

let container: HTMLDivElement;
let root: Root;
let stored: Note[];
let moveError: Error | null;
let saveGate: Promise<void> | null;
let hit: Element | null;

const card = () => container.querySelector<HTMLElement>('[data-note-id="1"]')!;
const folder = (category: string) =>
  container.querySelector<HTMLElement>(`[data-note-category="${category}"]`)!;
const toggle = () => container.querySelector<HTMLButtonElement>('[aria-controls="notes-sidebar"]')!;
const sidebar = () => container.querySelector<HTMLElement>("#notes-sidebar")!;

function pointer(target: EventTarget, type: string, x: number, overrides: PointerEventInit = {}) {
  target.dispatchEvent(
    new PointerEvent(type, {
      bubbles: true,
      cancelable: true,
      pointerId: 1,
      pointerType: "mouse",
      isPrimary: true,
      button: 0,
      buttons: type === "pointerup" ? 0 : 1,
      clientX: x,
      clientY: 100,
      ...overrides,
    }),
  );
}

async function beginDrag(category: string | null) {
  hit = category === null ? null : folder(category).firstElementChild;
  await act(async () => {
    pointer(card(), "pointerdown", 20);
    pointer(window, "pointermove", 80);
  });
}

async function drop() {
  await act(async () => pointer(window, "pointerup", 80));
}

const moves = () =>
  vi.mocked(invoke).mock.calls.filter(([command]) => command === "notes_move_category");

beforeEach(async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.clearAllMocks();
  vi.useFakeTimers({ toFake: ["requestAnimationFrame", "cancelAnimationFrame"] });
  localStorage.clear();
  stored = [
    {
      id: "1",
      title: "会议笔记",
      content: "尚未归类的正文",
      category: "",
      fileName: "1.md",
      createdAt: "2026-09-24T00:00:00Z",
      updatedAt: "2026-09-24T00:00:00Z",
      wordCount: 8,
    },
  ];
  moveError = null;
  saveGate = null;
  hit = null;
  vi.spyOn(document, "elementFromPoint").mockImplementation(() => hit);
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    const params = args as { id: string; category: string; request: SaveNoteRequest };
    switch (command) {
      case "config_get":
        return { defaultViewMode: "edit", dataDir: "/notes", fontSize: 14, noteAutoSave: false };
      case "notes_list":
        return stored.map((note) => ({ ...note, preview: note.content }));
      case "categories_list":
        return ["工作", "归档"];
      case "notes_get":
        return { ...stored.find((note) => note.id === params.id)! };
      case "notes_move_category": {
        if (moveError) throw moveError;
        const note = stored.find((item) => item.id === params.id)!;
        note.category = params.category;
        return { ...note, preview: note.content };
      }
      case "notes_update": {
        await saveGate;
        const note = stored.find((item) => item.id === params.id)!;
        Object.assign(note, params.request);
        return { ...note };
      }
      case "update_status":
        return { status: "idle", currentVersion: "1.2.0" };
      case "lark_sync_status":
        return EMPTY_SYNC_STATUS;
      default:
        return null;
    }
  });
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await act(async () => root.render(<MainWindow />));
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("note category dragging", () => {
  test("moves into a collapsed empty folder, expands it, and keeps the editor open", async () => {
    expect(folder("工作").querySelector(".expanded")).toBeNull();
    await beginDrag("工作");
    expect(container.querySelector('[role="status"]')?.textContent).toContain("→ 工作");
    expect(folder("工作").firstElementChild?.className).toContain("ring-1");
    await drop();
    expect(moves()).toEqual([["notes_move_category", { id: "1", category: "工作" }]]);
    expect(folder("工作").querySelector(".expanded [data-note-id]")).not.toBeNull();
    expect(container.querySelector("textarea")?.value).toBe("尚未归类的正文");
    expect(container.querySelector('[role="status"]')).toBeNull();
  });

  test("moves between categories and back into an empty Uncategorized group", async () => {
    for (const category of ["工作", "归档", ""]) {
      await beginDrag(category);
      await drop();
      expect(card().closest("[data-note-category]")).toBe(folder(category));
    }
    expect(moves()).toHaveLength(3);
    expect(stored[0].category).toBe("");
  });

  test.each([null, ""])("ignores an invalid or unchanged target: %s", async (category) => {
    await beginDrag(category);
    await drop();
    expect(moves()).toHaveLength(0);
    expect(stored[0].category).toBe("");
  });

  test.each(["Escape", "blur", "pointercancel"])("cancels a drag on %s", async (reason) => {
    await beginDrag("工作");
    await act(async () => {
      window.dispatchEvent(
        reason === "Escape" ? new KeyboardEvent("keydown", { key: "Escape" }) : new Event(reason),
      );
    });
    await drop();
    expect(moves()).toHaveLength(0);
    expect(container.querySelector('[role="status"]')).toBeNull();
    expect(document.body.style.cursor).toBe("");
  });

  test("does not turn clicks, small movements, or right clicks into moves", async () => {
    hit = folder("工作");
    await act(async () => {
      pointer(card(), "pointerdown", 20);
      pointer(window, "pointermove", 23);
      pointer(window, "pointerup", 23);
      pointer(card(), "pointerdown", 20, { button: 2, buttons: 2 });
      pointer(window, "pointermove", 80, { buttons: 2 });
      pointer(window, "pointerup", 80, { button: 2 });
    });
    expect(moves()).toHaveLength(0);
  });

  test("suppresses the release click so dropping does not toggle a category", async () => {
    const onClick = vi.fn();
    folder("工作").addEventListener("click", onClick);
    await beginDrag("工作");
    await act(async () => {
      pointer(window, "pointerup", 80);
      folder("工作").firstElementChild!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect(onClick).not.toHaveBeenCalled();
  });

  test("shows move failures and leaves the note in its original category", async () => {
    moveError = new Error("无法移动笔记");
    await beginDrag("工作");
    await drop();
    expect(card().closest("[data-note-category]")).toBe(folder(""));
    expect(showToast).toHaveBeenCalledWith("无法移动笔记");
    expect(document.body.style.userSelect).toBe("");
  });

  test("waits for an in-flight save and uses the new category for subsequent saves", async () => {
    let resolveSave!: () => void;
    saveGate = new Promise<void>((resolve) => {
      resolveSave = resolve;
    });
    const save = () =>
      document.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "s",
          code: "KeyS",
          ctrlKey: true,
          bubbles: true,
          cancelable: true,
        }),
      );
    await act(async () => {
      save();
    });
    expect(vi.mocked(invoke).mock.calls.some(([cmd]) => cmd === "notes_update")).toBe(true);
    await beginDrag("工作");
    await drop();
    expect(moves()).toHaveLength(0);
    await act(async () => {
      resolveSave();
    });
    expect(stored[0].category).toBe("工作");
    await act(async () => {
      save();
    });
    const saves = vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "notes_update");
    expect(saves[saves.length - 1]?.[1]).toMatchObject({ request: { category: "工作" } });
    expect(stored[0].content).toBe("尚未归类的正文");
  });
});

describe("sidebar visibility", () => {
  test("collapses, persists across remount, and expands from the title bar", async () => {
    await act(async () => toggle().click());
    expect(sidebar().style.width).toBe("0px");
    expect(sidebar().hasAttribute("inert")).toBe(true);
    expect(toggle().getAttribute("aria-expanded")).toBe("false");
    expect(localStorage.getItem("sidebar-collapsed")).toBe("true");
    await act(async () => root.unmount());
    root = createRoot(container);
    await act(async () => root.render(<MainWindow />));
    expect(sidebar().style.width).toBe("0px");
    await act(async () => toggle().click());
    expect(sidebar().style.width).toBe("280px");
    expect(sidebar().hasAttribute("inert")).toBe(false);
    expect(localStorage.getItem("sidebar-collapsed")).toBe("false");
  });

  test("search shortcut reopens the sidebar and focuses search", async () => {
    await act(async () => toggle().click());
    await act(async () => {
      document.dispatchEvent(
        new KeyboardEvent("keydown", {
          key: "f",
          code: "KeyF",
          ctrlKey: true,
          bubbles: true,
          cancelable: true,
        }),
      );
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(20);
    });
    expect(toggle().getAttribute("aria-expanded")).toBe("true");
    expect(document.activeElement).toBe(container.querySelector('[placeholder="搜索笔记…"]'));
  });
});
