import { useEffect, useState } from "react";
import { Accordion, Alert, Button, Card, Checkbox, Code, Group, NumberInput, Select, Stack, Table, Text, Textarea, TextInput, Title } from "@mantine/core";
import type { ApplyOutputCleanup, OutputCleanupPreview, OutputCleanupResult, RuntimeSettings, SaveRuntimeSettings, WorkerInfo } from "../../runtime/contracts";
import { expectResult, workerCall } from "../../runtime/adapter";

type Project = {id:string;workspaceRoot:string;allowDirectCommands:boolean;maxConcurrentMutations:number;recipes:unknown[];recipeProfile?:string|null};
type Config = {instanceId:string;nodeId:string;stateDirectory:string;worker:{controllerUrl?:string|null;keepRunningOnUiExit:boolean};delivery?:unknown;projects:Project[];recipeProfiles?:{id:string;label:string;description:string}[];maxOutputBytesPerTask?:number;outputRetention?:{automatic:boolean;olderThanDays:number}};
function object(value:unknown):value is Record<string,unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
// JSON text can be syntactically valid while its fields are not safe to render
// or edit through the friendly controls. Worker remains the policy authority.
function formConfig(value:unknown):Config|null {
  if (!object(value) || !object(value.worker) ||
      typeof value.worker.keepRunningOnUiExit !== "boolean" ||
      (value.worker.controllerUrl != null && typeof value.worker.controllerUrl !== "string") ||
      !Array.isArray(value.projects) || !value.projects.every((p:unknown)=>object(p) &&
        typeof p.id === "string" && typeof p.workspaceRoot === "string" &&
        typeof p.allowDirectCommands === "boolean" && Array.isArray(p.recipes) &&
        (p.recipeProfile == null || typeof p.recipeProfile === "string")) ||
      (value.recipeProfiles !== undefined && (!Array.isArray(value.recipeProfiles) ||
        !value.recipeProfiles.every((p:unknown)=>object(p) && typeof p.id === "string" && typeof p.label === "string" && typeof p.description === "string"))) ||
      (value.maxOutputBytesPerTask !== undefined && typeof value.maxOutputBytesPerTask !== "number") ||
      (value.outputRetention !== undefined && (!object(value.outputRetention) ||
        typeof value.outputRetention.automatic !== "boolean" || typeof value.outputRetention.olderThanDays !== "number"))) return null;
  return value as Config;
}
export function SettingsPanel({info,connected,closing,onBlocked}:{info:WorkerInfo;connected:boolean;closing:boolean;onBlocked:(blocked:boolean)=>void}) {
  const [settings,setSettings]=useState<RuntimeSettings|null>(null); const [json,setJson]=useState("");
  const [projectIndex,setProjectIndex]=useState("0"); const [error,setError]=useState<string|null>(null);
  const [saved,setSaved]=useState<string|null>(null); const [backup,setBackup]=useState<string|null>(null);
  const [busy,setBusy]=useState(false); const [saveRequest,setSaveRequest]=useState<SaveRuntimeSettings|null>(null);
  const [days,setDays]=useState(30); const [cleanupProject,setCleanupProject]=useState<string|null>(null);
  const [preview,setPreview]=useState<OutputCleanupPreview|null>(null); const [cleanupRequest,setCleanupRequest]=useState<ApplyOutputCleanup|null>(null);
  const [result,setResult]=useState<OutputCleanupResult|null>(null);
  const dirty=!!settings && json!==settings.savedJson;
  useEffect(()=>{onBlocked(dirty||busy);return()=>onBlocked(false);},[dirty,busy,onBlocked]);
  let config:Config|null=null;try{config=formConfig(JSON.parse(json));}catch{/* Raw editor preserves incomplete JSON. */}
  async function load(){
    setBusy(true);setError(null);
    try{const value=expectResult(await workerCall({method:"get_runtime_settings"}),"runtime_settings").settings;setSettings(value);setJson(value.savedJson);setSaveRequest(null);setSaved(null);setBackup(null);setProjectIndex("0");}
    catch(e){setError(String(e));}finally{setBusy(false);}
  }
  useEffect(()=>{if(info.maintenanceAvailable && connected && (!settings||(!dirty&&settings.generation!==info.generation)))void load();},[info.maintenanceAvailable,connected,info.generation]);
  function change(value:string){setJson(value);setSaveRequest(null);setSaved(null);setBackup(null);}
  function update(edit:(value:Config)=>void){if(config){const copy=structuredClone(config);edit(copy);change(JSON.stringify(copy,null,2));}}
  const project=config?.projects?.[Number(projectIndex)];
  async function save(){
    if(!settings)return;setBusy(true);setError(null);setSaved(null);
    const request=saveRequest??{generation:settings.generation,requestId:crypto.randomUUID(),expectedSha256:settings.savedSha256,json};setSaveRequest(request);
    try{const value=expectResult(await workerCall({method:"save_runtime_settings",request}),"settings_saved").saved;setSettings(value.settings);setJson(value.settings.savedJson);setSaveRequest(null);setBackup(value.backupPath);setSaved(value.settings.restartRequired?"Сохранено и перечитано. Действующая конфигурация изменится после ручного перезапуска worker.":"Сохранено и перечитано. Действующая конфигурация совпадает с файлом.");}
    catch(e){setError(`Сохранение не подтверждено: ${String(e)}. Повтор использует тот же ID и показанный JSON.`);}finally{setBusy(false);}
  }
  async function prepareCleanup(){
    setBusy(true);setError(null);setPreview(null);setResult(null);setCleanupRequest(null);
    try{const value=expectResult(await workerCall({method:"preview_output_cleanup",query:{olderThanDays:days,projectId:cleanupProject}}),"output_cleanup_preview").preview;setPreview(value);setCleanupRequest({idempotencyKey:crypto.randomUUID(),plan:value.plan});}
    catch(e){setError(String(e));}finally{setBusy(false);}
  }
  async function cleanup(){
    if(!cleanupRequest)return;setBusy(true);setError(null);
    try{const value=expectResult(await workerCall({method:"apply_output_cleanup",request:cleanupRequest}),"output_cleanup_result").result;setResult(value);}
    catch(e){setError(`Очистка не подтверждена: ${String(e)}. Повтор сверяет тот же план и ключ; новый предпросмотр получает новый ключ.`);}finally{setBusy(false);}
  }
  const disabled=!connected||busy||closing;
  return <Card id="native-settings" data-settings-generation={settings?.generation} data-settings-busy={busy} tabIndex={-1} withBorder><Stack>
    <Title order={3}>Проекты, соединение и хранение</Title>
    {!info.maintenanceAvailable?<Alert color="yellow">Настройки требуют общего обновления worker и desktop.</Alert>:<>
      <Text size="sm">Worker продолжает работать с действующими настройками до ручного перезапуска. Эта форма не запускает и не останавливает процессы.</Text>
      {info.maintenanceError && <Alert color="yellow" title="Автоматическая очистка не подтверждена">{info.maintenanceError}</Alert>}
      <Group><Button variant="light" data-testid="settings-reload" disabled={disabled||dirty} onClick={()=>void load()}>Перечитать файл</Button><Button variant="subtle" disabled={busy||!dirty} onClick={()=>{change(settings?.savedJson??"");}}>Отменить несохранённые изменения</Button><Text role="status" size="sm">{dirty?"Есть несохранённые изменения":settings?"Форма совпадает с сохранённым файлом":"Настройки ещё не прочитаны"}</Text></Group>
      {settings && <>
        <Text size="xs">Конфиг запущенного worker: {settings.path} · SHA-256 {settings.savedSha256}</Text>
        {settings.restartRequired && <Alert color="yellow" data-testid="settings-restart-required">В файле есть изменения, которые ещё не применены. Перед ручным перезапуском проверьте активные задачи и приложения: остановка worker завершит управляемые процессы.</Alert>}
        <Text size="xs">Instance: {info.instanceId} · node: {info.nodeId}. Эти поля и stateDirectory закреплены за установкой.</Text>
        {settings.generation!==info.generation && <Alert color="yellow">Worker перезапущен. Несохранённая правка оставлена в форме; отмените её и перечитайте файл перед новым сохранением.</Alert>}
        {!config && <Alert color="yellow" data-testid="settings-invalid-form">JSON неполон или поля имеют неверные типы. Исправьте текст ниже; ваша правка сохранена в форме. Worker проверит полный конфиг перед записью.</Alert>}
        {config && <>
          <TextInput label="Адрес controller после перезапуска" data-testid="settings-controller-url" value={config.worker?.controllerUrl??""} disabled={disabled} description="Локальный ws://IPv4:port. Пустое поле отключает связь с Debian; локальное выполнение остаётся доступным." onChange={(e)=>{const value=e.currentTarget.value;update((c)=>{c.worker.controllerUrl=value||null;});}} />
          <Select label="Настраиваемый проект" value={projectIndex} onChange={(v)=>setProjectIndex(v??"0")} disabled={disabled} data={config.projects.map((p,i)=>({value:String(i),label:p.id||`Новый проект ${i+1}`}))} />
          {project && <>
            <Select label="Общий профиль рецептов" description="Рецепты профиля дополняются локальными; совпадающие ID отклоняются. Применение после ручного перезапуска worker." clearable value={project.recipeProfile??null} data={config.recipeProfiles?.map((p)=>({value:p.id,label:p.label}))??[]} disabled={disabled} onChange={(v)=>update((c)=>{c.projects[Number(projectIndex)].recipeProfile=v;})} />
            <TextInput label="ID проекта" value={project.id} disabled={disabled} onChange={(e)=>{const value=e.currentTarget.value;update((c)=>{c.projects[Number(projectIndex)].id=value;});}} />
            <TextInput label="Локальный Windows корень проекта" value={project.workspaceRoot} disabled={disabled} description="Существующий отдельный каталог на локальном диске; workspace не является sandbox." onChange={(e)=>{const value=e.currentTarget.value;update((c)=>{c.projects[Number(projectIndex)].workspaceRoot=value;});}} />
            <Checkbox label="Разрешить произвольные команды агента в этом проекте" checked={project.allowDirectCommands} disabled={disabled} onChange={(e)=>{const value=e.currentTarget.checked;update((c)=>{c.projects[Number(projectIndex)].allowDirectCommands=value;});}} />
            <Text size="sm">Рецепты: {project.recipes?.length??0}. Их executable, argv, cwd и timeout доступны в JSON ниже.</Text>
          </>}
          <Group><Button variant="light" disabled={disabled||config.projects.length>=32} onClick={()=>{const index=config!.projects.length;update((c)=>{let n=index+1;while(c.projects.some((p)=>p.id===`project-${n}`))n++;c.projects.push({id:`project-${n}`,workspaceRoot:"",allowDirectCommands:false,maxConcurrentMutations:1,recipes:[]});});setProjectIndex(String(index));}}>Добавить проект</Button><Button variant="subtle" color="orange" disabled={disabled||config.projects.length<=1} onClick={()=>{update((c)=>{c.projects.splice(Number(projectIndex),1);});setProjectIndex("0");}}>Убрать проект из конфигурации</Button></Group>
          <NumberInput label="Квота сохранённого stdout/stderr одной задачи, байт" value={config.maxOutputBytesPerTask??16777216} min={4096} max={268435456} allowDecimal={false} disabled={disabled} onChange={(v)=>update((c)=>{c.maxOutputBytesPerTask=Number(v);})} />
          <Checkbox data-testid="settings-automatic-cleanup" label="После перезапуска автоматически очищать старый output" checked={config.outputRetention?.automatic??false} disabled={disabled} onChange={(e)=>{const value=e.currentTarget.checked;update((c)=>{c.outputRetention={automatic:value,olderThanDays:c.outputRetention?.olderThanDays??30};});}} />
          <NumberInput label="Возраст output для автоматической очистки, дней" value={config.outputRetention?.olderThanDays??30} min={1} max={3650} allowDecimal={false} disabled={disabled} onChange={(v)=>update((c)=>{c.outputRetention={automatic:c.outputRetention?.automatic??false,olderThanDays:Number(v)};})} />
        </>}
        <Textarea data-testid="settings-json" label="Полный сохранённый конфиг · редактируемый предпросмотр" description="JSON позволяет изменить recipes, recipeProfiles и delivery mapping. Неверный JSON/пути/лимиты отклоняются до записи. Сохранение создаёт локальную копию прежнего конфига; она также содержит параметры команд." value={json} readOnly={disabled} onChange={(e)=>change(e.currentTarget.value)} autosize minRows={4} maxRows={15} spellCheck={false} />
        <Accordion><Accordion.Item value="active"><Accordion.Control>Действующий конфиг worker · сравнить с предпросмотром</Accordion.Control><Accordion.Panel><Code block>{settings.activeJson}</Code></Accordion.Panel></Accordion.Item></Accordion>
        <Button data-testid="settings-save" loading={busy} disabled={disabled||(!dirty&&!saveRequest)} onClick={()=>void save()}>Проверить и сохранить конфиг</Button>
        {saved && <Alert color="green" data-testid="settings-saved">{saved}{backup && <Text size="xs">Прежняя версия: {backup}</Text>}</Alert>}
        <Text size="sm">Перезапуск: завершите worker через Ctrl+C, затем запустите прежний worker.exe с <Code>--config {JSON.stringify(settings.path)} serve</Code>. Desktop можно оставить открытым; он покажет новое подтверждение и generation после запуска.</Text>
      </>}
      <Title order={4}>Явная очистка старого stdout/stderr</Title>
      <Text size="sm">Только output завершённых задач старше срока, до 50 за раз. Активные/неизвестные задачи и связанные данные активных workflow/приложений или непрочитанного внимания защищены. Команды, результаты, история, ключи запросов, snapshots и отчёты сохраняются. SQLite переиспользует освобождённое место; размер файла может не уменьшиться сразу.</Text>
      <Group align="end"><NumberInput label="Очистить output старше, дней" value={days} min={1} max={3650} allowDecimal={false} disabled={busy} onChange={(v)=>{setDays(Number(v));setPreview(null);setCleanupRequest(null);setResult(null);}} /><Select label="Проект очистки" clearable data={info.projects.map((p)=>p.id)} value={cleanupProject} disabled={busy} onChange={(v)=>{setCleanupProject(v);setPreview(null);setCleanupRequest(null);setResult(null);}} /><Button data-testid="cleanup-preview" disabled={disabled} onClick={()=>void prepareCleanup()}>Показать, что будет очищено</Button></Group>
      {preview && <>
        <Text data-testid="cleanup-summary">Подходит {preview.entries.length} задач · {preview.bytes} байт.{preview.hasMore?" Есть следующие партии — для них потребуется новый предпросмотр.":""}</Text>
        <Table><Table.Thead><Table.Tr><Table.Th>Task ID</Table.Th><Table.Th>Проект</Table.Th><Table.Th>Output, байт</Table.Th></Table.Tr></Table.Thead><Table.Tbody>{preview.entries.map((e)=><Table.Tr key={e.taskId}><Table.Td>{e.taskId}</Table.Td><Table.Td>{e.projectId}</Table.Td><Table.Td>{e.bytes}</Table.Td></Table.Tr>)}</Table.Tbody></Table>
        <Button data-testid="cleanup-apply" color="orange" disabled={disabled||!preview.entries.length||!!result} onClick={()=>void cleanup()}>Удалить output только показанной партии</Button>
      </>}
      {result && <Alert data-testid="cleanup-result" color="green">Очистка записана в журнал: {result.removedTasks} задач · {result.removedBytes} байт. Key: {result.idempotencyKey}</Alert>}
      {error && <Alert color="yellow" role="alert">{error}</Alert>}
    </>}
  </Stack></Card>;
}
