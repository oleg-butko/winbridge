import { Alert, Button, Card, Checkbox, Group, NumberInput, Select, Stack, Text, TextInput, Title } from "@mantine/core";
import type { CompanionStatus, NotificationPreferences } from "../../runtime/contracts";
import type { DesktopLogStatus } from "../../runtime/desktopEvents";

const time = (minute: number) => `${String(Math.floor(minute / 60)).padStart(2, "0")}:${String(minute % 60).padStart(2, "0")}`;
const minute = (value: string) => { const [h, m] = value.split(":").map(Number); return h * 60 + m; };

export function CompanionPanel({ status, preferences: p, closing, error, companionError, preferencesStatus, persist, close, desktopLog }: {
  status: CompanionStatus | null; preferences: NotificationPreferences | null; closing: boolean;
  error: string | null; companionError: string | null; preferencesStatus: string; persist: (p: NotificationPreferences) => void; close: (quit: boolean) => void;
  desktopLog: DesktopLogStatus | null;
}) {
  return <Card id="native-companion" tabIndex={-1} withBorder><Stack>
    <Title order={3}>Tray и уведомления</Title>
    <Text size="sm">Закрытие окна сохраняет формы и скрывает desktop. Worker выполняет принятые задачи независимо от интерфейса. Сигналы читает фоновый Rust-процесс даже при скрытом окне.</Text>
    <Text size="sm">Tray: {status?.trayReady ? "готов" : "ещё не подтверждён"} · непрочитанное внимание: {status?.pendingCount ?? "неизвестно"} · в очереди / выполняются: {status?.activeTaskCount ?? "неизвестно"}</Text>
    <Text size="xs" c="dimmed">Фоновое подтверждение worker: {status?.lastConfirmedAtMs ? new Date(status.lastConfirmedAtMs).toLocaleString() : "ещё не получено"}{status && !status.workerConnected ? " · нет свежего соединения" : ""}</Text>
    {desktopLog?.path && <Text size="xs">Журнал desktop: {desktopLog.path} · пропущено событий: {desktopLog.droppedRecords}</Text>}
    {desktopLog?.error && <Alert color="yellow" title="Журнал desktop недоступен">{desktopLog.error}</Alert>}
    {status?.error && <Alert color="yellow" title="Фоновый companion требует внимания">{status.error}</Alert>}
    {companionError && <Alert color="yellow" title="Нет свежего состояния companion">{companionError}</Alert>}
    {p && <Stack gap="sm">
      <Checkbox label="Показывать popup" checked={p.popupEnabled} disabled={closing} onChange={(e) => persist({ ...p, popupEnabled: e.currentTarget.checked })} />
      <Checkbox label="Встроенный звук Windows" checked={p.soundEnabled} disabled={closing} onChange={(e) => persist({ ...p, soundEnabled: e.currentTarget.checked })} />
      <Group grow>
        <NumberInput label="Громкость сигнала, %" value={p.volumePercent} min={0} max={100} disabled={closing} onChange={(n) => persist({ ...p, volumePercent: Number(n) })} />
        <NumberInput label="Интервал сигналов, секунд" value={p.minimumIntervalSeconds} min={1} max={3600} disabled={closing} onChange={(n) => persist({ ...p, minimumIntervalSeconds: Number(n) })} />
        <Select label="Минимальная важность" value={p.minimumSeverity} data={[{value:"info",label:"Информация"},{value:"warning",label:"Предупреждение"},{value:"error",label:"Ошибка"}]} disabled={closing} onChange={(s) => { if (s) persist({...p, minimumSeverity:s as NotificationPreferences["minimumSeverity"]}); }} />
      </Group>
      <Checkbox label="Тихие часы" checked={p.quietHoursEnabled} disabled={closing} onChange={(e) => persist({ ...p, quietHoursEnabled: e.currentTarget.checked })} />
      <Group grow>
        <TextInput type="time" label="Начало тихих часов" value={time(p.quietStartMinute)} disabled={closing || !p.quietHoursEnabled} onChange={(e) => { if (e.currentTarget.value) persist({...p, quietStartMinute:minute(e.currentTarget.value)}); }} />
        <TextInput type="time" label="Конец тихих часов" value={time(p.quietEndMinute)} disabled={closing || !p.quietHoursEnabled} onChange={(e) => { if (e.currentTarget.value) persist({...p, quietEndMinute:minute(e.currentTarget.value)}); }} />
      </Group>
      <Text size="xs" c="dimmed">Время Windows; переход через полночь поддерживается. Начало и конец должны различаться. Popup объединяет события за интервал; все записи остаются в журнале. Открытие или скрытие popup не отмечает внимание прочитанным.</Text>
      {error && <Alert color="yellow" title="Настройки уведомлений не сохранены">{error}</Alert>}
      <Text size="xs" c="dimmed">{preferencesStatus}</Text>
    </Stack>}
    <Group>
      <Button data-testid="companion-hide" variant="light" disabled={closing || !status?.trayReady} onClick={() => close(false)}>Скрыть в tray</Button>
      <Button data-testid="companion-quit" variant="subtle" disabled={closing} onClick={() => close(true)}>Завершить интерфейс</Button>
    </Group>
  </Stack></Card>;
}
