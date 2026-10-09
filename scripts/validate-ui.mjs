// Integration test: browser transport shim -> real local worker, never fixture task data.
import { createHash } from "node:crypto";
import { promisify } from "node:util";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, writeFile, rm } from "node:fs/promises";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawn, execFile } from "node:child_process";
import { createConnection, createServer as createTcpServer } from "node:net";
import { createServer } from "vite";
import { chromium } from "playwright";

const root = resolve(import.meta.dirname, "..");
const run = await mkdtemp(join(tmpdir(), "winbridge-ui-"));
const project = join(run, "project with spaces");
const config = join(run, "worker.local.json");
const socket = join(run, "state/worker-ui-test.sock");
const workerExe = join(root, "target/debug/winbridge-worker");
const controllerExe = join(root, "target/debug/winbridge-controller");
const exec = promisify(execFile);
const shared = join(run, "shared");
const source = join(run, "snapshot-source");
await mkdir(shared); await mkdir(source);
await writeFile(join(source, "Привет.txt"), "UI snapshot ✓");
await mkdir(join(source, "src"));
for (const name of ["Cargo.toml", "Cargo.lock", "src/main.rs"]) {
  await writeFile(join(source, name), await readFile(join(root, "fixtures/snapshot-project", name)));
}
for (const args of [["init", "-q"], ["add", "."], ["-c", "user.name=Fixture", "-c", "user.email=fixture@local", "commit", "-qm", "UI snapshot"]]) await exec("git", ["-C", source, ...args]);
const controllerConfig = join(run, "controller.local.json");
const portProbe = createTcpServer();
await new Promise((resolve) => portProbe.listen(0, "127.0.0.1", resolve));
const controllerPort = portProbe.address().port;
await new Promise((resolve) => portProbe.close(resolve));
await writeFile(controllerConfig, JSON.stringify({ schemaVersion: 1, instanceId: "ui-test", nodeId: "debian-ui-test", stateDirectory: join(run, "controller"),
  controller: { listenAddress: `127.0.0.1:${controllerPort}`, workerNodeId: "linux-ui-test" }, delivery: {adapter:"shared-folder", root:shared, namespace:"WinBridge"}, projects:[{id:"fixture",sourceRoot:source,sourceMode:"commit"}] }));
const fixture = join(root, "target/debug/winbridge-fixture");
const preferencesPath = join(run, "winbridge-desktop-state.local.json");
const otherProject = join(run, "second project");
await mkdir(project);
await mkdir(otherProject);
await mkdir(join(project, "nested"));
await writeFile(config, JSON.stringify({ schemaVersion: 1, instanceId: "ui-test", nodeId: "linux-ui-test", stateDirectory: join(run, "state"),
  delivery: {adapter:"shared-folder",root:shared,namespace:"WinBridge",peerRoot:shared}, worker: { keepRunningOnUiExit: true, controllerUrl: `ws://127.0.0.1:${controllerPort}` }, projects: [{ id: "fixture", workspaceRoot: project, allowDirectCommands: true, maxConcurrentMutations: 1, recipes: [{ id: "direct", executable: fixture, arguments: ["success"], workingDirectory: ".", timeoutMs: 10000 }] },
    { id: "other-fixture", workspaceRoot: otherProject, allowDirectCommands: true, maxConcurrentMutations: 1, recipes: [] }] }));

function call(request, endpoint = socket) {
  return new Promise((resolve, reject) => {
    const stream = createConnection(endpoint);
    let received = Buffer.alloc(0);
    stream.setTimeout(10000, () => stream.destroy(new Error("IPC deadline")));
    stream.on("connect", () => { const bytes = Buffer.from(JSON.stringify(request)); const prefix = Buffer.alloc(4); prefix.writeUInt32LE(bytes.length); stream.write(Buffer.concat([prefix, bytes])); });
    stream.on("data", (bytes) => {
      received = Buffer.concat([received, bytes]);
      if (received.length >= 4 && received.length >= received.readUInt32LE(0) + 4) {
        try { resolve(JSON.parse(received.subarray(4).toString())); } catch (error) { reject(error); }
        stream.end();
      }
    });
    stream.on("error", reject);
    stream.on("end", () => { if (received.length < 4) reject(new Error("IPC ended without response")); });
  });
}
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function until(check, message) {
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) { try { if (await check()) return; } catch {} await sleep(50); }
  throw new Error(message);
}
let worker;
let controller;
let browser;
let server;
let crashedPid;
async function startWorker() {
  worker = spawn(workerExe, ["--config", config, "serve"], { stdio: ["ignore", "ignore", "pipe"] });
  worker.stderr.on("data", () => {});
  await until(async () => (await call({ method: "info" })).kind === "info", "worker unavailable");
}
async function startController() {
  controller = spawn(controllerExe, ["--config", controllerConfig, "serve"], { stdio: ["ignore", "ignore", "pipe"] });
  controller.stderr.on("data", () => {});
}
async function stopController() {
  if (!controller || controller.exitCode !== null || controller.signalCode !== null) return;
  const closed = new Promise((resolve) => controller.once("exit", resolve));
  controller.kill("SIGINT");
  const completed = await Promise.race([closed.then(() => true), sleep(10000).then(() => false)]);
  if (!completed) { controller.kill("SIGKILL"); await closed; throw new Error("controller shutdown deadline exceeded"); }
}
async function latestTask() {
  const summary = (await call({ method: "list_tasks" })).tasks[0];
  return (await call({ method: "get_task", taskId: summary.taskId })).task;
}
async function stopWorker(signal = "SIGINT") {
  if (!worker || worker.exitCode !== null || worker.signalCode !== null) return;
  const closed = new Promise((resolve) => worker.once("exit", resolve));
  worker.kill(signal);
  const completed = await Promise.race([closed.then(() => true), sleep(10000).then(() => false)]);
  if (!completed) { worker.kill("SIGKILL"); await closed; throw new Error("worker shutdown deadline exceeded"); }
}
try {
  await startController();
  await startWorker();
  server = await createServer({ server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  const address = server.httpServer.address();
  const url = `http://127.0.0.1:${address.port}`;
  const executablePath = process.env.WINBRIDGE_BROWSER ?? (existsSync("/usr/bin/chromium") ? "/usr/bin/chromium" : undefined);
  browser = await chromium.launch({ executablePath, chromiumSandbox: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 1000 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  let loseFirstAck = true;
  let loseFirstImportAck = true;
  let loseFirstWorkflowAck = true;
  let loseFirstAttentionAck = true;
  let loseFirstAttentionConfirmation = true;
  let acceptedWorkflowId;
  let importedId;
  let submittedId;
  let draftSaveDelay = 0;
  let failDraftSaves = false;
  let failDeliverySaves = false;
  const diagnosticsDirectory=join(run,"diagnostics");
  await mkdir(diagnosticsDirectory);
  const diagnosticSaves=[];
  let loseDiagnosticSaveAck=false;
  let loseSettingsSaveAck=false;
  let loseCleanupAck=false;
  const deltaImportCalls=[];
  const settingsCalls=[];const cleanupCalls=[];
  async function attachNative(nativePage) {
    await nativePage.exposeFunction("workerBridge", async (request) => {
      const response = await call(request, ["import_delta_snapshot", "submit_workflow", "request_attention", "list_attention", "acknowledge_attention", "list_attention_events", "acknowledge_attention_event", "history", "prepare_task_diagnostic", "storage_usage"].includes(request.method) ? join(run, "controller/controller-ui-test.sock") : socket);
      if(request.method==="save_runtime_settings") {settingsCalls.push(request);if(loseSettingsSaveAck&&response.kind==="settings_saved"){loseSettingsSaveAck=false;throw new Error("Test: settings save ACK lost");}}
      if(request.method==="apply_output_cleanup") {cleanupCalls.push(request);if(loseCleanupAck&&response.kind==="output_cleanup_result"){loseCleanupAck=false;throw new Error("Test: cleanup ACK lost");}}
      if (request.method === "submit" && loseFirstAck) { loseFirstAck = false; submittedId = response.task.taskId; throw new Error("Test: acceptance response was lost"); }
      if(request.method==="import_delta_snapshot") deltaImportCalls.push(request);
      if (["import_snapshot", "import_delta_snapshot"].includes(request.method) && loseFirstImportAck) { loseFirstImportAck = false; importedId = response.import.importId; throw new Error("Test: import ACK lost"); }
      if (request.method === "submit_workflow" && loseFirstWorkflowAck && response.kind === "workflow") { loseFirstWorkflowAck = false; acceptedWorkflowId = response.workflow.workflowId; throw new Error("Test: workflow ACK lost"); }
      if (request.method === "request_attention" && loseFirstAttentionAck && response.kind === "attention") { loseFirstAttentionAck = false; throw new Error("Test: attention acceptance ACK lost"); }
      if (["acknowledge_attention","acknowledge_attention_event"].includes(request.method) && loseFirstAttentionConfirmation && ["attention","attention_event"].includes(response.kind)) { loseFirstAttentionConfirmation = false; throw new Error("Test: attention confirmation ACK lost"); }
      return response;
    });
    // Native filesystem persistence is covered by core tests. Here the IPC shim
    // uses a real file across fresh WebViews to test the form and close lifecycle.
    await nativePage.exposeFunction("diagnosticBridge", async(command,args)=>{
      if(command==="diagnostic_directory")return diagnosticsDirectory;
      assert.equal(command,"save_diagnostic");
      assert.match(args.requestId,/^[a-f0-9-]{36}$/i);
      const parsed=JSON.parse(args.json);
      assert.equal(parsed.kind,"winbridge_task_diagnostic");
      const path=join(diagnosticsDirectory,`diagnostic-${args.requestId}.json`);
      const bytes=Buffer.from(args.json);
      try {await writeFile(path,bytes,{flag:"wx"});}catch(e){if(e.code!=="EEXIST")throw e;}
      const actual=await readFile(path);
      assert.deepEqual(actual,bytes,"Diagnostic save overwrote a previous report");
      diagnosticSaves.push({id:args.requestId,json:args.json,path});
      if(loseDiagnosticSaveAck){loseDiagnosticSaveAck=false;throw new Error("Test: diagnostic file written but save ACK lost");}
      return {path,bytes:actual.length,sha256:createHash("sha256").update(actual).digest("hex")};
    });
    await nativePage.exposeFunction("draftBridge", async (command, args) => {
      const state = existsSync(preferencesPath) ? JSON.parse(await readFile(preferencesPath, "utf8")) : { schemaVersion: 1, drafts: [], deliveryDrafts: [] };
      state.deliveryDrafts ??= [];
      state.workflowProfiles ??= [];
      state.notificationPreferences ??= {popupEnabled:true,soundEnabled:true,volumePercent:25,minimumSeverity:"warning",minimumIntervalSeconds:30,quietHoursEnabled:false,quietStartMinute:1320,quietEndMinute:480};
      if (command === "load_command_drafts") return state;
      assert.ok(["save_command_draft", "save_delivery_draft", "save_workflow_profile", "save_notification_preferences"].includes(command));
      if (draftSaveDelay) await sleep(draftSaveDelay);
      if (failDraftSaves || (command === "save_delivery_draft" && failDeliverySaves)) throw new Error("Test: local state directory is unavailable");
      if (command === "save_notification_preferences") { state.notificationPreferences = args.preferences; await writeFile(preferencesPath, JSON.stringify(state)); return; }
      const draft = args.draft ?? args.profile;
      const field = command === "save_workflow_profile" ? "workflowProfiles" : command === "save_command_draft" ? "drafts" : "deliveryDrafts";
      state[field] = state[field].filter((saved) => saved.workerInstanceId !== draft.workerInstanceId || saved.projectId !== draft.projectId || saved.workspaceRoot !== draft.workspaceRoot || (field === "workflowProfiles" && saved.name !== draft.name));
      state[field].push(draft);
      await writeFile(preferencesPath, JSON.stringify(state));
    });
    await nativePage.addInitScript(() => {
      const callbacks = new Map(); const listeners = new Map(); let nextCallback = 0;
      window.isTauri = true;
      window.__hidden = false;
      window.__destroyed = false;
      window.__companion = {trayReady:true,workerConnected:true,lastConfirmedAtMs:Date.now(),pendingCount:0,activeTaskCount:0,error:null,latestAttention:null,openTaskId:null,openTaskSequence:0,quitRequestSequence:0};
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: (_event, id) => listeners.delete(id) };
      window.__testClose = async () => {
        for (const [id, registration] of listeners) {
          if (registration.event === "tauri://close-requested") await callbacks.get(registration.handler)({ event: registration.event, id, payload: null });
        }
      };
      window.__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: "main" } },
        transformCallback: (callback) => { const id = ++nextCallback; callbacks.set(id, callback); return id; },
        invoke: (command, args) => {
          if (["diagnostic_directory","save_diagnostic"].includes(command)) return window.diagnosticBridge(command,args);
          if (command === "worker_rpc") return window.workerBridge(args.request);
          if (command === "desktop_build") return Promise.resolve({ version: "0.1.0", sourceCommit: "browser-test", sourceDirty: false, sourceManifestSha256: null, buildId: "test-shell" });
          if (["load_command_drafts", "save_command_draft", "save_delivery_draft", "save_workflow_profile", "save_notification_preferences"].includes(command)) return window.draftBridge(command, args);
          if (command === "desktop_validation_status") return Promise.resolve(structuredClone(window.__validation ?? {enabled:false,phase:"",step:"",completed:false,error:null}));
          if (command === "abort_desktop_validation") { window.__aborted=true; return Promise.resolve(); }
          if (command === "companion_status") return Promise.resolve(window.__companion);
          if (command === "companion_close") { if (args.quit) window.__destroyed = true; else { if (!window.__companion.trayReady) return Promise.reject("Tray unavailable"); window.__hidden = true; } return Promise.resolve(); }
          if (command === "plugin:event|listen") { const id = args.handler; listeners.set(id, args); return Promise.resolve(id); }
          if (command === "plugin:event|unlisten") { listeners.delete(args.eventId); return Promise.resolve(); }
          if (command === "plugin:window|destroy") { window.__destroyed = true; return Promise.resolve(); }
          throw new Error("unexpected command: " + command);
        },
      };
    });
  }
  await attachNative(page);
  await page.goto(url);
  await page.getByText("Worker подключён · linux · ui-test", { exact: true }).waitFor();
  await page.getByText("Controller Debian подключён", { exact: true }).waitFor();
  const published = JSON.parse((await exec(controllerExe,["--config",controllerConfig,"publish","fixture","HEAD"])).stdout);
  const discovered = JSON.parse((await exec(controllerExe,["--config",controllerConfig,"snapshots","fixture"])).stdout);
  assert.equal(discovered.kind, "snapshots");
  assert.ok(discovered.page.snapshots.some((s) => s.snapshotId === published.snapshotId && s.metadataError === null), "WebSocket discovery omitted the published commit");
  await page.getByRole("button", {name:"Обновить список пакетов",exact:true}).click();
  const deliverySnapshot = page.getByRole("textbox", {name:"Snapshot ID опубликованного пакета",exact:true});
  await deliverySnapshot.fill(published.snapshotId);
  await page.getByRole("option", {name:new RegExp(`^${published.snapshotId} · commit`)}).click();
  await page.getByText(/Выбор сохранён\. Восстановление/).waitFor();
  await page.getByRole("button",{name:"Посмотреть состав snapshot",exact:true}).click();
  await page.getByRole("cell",{name:"Привет.txt",exact:true}).waitFor();
  await page.getByText(`Debian shared: ${shared}/WinBridge/deliveries/fixture/${published.snapshotId}`,{exact:true}).waitFor();
  await page.getByRole("button",{name:"Импортировать пакет",exact:true}).click();
  await page.getByText("Приём импорта не подтверждён",{exact:true}).waitFor();
  await page.getByRole("button",{name:"Проверить приём импорта",exact:true}).click();
  await page.getByText("Готово для сборки",{exact:true}).waitFor();
  const imported=(await call({method:"get_import",importId:importedId})).import;
  assert.equal(imported.status,"succeeded");
  assert.equal((await call({method:"list_imports"})).imports.length,1);
  assert.equal(await readFile(join(imported.workspacePath,"Привет.txt"),"utf8"),"UI snapshot ✓");
  await page.getByLabel("Snapshot ID для сборки",{exact:true}).fill(published.snapshotId);
  await page.getByText(/Поля сохранены\. Восстановление/).waitFor();
  assert.equal(JSON.parse(await readFile(preferencesPath,"utf8")).drafts.at(-1).snapshotId,published.snapshotId);
  await page.getByLabel("Snapshot ID для сборки",{exact:true}).fill("");
  await page.getByLabel("Executable", { exact: true }).fill(fixture);
  await page.getByLabel("Аргументы — JSON-массив строк", { exact: true }).fill('["success"]');
  await page.getByRole("button", { name: "Передать задачу worker", exact: true }).click();
  await page.getByText("Приём задачи не подтверждён", { exact: true }).waitFor();
  await page.getByRole("button", { name: "Проверить приём — повторить тот же запрос", exact: true }).click();
  await page.getByText("Exit code: 0", { exact: true }).waitFor();
  const record = (await call({ method: "get_task", taskId: submittedId })).task;
  assert.equal(record.status, "succeeded");
  assert.equal((await readFile(join(project, "runs.log"), "utf8")).trim().split("\n").length, 1);
  await page.getByRole("tab", { name: "stdout / stderr", exact: true }).click();
  await page.getByText(/stdout: success Привет ✓/).waitFor();
  await page.getByText(/stderr: diagnostic/).waitFor();
  assert.deepEqual(errors, []);
  await page.screenshot({ path: join(root, "docs/screenshots/snapshot-worker-linux.png"), fullPage: true });
  const chunks = (await call({ method: "read_output", taskId: submittedId, afterSequence: 0 })).chunks;
  const reportPath = join(run, "success.task-report.json");
  await writeFile(reportPath, JSON.stringify({ schemaVersion: 1, platform: "linux", task: record, output: chunks, outputComplete: true }));

  await page.getByLabel("Аргументы — JSON-массив строк", { exact: true }).fill('["tree"]');
  await page.getByRole("button", { name: "Передать задачу worker", exact: true }).click();
  await until(async () => { const task = await latestTask(); return task.submission.command.arguments[0] === "tree" && task.status === "running" && task.processId !== null; }, "tree did not start");
  const tree = await latestTask();
  await page.getByRole("button", { name: "Отменить задачу", exact: true }).click();
  await until(async () => (await call({ method: "get_task", taskId: tree.taskId })).task.status === "cancelled", "tree was not cancelled");
  await page.locator('#native-task-detail .mantine-Badge-root').filter({ hasText: "Отменено" }).waitFor();

  await page.getByLabel("Аргументы — JSON-массив строк", { exact: true }).fill('["long"]');
  await page.getByRole("button", { name: "Передать задачу worker", exact: true }).click();
  await until(async () => { const task = await latestTask(); return task.submission.command.arguments[0] === "long" && task.status === "running" && task.processId !== null; }, "long command did not start");
  const long = await latestTask();
  await stopController();
  await page.getByText("Нет свежего соединения с controller Debian", { exact: true }).waitFor();
  assert.equal((await call({ method: "get_task", taskId: long.taskId })).task.status, "running", "controller exit cancelled a task");
  await startController();
  await page.getByText("Controller Debian подключён", { exact: true }).waitFor();
  assert.equal((await latestTask()).taskId, long.taskId, "reconnect created a new task");
  crashedPid = long.processId;
  await page.locator('#native-task-detail .mantine-Badge-root').filter({ hasText: "Выполняется" }).waitFor();
  await stopWorker("SIGKILL");
  await page.getByText("Нет свежего подтверждения worker", { exact: true }).first().waitFor();
  await page.getByText("Нет свежего соединения с controller Debian", { exact: true }).waitFor();
  assert.equal(await page.getByText("Controller Debian подключён", { exact: true }).count(), 0, "stale worker info presented controller as online");
  assert.equal(await page.locator('#native-task-detail .mantine-Badge-root').filter({ hasText: "Выполняется" }).count(), 1);
  try { process.kill(-crashedPid, "SIGKILL"); } catch (error) { if (error.code !== "ESRCH") throw error; } crashedPid = undefined;
  await startWorker();
  await page.locator('#native-task-detail .mantine-Badge-root').filter({ hasText: "Результат неизвестен" }).waitFor();
  assert.equal((await call({ method: "get_task", taskId: long.taskId })).task.status, "indeterminate");

  const reportPage = await browser.newPage({ viewport: { width: 1280, height: 1000 } });
  reportPage.on("pageerror", (error) => errors.push(error.message));
  await reportPage.goto(url);
  await reportPage.locator('input[type="file"]').setInputFiles(reportPath);
  await reportPage.getByText("Сохранённый результат · linux", { exact: true }).waitFor();
  await reportPage.getByRole("tab", { name: "stdout / stderr", exact: true }).first().click();
  await reportPage.getByText(/stdout: success Привет ✓/).waitFor();
  const invalidReport = join(run, "invalid.json");
  await writeFile(invalidReport, '{"schemaVersion":999}');
  await reportPage.locator('input[type="file"]').setInputFiles(invalidReport);
  await reportPage.getByText(/Файл не соответствует формату TaskReport/).waitFor();
  assert.deepEqual(errors, []);
  await page.getByRole("textbox", { name: "Способ запуска", exact: true }).click();
  await page.getByRole("option", { name: "Рецепт: direct", exact: true }).click();
  await page.getByRole("button", { name: "Передать задачу worker", exact: true }).click();
  await until(async () => { const task = await latestTask(); return task.submission.command.kind === "recipe" && task.submission.command.recipeId === "direct" && task.status === "succeeded"; }, "recipe did not use the shared command service");

  await page.getByRole("textbox", { name: "Способ запуска", exact: true }).click();
  await page.getByRole("option", { name: "Прямая команда", exact: true }).click();
  const savedArgv = '[\n  "long", "Привет ✓"\n]';
  await page.getByLabel("Аргументы — JSON-массив строк", { exact: true }).fill(savedArgv);
  await page.getByLabel("Рабочая папка относительно корня проекта", { exact: true }).fill("nested");
  await page.getByLabel("Timeout, секунд", { exact: true }).fill("120");
  await page.getByText(/Поля сохранены\. Восстановление/).waitFor();
  const tasksBeforeRestore = (await call({ method: "list_tasks" })).tasks.map((task) => task.taskId);
  await page.getByRole("textbox", { name: "Зарегистрированный проект", exact: true }).click();
  await page.getByRole("option", { name: "other-fixture", exact: true }).click();
  await until(async () => await page.getByLabel("Executable", { exact: true }).inputValue() === "", "another project inherited executable");
  assert.equal(await page.getByLabel("Аргументы — JSON-массив строк", { exact: true }).inputValue(), "[]");
  await page.getByLabel("Executable", { exact: true }).fill("other-program.exe");
  await page.getByLabel("Аргументы — JSON-массив строк", { exact: true }).fill('["other project"]');
  await page.getByText(/Поля сохранены\. Восстановление/).waitFor();
  await page.getByRole("textbox", { name: "Зарегистрированный проект", exact: true }).click();
  await page.getByRole("option", { name: "fixture", exact: true }).click();
  await until(async () => await page.getByLabel("Executable", { exact: true }).inputValue() === fixture, "fixture draft was not restored");
  assert.equal(await page.getByLabel("Аргументы — JSON-массив строк", { exact: true }).inputValue(), savedArgv);
  // Delivery project choice and per-project snapshot are independent of command fields.
  await page.getByRole("textbox", {name:"Проект доставки", exact:true}).click();
  await page.getByRole("option", {name:"other-fixture",exact:true}).click();
  await page.getByText("Опубликованных пакетов для этого проекта пока нет.", {exact:true}).waitFor();
  assert.equal(await deliverySnapshot.inputValue(), "");
  await page.getByRole("textbox", {name:"Проект доставки", exact:true}).click();
  await page.getByRole("option", {name:"fixture",exact:true}).click();
  await until(async () => (await deliverySnapshot.inputValue()).startsWith(published.snapshotId), "per-project delivery snapshot was not restored");
  await writeFile(join(source, "Привет.txt"), "UI second snapshot ✓");
  await exec("git", ["-C",source,"add","."]);
  await exec("git", ["-C",source,"-c","user.name=Fixture","-c","user.email=fixture@local","commit","-qm","second snapshot"]);
  const secondPublished = JSON.parse((await exec(controllerExe,["--config",controllerConfig,"publish","fixture","HEAD"])).stdout);
  const deltaPublished=JSON.parse((await exec(controllerExe,["--config",controllerConfig,"publish-delta","fixture","HEAD",published.snapshotId])).stdout);
  assert.equal(deltaPublished.ready.schemaVersion,2);
  await page.getByRole("button",{name:"Обновить список пакетов",exact:true}).click();
  await deliverySnapshot.fill(deltaPublished.ready.snapshotId);
  await page.getByRole("option",{name:new RegExp(`^${deltaPublished.ready.snapshotId} · commit`)}).click();
  await page.getByRole("button",{name:"Посмотреть состав snapshot",exact:true}).click();
  await page.getByText("Delta от проверенного snapshot",{exact:true}).waitFor();
  await page.getByText(`База: ${published.snapshotId}`,{exact:true}).waitFor();
  loseFirstImportAck=true;
  const beforeDelta=(await call({method:"list_imports"})).imports.length;
  await page.getByRole("button",{name:"Импортировать пакет",exact:true}).click();
  await page.getByText("Приём импорта не подтверждён",{exact:true}).waitFor();
  await page.getByRole("button",{name:"Проверить приём импорта",exact:true}).click();
  await page.getByText("Готово для сборки",{exact:true}).waitFor();
  assert.equal(deltaImportCalls.length,2);assert.deepEqual(deltaImportCalls[0],deltaImportCalls[1]);
  assert.equal((await call({method:"list_imports"})).imports.length,beforeDelta+1);
  const deltaImported=(await call({method:"get_import",importId:importedId})).import;
  assert.equal(deltaImported.status,"succeeded");
  assert.equal(await readFile(join(deltaImported.workspacePath,"Привет.txt"),"utf8"),"UI second snapshot ✓");
  assert.equal(await readFile(join(imported.workspacePath,"Привет.txt"),"utf8"),"UI snapshot ✓");

  await page.getByRole("button", {name:"Обновить список пакетов",exact:true}).click();
  draftSaveDelay = 250;
  const lastArgv = '["long", "last edit before close ✓"]';
  await page.getByLabel("Громкость сигнала, %", {exact:true}).fill("30");
  await page.getByLabel("Аргументы — JSON-массив строк", { exact: true }).fill(lastArgv);
  await deliverySnapshot.fill(secondPublished.snapshotId);
  await page.getByRole("option", {name:new RegExp(`^${secondPublished.snapshotId} · commit`)}).click();
  await page.evaluate(() => window.__testClose());
  assert.equal(await page.evaluate(() => window.__hidden), true);
  assert.equal(await page.evaluate(() => window.__destroyed), false, "hide must keep the companion alive");
  assert.equal(JSON.parse(await readFile(preferencesPath, "utf8")).drafts.at(-1).argumentsJson, lastArgv);
  assert.equal(JSON.parse(await readFile(preferencesPath, "utf8")).deliveryDrafts.at(-1).snapshotId, secondPublished.snapshotId);
  await page.close();
  draftSaveDelay = 0;
  const restoredPage = await browser.newPage({ viewport: { width: 1280, height: 1000 } });
  restoredPage.on("pageerror", (error) => errors.push(error.message));
  await attachNative(restoredPage);
  await restoredPage.goto(url);
  await restoredPage.getByText(/Поля восстановлены из локального файла/).waitFor();
  assert.equal(await restoredPage.getByLabel("Executable", { exact: true }).inputValue(), fixture);
  assert.equal(await restoredPage.getByLabel("Аргументы — JSON-массив строк", { exact: true }).inputValue(), lastArgv);
  assert.equal(await restoredPage.getByLabel("Рабочая папка относительно корня проекта", { exact: true }).inputValue(), "nested");
  assert.equal(await restoredPage.getByLabel("Timeout, секунд", { exact: true }).inputValue(), "120");
  assert.equal(await restoredPage.getByLabel("Громкость сигнала, %", {exact:true}).inputValue(), "30");
  await restoredPage.getByText(/Выбор восстановлен из локального файла/).waitFor();
  assert.equal(await restoredPage.getByRole("textbox", {name:"Проект доставки", exact:true}).inputValue(), "fixture");
  assert.ok((await restoredPage.getByRole("textbox", {name:"Snapshot ID опубликованного пакета", exact:true}).inputValue()).startsWith(secondPublished.snapshotId));
  assert.equal((await call({method:"list_imports"})).imports.length, beforeDelta+1, "delivery restore started an import");
  assert.deepEqual((await call({ method: "list_tasks" })).tasks.map((task) => task.taskId), tasksBeforeRestore, "form restore submitted a task");
  failDeliverySaves = true;
  await restoredPage.getByRole("textbox", {name:"Snapshot ID опубликованного пакета", exact:true}).fill(published.snapshotId);
  await restoredPage.getByRole("option", {name:new RegExp(`^${published.snapshotId} · commit`)}).click();
  await restoredPage.getByText("Не удалось сохранить выбор доставки", {exact:true}).waitFor();
  // A later successful command save must not hide an earlier delivery failure at close.
  await restoredPage.getByLabel("Timeout, секунд", {exact:true}).fill("121");
  await restoredPage.getByLabel("Timeout, секунд", {exact:true}).fill("120");
  await restoredPage.getByText(/Поля сохранены\. Восстановление/).waitFor();
  await restoredPage.evaluate(() => window.__testClose());
  await restoredPage.getByText("Окно осталось открытым: закрытие не подтверждено", {exact:true}).waitFor();
  assert.equal(await restoredPage.evaluate(() => window.__hidden), false);
  assert.equal(JSON.parse(await readFile(preferencesPath,"utf8")).deliveryDrafts.at(-1).snapshotId, secondPublished.snapshotId);
  failDeliverySaves = false;
  await restoredPage.getByRole("textbox", {name:"Snapshot ID опубликованного пакета", exact:true}).fill(secondPublished.snapshotId);
  await restoredPage.getByRole("option", {name:new RegExp(`^${secondPublished.snapshotId} · commit`)}).click();
  await restoredPage.getByText(/Выбор сохранён\. Восстановление/).waitFor();
  failDraftSaves = true;
  await restoredPage.getByLabel("Аргументы — JSON-массив строк", { exact: true }).fill("[");
  await restoredPage.getByText("Не удалось сохранить поля команды", { exact: true }).waitFor();
  await restoredPage.evaluate(() => window.__testClose());
  await restoredPage.getByText("Окно осталось открытым: закрытие не подтверждено", { exact: true }).waitFor();
  assert.equal(await restoredPage.evaluate(() => window.__destroyed), false);
  assert.equal(JSON.parse(await readFile(preferencesPath, "utf8")).drafts.at(-1).argumentsJson, lastArgv);
  await restoredPage.getByRole("button", { name: "Закрыть без сохранения", exact: true }).click();
  await until(async () => await restoredPage.evaluate(() => window.__hidden), "explicit close did not hide the window");
  assert.deepEqual(errors, []);
  failDraftSaves = false;
  const workflowPage = await browser.newPage({ viewport: { width: 1280, height: 1000 } });
  workflowPage.on("pageerror", (error) => errors.push(error.message));
  await attachNative(workflowPage);
  await workflowPage.goto(url);
  await workflowPage.getByText("Worker подключён · linux · ui-test", { exact: true }).waitFor();
  await workflowPage.getByRole("button", { name: "Посмотреть состав snapshot", exact: true }).click();
  await workflowPage.getByRole("button", { name: "Использовать в workflow", exact: true }).click();
  const profileField = workflowPage.getByLabel("Параметры workflow — JSON", { exact: true });
  const configured = JSON.parse(await profileField.inputValue());
  configured.artifacts = [{ name: "fixture executable", path: "build-cache/debug/snapshot-fixture" }];
  await profileField.fill(JSON.stringify(configured, null, 2));
  await workflowPage.getByLabel("Имя профиля", { exact: true }).fill("fixture profile");
  await workflowPage.getByRole("button", { name: "Сохранить профиль", exact: true }).click();
  await workflowPage.getByText("Именованный профиль сохранён", { exact: true }).waitFor();
  await workflowPage.reload();
  await until(async () => (await profileField.inputValue()).includes("fixture executable"), "workflow draft not restored after reload");
  assert.equal((await call({ method: "list_workflows" })).workflows.length, 0, "profile restore submitted a command");
  await workflowPage.getByRole("button", { name: "Запустить workflow", exact: true }).click();
  await workflowPage.getByText("Приём workflow не подтверждён", { exact: true }).waitFor();
  await workflowPage.getByRole("button", { name: "Проверить приём workflow", exact: true }).click();
  try { await workflowPage.getByText("Приложение готово", { exact: true }).waitFor(); }
  catch (error) { console.error(JSON.stringify(await call({ method: "get_workflow", workflowId: acceptedWorkflowId }), null, 2)); throw error; }
  await workflowPage.getByText(/start pid=\d+ · Привет ✓/).waitFor();
  await workflowPage.getByText("Результаты сборки", { exact: true }).waitFor();
  await workflowPage.getByLabel("Фильтр Лог", { exact: true }).fill("never-match");
  await workflowPage.getByText("Нет строк по фильтру", { exact: true }).waitFor();
  await workflowPage.getByLabel("Фильтр Лог", { exact: true }).fill("Привет");
  await workflowPage.getByText(/start pid=\d+ · Привет ✓/).waitFor();
  const completedWorkflow = (await call({ method: "get_workflow", workflowId: acceptedWorkflowId })).workflow;
  assert.equal(completedWorkflow.status, "succeeded");
  assert.equal(completedWorkflow.submission.snapshotId, secondPublished.snapshotId);
  assert.equal(completedWorkflow.steps.length, 5);
  assert.ok(completedWorkflow.steps.every((s) => s.status === "succeeded"));
  assert.equal((await call({ method: "list_workflows" })).workflows.length, 1, "lost workflow ACK created a duplicate pipeline");
  const readyApp = (await call({ method: "get_application", applicationId: completedWorkflow.applicationId })).application;
  await stopController();
  await workflowPage.getByText("Нет свежего соединения с controller Debian", { exact: true }).waitFor();
  assert.equal((await call({ method: "get_application", applicationId: readyApp.applicationId })).application.processId, readyApp.processId);
  await startController();
  await workflowPage.getByText("Controller Debian подключён", { exact: true }).waitFor();
  await workflowPage.getByRole("button", { name: "Открыть задачу build", exact: true }).click();
  await workflowPage.getByText("Exit code: 0", { exact: true }).waitFor();
  await workflowPage.getByRole("button", { name: "Заполнить профиль из workflow", exact: true }).click();
  const restoredProfile = JSON.parse(await workflowPage.getByLabel("Параметры workflow — JSON", { exact: true }).inputValue());
  assert.equal(restoredProfile.snapshotId, secondPublished.snapshotId);
  assert.equal((await call({ method: "list_workflows" })).workflows.length, 1, "restoring a profile submitted another workflow");
  await workflowPage.screenshot({ path: join(root, "docs/screenshots/workflow-worker-linux.png"), fullPage: true });
  await workflowPage.getByRole("button", { name: "Остановить приложение", exact: true }).click();
  await workflowPage.getByText("Приложение остановлено", { exact: true }).waitFor();
  assert.equal((await call({ method: "get_workflow", workflowId: acceptedWorkflowId })).workflow.status, "succeeded", "app stop rewrote completed startup history");
  // Deliberately missing executable: render the actual accepted encoded command, no script runs.
  const readableScript = "# Проверка прозрачности ✓ 🚀\nWrite-Output 'quoted \"text\"'\n";
  const readableArgv = ["-NoProfile", "-EncodedCommand", Buffer.from(readableScript, "utf16le").toString("base64")];
  const encodedTask = (await call({ method: "submit", request: {protocolVersion:1,requestId:"encoded-ui",projectId:"other-fixture",idempotencyKey:"encoded-ui",timeoutMs:5000,command:{kind:"direct",executable:"/deliberately-missing/pwsh.exe",arguments:readableArgv,workingDirectory:"."}} })).task;
  await until(async () => (await call({method:"get_task",taskId:encodedTask.taskId})).task.status === "failed", "intentional missing executable did not fail");
  // An automatic outcome appears, links the task, and does not acknowledge it on opening.
  await workflowPage.locator(`[data-attention-id]`).filter({hasText:encodedTask.taskId}).getByRole("button",{name:"Открыть задачу",exact:true}).click();
  await workflowPage.getByTestId("powershell-script").waitFor();
  assert.equal(await workflowPage.getByTestId("powershell-script").textContent(), readableScript);
  assert.equal(await workflowPage.getByText(JSON.stringify(readableArgv,null,2),{exact:true}).isVisible(),false,"Base64 remained in the default command view");
  await workflowPage.getByRole("button",{name:"Точные аргументы запуска · включая Base64",exact:true}).click();
  await workflowPage.getByText(JSON.stringify(readableArgv,null,2),{exact:true}).waitFor();
  assert.deepEqual((await call({method:"get_task",taskId:encodedTask.taskId})).task.resolvedCommand.arguments,readableArgv);
  const eventRequest={method:"request_attention",request:{requestId:"one",idempotencyKey:"ui-attention",taskId:encodedTask.taskId,severity:"warning",message:"Проверьте понятный сценарий ✓"}};
  await assert.rejects(workflowPage.evaluate((request)=>window.workerBridge(request),eventRequest),/attention acceptance ACK lost/);
  eventRequest.request.requestId="after-reconnect";
  const event=(await workflowPage.evaluate((request)=>window.workerBridge(request),eventRequest)).attention;
  const eventCard=workflowPage.locator(`[data-attention-id="${event.attentionId}"]`);
  await eventCard.getByText(event.message,{exact:true}).waitFor();
  assert.equal((await call({method:"list_attention",afterSequence:0,pendingOnly:true})).page.records.filter((r)=>r.message===event.message).length,1);
  await eventCard.getByRole("button",{name:"Открыть задачу",exact:true}).click();
  assert.equal((await call({method:"list_attention",afterSequence:0,pendingOnly:true})).page.records.find((r)=>r.attentionId===event.attentionId).acknowledgedAtMs,null);
  await eventCard.getByRole("button",{name:"Прочитано",exact:true}).click();
  await workflowPage.getByText(/Test: attention confirmation ACK lost/).waitFor();
  // Retry the same confirmation after loss; the worker keeps the original acknowledgement time.
  const acknowledged=(await call({method:"acknowledge_attention",attentionId:event.attentionId})).attention;
  assert.ok(acknowledged.acknowledgedAtMs !== null);
  await workflowPage.getByRole("button",{name:"Проверить подтверждение",exact:true}).click();
  await until(async () => !await workflowPage.getByText(/Test: attention confirmation ACK lost/).isVisible(),"Confirmation retry did not resolve the lost ACK");
  await workflowPage.reload();
  await workflowPage.getByLabel("Показать также подтверждённые события",{exact:true}).check();
  await eventCard.getByText(/Прочитано:/).waitFor();
  await stopWorker();
  await workflowPage.getByText("Worker недоступен",{exact:true}).waitFor();
  await startWorker();
  await workflowPage.getByText("Worker подключён · linux · ui-test",{exact:true}).waitFor();
  await eventCard.getByText(/Прочитано:/).waitFor();
  assert.deepEqual((await call({method:"acknowledge_attention",attentionId:event.attentionId})).attention,acknowledged);
  // Background popup navigation is consumed after the main WebView resumes.
  await workflowPage.evaluate((id) => { window.__companion.openTaskId=id; window.__companion.openTaskSequence++; }, completedWorkflow.steps.find((s)=>s.name==="build").operationId);
  await workflowPage.getByText("Exit code: 0",{exact:true}).waitFor();
  // Generic workflow target exists even when import failed before any task.
  const missingRequest={...completedWorkflow.submission,requestId:"missing-import-ui",idempotencyKey:"missing-import-ui",snapshotId:"missing-published-snapshot"};
  const missingWorkflow=(await workflowPage.evaluate((request)=>window.workerBridge({method:"submit_workflow",request}),missingRequest)).workflow;
  await until(async()=>(await call({method:"get_workflow",workflowId:missingWorkflow.workflowId})).workflow.status==="failed","missing import did not fail");
  const failureEvent=(await call({method:"list_attention_events",afterSequence:0,pendingOnly:true})).page.records.find((r)=>r.target.kind==="workflow" && r.target.workflowId===missingWorkflow.workflowId);
  assert.ok(failureEvent);
  const workflowCard=workflowPage.locator(`[data-attention-id="${failureEvent.attentionId}"]`);
  await workflowCard.getByRole("button",{name:"Открыть workflow",exact:true}).click();
  await workflowPage.locator(`[data-confirmed-workflow-id="${missingWorkflow.workflowId}"]`).waitFor();
  assert.equal((await call({method:"list_attention_events",afterSequence:0,pendingOnly:true})).page.records.find((r)=>r.attentionId===failureEvent.attentionId).acknowledgedAtMs,null);
  await workflowCard.getByRole("button",{name:"Прочитано",exact:true}).click();
  await until(async()=>(await call({method:"list_attention_events",afterSequence:0,pendingOnly:false})).page.records.find((r)=>r.attentionId===failureEvent.attentionId).acknowledgedAtMs!==null,"workflow event ACK missing");
  // Application target opens the actual application without rewriting startup success.
  const crashRequest={...completedWorkflow.submission,requestId:"crash-ui",idempotencyKey:"crash-ui",start:{...completedWorkflow.submission.start,arguments:["serve",join(project,"logs/app.log"),"crash-late"]}};
  const crashWorkflow=(await workflowPage.evaluate((request)=>window.workerBridge({method:"submit_workflow",request}),crashRequest)).workflow;
  await until(async()=>(await call({method:"get_workflow",workflowId:crashWorkflow.workflowId})).workflow.status==="succeeded","crash fixture startup failed");
  const crashDone=(await call({method:"get_workflow",workflowId:crashWorkflow.workflowId})).workflow;
  await until(async()=>(await call({method:"get_application",applicationId:crashDone.applicationId})).application.status==="failed","application did not crash");
  const appEvent=(await call({method:"list_attention_events",afterSequence:0,pendingOnly:true})).page.records.find((r)=>r.target.kind==="application" && r.target.applicationId===crashDone.applicationId);
  assert.ok(appEvent);
  await workflowPage.locator(`[data-attention-id="${appEvent.attentionId}"]`).getByRole("button",{name:"Открыть приложение",exact:true}).click();
  await workflowPage.locator(`[data-confirmed-application-id="${crashDone.applicationId}"]`).waitFor();
  assert.equal((await call({method:"get_workflow",workflowId:crashWorkflow.workflowId})).workflow.status,"succeeded");
  // A popup target can select an older workflow outside the recent ten.
  for (let i=0;i<11;i++) {
    const queued=(await call({method:"submit_workflow",request:{...missingRequest,requestId:`older-${i}`,idempotencyKey:`older-${i}`}})).workflow;
    await call({method:"cancel_workflow",workflowId:queued.workflowId});
  }
  assert.ok(!(await call({method:"list_workflows"})).workflows.some((r)=>r.workflowId===missingWorkflow.workflowId));
  await workflowPage.evaluate((target)=>{window.__companion.openTarget=target;window.__companion.openTaskSequence++;},failureEvent.target);
  await workflowPage.locator(`[data-confirmed-workflow-id="${missingWorkflow.workflowId}"]`).waitFor();
  // History opens exact old operations; exports use real worker preview and create-only files.
  const historical=(await call({method:"submit",request:{protocolVersion:1,requestId:"diagnostic-old",projectId:"other-fixture",idempotencyKey:"diagnostic-old",timeoutMs:10000,command:{kind:"direct",executable:fixture,arguments:["arguments","--token","browser-fake-secret","Привет ✓"],workingDirectory:"."}}})).task;
  await until(async()=>(await call({method:"get_task",taskId:historical.taskId})).task.status==="succeeded","history fixture did not finish");
  for(let i=0;i<55;i++){
    const recent=(await call({method:"submit",request:{protocolVersion:1,requestId:`history-${i}`,projectId:"other-fixture",idempotencyKey:`history-${i}`,timeoutMs:10000,command:{kind:"direct",executable:fixture,arguments:["success"],workingDirectory:"."}}})).task;
    await call({method:"cancel",taskId:recent.taskId});
    await until(async()=>!["running","queued"].includes((await call({method:"get_task",taskId:recent.taskId})).task.status),"history cancellation did not finish");
  }
  const usageBefore=(await call({method:"storage_usage"})).usage;
  const history=workflowPage.locator("#native-history");
  await workflowPage.keyboard.press("Control+k");
  assert.equal(await history.getByLabel("Поиск истории по ID или названию",{exact:true}).evaluate(e=>e===document.activeElement),true);
  // Fresh unfiltered first page must omit the older task, and its next page must contain it.
  await history.getByRole("button",{name:"Свежие записи",exact:true}).click();
  await until(async()=>await history.getByTestId("history-older").isEnabled(),"History page cursor missing");
  assert.equal(await history.getByTestId(`history-open-${historical.taskId}`).count(),0);
  await history.getByTestId("history-older").click();
  await history.getByTestId(`history-open-${historical.taskId}`).click();
  await workflowPage.locator(`[data-confirmed-task-id="${historical.taskId}"]`).waitFor();
  await history.getByTestId("diagnostic-prepare").click();
  const preview=history.getByTestId("diagnostic-preview");
  await preview.waitFor();
  let report=JSON.parse(await preview.inputValue());
  assert.equal(report.commandIncluded,false);assert.equal(report.outputIncluded,false);
  assert.ok(!JSON.stringify(report).includes("browser-fake-secret"));
  await history.getByTestId("diagnostic-include-command").check();
  await history.getByTestId("diagnostic-include-output").check();
  await history.getByTestId("diagnostic-redactions").fill("browser-fake-secret");
  await history.getByTestId("diagnostic-prepare").click();
  await until(async()=>JSON.parse(await preview.inputValue()).outputIncluded,"Output preview missing");
  report=JSON.parse(await preview.inputValue());
  assert.equal(report.task.taskId,historical.taskId);
  assert.ok(report.output.stdout.includes("Привет ✓"));
  assert.ok(!JSON.stringify(report).includes("browser-fake-secret"));
  report.reviewNote="Ручное изменение ✓";
  const reviewed=JSON.stringify(report,null,2);
  await preview.fill(reviewed);
  loseDiagnosticSaveAck=true;
  await history.getByTestId("diagnostic-save").click();
  await history.getByText(/Сохранение не подтверждено.*save ACK lost/).waitFor();
  assert.equal(await history.getByTestId("diagnostic-saved").count(),0);
  await history.getByTestId("diagnostic-save").click();
  await history.getByTestId("diagnostic-saved").waitFor();
  assert.equal(diagnosticSaves.at(-1).id,diagnosticSaves.at(-2).id,"Lost ACK retry got a new file ID");
  assert.equal(await readFile(diagnosticSaves.at(-1).path,"utf8"),reviewed);
  report.reviewNote="Вторая версия ✓";
  await preview.fill(JSON.stringify(report,null,2));
  await history.getByTestId("diagnostic-save").click();
  await history.getByTestId("diagnostic-saved").waitFor();
  assert.notEqual(diagnosticSaves.at(-1).id,diagnosticSaves.at(-2).id,"Edited preview reused an old file ID");
  assert.equal(await readFile(diagnosticSaves.at(-2).path,"utf8"),reviewed,"Editing report changed the previous saved file");
  // Quick actions can open an import outside the recent selection without a command.
  await workflowPage.keyboard.press("Control+Shift+p");
  const palette=workflowPage.getByRole("dialog",{name:"Быстрые действия",exact:true});
  await palette.getByLabel("Тип операции по ID",{exact:true}).click();
  await workflowPage.getByRole("option",{name:"Импорты",exact:true}).click();
  await palette.getByLabel("ID операции",{exact:true}).fill(importedId);
  await palette.getByRole("button",{name:"Открыть по ID",exact:true}).click();
  await workflowPage.locator(`[data-confirmed-import-id="${importedId}"]`).waitFor();
  await history.getByLabel("Поиск истории по ID или названию",{exact:true}).fill(historical.taskId);
  await history.getByTestId(`history-open-${historical.taskId}`).waitFor();
  assert.equal(await history.locator("[data-history-id]").count(),1);
  const usageAfter=(await call({method:"storage_usage"})).usage;
  assert.equal(usageBefore.taskCount,usageAfter.taskCount,"History/report UI executed a task");
  assert.equal(usageBefore.pendingAttentionCount,usageAfter.pendingAttentionCount,"History/report UI acknowledged an event");
  const settings=workflowPage.locator("#native-settings");
  await workflowPage.getByRole("navigation",{name:"Разделы рабочего окна"}).getByRole("button",{name:"Настройки",exact:true}).focus();await workflowPage.keyboard.press("Enter");assert.equal(await settings.evaluate(e=>e===document.activeElement),true);
  const settingsJson=settings.getByTestId("settings-json");await settingsJson.waitFor();
  const settingsBefore=await readFile(config,"utf8");const settingsEdit=JSON.parse(settingsBefore);settingsEdit.maxOutputBytesPerTask=8192;settingsEdit.outputRetention={automatic:false,olderThanDays:30};
  // Well-formed JSON with invalid nested types must not crash the workbench or
  // mutate the original config. The raw text remains available for correction.
  for (const malformed of [
    {...settingsEdit,worker:"invalid"},
    {...settingsEdit,projects:[{...settingsEdit.projects[0],recipes:{length:{invalid:true}}}]},
    {...settingsEdit,outputRetention:{automatic:{invalid:true},olderThanDays:30}},
  ]) {
    const text=JSON.stringify(malformed);await settingsJson.fill(text);
    await settings.getByTestId("settings-invalid-form").waitFor();
    assert.equal(await settingsJson.inputValue(),text);
    assert.equal(await settings.getByTestId("settings-controller-url").count(),0);
    assert.deepEqual(errors,[]);
  }
  await settings.getByTestId("settings-save").click();await settings.getByText(/Config JSON/).waitFor();
  assert.equal(await readFile(config,"utf8"),settingsBefore);
  const settingsPreview=JSON.stringify(settingsEdit,null,2);await settingsJson.fill(settingsPreview);
  await workflowPage.getByTestId("companion-quit").click();await workflowPage.getByText(/Сохраните или отмените изменения конфигурации/).waitFor();
  loseSettingsSaveAck=true;await settings.getByTestId("settings-save").click();await settings.getByText(/settings save ACK lost/).waitFor();
  assert.equal(await settings.getByTestId("settings-saved").count(),0);await settings.getByTestId("settings-save").click();await settings.getByTestId("settings-saved").waitFor();
  assert.equal(settingsCalls.at(-1).request.requestId,settingsCalls.at(-2).request.requestId);assert.equal(await readFile(config,"utf8"),settingsPreview);
  const runtimeSettings=(await call({method:"get_runtime_settings"})).settings;assert.equal(runtimeSettings.restartRequired,true);assert.notEqual(JSON.parse(runtimeSettings.activeJson).maxOutputBytesPerTask,8192);
  await exec("python3",["-c","import sqlite3,sys,json,time; c=sqlite3.connect(sys.argv[1]); raw=c.execute('SELECT record FROM tasks WHERE id=?',(sys.argv[2],)).fetchone()[0]; t=json.loads(raw); t['finishedAtMs']=int(time.time()*1000)-40*86400000; c.execute('UPDATE tasks SET record=? WHERE id=?',(json.dumps(t),sys.argv[2])); c.commit()",join(run,"state/journal.sqlite"),historical.taskId]);
  await settings.getByTestId("cleanup-preview").click();await settings.getByTestId("cleanup-summary").filter({hasText:"1 задач"}).waitFor();
  const cleanupBefore=(await call({method:"get_task",taskId:historical.taskId})).task;loseCleanupAck=true;
  await settings.getByTestId("cleanup-apply").click();await settings.getByText(/cleanup ACK lost/).waitFor();assert.equal(await settings.getByTestId("cleanup-result").count(),0);
  await settings.getByTestId("cleanup-apply").click();await settings.getByTestId("cleanup-result").waitFor();assert.equal(cleanupCalls.at(-1).request.idempotencyKey,cleanupCalls.at(-2).request.idempotencyKey);
  const cleanupAfter=(await call({method:"get_task",taskId:historical.taskId})).task;assert.equal(cleanupAfter.status,cleanupBefore.status);assert.equal(cleanupAfter.submission.idempotencyKey,cleanupBefore.submission.idempotencyKey);assert.ok(cleanupAfter.outputRemovedAtMs);assert.equal((await call({method:"read_output",taskId:historical.taskId,afterSequence:0})).chunks.length,0);
  await history.getByTestId("diagnostic-prepare").click();await until(async()=>JSON.parse(await history.getByTestId("diagnostic-preview").inputValue()).task.outputRemovedAtMs,"Diagnostic hid output cleanup");
  // A real journal failure must block new launches without presenting stale running as fresh.
  await exec("python3", ["-c", "import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute(\"CREATE TRIGGER reject_outcome BEFORE INSERT ON attention WHEN NEW.origin='task_outcome' BEGIN SELECT RAISE(ABORT,'expected UI journal fault'); END\"); c.commit()", join(run,"state/journal.sqlite")]);
  const uncommitted=(await call({method:"submit",request:{protocolVersion:1,requestId:"journal-fault-ui",projectId:"other-fixture",idempotencyKey:"journal-fault-ui",timeoutMs:5000,command:{kind:"direct",executable:"/deliberately-missing/fixture",arguments:[],workingDirectory:"."}}})).task;
  await workflowPage.getByText("Результат операции не записан · новые запуски приостановлены",{exact:true}).waitFor();
  assert.equal(await workflowPage.getByRole("button",{name:"Передать задачу worker",exact:true}).isDisabled(),true);
  assert.equal((await call({method:"get_task",taskId:uncommitted.taskId})).kind,"error");
  await exec("python3",["-c","import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute('DROP TRIGGER reject_outcome'); c.commit()",join(run,"state/journal.sqlite")]);
  await stopWorker(); await startWorker();
  await until(async()=>!(await call({method:"info"})).info.journalRecoveryRequired,"worker journal recovery did not clear fault");
  assert.equal((await call({method:"get_task",taskId:uncommitted.taskId})).task.status,"indeterminate");
  // Loss of tray keeps the window accessible; explicit quit is still available.
  await workflowPage.evaluate(() => { window.__companion.trayReady=false; });
  await workflowPage.evaluate(() => window.__testClose());
  await workflowPage.getByText("Окно осталось открытым: закрытие не подтверждено",{exact:true}).waitFor();
  assert.equal(await workflowPage.evaluate(() => window.__hidden),false);
  await workflowPage.getByRole("button",{name:"Завершить интерфейс",exact:true}).click();
  await until(async () => await workflowPage.evaluate(() => window.__destroyed),"explicit companion quit did not exit");
  // Explicit test mode is visibly distinct and offers a deliberate abort action.
  await workflowPage.addInitScript(() => { window.__validation={enabled:true,phase:"initial",step:"Проверка скрытия и возврата окна",completed:false,error:null}; });
  await workflowPage.reload();
  const validationBanner = workflowPage.getByTestId("validation-banner");
  await validationBanner.getByText("initial: Проверка скрытия и возврата окна",{exact:true}).waitFor();
  await workflowPage.evaluate(() => { window.__validation.step="Проверка повторного запуска"; });
  await validationBanner.getByText("initial: Проверка повторного запуска",{exact:true}).waitFor();
  await validationBanner.getByRole("button",{name:"Прервать проверку",exact:true}).click();
  assert.equal(await workflowPage.evaluate(() => window.__aborted),true);
  await workflowPage.evaluate(() => { window.__validation.enabled=false; });
  await until(async()=>!await validationBanner.isVisible(),"Validation banner did not clear");
  assert.deepEqual(errors, []);
  console.log("UI verified against real Unix worker: command/output/cancel/reconnect/recovery; snapshot discovery/dedup, pinned-base delta preview/import and same-method lost ACK; drafts/close failures; workflow/profiles/artifacts/log/lifecycle. Stage 5: decoded PowerShell, durable attention ACK/restart; notification preferences and exact popup targets. History: paging/keyboard/redacted editable diagnostics. Settings/retention: keyboard navigation, unsaved close guard, exact reviewed config, save ACK loss with same ID, preview/actual output cleanup and same-key reconciliation; task history/keys retained. Native windows and diagnostic publication are validated separately on Windows.");
} finally {
  if (crashedPid) { try { process.kill(-crashedPid, "SIGKILL"); } catch {} }
  await browser?.close();
  await server?.close();
  await stopWorker();
  await stopController();
  await rm(run, { recursive: true, force: true });
}
