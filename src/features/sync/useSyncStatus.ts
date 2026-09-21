import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { EMPTY_SYNC_STATUS, getSyncStatus, markSyncEditor, type SyncStatus } from "./api";

export function useSyncStatus(initial = EMPTY_SYNC_STATUS) {
  const [status, setStatus] = useState(initial);
  useEffect(() => {
    let active = true;
    let receivedEvent = false;
    const off = listen<SyncStatus>("lark-sync-status", ({ payload }) => {
      receivedEvent = true;
      if (active) setStatus(payload);
    });
    void getSyncStatus()
      .then((value) => {
        if (active && !receivedEvent) setStatus(value);
      })
      .catch(() => undefined);
    return () => {
      active = false;
      void off.then((fn) => fn()).catch(() => undefined);
    };
  }, []);
  return status;
}

/** Protect unsaved buffers, which are not yet represented by local files. */
export function useSyncEditor(noteId: string | null, dirty: boolean) {
  useEffect(() => {
    void markSyncEditor(dirty ? noteId : null).catch(() => undefined);
    return () => {
      void markSyncEditor(null).catch(() => undefined);
    };
  }, [noteId, dirty]);
}
