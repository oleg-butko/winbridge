import { Text, Timeline } from "@mantine/core";
import type { JournalEvent } from "../../runtime/contracts";

import { statuses } from "./taskStatus";

const phases: Record<string, string> = {
  copy_payload: "Копирование ZIP на локальный диск", verify_cached_payload: "Проверка локальных блоков ZIP",
  assemble_payload: "Сборка ZIP на локальном диске", payload_checkpoint: "Блок сохранён и проверен",
  verify_payload: "Проверка полного ZIP", extract: "Распаковка и проверка файлов",
  reuse_base: "Копирование и проверка файлов из базового snapshot", verify_local: "Проверка локальных исходников",
  publish_local: "Публикация поколения",
};
function summary(value: string) {
  const prefix = "Snapshot phase: ";
  return value.startsWith(prefix) ? phases[value.slice(prefix.length)] ?? value : value;
}
export function JournalTimeline({ events }: { events: JournalEvent[] }) {
  return <Timeline active={events.length - 1}>{events.map((event) => <Timeline.Item key={event.sequence} title={statuses[event.status].label} color={statuses[event.status].color}>
    <Text size="sm">{summary(event.summary)}</Text>
    <Text size="xs" c="dimmed">{new Date(event.occurredAtMs).toLocaleString()} · событие {event.sequence}</Text>
  </Timeline.Item>)}</Timeline>;
}
