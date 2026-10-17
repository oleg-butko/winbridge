import { Alert, Badge, Group, Stack, Text } from "@mantine/core";
import type { ResourceKind, WorkerInfo } from "../../runtime/contracts";
import { interpretResourceScheduling } from "../../runtime/discovery";
const kinds: Record<ResourceKind, string> = { build: "Сборка", cache: "Кэш", binary: "Бинарник", ipc: "IPC", port: "Порт", interactive: "Интерфейс" };
export function ResourceSchedulingSummary({ info, connected }: { info: WorkerInfo; connected: boolean }) {
  const view = interpretResourceScheduling(info);
  if (!view) return <Text size="sm">Общие ресурсы не подтверждены этим worker.</Text>;
  return <Alert data-testid="resource-scheduling" title="Общие ресурсы проектов" color={!connected || view.resources.some((r) => r.uncertainUnits) ? "yellow" : "blue"}><Stack gap="xs">
    <Badge>{view.enabled ? "Политика общих ресурсов включена" : "Политика общих ресурсов не задана"}</Badge>
    <Text size="sm">Запуск получает весь объявленный набор одновременно. Сборка и кэш освобождаются при готовности приложения; бинарник, IPC, порт и интерфейс остаются у физического владельца до завершения.</Text>
    {!view.enabled && <Text size="sm">Действуют прежние ограничения проектов и общего числа работ. Ранее принятые требования сохраняются.</Text>}
    {view.resources.map((r) => <Stack gap={2} key={r.id}>
      <Group><Text fw={600}>{r.id}</Text><Text size="sm">{kinds[r.kind]} · занято {r.usedUnits}/{r.capacity ?? "не задано"} · освобождение не подтверждено: {r.uncertainUnits}</Text></Group>
      {r.capacity === null && <Text size="xs">Ресурс удалён из текущей политики; принятые требования не отменены.</Text>}
      <details><summary>Владельцы {r.id}</summary>{view.owners.filter((o) => o.resourceId === r.id).map((o) => <Text size="xs" key={o.leaseId}>{o.projectId} · {o.operationKind} {o.operationId} · {o.units} · {o.state === "uncertain" ? "физическое освобождение не подтверждено" : o.applicationLifetime ? "работающее приложение" : "исполнитель"} · lease {o.leaseId}</Text>)}</details>
    </Stack>)}
    {view.truncated && <Text fw={600}>Список ресурсов или владельцев сокращён; счётчики показанных ресурсов полные.</Text>}
    <Text size="xs">Это объявленные конфликты WinBridge, а не все процессы и ресурсы ОС. Причина ожидания находится в журнале операции. Изменение политики требует перезапуска worker.</Text>
    <Text size="xs">Политика owner {view.policyGeneration}; срез {new Date(view.observedAtMs).toLocaleString()} · {view.context.instanceId} / {view.context.journalIncarnation}.</Text>
    {!connected && <Text fw={600}>Нет свежей связи: показан последний сохранённый срез ресурсов.</Text>}
    {view.readOnly && <Text fw={600}>Журнал только для чтения; владельцы в архиве не подтверждают текущее исполнение.</Text>}
  </Stack></Alert>;
}
