import type { TaskStatus } from "../../runtime/contracts";

export const statuses: Record<TaskStatus, { label: string; color: string }> = {
  queued: { label: "В очереди", color: "gray" }, running: { label: "Выполняется", color: "blue" },
  succeeded: { label: "Завершено", color: "teal" }, failed: { label: "Ошибка", color: "red" },
  cancelled: { label: "Отменено", color: "gray" }, interrupted: { label: "Прервано", color: "orange" },
  indeterminate: { label: "Результат неизвестен", color: "yellow" },
};
