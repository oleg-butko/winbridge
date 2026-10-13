import { useEffect, useRef, useState } from "react";
import { Alert, Badge, Button, Card, Code, Group, Progress, Select, Stack, Table, Text, Title } from "@mantine/core";
import { expectResult, workerCall } from "../../runtime/adapter";
import type { DeliveryDraft, ImportRecord, ImportSubmission, PublishedSnapshotPage, SnapshotPreview, WorkerInfo } from "../../runtime/contracts";
import { JournalTimeline } from "../tasks/JournalTimeline";
import { statuses } from "../tasks/TaskDetail";
import type { WorkflowSnapshot } from "../workflows/WorkflowWorkbench";

const phases: Record<string, string> = { queued: "Ожидает свободного проекта", validate: "Проверка пакета", copy_payload: "Копирование ZIP на локальный диск", extract: "Распаковка и проверка файлов", reuse_base: "Копирование и проверка файлов из базового snapshot", verify_local: "Проверка локальных исходников", publish_local: "Публикация поколения", verified: "Готово для сборки" };
export function SnapshotWorkbench({ navigation, info, connected, closing, drafts, persist, onWorkflowSnapshot }: {
  navigation?: {sequence:number;importId:string}|null;
  info: WorkerInfo; connected: boolean; closing: boolean; drafts: DeliveryDraft[] | null;
  persist: (draft: DeliveryDraft) => Promise<void>;
  onWorkflowSnapshot?: (snapshot: WorkflowSnapshot) => void;
}) {
  const [selection, setSelection] = useState<DeliveryDraft | null>(null);
  const project = selection?.projectId ?? null;
  const snapshot = selection?.snapshotId ?? null;
  const scopeKey = selection ? JSON.stringify([selection.workerInstanceId, project, selection.workspaceRoot]) : null;
  const [page, setPage] = useState<PublishedSnapshotPage | null>(null);
  const pageScope = useRef<string | null>(null);
  const [listError, setListError] = useState<string | null>(null);
  const [listBusy, setListBusy] = useState(false);
  const [refresh, setRefresh] = useState(0);
  const [saveStatus, setSaveStatus] = useState("Выбор пока не сохранён");
  const [saveError, setSaveError] = useState<string | null>(null);
  const saveSequence = useRef(0);
  const [preview, setPreview] = useState<SnapshotPreview | null>(null);
  const [imports, setImports] = useState<ImportRecord[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  useEffect(()=>{if(navigation){setSelected(navigation.importId);setRecord(null);}},[navigation]);
  const [record, setRecord] = useState<ImportRecord | null>(null);
  const [pendingDelta, setPendingDelta] = useState(false);
  const [pending, setPending] = useState<ImportSubmission | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pollError, setPollError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirmedAt, setConfirmedAt] = useState<number | null>(null);
  useEffect(() => {
    if (drafts === null) return;
    const matches = (draft: DeliveryDraft) => draft.workerInstanceId === info.instanceId
      && info.projects.some((p) => p.id === draft.projectId && p.workspaceRoot === draft.workspaceRoot);
    if (selection && matches(selection)) return;
    const saved = [...drafts].reverse().find(matches);
    const first = info.projects[0];
    setSelection(saved ?? (first ? { workerInstanceId: info.instanceId, projectId: first.id, workspaceRoot: first.workspaceRoot, snapshotId: null } : null));
    setPreview(null);
    setSaveStatus(saved ? "Выбор восстановлен из локального файла" : "Выбор пока не сохранён");
  }, [info, drafts, selection]);
  useEffect(() => {
    let disposed = false;
    if (pageScope.current !== scopeKey) { setPage(null); pageScope.current = scopeKey; }
    if (!project || !connected) { setListBusy(false); return; }
    setListBusy(true); setListError(null);
    void workerCall({ method: "list_snapshots", projectId: project, offset: 0 }).then((response) => {
      const next = expectResult(response, "snapshots").page;
      if (!disposed) setPage(next);
    }).catch((error) => { if (!disposed) setListError(String(error)); })
      .finally(() => { if (!disposed) setListBusy(false); });
    return () => { disposed = true; };
  }, [scopeKey, connected, refresh]);
  function choose(projectId: string | null, snapshotId?: string | null) {
    const registered = info.projects.find((p) => p.id === projectId);
    if (!registered) return;
    const saved = drafts?.find((d) => d.workerInstanceId === info.instanceId && d.projectId === projectId && d.workspaceRoot === registered.workspaceRoot);
    const next: DeliveryDraft = { workerInstanceId: info.instanceId, projectId: registered.id, workspaceRoot: registered.workspaceRoot,
      snapshotId: snapshotId === undefined ? saved?.snapshotId ?? null : snapshotId };
    setSelection(next); setPreview(null); setError(null); setSaveError(null); setSaveStatus("Сохранение выбора…");
    const sequence = ++saveSequence.current;
    void persist(next).then(() => { if (sequence === saveSequence.current) setSaveStatus("Выбор сохранён"); })
      .catch((error) => { if (sequence === saveSequence.current) { setSaveError(String(error)); setSaveStatus("Выбор не сохранён"); } });
  }
  async function morePackages() {
    if (!project || page?.nextOffset == null) return;
    setListBusy(true); setListError(null);
    try {
      const next = expectResult(await workerCall({ method: "list_snapshots", projectId: project, offset: page.nextOffset }), "snapshots").page;
      setPage((previous) => previous?.projectId === next.projectId ? { ...next, snapshots: [...new Map([...previous.snapshots, ...next.snapshots].map((s) => [s.snapshotId, s])).values()] } : next);
    } catch (error) { setListError(String(error)); }
    finally { setListBusy(false); }
  }
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
  async function send(request: ImportSubmission, delta = false) {
    setPending(request); setPendingDelta(delta); setBusy(true); setError(null);
    try {
      const response = await workerCall({ method: delta ? "import_delta_snapshot" : "import_snapshot", request });
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
  const copying = ["copy_payload", "verify_cached_payload", "assemble_payload", "payload_checkpoint", "verify_payload"].includes(record?.phase ?? "");
  const progressKnown = record?.totalBytes != null && !copying;
  const visiblePage = pageScope.current === scopeKey ? page : null;
  const packages = visiblePage?.projectId === project ? visiblePage.snapshots : [];
  const packageOptions = packages.map((s) => ({ value: s.snapshotId, label: s.metadataError ? `${s.snapshotId} · ошибка метаданных`
    : `${s.snapshotId} · commit ${s.source?.commit.slice(0, 8)} · ${s.totalFiles} файлов · ZIP ${s.payloadBytes?.toLocaleString()} байт${s.delta ? " · delta" : ""}${s.resumable ? " · resume" : ""}` }));
  if (snapshot && !packages.some((s) => s.snapshotId === snapshot)) packageOptions.unshift({ value: snapshot, label: `${snapshot} · сохранённый выбор, нет в загруженном списке` });
  const chosen = packages.find((s) => s.snapshotId === snapshot);
  return <Card id="native-import-detail" tabIndex={-1} withBorder><Stack>
    <Title order={3}>Доставка исходников</Title>
    <Text size="sm">Controller публикует точный Git commit в shared. Worker проверяет пакет и создаёт отдельное поколение на локальном диске Windows.</Text>
    {info.snapshotResumeAvailable && <Text size="sm">Пакеты с блоками поддерживают возобновление: новый явный импорт проверит локальные блоки и получит недостающие из shared. Прежний ключ возвращает прежний Import ID. Сборка запускается отдельно.</Text>}
    <Select label="Проект доставки" data={info.projects.map((p) => p.id)} value={project} onChange={(id) => choose(id)} disabled={busy || listBusy || pending !== null || closing || drafts === null} allowDeselect={false} />
    <Select label="Snapshot ID опубликованного пакета" description="Список из shared-папки worker. Поиск по ID или commit. Метаданные не подтверждают целостность ZIP." searchable clearable nothingFoundMessage="Пакет не найден в загруженном списке" data={packageOptions} value={snapshot} onChange={(id) => choose(project, id)} disabled={busy || listBusy || pending !== null || closing || !selection} />
    <Group><Button variant="light" onClick={() => setRefresh((r) => r + 1)} loading={listBusy} disabled={!connected || !project || closing}>Обновить список пакетов</Button>
      {visiblePage?.nextOffset != null && <Button variant="light" onClick={() => void morePackages()} disabled={!connected || listBusy || closing}>Следующие 100 пакетов</Button>}</Group>
    {visiblePage && <Text size="xs">Папка пакетов: {visiblePage.sharedPath}</Text>}
    {visiblePage && packages.length === 0 && <Text size="sm">Опубликованных пакетов для этого проекта пока нет.</Text>}
    {!connected && <Text c="yellow">Shared-список не обновлён: worker недоступен. Сохранённый выбор остаётся в форме.</Text>}
    {listError && <Alert color="yellow" title="Не удалось обновить список пакетов">{listError}</Alert>}
    {chosen?.metadataError && <Alert color="yellow" title="Метаданные выбранного пакета недоступны">{chosen.metadataError}</Alert>}
    <Text size="xs" c="dimmed">{saveStatus}. Восстановление выбора не запускает импорт или сборку.</Text>
    {saveError && <Alert color="yellow" title="Не удалось сохранить выбор доставки">{saveError}</Alert>}
    <Button variant="light" onClick={() => void inspect()} loading={busy} disabled={!connected || !project || !snapshot || pending !== null || closing}>Посмотреть состав snapshot</Button>
    {preview && <>
      <Text size="sm">Commit: {preview.source.commit}</Text>
      <Text size="sm">Debian shared: {preview.peerSharedPath ?? "Укажите delivery.peerRoot в локальном worker JSON"}</Text>
      <Text size="sm">Windows shared: {preview.sharedPath}</Text><Text size="sm">Локальные исходники: {preview.workspacePath}</Text>
      <Text size="sm">{preview.totalFiles} файлов · {preview.totalBytes} байт исходников · ZIP {preview.payloadBytes} байт</Text>
      {preview.delta && <Alert color="blue" title="Delta от проверенного snapshot">
        <Text size="sm">База: {preview.delta.snapshotId}</Text>
        <Text size="xs">SHA-256 manifest базы: {preview.delta.manifestSha256}</Text>
        <Text size="sm">{preview.delta.reusedFiles} файлов копируются из локальной базы с проверкой SHA-256; {preview.totalFiles - preview.delta.reusedFiles} файлов передаются в ZIP. Полный состав нового поколения показан ниже.</Text>
        <Text size="sm">Прежнее поколение сохраняется. При отсутствии или повреждении базы нужен явный полный импорт.</Text>
      </Alert>}
      {preview.delta && !info.snapshotDeltaAvailable && <Alert color="yellow">Этот worker не поддерживает delta. Обновите worker или выберите полный пакет.</Alert>}
      {preview.resumable && <Alert color={info.snapshotResumeAvailable ? "blue" : "yellow"} title="Пакет с возобновлением передачи">
        {info.snapshotResumeAvailable ? "Блоки ZIP по 1 МиБ проверяются по хэшам manifest и сохраняются на локальном диске. После обрыва нажмите «Импортировать пакет»: новый импорт использует проверенные блоки. Восстановление интерфейса ничего не запускает." : "Этот worker не поддерживает resume. Обновите worker или выберите прежний полный пакет."}
      </Alert>}
      <Text size="xs">Manifest SHA-256: {preview.manifestSha256}</Text>
      <Alert color="blue">Это предварительный состав. Проверка ZIP и всех файлов выполняется при импорте; сборка запускается отдельной командой.</Alert>
      {onWorkflowSnapshot && <Button variant="light" onClick={() => onWorkflowSnapshot({ projectId: preview.projectId, snapshotId: preview.snapshotId, manifestSha256: preview.manifestSha256 })} disabled={busy || closing}>Использовать в workflow</Button>}
      <Table.ScrollContainer minWidth={450}><Table><Table.Thead><Table.Tr><Table.Th>Файл</Table.Th><Table.Th>Байт</Table.Th><Table.Th>SHA-256</Table.Th></Table.Tr></Table.Thead><Table.Tbody>
        {preview.files.map((file) => <Table.Tr key={file.path}><Table.Td>{file.path}</Table.Td><Table.Td>{file.bytes}</Table.Td><Table.Td><Code>{file.sha256}</Code></Table.Td></Table.Tr>)}
      </Table.Tbody></Table></Table.ScrollContainer>
      {preview.nextOffset !== null && <Button variant="light" onClick={() => void inspect(preview.nextOffset!)} disabled={!connected || busy || closing}>Следующие 100 файлов</Button>}
      <Button onClick={() => void send({ protocolVersion: info.protocolVersion, requestId: crypto.randomUUID(), projectId: preview.projectId, snapshotId: preview.snapshotId, manifestSha256: preview.manifestSha256, idempotencyKey: crypto.randomUUID(), timeoutMs: 1800000 }, !!preview.delta)} disabled={!!preview.resumable && !info.snapshotResumeAvailable || !!preview.delta && !info.snapshotDeltaAvailable || !connected || !!info.journalRecoveryRequired || busy || pending !== null || closing}>Импортировать пакет</Button>
    </>}
    {pending && <Alert color="yellow" title="Приём импорта не подтверждён">Повтор отправит прежние параметры и ключ.
      <Button onClick={() => void send(pending, pendingDelta)} loading={busy} disabled={!connected || !!info.journalRecoveryRequired || closing}>Проверить приём импорта</Button>
    </Alert>}
    {pollError && <Alert color="yellow" title="Нет свежего подтверждения worker">{pollError}</Alert>}
    {error && <Alert color="yellow" title="Нет свежего результата операции">{error}</Alert>}
    <Select label="Последние 10 импортов" value={selected ?? record?.importId ?? null} onChange={setSelected} data={(record&&!imports.some((r)=>r.importId===record.importId)?[record,...imports]:imports).map((r) => ({ value: r.importId, label: `${r.submission.projectId} · ${statuses[r.status].label} · ${r.importId}` }))} />
    {record && <Stack gap="xs" data-confirmed-import-id={record.importId}>
      <Group><Badge color={statuses[record.status].color}>{statuses[record.status].label}</Badge><Text>{({ verify_cached_payload: "Проверка локальных блоков ZIP", assemble_payload: "Сборка ZIP на локальном диске", payload_checkpoint: "Блок сохранён и проверен", verify_payload: "Проверка полного ZIP" } as Record<string, string>)[record.phase] ?? phases[record.phase] ?? record.phase}</Text></Group>
      <Text size="xs">Import ID: {record.importId} · подтверждение: {confirmedAt === null ? "ожидается" : new Date(confirmedAt).toLocaleTimeString()}</Text>
      {!connected && <Text c="yellow">Worker недоступен. Показано последнее подтверждение; импорт не запускается повторно.</Text>}
      {record.delta && <Text size="sm">Delta: база {record.delta.snapshotId} · {record.delta.reusedFiles} файлов / {record.delta.reusedBytes} байт из локальной базы · ZIP {record.delta.payloadBytes} байт. SHA-256 manifest базы: {record.delta.manifestSha256}</Text>}
      {record.transfer && <Text size="sm">Передача ZIP: {record.transfer.payloadBytes} байт · проверено из локального кэша {record.transfer.reusedBytes} байт · получено и проверено из shared {record.transfer.sharedBytes} байт. Размер блока: {record.transfer.chunkBytes} байт.</Text>}
      <Text>Snapshot: {record.submission.snapshotId} · commit: {record.commit ?? "ещё не проверен"}</Text>
      <Text size="sm">Из: {record.sharedPath}</Text><Text size="sm">В: {record.workspacePath}</Text>
      <Text size="sm">{record.completedFiles} / {record.totalFiles ?? "?"} файлов · {record.completedBytes} байт {copying ? "ZIP" : "исходников"}</Text>
      {progressKnown && <Progress value={record.totalBytes === 0 ? 100 : Math.min(100, record.completedBytes / record.totalBytes! * 100)} />}
      {record.reason && <Alert color={statuses[record.status].color}>{record.reason}</Alert>}
      {record.cancelRequested && !record.finishedAtMs && <Text>Отмена запрошена; ожидается ближайшая проверка worker.</Text>}
      <Button color="orange" variant="light" onClick={() => void cancel()} disabled={!connected || !!info.journalInspection || busy || closing || record.cancelRequested || !["queued", "running"].includes(record.status)}>Отменить импорт</Button>
      <JournalTimeline events={record.events} />
    </Stack>}
  </Stack></Card>;
}
