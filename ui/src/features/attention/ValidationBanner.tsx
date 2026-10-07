import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Alert, Button, Stack, Text } from "@mantine/core";
import type { DesktopValidationStatus } from "../../runtime/contracts";

export function ValidationBanner({ compact = false }: { compact?: boolean }) {
  const [status, setStatus] = useState<DesktopValidationStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const next = await invoke<DesktopValidationStatus>("desktop_validation_status");
        if (!disposed) setStatus(next);
        if (next.enabled && !disposed) timer = setTimeout(() => void poll(), 500);
      } catch (e) { if (!disposed) setError(String(e)); }
    }
    void poll();
    return () => { disposed = true; clearTimeout(timer); };
  }, []);
  if (!status?.enabled) return null;
  return <Alert data-testid="validation-banner" color={status.error ? "red" : "yellow"} title="Автоматическая проверка · отдельный тестовый runtime" style={{ position: "sticky", top: 0, zIndex: 100 }}>
    <Stack gap="xs">
      <Text size="sm">{status.phase}: {status.step}</Text>
      {!compact && <>
        <Text size="sm">Не закрывайте и не меняйте это окно до завершения. Сценарий сам скрывает и открывает окна, проверяет popup и звук. Рабочий worker должен оставаться запущенным.</Text>
        <Button size="xs" color="red" variant="outline" onClick={() => void invoke("abort_desktop_validation").catch((e) => setError(String(e)))}>Прервать проверку</Button>
      </>}
      {(status.error || error) && <Text size="sm">{status.error ?? error}</Text>}
    </Stack>
  </Alert>;
}
