import { useEffect, useState } from "react";
import { Alert, Badge, Button, Card, Code, Group, Progress, Select, Stack, Table, Text, TextInput, Timeline, Title } from "@mantine/core";
import { expectResult, workerCall } from "../../runtime/adapter";
import type { ImportRecord, ImportSubmission, SnapshotPreview, WorkerInfo } from "../../runtime/contracts";
import { statuses } from "../tasks/TaskDetail";

const phases: Record<string, string> = { queued: "Ожидает свободного проекта", validate: "Проверка пакета", copy_payload: "Копирование ZIP на локальный диск", extract: "Распаковка и проверка файлов", verify_local: "Проверка локальных исходников", publish_local: "Публикация поколения", verified: "Готово для сборки" };
export function SnapshotWorkbench({ info, connected }: { info: WorkerInfo; connected: boolean }) {
  const [project, setProject] = useState<string | null>(info.projects[0]?.id ?? null);
  const [snapshot, setSnapshot] = useState("");
  const [preview, setPreview] = useState<SnapshotPreview | null>(null);
  const [imports, setImports] = useState<ImportRecord[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [record, setRecord] = useState<ImportRecord | null>(null);
  const [pending, setPending] = useState<ImportSubmission | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pollError, setPollError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirmedAt, setConfirmedAt] = useState<number | null>(null);
  useEffect(() => {
    let disposed = false; let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const list = expectResult(await workerCall({ method: "list_imports" }), "imports").imports;
        const id = selected ?? list[0]?.importId;
        const current = id ? expectResult(await workerCall({ method: "get_import", importId: id }), "import").import : null;
        if (!disposed) { setImports(list); setRecord(current); setConfirmedAt(Date.now()); setPollError(null); }
      } catch (error) { if (!disposed) setPollError(String(error)); }
      if (!disposed) timer = setTimeout(() => void poll(), 750);
    }
    void poll(); return () => { disposed = true; clearTimeout(timer); };
  }, [selected]);
  async function inspect(offset = 0) {
    if (!project || !snapshot) return;
    setBusy(true); setError(null);
    try {
      const next = expectResult(await workerCall({ method: "preview_snapshot", projectId: project, snapshotId: snapshot, offset }), "snapshot_preview").preview;
      setPreview((previous) => offset && previous && previous.manifestSha256 === next.manifestSha256 ? { ...next, files: [...previous.files, ...next.files] } : next);
    } catch (error) { setError(String(error)); }
    finally { setBusy(false); }
  }
  async function send(request: ImportSubmission) {
    setPending(request); setBusy(true); setError(null);
    try {
      const response = await workerCall({ method: "import_snapshot", request });
      if (response.kind === "error") { setPending(null); throw new Error(response.message); }
      const importRecord = expectResult(response, "import").import;
      setSelected(importRecord.importId); setRecord(importRecord); setPending(null);
    } catch (error) { setError(String(error)); }
    finally { setBusy(false); }
  }
  async function cancel() {
    if (!record) return;
    setBusy(true);
    try { setRecord(expectResult(await workerCall({ method: "cancel_import", importId: record.importId }), "import").import); }
    catch (error) { setError(String(error)); }
    finally { setBusy(false); }
  }
  const copying = record?.phase === "copy_payload";
  const progressKnown = record?.totalBytes != null && !copying;
  return <Card withBorder><Stack>
    <Title order={3}>Доставка исходников</Title>
    <Text size="sm">Controller публикует точный Git commit в shared. Worker проверяет пакет и создаёт отдельное поколение на локальном диске Windows.</Text>
    <Select label="Проект доставки" data={info.projects.map((p) => p.id)} value={project} onChange={(id) => { setProject(id); setPreview(null); }} disabled={busy || pending !== null} />
    <TextInput label="Snapshot ID опубликованного пакета" value={snapshot} onChange={(e) => { setSnapshot(e.currentTarget.value); setPreview(null); }} disabled={busy || pending !== null} />
    <Button variant="light" onClick={() => void inspect()} loading={busy} disabled={!connected || !project || !snapshot || pending !== null}>Посмотреть состав snapshot</Button>
    {preview && <>
      <Text size="sm">Commit: {preview.source.commit}</Text>
      <Text size="sm">Debian shared: {preview.peerSharedPath ?? "Укажите delivery.peerRoot в локальном worker JSON"}</Text>
      <Text size="sm">Windows shared: {preview.sharedPath}</Text><Text size="sm">Локальные исходники: {preview.workspacePath}</Text>
      <Text size="sm">{preview.totalFiles} файлов · {preview.totalBytes} байт исходников · ZIP {preview.payloadBytes} байт</Text>
      <Text size="xs">Manifest SHA-256: {preview.manifestSha256}</Text>
      <Alert color="blue">Это предварительный состав. Проверка ZIP и всех файлов выполняется при импорте; сборка запускается отдельной командой.</Alert>
      <Table.ScrollContainer minWidth={450}><Table><Table.Thead><Table.Tr><Table.Th>Файл</Table.Th><Table.Th>Байт</Table.Th><Table.Th>SHA-256</Table.Th></Table.Tr></Table.Thead><Table.Tbody>
        {preview.files.map((file) => <Table.Tr key={file.path}><Table.Td>{file.path}</Table.Td><Table.Td>{file.bytes}</Table.Td><Table.Td><Code>{file.sha256}</Code></Table.Td></Table.Tr>)}
      </Table.Tbody></Table></Table.ScrollContainer>
      {preview.nextOffset !== null && <Button variant="light" onClick={() => void inspect(preview.nextOffset!)} disabled={busy}>Следующие 100 файлов</Button>}
      <Button onClick={() => void send({ protocolVersion: info.protocolVersion, requestId: crypto.randomUUID(), projectId: preview.projectId, snapshotId: preview.snapshotId, manifestSha256: preview.manifestSha256, idempotencyKey: crypto.randomUUID(), timeoutMs: 1800000 })} disabled={!connected || busy || pending !== null}>Импортировать проверенный пакет</Button>
    </>}
    {pending && <Alert color="yellow" title="Приём импорта не подтверждён">Повтор отправит прежние параметры и ключ.
      <Button onClick={() => void send(pending)} loading={busy}>Проверить приём импорта</Button>
    </Alert>}
    {pollError && <Alert color="yellow" title="Нет свежего подтверждения worker">{pollError}</Alert>}
    {error && <Alert color="yellow" title="Нет свежего результата операции">{error}</Alert>}
    <Select label="Последние 10 импортов" value={selected ?? record?.importId ?? null} onChange={setSelected} data={imports.map((r) => ({ value: r.importId, label: `${r.submission.projectId} · ${statuses[r.status].label} · ${r.importId}` }))} />
    {record && <Stack gap="xs">
      <Group><Badge color={statuses[record.status].color}>{statuses[record.status].label}</Badge><Text>{phases[record.phase] ?? record.phase}</Text></Group>
      <Text size="xs">Import ID: {record.importId} · подтверждение: {confirmedAt === null ? "ожидается" : new Date(confirmedAt).toLocaleTimeString()}</Text>
      {!connected && <Text c="yellow">Worker недоступен. Показано последнее подтверждение; импорт не запускается повторно.</Text>}
      <Text>Snapshot: {record.submission.snapshotId} · commit: {record.commit ?? "ещё не проверен"}</Text>
      <Text size="sm">Из: {record.sharedPath}</Text><Text size="sm">В: {record.workspacePath}</Text>
      <Text size="sm">{record.completedFiles} / {record.totalFiles ?? "?"} файлов · {record.completedBytes} байт {copying ? "ZIP" : "исходников"}</Text>
      {progressKnown && <Progress value={record.totalBytes === 0 ? 100 : Math.min(100, record.completedBytes / record.totalBytes! * 100)} />}
      {record.reason && <Alert color={statuses[record.status].color}>{record.reason}</Alert>}
      {record.cancelRequested && !record.finishedAtMs && <Text>Отмена запрошена; ожидается ближайшая проверка worker.</Text>}
      <Button color="orange" variant="light" onClick={() => void cancel()} disabled={!connected || busy || record.cancelRequested || !["queued", "running"].includes(record.status)}>Отменить импорт</Button>
      <Timeline active={record.events.length - 1}>{record.events.map((event) => <Timeline.Item key={event.sequence} title={statuses[event.status].label}><Text size="sm">{event.summary}</Text></Timeline.Item>)}</Timeline>
    </Stack>}
  </Stack></Card>;
}
