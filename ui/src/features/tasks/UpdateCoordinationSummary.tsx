import { Alert, Badge, Stack, Text } from "@mantine/core";
import type { WorkerInfo, WorkerUpdatePhase } from "../../runtime/contracts";
import { interpretUpdateCoordination } from "../../runtime/updateCoordination";
const phases: Record<WorkerUpdatePhase, [string, string]> = {
  source_pending: ["Ожидание исходников", "Прежние задачи связанных проектов завершаются. Новые запросы этих проектов временно не принимаются."],
  source_held: ["Исходники заняты обновлением", "Связанные проекты ждут освобождения исходников. Остальные проекты продолжают работать."],
  draining: ["Ожидание завершения работы", "Новые запросы временно не принимаются. Принятые задачи продолжаются; приложения нужно завершить явно."],
  quiescing: ["Проверка остановки исполнителей", "Новые запуски и изменения журнала заблокированы. Проверяется фактическое завершение процессов."],
  quiesced: ["Worker готов к передаче управления", "Запуски заблокированы, завершение исполнителей подтверждено. Установка новой версии ещё не подтверждена."],
  released: ["Блокировка обновления снята", "Пользовательская пауза очереди сохранена. Этот результат не подтверждает установку новой версии."],
};
export function UpdateCoordinationSummary({ info, connected }: { info: WorkerInfo; connected: boolean }) {
  const view = interpretUpdateCoordination(info);
  if (view.kind === "missing") return null;
  if (view.kind === "unsupported") return <Alert data-testid="update-coordination" color="yellow" title="Состояние обновления не распознано">Версия или область данных не совпадает. Снятие блокировки и успешная установка не подтверждены.</Alert>;
  const { lease, readOnly, observedAtMs } = view;
  const [title, explanation] = phases[lease.phase];
  return <Alert data-testid="update-coordination" title="Обновление WinBridge" color={connected && !readOnly ? "blue" : "yellow"}><Stack gap="xs">
    <Badge>{title}</Badge><Text>{explanation}</Text>
    <Text size="sm">{lease.reason}</Text>
    <Text size="sm">Исходники: {lease.spec.sourceDirectory}</Text>
    <Text size="sm">Связанные проекты: {lease.spec.projects.join(", ") || "нет зарегистрированных проектов в этой папке"}</Text>
    <Text size="xs">Операция {lease.spec.updateId} · версия состояния {lease.revision} · срез {new Date(observedAtMs).toLocaleString()}</Text>
    {!connected && <Text fw={600}>Нет свежей связи: показано последнее известное состояние обновления. Завершение не подтверждено.</Text>}
    {readOnly && <Text fw={600}>Журнал только для чтения. Это сохранённая блокировка, а не подтверждение действующего обновления.</Text>}
  </Stack></Alert>;
}
