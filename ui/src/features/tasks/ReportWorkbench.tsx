import { useState } from "react";
import { Alert, Button, FileButton, Stack, Text } from "@mantine/core";
import type { TaskReport } from "../../runtime/contracts";
import { TaskDetail, statuses } from "./TaskDetail";
import { OutputBuffer } from "./outputBuffer";

function parseReport(text: string): TaskReport {
  const report = JSON.parse(text) as TaskReport;
  const task = report.task;
  const strings = (values: unknown): values is string[] => Array.isArray(values) && values.every((value) => typeof value === "string");
  const number = (value: unknown): value is number => typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
  const nullableTime = (value: unknown) => value === null || number(value);
  const command = task?.submission?.command;
  if (report?.schemaVersion !== 1 || typeof report.platform !== "string" || typeof report.outputComplete !== "boolean" || !task || typeof task.taskId !== "string" || !Object.hasOwn(statuses, task.status)
    || !Array.isArray(task.events) || !task.events.every((event) => event && Object.hasOwn(statuses, event.status) && typeof event.summary === "string" && number(event.sequence) && number(event.occurredAtMs))
    || !task.resolvedCommand || !strings(task.resolvedCommand.arguments) || typeof task.resolvedCommand.executable !== "string" || typeof task.resolvedCommand.workingDirectory !== "string" || !number(task.resolvedCommand.timeoutMs)
    || !command || !strings(command.arguments) || (command.kind === "direct" ? typeof command.executable !== "string" || typeof command.workingDirectory !== "string" : command.kind !== "recipe" || typeof command.recipeId !== "string")
    || typeof task.submission.projectId !== "string" || typeof task.submission.requestId !== "string" || !number(task.acceptedAtMs) || !nullableTime(task.startedAtMs) || !nullableTime(task.finishedAtMs) || !nullableTime(task.processId)
    || (task.exitCode !== null && !Number.isInteger(task.exitCode)) || (task.reason !== null && typeof task.reason !== "string") || typeof task.cancelRequested !== "boolean" || !number(task.outputBytes) || typeof task.outputTruncated !== "boolean" || !Array.isArray(report.output)) throw new Error("Файл не соответствует формату TaskReport WinBridge");
  let previous = 0;
  for (const chunk of report.output) {
    if (!chunk || !number(chunk.sequence) || chunk.sequence <= previous || !number(chunk.offset) || !["stdout", "stderr"].includes(chunk.stream) || !Array.isArray(chunk.bytes) || !chunk.bytes.every((value) => Number.isInteger(value) && value >= 0 && value <= 255)) throw new Error("Некорректные байты, последовательность или имя потока в отчёте");
    previous = chunk.sequence;
  }
  if (report.outputComplete && report.output.reduce((n, chunk) => n + chunk.bytes.length, 0) !== task.outputBytes) throw new Error("Полный отчёт не содержит заявленное количество сохранённых байтов");
  const buffer = new OutputBuffer(); buffer.append(report.output);
  if (report.outputComplete && buffer.snapshot().hasGaps) throw new Error("Полный отчёт содержит пропуски sequence или byte offsets");
  return report;
}
export function ReportWorkbench() {
  const [report, setReport] = useState<TaskReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  async function load(file: File | null) {
    if (!file) return;
    try { if (file.size > 4 * 1024 * 1024) throw new Error("Отчёт больше 4 MiB; выберите отдельную задачу с небольшим выводом"); setReport(parseReport(await file.text())); setError(null); }
    catch (error) { setError(String(error)); }
  }
  return <Stack>
    <FileButton onChange={(file) => void load(file)} accept="application/json">{(props) => <Button {...props} variant="light">Открыть сохранённый отчёт worker</Button>}</FileButton>
    {error && <Alert color="red">{error}</Alert>}
    {report && <>
      <Alert color="blue" title={`Сохранённый результат · ${report.platform}`}>
        Это отчёт выполнения, выбранный пользователем. Текущее состояние worker не запрашивается; команды из браузера не выполняются.
        {!report.outputComplete && <Text mt="xs">Отчёт содержит часть сохранённого вывода.</Text>}
      </Alert>
      <TaskDetail key={report.task.taskId} task={report.task} chunks={report.output} />
    </>}
  </Stack>;
}
