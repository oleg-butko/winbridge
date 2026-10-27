// Linux browser + real worker IPC. No Windows, builds, commands or production config.
import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createConnection, createServer } from "node:net";
import { spawn } from "node:child_process";
import { createServer as viteServer } from "vite";
import { chromium } from "playwright";

const root = resolve(import.meta.dirname, "..");
const attributionMode = process.argv.includes("--attribution");
const run = await mkdtemp(join(tmpdir(), "wb-links-"));
await mkdir(join(root, ".winbridge"), { recursive: true });
const harness = await mkdtemp(join(root, ".winbridge", "connections-ui-"));
const socket = join(run, "state/worker-links-ui.sock");
const streams = new Set();
const stalled = createServer((stream) => { streams.add(stream); stream.on("close", () => streams.delete(stream)); });
const closed = createServer();
const listen = (server) => new Promise((done) => server.listen(0, "127.0.0.1", done));
const delay = (ms) => new Promise((done) => setTimeout(done, ms));
async function until(check) {
  const deadline = Date.now() + 10000;
  while (!await check()) { assert.ok(Date.now() < deadline, "Diagnostic fixture deadline"); await delay(20); }
}
function call(request) {
  return new Promise((done, reject) => {
    const stream = createConnection(socket); let received = Buffer.alloc(0);
    stream.setTimeout(2000, () => stream.destroy(new Error("Diagnostic IPC timeout")));
    stream.on("connect", () => { const bytes = Buffer.from(JSON.stringify(request)); const size = Buffer.alloc(4); size.writeUInt32LE(bytes.length); stream.write(Buffer.concat([size, bytes])); });
    stream.on("data", (bytes) => {
      received = Buffer.concat([received, bytes]);
      if (received.length >= 4 && received.length >= 4 + received.readUInt32LE(0)) {
        stream.end();
        try { done(JSON.parse(received.subarray(4).toString())); } catch (error) { reject(error); }
      }
    });
    stream.on("error", reject);
    stream.on("end", () => { if (received.length < 4 || received.length < 4 + received.readUInt32LE(0)) reject(new Error("Incomplete diagnostic reply")); });
  });
}
let worker, vite, browser, releaseHeld;
let logs = "";
function waitExit(child, milliseconds) {
  return new Promise((done) => {
    const finish = (stopped) => { clearTimeout(timer); child.off("exit", exited); done(stopped); };
    const exited = () => finish(true);
    const timer = setTimeout(() => finish(false), milliseconds);
    child.once("exit", exited);
    if (child.exitCode !== null || child.signalCode !== null) finish(true);
  });
}
async function stopWorker() {
  if (!worker?.pid || worker.exitCode !== null || worker.signalCode !== null) return true;
  const stopped = waitExit(worker, 3000); worker.kill("SIGINT");
  if (await stopped) return true;
  const killed = waitExit(worker, 3000); worker.kill("SIGKILL");
  assert.ok(await killed, "Owned test worker did not exit after SIGKILL");
  return false;
}
try {
  await listen(stalled); await listen(closed);
  const endpoints = [stalled.address().port, closed.address().port];
  await new Promise((done) => closed.close(done));
  const config = join(run, "worker.json");
  await writeFile(config, JSON.stringify({ schemaVersion: 1, instanceId: "links-ui", nodeId: "worker", stateDirectory: join(run, "state"),
    worker: { keepRunningOnUiExit: true, controllerTopology: { schemaVersion: 1, primary: "debian-203", controllers: endpoints.map((port, i) => ({ id: `debian-${203 + i}`, url: `ws://127.0.0.1:${port}` })) } },
    projects: [{ id: "fixture", workspaceRoot: run, allowDirectCommands: false, maxConcurrentMutations: 1, recipes: attributionMode ? [{id:"proof",executable:"/bin/true",arguments:[],workingDirectory:".",timeoutMs:2000}] : [] }] }));
  worker = spawn(join(process.env.CARGO_TARGET_DIR ?? join(root, "target"), "debug/winbridge-worker"), ["--config", config, "serve"], { stdio: ["ignore", "pipe", "pipe"] });
  worker.stdout.on("data", (data) => { logs += data; }); worker.stderr.on("data", (data) => { logs += data; });
  let startError; worker.on("error", (error) => { startError = error; });
  let info;
  await until(async () => {
    if (startError) throw startError;
    assert.equal(worker.exitCode, null, logs);
    try { info = (await call({ method: "info" })).info; return !!info; } catch { return false; }
  });
  const contract = { id: "winbridge.controller-connections", version: 1 };
  async function snapshot(context = info.fencedRequests.context) {
    const response = await call({ extensionVersion: 1, contract, context, payload: {} });
    assert.equal(response.result.kind, "data", JSON.stringify(response));
    assert.deepEqual(response.context, context);
    return response.result.payload;
  }
  await until(async () => (await snapshot()).connections.find((c) => c.id === "debian-204").connection.state === "disconnected");
  let attributionTask = null;
  if (attributionMode) {
    const accepted = await call({method:"submit",request:{protocolVersion:1,requestId:"attribution-ui",projectId:"fixture",idempotencyKey:"attribution-ui",timeoutMs:2000,command:{kind:"recipe",recipeId:"proof",arguments:[]}}});
    assert.equal(accepted.kind,"task");attributionTask=accepted.task.taskId;
    await until(async () => (await call({method:"get_task",taskId:attributionTask})).task.status === "succeeded");
  }
  await writeFile(join(harness, "index.html"), '<div id="root"></div><script type="module" src="./harness.tsx"></script>');
  await writeFile(join(harness, "harness.tsx"), `import React, {useState} from "react";
import {createRoot} from "react-dom/client";
import {MantineProvider} from "@mantine/core";
import "@mantine/core/styles.css";
import {ControllerConnectionsPanel} from "/@fs/${root}/ui/src/features/tasks/ControllerConnectionsPanel.tsx";
import {OperationAttributionPanel} from "/@fs/${root}/ui/src/features/tasks/OperationAttributionPanel.tsx";
function Harness(){const [props,setProps]=useState({info:${JSON.stringify(info)},connected:true});window.updateConnectionProps=(change)=>setProps((old)=>({...old,...change}));return <MantineProvider><ControllerConnectionsPanel key={props.info.generation} {...props}/>${attributionMode ? `<OperationAttributionPanel key={'author:'+props.info.generation} {...props} kind="task" operationId=${JSON.stringify(attributionTask)}/>` : ""}</MantineProvider>}
createRoot(document.getElementById("root")).render(<Harness/>);`);
  vite = await viteServer({ root, logLevel: "error", server: { host: "127.0.0.1", port: 0, fs: { allow: [root] } } });
  await vite.listen();
  browser = await chromium.launch({ headless: true, chromiumSandbox: true, executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH });
  const page = await browser.newPage(); const errors = [];
  page.on("pageerror", (error) => errors.push(String(error)));
  let calls = 0, holdNext = false, heldStarted = false;
  await page.exposeFunction("readConnectionSnapshot", async (request) => {
    calls++;
    const result = await snapshot(request.context);
    if (holdNext) { holdNext = false; heldStarted = true; await new Promise((done) => { releaseHeld = done; }); }
    return result;
  });
  await page.exposeFunction("readAttribution", async (request) => {
    const response=await call({extensionVersion:1,contract:{id:"winbridge.operation-attribution",version:1},context:request.context,payload:request.query});
    assert.equal(response.result.kind,"data");return response.result.payload;
  });
  await page.addInitScript(() => { window.__TAURI_INTERNALS__ = { invoke: (command, args) => { if (command === "controller_connections") return window.readConnectionSnapshot(args.request); if(command==="operation_attribution") return window.readAttribution(args.request); throw new Error(`Unexpected command ${command}`); } }; });
  await page.goto(`http://127.0.0.1:${vite.httpServer.address().port}/.winbridge/${harness.split("/").at(-1)}/index.html`);
  const panel = page.getByTestId("controller-connections");
  const a = page.getByTestId("controller-link-debian-203"), b = page.getByTestId("controller-link-debian-204");
  await a.getByText("Primary · локальная сводка", { exact: true }).waitFor();
  await b.getByText(/Connection refused/).waitFor();
  assert.match(await panel.innerText(), /Обработчики RPC: 0 \/ 32/);
  assert.equal((await call({ method: "list_tasks" })).tasks.length, attributionMode ? 1 : 0);
  if(attributionMode) await page.getByTestId("operation-attribution").getByText("Локальный вход worker",{exact:true}).waitFor();

  // Old in-flight replies must not populate the view of a new worker generation.
  holdNext = true;
  await panel.getByRole("button", { name: "Обновить соединения", exact: true }).click();
  await until(() => heldStarted);
  await page.evaluate((info) => window.updateConnectionProps({ info: { ...info, generation: "new-generation" } }), info);
  await panel.getByText(/поколение worker изменились/).waitFor();
  releaseHeld(); releaseHeld = null;
  assert.equal(await a.count(), 0);
  await page.evaluate((info) => window.updateConnectionProps({ info }), info);
  await a.waitFor(); await b.waitFor();

  // A real lost IPC retains the previous snapshot with a visible error.
  assert.ok(await stopWorker(), "Worker required forced termination");
  await panel.getByRole("button", { name: "Обновить соединения", exact: true }).click();
  await panel.getByText("Соединения не проверены", { exact: true }).waitFor();
  await a.waitFor(); assert.match(await panel.innerText(), /Сохранённый срез/);
  await page.evaluate(() => window.updateConnectionProps({ connected: false }));
  await panel.getByText(/Нет свежей связи с worker/).waitFor();
  assert.equal(await panel.getByRole("button", { name: "Обновить соединения", exact: true }).isDisabled(), true);

  const before = calls;
  const oldInfo = structuredClone(info); oldInfo.contractCatalog.supported = oldInfo.contractCatalog.supported.filter((c) => c.id !== contract.id && c.id!=="winbridge.operation-attribution");
  await page.evaluate((info) => window.updateConnectionProps({ info, connected: true }), oldInfo);
  await panel.waitFor({ state: "hidden" }); assert.equal(calls, before, "Old worker received an unsupported query");
  assert.deepEqual(errors, []);
  if(attributionMode) await page.getByTestId("operation-attribution").getByText("Эта версия worker не предоставляет сведения об авторе.",{exact:true}).waitFor();
  console.log(`Controller connections UI: two real links, primary/error/counters, stale generation, lost IPC, offline and old-worker negotiation passed; ${attributionMode ? "one owned /bin/true task, real immutable creator UI" : "no tasks accepted"}.`);
} catch (error) {
  console.error(logs); throw error;
} finally {
  releaseHeld?.();
  await browser?.close(); await vite?.close();
  await stopWorker();
  for (const stream of streams) stream.destroy();
  if (stalled.listening) await new Promise((done) => stalled.close(done));
  await rm(harness, { recursive: true, force: true }); await rm(run, { recursive: true, force: true });
}
