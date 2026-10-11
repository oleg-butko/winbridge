import { invoke } from "@tauri-apps/api/core";

export type CloseSource = "titlebar" | "panel" | "tray" | "recovery";
export type ClosePhase = "none" | "settings" | "saves" | "native";
export type DesktopLogStatus = { path: string | null; error: string | null; droppedRecords: number };
type Event = "heartbeat" | "error" | "close.request" | "close.busy" | "close.wait" | "close.saved" | "close.invoke" | "close.done" | "close.failed" | "close.timeout" | "close.force" | "popup.hide" | "popup.open" | "popup.done" | "popup.failed";

export function desktopEvent(event: Event, source: CloseSource | "runtime" | "popup" = "runtime", quit = false, phase: ClosePhase = "none", pendingSaves = 0, elapsedMs = 0) {
  // Only fixed categories/counts, never form values, command arguments or exception text.
  void invoke("desktop_ui_event", { event, source, quit, phase, pendingSaves, elapsedMs }).catch(() => {});
}

export class CloseTimeout extends Error {
  constructor(public readonly phase: "saves" | "native") {
    super(phase === "saves"
      ? "Сохранение форм не подтвердилось за 10 секунд. Ожидание закрытия остановлено; запись может ещё выполняться. Можно повторить попытку или явно закрыть без сохранения. Worker продолжает работу."
      : "Закрытие интерфейса не подтвердилось за 10 секунд. Запрос мог быть принят; проверьте окно и журнал desktop. Worker продолжает работу.");
  }
}

export async function closeDeadline<T>(promise: Promise<T>, phase: "saves" | "native", timeoutMs = 10000): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([promise, new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new CloseTimeout(phase)), timeoutMs);
    })]);
  } finally { clearTimeout(timer); }
}
