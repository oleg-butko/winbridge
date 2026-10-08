import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Alert, Badge, Button, Group, Stack, Text } from "@mantine/core";
import type { CompanionStatus } from "../../runtime/contracts";
import { ValidationBanner } from "./ValidationBanner";

export function AttentionPopup() {
  const [status, setStatus] = useState<CompanionStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let disposed = false; let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try { const s = await invoke<CompanionStatus>("companion_status"); if (!disposed) setStatus(s); }
      catch (e) { if (!disposed) setError(String(e)); }
      if (!disposed) timer = setTimeout(() => void poll(), 500);
    }
    void poll(); return () => { disposed = true; clearTimeout(timer); };
  }, []);
  async function action(command: string) { try { await invoke(command); } catch (e) { setError(String(e)); } }
  const record = status?.latestAttention;
  return <Stack p="md" gap="sm">
    <ValidationBanner compact />
    <Group justify="space-between"><Text fw={600}>WinBridge · требуется внимание</Text><Badge color={record?.severity === "error" ? "red" : "yellow"}>{record?.severity ?? "…"}</Badge></Group>
    <Text size="sm">Проект: {record?.projectId ?? "…"} · непрочитано: {status?.pendingCount ?? "неизвестно"}</Text>
    <Text size="sm" style={{ maxHeight: 88, overflowY: "auto", whiteSpace: "pre-wrap" }}>{record?.message ?? "Получаем событие…"}</Text>
    <Group><Button data-testid="popup-open-task" disabled={!record} onClick={() => void action("companion_open_task")}>{record?.target.kind === "workflow" ? "Открыть workflow" : record?.target.kind === "application" ? "Открыть приложение" : "Открыть задачу"}</Button><Button variant="subtle" onClick={() => void action("companion_dismiss")}>Скрыть</Button></Group>
    <Text size="xs" c="dimmed">Подтверждение — отдельно в журнале внимания.</Text>
    {error && <Alert color="red">{error}</Alert>}
  </Stack>;
}
