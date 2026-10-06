// Integration test: browser transport shim -> real local worker, never fixture task data.
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
  let acceptedWorkflowId;
  let importedId;
  let submittedId;
  let draftSaveDelay = 0;
  let failDraftSaves = false;
  let failDeliverySaves = false;
  async function attachNative(nativePage) {
    await nativePage.exposeFunction("workerBridge", async (request) => {
      const response = await call(request, request.method === "submit_workflow" ? join(run, "controller/controller-ui-test.sock") : socket);
      if (request.method === "submit" && loseFirstAck) { loseFirstAck = false; submittedId = response.task.taskId; throw new Error("Test: acceptance response was lost"); }
      if (request.method === "import_snapshot" && loseFirstImportAck) { loseFirstImportAck = false; importedId = response.import.importId; throw new Error("Test: import ACK lost"); }
      if (request.method === "submit_workflow" && loseFirstWorkflowAck && response.kind === "workflow") { loseFirstWorkflowAck = false; acceptedWorkflowId = response.workflow.workflowId; throw new Error("Test: workflow ACK lost"); }
      return response;
    });
    // Native filesystem persistence is covered by core tests. Here the IPC shim
    // uses a real file across fresh WebViews to test the form and close lifecycle.
    await nativePage.exposeFunction("draftBridge", async (command, args) => {
      const state = existsSync(preferencesPath) ? JSON.parse(await readFile(preferencesPath, "utf8")) : { schemaVersion: 1, drafts: [], deliveryDrafts: [] };
      state.deliveryDrafts ??= [];
      state.workflowProfiles ??= [];
      if (command === "load_command_drafts") return state;
      assert.ok(["save_command_draft", "save_delivery_draft", "save_workflow_profile"].includes(command));
      if (draftSaveDelay) await sleep(draftSaveDelay);
      if (failDraftSaves || (command === "save_delivery_draft" && failDeliverySaves)) throw new Error("Test: local state directory is unavailable");
      const draft = args.draft ?? args.profile;
      const field = command === "save_workflow_profile" ? "workflowProfiles" : command === "save_command_draft" ? "drafts" : "deliveryDrafts";
      state[field] = state[field].filter((saved) => saved.workerInstanceId !== draft.workerInstanceId || saved.projectId !== draft.projectId || saved.workspaceRoot !== draft.workspaceRoot || (field === "workflowProfiles" && saved.name !== draft.name));
      state[field].push(draft);
      await writeFile(preferencesPath, JSON.stringify(state));
    });
    await nativePage.addInitScript(() => {
      const callbacks = new Map(); const listeners = new Map(); let nextCallback = 0;
      window.isTauri = true;
      window.__destroyed = false;
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
          if (command === "worker_rpc") return window.workerBridge(args.request);
          if (command === "desktop_build") return Promise.resolve({ version: "0.1.0", sourceCommit: "browser-test", sourceDirty: false, sourceManifestSha256: null, buildId: "test-shell" });
          if (["load_command_drafts", "save_command_draft", "save_delivery_draft", "save_workflow_profile"].includes(command)) return window.draftBridge(command, args);
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
  await page.locator('.mantine-Badge-root').filter({ hasText: "Отменено" }).waitFor();

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
  await page.locator('.mantine-Badge-root').filter({ hasText: "Выполняется" }).waitFor();
  await stopWorker("SIGKILL");
  await page.getByText("Нет свежего подтверждения worker", { exact: true }).first().waitFor();
  await page.getByText("Нет свежего соединения с controller Debian", { exact: true }).waitFor();
  assert.equal(await page.getByText("Controller Debian подключён", { exact: true }).count(), 0, "stale worker info presented controller as online");
  assert.equal(await page.locator('.mantine-Badge-root').filter({ hasText: "Выполняется" }).count(), 1);
  try { process.kill(-crashedPid, "SIGKILL"); } catch (error) { if (error.code !== "ESRCH") throw error; } crashedPid = undefined;
  await startWorker();
  await page.locator('.mantine-Badge-root').filter({ hasText: "Результат неизвестен" }).waitFor();
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
  await page.getByRole("button", {name:"Обновить список пакетов",exact:true}).click();
  draftSaveDelay = 250;
  const lastArgv = '["long", "last edit before close ✓"]';
  await page.getByLabel("Аргументы — JSON-массив строк", { exact: true }).fill(lastArgv);
  await deliverySnapshot.fill(secondPublished.snapshotId);
  await page.getByRole("option", {name:new RegExp(`^${secondPublished.snapshotId} · commit`)}).click();
  await page.evaluate(() => window.__testClose());
  assert.equal(await page.evaluate(() => window.__destroyed), true);
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
  await restoredPage.getByText(/Выбор восстановлен из локального файла/).waitFor();
  assert.equal(await restoredPage.getByRole("textbox", {name:"Проект доставки", exact:true}).inputValue(), "fixture");
  assert.ok((await restoredPage.getByRole("textbox", {name:"Snapshot ID опубликованного пакета", exact:true}).inputValue()).startsWith(secondPublished.snapshotId));
  assert.equal((await call({method:"list_imports"})).imports.length, 1, "delivery restore started an import");
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
  await restoredPage.getByText("Окно осталось открытым: поля не сохранены", {exact:true}).waitFor();
  assert.equal(await restoredPage.evaluate(() => window.__destroyed), false);
  assert.equal(JSON.parse(await readFile(preferencesPath,"utf8")).deliveryDrafts.at(-1).snapshotId, secondPublished.snapshotId);
  failDeliverySaves = false;
  await restoredPage.getByRole("textbox", {name:"Snapshot ID опубликованного пакета", exact:true}).fill(secondPublished.snapshotId);
  await restoredPage.getByRole("option", {name:new RegExp(`^${secondPublished.snapshotId} · commit`)}).click();
  await restoredPage.getByText(/Выбор сохранён\. Восстановление/).waitFor();
  failDraftSaves = true;
  await restoredPage.getByLabel("Аргументы — JSON-массив строк", { exact: true }).fill("[");
  await restoredPage.getByText("Не удалось сохранить поля команды", { exact: true }).waitFor();
  await restoredPage.evaluate(() => window.__testClose());
  await restoredPage.getByText("Окно осталось открытым: поля не сохранены", { exact: true }).waitFor();
  assert.equal(await restoredPage.evaluate(() => window.__destroyed), false);
  assert.equal(JSON.parse(await readFile(preferencesPath, "utf8")).drafts.at(-1).argumentsJson, lastArgv);
  await restoredPage.getByRole("button", { name: "Закрыть без сохранения", exact: true }).click();
  await until(async () => await restoredPage.evaluate(() => window.__destroyed), "explicit close did not close the window");
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
  assert.deepEqual(errors, []);
  console.log("UI verified against real Unix worker: command/output/cancel/reconnect/recovery; snapshot discovery/refresh/commit/Unicode/mapping, lost import ACK/dedup; independent delivery/command drafts and close/save failures. Workflow: real import/build/start/readiness/log, lost ACK/dedup, controller reconnect preserves app PID, linked build task, restore profile without commands, separate app stop and startup history.");
} finally {
  if (crashedPid) { try { process.kill(-crashedPid, "SIGKILL"); } catch {} }
  await browser?.close();
  await server?.close();
  await stopWorker();
  await stopController();
  await rm(run, { recursive: true, force: true });
}
