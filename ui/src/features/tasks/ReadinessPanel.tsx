import { Alert, Badge, Button, Group, Stack, Text } from "@mantine/core";
import type { ReadinessReason, VerificationStatus, WorkerInfo } from "../../runtime/contracts";
import { interpretControllerProjection } from "../../runtime/discovery";

const reason: Record<ReadinessReason, string> = {
  unsupported_contract: "Контракт не поддерживается", missing_identity: "Нет полной информации о версиях и области", invalid_identity: "Информация о версиях или области некорректна",
  missing_evidence: "Нет сохранённой квитанции проверки", invalid_evidence: "Квитанция некорректна", identity_changed: "Версии, область, маршрут или настройки изменились",
  schema_changed: "Изменилась схема контракта", build_unverified: "Эти сборки ещё не проверены", pair_untested: "Эта пара ещё не проверена", route_not_fresh: "Нет свежего подтверждения связи", policy_denied: "Действующие правила проекта блокируют готовность",
};
const status: Record<VerificationStatus, string> = { accepted: "Принята", running: "Выполняется", succeeded: "Пройдена", failed: "Ошибка", indeterminate: "Исход неизвестен; нужна сверка прежнего запроса" };
export function ReadinessPanel({ info, connected, onTask }: { info: WorkerInfo; connected: boolean; onTask: (id: string) => void }) {
  const observation = interpretControllerProjection(info);
  if (observation.kind !== "known") return <Text size="xs">Свежая сводка проверки пар от controller пока не получена. {observation.kind === "unsupported" && "Формат или текущая сессия не подтверждены."}</Text>;
  const projection = observation.projection;
  return <Stack gap="xs" data-testid="contract-readiness">
    {!connected && <Alert color="yellow">Последние известные данные. Текущая готовность не подтверждена.</Alert>}
    <Text size="xs">Проверка совместимости по именованному профилю. Команды по-прежнему проверяются правилами worker.</Text>
    {projection.views.map((view, index) => <Stack gap={3} key={`${view.projectId}-${index}`}>
      <Text size="sm" fw={600}>{view.projectId} · {view.contract.id} v{view.contract.version}</Text>
      <Group gap="xs">{([["Поддержка", view.readiness.supported], ["Проверены сборки", view.readiness.buildVerified], ["Проверена пара", view.readiness.pairTested], ["Готовность профиля", connected && view.readiness.enabled]] as const).map(([label, value]) => <Badge key={label} color={value ? "green" : "gray"} variant="light">{label}: {value ? "да" : "нет"}</Badge>)}</Group>
      <Text size="xs">Профиль: {view.testProfile}. {view.readiness.reasons.map((value) => reason[value]).join("; ") || (connected ? "Свежий маршрут и сохранённые доказательства совпадают." : "Связь с worker недоступна.")}</Text>
      {view.identity && <Text size="xs">Consumer {view.identity.consumer.sourceCommit?.slice(0, 7)} / build {view.identity.consumer.buildId} → controller {projection.controllerBuild.sourceCommit?.slice(0, 7)} → worker {view.identity.provider.sourceCommit?.slice(0, 7)} / build {view.identity.provider.buildId}. Область: {view.identity.context.journalIncarnation}.</Text>}
      {view.receiptId && <Text size="xs" style={{ overflowWrap: "anywhere" }}>Квитанция: {view.receiptId}</Text>}
    </Stack>)}
    {projection.verifications.map((verification) => {
      const sameJournal = info.fencedRequests?.context.instanceId === verification.identity.context.instanceId && info.fencedRequests.context.journalIncarnation === verification.identity.context.journalIncarnation;
      return <details key={verification.verificationKey}><summary>Проверка {verification.verificationKey}: {status[verification.status]}</summary><Text size="xs">{verification.detail}</Text>
        {verification.taskIds.length ? <Group gap="xs">{verification.taskIds.map((id, index) => <Button key={id} size="compact-xs" variant="subtle" disabled={!sameJournal} onClick={() => onTask(id)}>Открыть задачу {index + 1}: {id}</Button>)}</Group> : <Text size="xs">Принятие задач ещё не подтверждено</Text>}
        {!sameJournal && <Text size="xs">Исторические задачи относятся к другой области журнала.</Text>}
      </details>;
    })}
    {projection.truncated && <Text size="xs">Сводка ограничена последними проверками; полный журнал здесь не показан.</Text>}
  </Stack>;
}
