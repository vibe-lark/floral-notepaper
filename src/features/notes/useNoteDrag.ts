import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import type { NoteMetadata } from "./types";

interface NoteDrag {
  note: NoteMetadata;
  x: number;
  y: number;
  targetCategory: string | null;
}

// Use pointer events: Tauri's native file-drop handler intercepts HTML5 DnD on Windows.
// Keeping it enabled also preserves dropping Markdown files and images into the editor.
export function useNoteDrag(onMove: (noteId: string, category: string) => void) {
  const [drag, setDrag] = useState<NoteDrag | null>(null);
  const onMoveRef = useRef(onMove);
  onMoveRef.current = onMove;
  const pending = useRef<{
    note: NoteMetadata;
    pointerId: number;
    startX: number;
    startY: number;
    x: number;
    y: number;
    active: boolean;
  } | null>(null);

  useEffect(() => {
    let frame = 0;
    let suppressClick = false;
    let clickTimer: ReturnType<typeof setTimeout> | undefined;
    let previousCursor = "";
    let previousUserSelect = "";

    const categoryAt = (x: number, y: number) =>
      document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-note-category]")?.dataset
        .noteCategory ?? null;

    const update = () => {
      const current = pending.current;
      if (!current?.active) return;
      const category = categoryAt(current.x, current.y);
      setDrag({
        note: current.note,
        x: current.x,
        y: current.y,
        targetCategory: category === current.note.category ? null : category,
      });
    };

    const scroll = () => {
      const current = pending.current;
      if (!current?.active) return;
      const list = document
        .elementFromPoint(current.x, current.y)
        ?.closest<HTMLElement>("[data-note-list]");
      if (list) {
        const rect = list.getBoundingClientRect();
        const edge = 36;
        const delta = current.y < rect.top + edge ? -8 : current.y > rect.bottom - edge ? 8 : 0;
        if (delta) {
          list.scrollTop += delta;
          update();
        }
      }
      frame = requestAnimationFrame(scroll);
    };

    const finish = () => {
      if (pending.current?.active) {
        document.body.style.cursor = previousCursor;
        document.body.style.userSelect = previousUserSelect;
        // The browser dispatches click after pointerup; do not select a note or toggle a folder.
        clickTimer = setTimeout(() => {
          suppressClick = false;
        }, 0);
      }
      cancelAnimationFrame(frame);
      pending.current = null;
      setDrag(null);
    };

    const move = (event: PointerEvent) => {
      const current = pending.current;
      if (!current || event.pointerId !== current.pointerId) return;
      if (!(event.buttons & 1)) {
        finish();
        return;
      }
      current.x = event.clientX;
      current.y = event.clientY;
      if (!current.active) {
        if (Math.hypot(current.x - current.startX, current.y - current.startY) < 6) return;
        current.active = true;
        previousCursor = document.body.style.cursor;
        previousUserSelect = document.body.style.userSelect;
        document.body.style.cursor = "grabbing";
        document.body.style.userSelect = "none";
        clearTimeout(clickTimer);
        suppressClick = true;
        frame = requestAnimationFrame(scroll);
      }
      event.preventDefault();
      update();
    };

    const up = (event: PointerEvent) => {
      const current = pending.current;
      if (!current || event.pointerId !== current.pointerId) return;
      const category = current.active ? categoryAt(event.clientX, event.clientY) : null;
      finish();
      if (category !== null && category !== current.note.category) {
        onMoveRef.current(current.note.id, category);
      }
    };
    const cancel = () => finish();
    const keyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && pending.current) {
        event.preventDefault();
        finish();
      }
    };
    const click = (event: MouseEvent) => {
      if (!suppressClick) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      suppressClick = false;
    };
    window.addEventListener("pointermove", move, { passive: false });
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("blur", cancel);
    window.addEventListener("keydown", keyDown);
    window.addEventListener("click", click, true);
    return () => {
      finish();
      clearTimeout(clickTimer);
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("blur", cancel);
      window.removeEventListener("keydown", keyDown);
      window.removeEventListener("click", click, true);
    };
  }, []);

  const startNoteDrag = (event: ReactPointerEvent, note: NoteMetadata) => {
    if (event.button !== 0 || !event.isPrimary || event.pointerType === "touch") return;
    pending.current = {
      note,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      x: event.clientX,
      y: event.clientY,
      active: false,
    };
  };

  return { drag, startNoteDrag };
}
