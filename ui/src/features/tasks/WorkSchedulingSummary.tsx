import { Alert, Badge, Group, Stack, Text } from "@mantine/core";
import type { WorkerInfo } from "../../runtime/contracts";
import { interpretWorkScheduling } from "../../runtime/discovery";

export function WorkSchedulingSummary({ info, connected }: { info: WorkerInfo; connected: boolean }) {
  const view = interpretWorkScheduling(info);
  if (!view) return <Text size="sm">Данные об общем лимите работ не подтверждены этим worker.</Text>;
  return <Alert data-testid="work-scheduling" title="Общая нагрузка" color={!connected || view.uncertainSlots ? "yellow" : "blue"}><Stack gap="xs">
    <Group><Badge>{view.maxConcurrentWork === null ? "Общий лимит не включён" : `Работы: ${view.occupiedSlots}/${view.maxConcurrentWork}`}</Badge>
      <Text size="sm">Занято слотов: {view.occupiedSlots}; ожидают исполнения: {view.queuedOperations}; освобождение не подтверждено: {view.uncertainSlots}.</Text></Group>
    <Text size="sm">{view.maxConcurrentWork === null ? "Действуют ограничения проектов." : "Готовые проекты получают очередь по последней выдаче ресурса."} Workflow занимает слот через исполняющийся дочерний шаг. После готовности приложения слот работы освобождается; само приложение продолжает выполняться.</Text>
    <Text size="xs">Ожидание включает команды, импорты и ещё не начатые workflow; запущенный workflow учитывается по дочернему шагу. Это лимит работ WinBridge, а не всех процессов Windows. Причины и владельцы ресурсов доступны в общем журнале. Изменение настройки требует перезапуска worker.</Text>
    <Text size="xs">Срез {new Date(view.observedAtMs).toLocaleString()} · область {view.context.instanceId} / {view.context.journalIncarnation}.</Text>
    {!connected && <Text fw={600}>Нет свежей связи: показан последний сохранённый срез.</Text>}
    {view.readOnly && <Text fw={600}>Журнал открыт для чтения; сохранённые ресурсы не подтверждают текущее исполнение.</Text>}
  </Stack></Alert>;
}
