import { useEffect, useRef } from "react";
import { shortcutPlatform } from "../settings/shortcutRecorder";
import { matchAppShortcut, type AppShortcut } from "./appShortcuts";

export function useAppShortcuts(
  actions: Partial<Record<AppShortcut, () => void | Promise<unknown>>>,
  onError: (error: unknown) => void,
) {
  const latest = useRef({ actions, onError });
  latest.current = { actions, onError };
  const busy = useRef(false);
  useEffect(() => {
    const handle = (event: KeyboardEvent) => {
      if (
        event.target instanceof Element &&
        event.target.closest('[data-shortcut-recorder="true"]')
      )
        return;
      const action = matchAppShortcut(event, shortcutPlatform());
      const callback = action ? latest.current.actions[action] : undefined;
      if (!callback) return;
      event.preventDefault();
      if (busy.current) return;
      busy.current = true;
      void Promise.resolve()
        .then(callback)
        .catch((error) => latest.current.onError(error))
        .finally(() => {
          busy.current = false;
        });
    };
    document.addEventListener("keydown", handle);
    return () => document.removeEventListener("keydown", handle);
  }, []);
}
