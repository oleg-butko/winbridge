import { useMemo, useState } from "react";
import { Accordion, Alert, Badge, Button, Card, Code, Grid, Group, Stack, Tabs, Text, Title } from "@mantine/core";
import type { OutputChunk, TaskRecord } from "../../runtime/contracts";
import { OutputBuffer } from "./outputBuffer";
import type { OutputState } from "./outputBuffer";
import { JournalTimeline } from "./JournalTimeline";
import { OutputView } from "./OutputView";
import { powerShellScript } from "./commandPresentation";

export { statuses } from "./taskStatus";
import { statuses } from "./taskStatus";
export function TaskDetail({ task, chunks = [], outputState }: { task: TaskRecord; chunks?: OutputChunk[]; outputState?: OutputState }) {
  const [copyState, setCopyState] = useState<string | null>(null);
  const status = statuses[task.status];
  const shell = useMemo(() => powerShellScript(task.resolvedCommand), [task.resolvedCommand]);
  const loadedBytes = chunks.reduce((n, chunk) => n + chunk.bytes.length, 0);
  const finalOutput = !["queued", "running"].includes(task.status) && loadedBytes === task.outputBytes - (task.removedOutputBytes ?? 0);
  const decoded = useMemo(() => { const buffer = new OutputBuffer(); buffer.append(chunks); if (finalOutput) buffer.finish(); return buffer.snapshot(); }, [chunks, finalOutput]);
  const output = outputState ?? decoded;
  async function copy() {
    try { await navigator.clipboard.writeText(JSON.stringify(task.resolvedCommand, null, 2)); setCopyState("Параметры скопированы"); }
    catch { setCopyState("Копирование недоступно; параметры можно выделить ниже"); }
  }
  return <Stack gap="lg">
    <Group justify="space-between"><Title order={2}>Команда в проекте {task.submission.projectId}</Title><Badge color={status.color}>{status.label}</Badge></Group>
    {task.recipeOrigin && <Text data-testid="task-recipe-origin" size="sm">Рецепт: {task.recipeOrigin.recipeId} · {task.recipeOrigin.profileId ? `профиль «${task.recipeOrigin.profileLabel}» (${task.recipeOrigin.profileId})` : "локальная конфигурация проекта"}. Источник сохранён при принятии задачи; параметры ниже взяты из журнала.</Text>}
    <Text size="sm" c="dimmed">Task ID: {task.taskId}</Text>
    <Alert color={status.color} title={task.exitCode === null ? status.label : `Exit code: ${task.exitCode}`}>
      {task.reason ?? (task.status === "queued" ? "Задача сохранена; ожидает свободного проекта." : task.processId === null ? "Worker готовит запуск команды; процесс ещё не подтверждён." : "Worker выполняет команду; результат ещё не подтверждён.")}
      {task.cancelRequested && task.status === "running" && <Text mt="xs">Отмена запрошена; ожидается подтверждение завершения дерева процессов.</Text>}
    </Alert>
    <Grid>
      <Grid.Col span={{ base: 12, md: 4 }}><Card withBorder h="100%"><Stack gap="sm">
        <Text fw={600}>Контекст выполнения</Text>
        <Text size="sm">Источник: {task.submission.command.kind === "recipe" ? `рецепт ${task.submission.command.recipeId}` : "прямая команда"}</Text>
        <Text size="sm">Snapshot: {task.submission.snapshotId ?? "корень проекта"}</Text>
        {task.snapshot && <><Text size="sm">Commit: {task.snapshot.source.commit}</Text><Text size="xs">Manifest SHA-256: {task.snapshot.manifestSha256}</Text></>}
        <Text size="sm">Request ID: {task.submission.requestId}</Text>
        <Text size="sm">Принято: {new Date(task.acceptedAtMs).toLocaleString()}</Text>
        <Text size="sm">Запуск: {task.startedAtMs === null ? "ожидается" : new Date(task.startedAtMs).toLocaleString()}</Text>
        <Text size="sm">Завершение: {task.finishedAtMs === null ? "ожидается" : new Date(task.finishedAtMs).toLocaleString()}</Text>
        <Text size="sm">PID: {task.processId ?? "ещё не создан"} · Timeout: {task.resolvedCommand.timeoutMs / 1000} с</Text>
      </Stack></Card></Grid.Col>
      <Grid.Col span={{ base: 12, md: 8 }}><Card withBorder><Stack gap="sm">
        <Group justify="space-between"><Text fw={600}>Фактические параметры команды</Text><Button size="xs" variant="light" onClick={() => void copy()}>Копировать параметры</Button></Group>
        <Text size="sm">Executable</Text><Code block>{task.resolvedCommand.executable}</Code>
        {shell && ("script" in shell ? <>
          <Text fw={600}>Сценарий PowerShell · декодирован из EncodedCommand</Text>
          <Text size="sm">В параметрах запуска сценарий передаётся PowerShell в Base64 (UTF-16LE), чтобы сохранить Unicode, кавычки и переносы строк. Ниже показан его текст; просмотр не выполняет команду.</Text>
          <Text size="sm">Параметры оболочки · текст сценария раскрыт ниже</Text><Code block>{JSON.stringify(task.resolvedCommand.arguments.map((arg, index) => index === shell.argumentIndex ? "[сценарий ниже]" : arg))}</Code>
          <Code block data-testid="powershell-script" style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere", maxHeight: 480, overflow: "auto" }}>{shell.script}</Code>
        </> : <Alert color="yellow" title="Сценарий PowerShell не декодирован">{shell.error}</Alert>)}
        {shell ? <Accordion variant="contained"><Accordion.Item value="raw-argv">
          <Accordion.Control>Точные аргументы запуска · включая Base64</Accordion.Control>
          <Accordion.Panel><Code block style={{ whiteSpace: "pre-wrap", overflowWrap: "anywhere" }}>{JSON.stringify(task.resolvedCommand.arguments, null, 2)}</Code>
            <Text size="xs">«Копировать параметры» копирует исходные executable, аргументы и cwd из журнала, включая EncodedCommand.</Text>
          </Accordion.Panel>
        </Accordion.Item></Accordion> : <><Text size="sm">Аргументы — отдельные значения</Text><Code block>{JSON.stringify(task.resolvedCommand.arguments)}</Code></>}
        <Text size="sm">Рабочая папка</Text><Code block>{task.resolvedCommand.workingDirectory}</Code>
        {copyState && <Text role="status" size="xs">{copyState}</Text>}
      </Stack></Card></Grid.Col>
    </Grid>
    <Tabs defaultValue="timeline"><Tabs.List><Tabs.Tab value="timeline">Ход выполнения</Tabs.Tab><Tabs.Tab value="output">stdout / stderr</Tabs.Tab></Tabs.List>
      <Tabs.Panel value="timeline" pt="md"><Card withBorder><JournalTimeline events={task.events} /></Card></Tabs.Panel>
      <Tabs.Panel value="output" pt="md"><Stack>
        {task.outputTruncated && <Alert color="yellow">Достигнута квота сохранения вывода. Часть байтов отброшена; worker продолжил читать оба потока.</Alert>}
        {task.outputRemovedAtMs && <Alert color="yellow">Сохранённый stdout/stderr очищен политикой хранения {new Date(task.outputRemovedAtMs).toLocaleString()} · {task.removedOutputBytes} байт. Команда, результат и история сохранены.</Alert>}
        {output.hasGaps && <Alert color="yellow">В загруженном output есть пропуски sequence или byte offsets. Разделённые фрагменты UTF-8 не склеиваются; этот просмотр не подтверждает полный вывод.</Alert>}
        <Text size="sm" c="dimmed" data-testid="output-loaded">Сохранено worker сейчас: {Math.max(0,task.outputBytes-(task.removedOutputBytes??0))} байт · загружено: {output.loadedBytes} байт · UTF-8 · {output.complete ? "чтение завершено" : "чтение продолжается"}</Text>
        {(["stdout", "stderr"] as const).map((stream) => <Card key={stream} withBorder><Text fw={600} mb="xs">{stream}</Text>
          <OutputView key={`${task.taskId}:${task.outputRemovedAtMs ?? 0}`} text={output[stream]} omittedCharacters={output.omittedCharacters[stream]} label={stream} empty="Нет сохранённого вывода" />
        </Card>)}
      </Stack></Tabs.Panel>
    </Tabs>
  </Stack>;
}
