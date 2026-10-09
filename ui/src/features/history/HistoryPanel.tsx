import { useEffect, useState } from "react";
import { useHotkeys } from "@mantine/hooks";
import { invoke } from "@tauri-apps/api/core";
import { Alert, Badge, Button, Card, Checkbox, Group, Modal, Select, Stack, Table, Text, Textarea, TextInput, Title } from "@mantine/core";
import type { DiagnosticPreview, HistoryCursor, HistoryEntry, HistoryKind, HistoryPage, SavedDiagnostic, StorageUsage, WorkerInfo } from "../../runtime/contracts";
import { expectResult, workerCall } from "../../runtime/adapter";

const kinds = [{value:"task",label:"Задачи"},{value:"import",label:"Импорты"},{value:"workflow",label:"Workflow"},{value:"application",label:"Приложения"}];
const states: Record<string,string> = {queued:"В очереди",running:"Выполняется",succeeded:"Успешно",failed:"Ошибка",cancelled:"Отменено",interrupted:"Прервано",indeterminate:"Результат неизвестен",starting:"Запускается",ready:"Готово",stopping:"Останавливается",stopped:"Остановлено",exited:"Завершилось"};
export function HistoryPanel({info,connected,closing,onOpen}: {info:WorkerInfo;connected:boolean;closing:boolean;onOpen:(entry:HistoryEntry)=>void}) {
  const [search,setSearch] = useState(""); const [project,setProject] = useState<string|null>(null);
  const [kind,setKind] = useState<HistoryKind|null>(null); const [status,setStatus] = useState<string|null>(null);
  const [before,setBefore] = useState<HistoryCursor|null>(null); const [page,setPage] = useState<HistoryPage|null>(null);
  const [usage,setUsage] = useState<StorageUsage|null>(null); const [error,setError] = useState<string|null>(null);
  const [revision,setRevision] = useState(0); const [palette,setPalette] = useState(false);
  const [operationId,setOperationId] = useState(""); const [operationKind,setOperationKind] = useState<HistoryKind>("task");
  const [taskId,setTaskId] = useState(""); const [includeCommand,setIncludeCommand] = useState(false); const [includeOutput,setIncludeOutput] = useState(false);
  const [redactions,setRedactions] = useState(""); const [preview,setPreview] = useState<DiagnosticPreview|null>(null); const [text,setText] = useState("");
  const [directory,setDirectory] = useState<string|null>(null); const [saveId,setSaveId] = useState<string|null>(null);
  const [saved,setSaved] = useState<SavedDiagnostic|null>(null); const [diagnosticError,setDiagnosticError] = useState<string|null>(null); const [busy,setBusy] = useState(false);
  useHotkeys([["mod+K",()=>{if(info.historyAvailable) document.getElementById("history-search")?.focus();}],["mod+shift+P",()=>{if(info.historyAvailable) setPalette(true);}]]);
  useEffect(()=>{setBefore(null);setPage(null);setUsage(null);},[info.instanceId,info.nodeId,info.generation]);
  useEffect(()=>{
    if(!info.historyAvailable) return;
    let disposed=false;
    void invoke<string>("diagnostic_directory").then((path)=>{if(!disposed)setDirectory(path);}).catch((e)=>{if(!disposed)setDiagnosticError(String(e));});
    return()=>{disposed=true;};
  },[info.historyAvailable]);
  useEffect(()=>{
    if(!info.historyAvailable || !connected) return;
    let disposed=false; let timer:ReturnType<typeof setTimeout>;
    async function poll(){
      try {
        const result=expectResult(await workerCall({method:"history",query:{projectId:project,kind,status,search,before}}),"history").page;
        const size=expectResult(await workerCall({method:"storage_usage"}),"storage_usage").usage;
        if(!disposed){setPage(result);setUsage(size);setError(null);}
      }catch(e){if(!disposed)setError(String(e));}
      if(!disposed)timer=setTimeout(()=>void poll(),3000);
    }
    timer=setTimeout(()=>void poll(),200);return()=>{disposed=true;clearTimeout(timer);};
  },[info.historyAvailable,info.instanceId,info.nodeId,info.generation,connected,project,kind,status,search,before,revision]);
  function open(entry:HistoryEntry){if(entry.kind==="task")setTaskId(entry.operationId);onOpen(entry);}
  async function prepare(){
    setBusy(true);setDiagnosticError(null);setSaved(null);setPreview(null);setText("");setSaveId(null);
    try {
      const result=expectResult(await workerCall({method:"prepare_task_diagnostic",request:{taskId,includeCommand,includeOutput,redactValues:redactions.split("\n").filter((s)=>s.length>0)}}),"diagnostic").preview;
      setPreview(result);setText(result.json);setSaveId(crypto.randomUUID());
    }catch(e){setDiagnosticError(String(e));}finally{setBusy(false);}
  }
  async function save(){
    setBusy(true);setDiagnosticError(null);setSaved(null);
    try {
      const requestId=saveId ?? crypto.randomUUID();setSaveId(requestId);
      JSON.parse(text);
      const result=await invoke<SavedDiagnostic>("save_diagnostic",{requestId,json:text});setSaved(result);
    }catch(e){setDiagnosticError(`Сохранение не подтверждено: ${String(e)}. Повтор сохраняет тот же ID; изменённый предпросмотр получает новый ID.`);}finally{setBusy(false);}
  }
  return <Card id="native-history" tabIndex={-1} withBorder><Stack>
    <Group justify="space-between"><Title order={3}>История операций</Title><Button variant="light" onClick={()=>setPalette(true)} disabled={!info.historyAvailable}>Быстрые действия · Ctrl+Shift+P</Button></Group>
    {!info.historyAvailable ? <Alert color="yellow">История и отчёты требуют общего обновления worker и desktop.</Alert> : <>
      <Text size="sm">Все записи журнала: задачи, импорты, workflow и приложения. Выбор открывает сохранённую операцию. Поиск: Ctrl+K. Фильтры и просмотр не выполняют команды.</Text>
      <Group align="end">
        <TextInput id="history-search" label="Поиск истории по ID или названию" value={search} maxLength={200} onChange={(e)=>{setSearch(e.currentTarget.value);setBefore(null);setPage(null);}} />
        <Select label="Проект истории" clearable data={info.projects.map((p)=>p.id)} value={project} onChange={(v)=>{setProject(v);setBefore(null);setPage(null);}} />
        <Select label="Тип истории" clearable data={kinds} value={kind} onChange={(v)=>{setKind(v as HistoryKind|null);setBefore(null);setPage(null);}} />
        <Select label="Статус истории" clearable data={Object.entries(states).map(([value,label])=>({value,label}))} value={status} onChange={(v)=>{setStatus(v);setBefore(null);setPage(null);}} />
      </Group>
      {(!connected||error) && <Alert color="yellow" title="Нет свежей истории">{error??"Worker недоступен; показаны последние прочитанные записи."}</Alert>}
      <Table.ScrollContainer minWidth={680}><Table><Table.Thead><Table.Tr><Table.Th>Операция</Table.Th><Table.Th>Проект</Table.Th><Table.Th>Состояние</Table.Th><Table.Th>Создана</Table.Th><Table.Th>Действие</Table.Th></Table.Tr></Table.Thead><Table.Tbody>
        {page?.records.map((entry)=><Table.Tr key={`${entry.kind}:${entry.operationId}`} data-history-id={entry.operationId}>
          <Table.Td><Text size="sm">{kinds.find((k)=>k.value===entry.kind)?.label} · {entry.label}</Text><Text size="xs" c="dimmed">{entry.operationId}</Text></Table.Td><Table.Td>{entry.projectId}</Table.Td><Table.Td><Badge>{states[entry.status]??entry.status}</Badge></Table.Td><Table.Td>{new Date(entry.createdAtMs).toLocaleString()}</Table.Td><Table.Td><Button size="xs" variant="light" onClick={()=>open(entry)} data-testid={`history-open-${entry.operationId}`}>Открыть</Button></Table.Td>
        </Table.Tr>)}
      </Table.Tbody></Table></Table.ScrollContainer>
      {page && !page.records.length && <Text>Записи по этим фильтрам не найдены.</Text>}
      <Group><Button variant="subtle" onClick={()=>{setBefore(null);setRevision((v)=>v+1);}} disabled={!connected}>Свежие записи</Button><Button data-testid="history-older" variant="light" disabled={!connected||!page?.hasMore} onClick={()=>{if(page){setBefore(page.nextCursor);setPage(null);}}}>Более ранние 50</Button>{page && <Text size="xs">Прочитано: {new Date(page.observedAtMs).toLocaleTimeString()}. Состояния могут изменяться.</Text>}</Group>
      {usage && <Alert color="blue" title="Размер журнала и квота output">
        Tasks: {usage.taskCount} · imports: {usage.importCount} · workflow: {usage.workflowCount} · приложения: {usage.applicationCount} · attention: {usage.attentionCount} ({usage.pendingAttentionCount} ждут подтверждения)
        <Text size="sm">Output: {usage.storedOutputBytes} байт · SQLite: {usage.journalBytes} байт · WAL: {usage.walBytes} байт. Квота output одной task: {usage.outputLimitBytesPerTask} байт. Эта панель только читает размеры.</Text>
      </Alert>}
      <Title order={4}>Диагностический отчёт задачи</Title><Text size="sm">По умолчанию параметры команды и output исключены. Выберите нужные данные, замените чувствительные значения и проверьте редактируемый JSON. Сохранение не меняет журнал worker.</Text>
      <TextInput label="Task ID для отчёта" data-testid="diagnostic-task-id" value={taskId} onChange={(e)=>setTaskId(e.currentTarget.value)} />
      <Group><Checkbox data-testid="diagnostic-include-command" label="Включить параметры команды" checked={includeCommand} onChange={(e)=>setIncludeCommand(e.currentTarget.checked)} /><Checkbox data-testid="diagnostic-include-output" label="Включить stdout/stderr" checked={includeOutput} onChange={(e)=>setIncludeOutput(e.currentTarget.checked)} /></Group>
      <Textarea data-testid="diagnostic-redactions" label="Значения для замены — одно на строку" value={redactions} onChange={(e)=>setRedactions(e.currentTarget.value)} minRows={2} />
      <Button data-testid="diagnostic-prepare" onClick={()=>void prepare()} loading={busy} disabled={!connected||!taskId||closing}>Подготовить предпросмотр</Button>
      {preview && <>
        <Text size="sm">Task ID: {preview.taskId} · данные на {new Date(preview.observedAtMs).toLocaleString()} · параметры: {preview.commandIncluded?"включены с маскированием":"исключены"} · output: {preview.outputIncluded?"включён":"исключён"}</Text>
        {preview.outputTruncated && <Alert color="yellow">Output неполон: квота worker, очистка по политике хранения или предел отчёта 64 KiB / 128 chunks. Предпросмотр не меняет журнал и процесс.</Alert>}
        <Textarea label="Редактируемый предпросмотр отчёта" data-testid="diagnostic-preview" value={text} readOnly={busy} onChange={(e)=>{setText(e.currentTarget.value);setSaveId(null);setSaved(null);}} autosize minRows={8} maxRows={20} spellCheck={false} />
        <Text size="xs">Каталог из локального desktop config: {directory??"недоступен"}</Text>
        <Button data-testid="diagnostic-save" onClick={()=>void save()} loading={busy} disabled={!directory||!text||closing}>Сохранить показанный JSON</Button>
      </>}
      {diagnosticError && <Alert color="yellow">{diagnosticError}</Alert>}
      {saved && <Alert color="green" title="Отчёт сохранён и проверен" data-testid="diagnostic-saved" data-diagnostic-path={saved.path} data-diagnostic-sha256={saved.sha256}><Text>{saved.path}</Text><Text size="xs">{saved.bytes} байт · SHA-256: {saved.sha256}</Text></Alert>}
      <Modal opened={palette} onClose={()=>setPalette(false)} title="Быстрые действия">
        <Stack><Button onClick={()=>{setPalette(false);requestAnimationFrame(()=>document.getElementById("history-search")?.focus());}}>Найти операцию в истории</Button><Select label="Тип операции по ID" data={kinds} value={operationKind} onChange={(v)=>setOperationKind(v as HistoryKind)} /><TextInput label="ID операции" value={operationId} onChange={(e)=>setOperationId(e.currentTarget.value)} /><Button disabled={!operationId.trim()} onClick={()=>{open({kind:operationKind,operationId:operationId.trim(),workflowId:null,projectId:"",label:"",status:"",createdAtMs:0,finishedAtMs:null});setPalette(false);}}>Открыть по ID</Button></Stack>
      </Modal>
    </>}
  </Stack></Card>;
}
