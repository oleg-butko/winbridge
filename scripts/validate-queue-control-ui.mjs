// Actual isolated Unix queue owner + production user controls, including lost ACK recovery.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import { createConnection } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createServer } from "vite";
import { chromium } from "playwright";

const root = resolve(import.meta.dirname, "..");
const run = await mkdtemp(join(tmpdir(), "winbridge-work-ui-"));
const instance = "work-ui";
const socket = join(run, "state/worker-work-ui.sock");
const ids = [];
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
function call(request) {
  return new Promise((resolve, reject) => {
    const stream = createConnection(socket); let data = Buffer.alloc(0); let complete = false;
    stream.setTimeout(5000, () => stream.destroy(new Error("Isolated worker IPC deadline")));
    stream.on("connect", () => { const body = Buffer.from(JSON.stringify(request)); const prefix = Buffer.alloc(4); prefix.writeUInt32LE(body.length); stream.write(Buffer.concat([prefix, body])); });
    stream.on("data", (part) => {
      data = Buffer.concat([data, part]);
      if (data.length >= 4) {
        const size = data.readUInt32LE(0);
        if (!size || size > 1024 * 1024) return stream.destroy(new Error("Invalid frame"));
        if (data.length >= 4 + size) { complete = true; try { resolve(JSON.parse(data.subarray(4, 4 + size))); } catch (e) { reject(e); } stream.end(); }
      }
    });
    stream.on("error", reject); stream.on("end", () => { if (!complete) reject(new Error("Incomplete IPC frame")); });
  });
}
async function until(check) { const deadline = Date.now() + 15000; while (!(await check())) { assert.ok(Date.now() < deadline, "isolated workload deadline"); await sleep(25); } }
const configPath = join(run, "worker.local.json");
const projects = [];
for (const id of ["a", "b"]) { const path = join(run, id); await mkdir(path); projects.push({ id, workspaceRoot: path, allowDirectCommands: true, maxConcurrentMutations: 1, recipes: [] }); }
await writeFile(configPath, JSON.stringify({ schemaVersion: 1, instanceId: instance, nodeId: "worker", stateDirectory: join(run, "state"), worker: { keepRunningOnUiExit: true }, projects, workScheduling: { maxConcurrentWork: 1 }, queuePolicy: { maxPendingRoots: 2, maxPendingRootsPerProject: 1 } }));
let worker;
let browser;
let server;
let calls = [];
let loseReply = true;
try {
  worker = spawn(join(root, "target/debug/winbridge-worker"), ["--config", configPath, "serve"], { stdio: ["ignore", "ignore", "pipe"] });
  let log = ""; worker.stderr.on("data", (part) => { log = (log + part.toString()).slice(-8192); });
  await until(async () => { assert.equal(worker.exitCode, null, log); try { return (await call({ method: "info" })).kind === "info"; } catch { return false; } });
  const initial = (await call({ method: "info" })).info;
  const context = initial.fencedRequests.context;
  const modulePath = "/__queue_control.tsx";
  const source = `import React from 'react';import {createRoot} from 'react-dom/client';import {MantineProvider} from '@mantine/core';import '@mantine/core/styles.css';import {QueueControlPanel} from '/src/features/tasks/QueueControlPanel.tsx';const root=createRoot(document.getElementById('root'));const send=request=>window.queueCommand(request);window.__renderQueue=(info,connected)=>root.render(React.createElement(MantineProvider,null,React.createElement(QueueControlPanel,{info,connected,send})));window.__renderQueue(${JSON.stringify(initial)},true);`;
  server = await createServer({ plugins: [{ name: "queue-control-validation", enforce: "pre",
    resolveId(id) { if (id === modulePath) return modulePath; }, load(id) { if (id === modulePath) return source; },
    configureServer(instance) { instance.middlewares.use(async (request, response, next) => {
      if (request.url !== "/__queue_control") return next();
      try { const html = await instance.transformIndexHtml(request.url, `<html lang="ru"><head><meta charset="UTF-8"></head><body><div id="root"></div><script type="module" src="${modulePath}"></script></body></html>`); response.setHeader("Content-Type", "text/html; charset=utf-8"); response.end(html); } catch (e) { next(e); }
    }); },
  }], server: { host: "127.0.0.1", port: 0, strictPort: false } }); await server.listen();
  browser = await chromium.launch({ executablePath: process.env.WINBRIDGE_BROWSER ?? (existsSync("/usr/bin/chromium") ? "/usr/bin/chromium" : undefined), chromiumSandbox: true });
  const page = await browser.newPage({ viewport: { width: 1250, height: 900 } }); const errors = []; page.on("pageerror", (e) => errors.push(e.message));
  await page.exposeFunction("queueCommand", async (request) => {
    calls.push(structuredClone(request)); const reply = await call(request);
    if (loseReply && request.command.kind === "set") { loseReply = false; throw new Error("Simulated lost ACK after actual commit"); }
    return reply;
  });
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/__queue_control`);
  const panel = page.getByTestId("queue-control");
  await panel.getByRole("button", { name: "Приостановить приём", exact: true }).click();
  await panel.getByText(/Автоматического повтора нет/).waitFor();
  assert.equal((await call({ method: "info" })).info.contractCatalog.queueControl.mode, "admission_paused");
  const original = calls[0]; assert.equal(original.command.expectedRevision, "0"); assert.equal(calls.length, 1);
  await page.reload(); await panel.getByRole("button", { name: "Проверить квитанцию" }).waitFor();
  assert.equal(calls.length, 1, "restoring a saved command must never execute it");
  await panel.getByRole("button", { name: "Проверить квитанцию" }).click();
  await panel.getByText(/Команда подтверждена квитанцией ревизии 1/).waitFor();
  assert.equal(calls[1].command.kind, "observe"); assert.equal(calls[1].command.commandId, original.command.commandId);
  const concurrent = await call({ queueControlVersion: 1, context, command: { kind: "set", commandId: "concurrent", expectedRevision: "1", mode: "paused" } });
  assert.equal(concurrent.result.kind, "applied");
  await panel.getByRole("button", { name: "Возобновить приём", exact: true }).click();
  await panel.getByText(/revision_conflict/).waitFor();
  assert.equal((await call({ method: "info" })).info.contractCatalog.queueControl.mode, "paused");
  await page.evaluate((info) => window.__renderQueue(info, true), (await call({ method: "info" })).info);
  await panel.getByRole("button", { name: "Возобновить приём", exact: true }).click();
  await panel.getByText(/Команда подтверждена квитанцией ревизии 3/).waitFor();
  await panel.getByRole("button", { name: "Возобновить запуск", exact: true }).click();
  await panel.getByText(/Команда подтверждена квитанцией ревизии 4/).waitFor();
  for (const projectId of ["a", "b"]) {
    const result = await call({ method: "submit", request: { protocolVersion: 1, requestId: projectId, idempotencyKey: projectId, projectId, timeoutMs: 60000, command: { kind: "direct", executable: join(root, "target/debug/winbridge-fixture"), arguments: ["long"], workingDirectory: "." } } });
    assert.equal(result.kind, "task"); ids.push(result.task.taskId);
  }
  let info; await until(async () => { info = (await call({ method: "info" })).info; return info.contractCatalog.queueControl.pendingRoots === 2; });
  await page.evaluate((info) => window.__renderQueue(info, true), info);
  await panel.getByText("Достигнут общий лимит 2.", { exact: true }).waitFor();
  await panel.getByText("Незавершённых запросов: 2 / 2.", { exact: true }).waitFor();
  const count = calls.length;
  await page.evaluate(() => { Storage.prototype.setItem = () => { throw new Error("Simulated storage failure"); }; });
  await panel.getByRole("button", { name: "Приостановить приём", exact: true }).click();
  await panel.getByText(/Команда не отправлена: не удалось сохранить/).waitFor();
  assert.equal(calls.length, count, "failed pre-send persistence must prevent submission");
  await page.evaluate((info) => window.__renderQueue(info, false), info);
  await panel.getByText(/Нет свежей связи/).waitFor();
  assert.equal(await panel.getByRole("button", { name: "Приостановить приём", exact: true }).isDisabled(), true);
  const evidence = join(root, ".winbridge/architecture-transition", `queue-ui-${Date.now()}`); await mkdir(evidence, { recursive: true });
  await panel.screenshot({ path: join(evidence, "queue.png") }); assert.deepEqual(errors, []);
  const result = { passed: true, platform: "linux", provider: initial.build, checks: ["actual-local-queue-command", "lost-ack-receipt", "persist-before-send", "restore-no-resubmit", "stale-revision-no-rebase", "separate-admission-dispatch", "actual-root-count-blockers", "storage-failure-no-dispatch", "offline-no-mutation"], evidence };
  await writeFile(join(evidence, "result.json"), JSON.stringify(result, null, 2), { flag: "wx", mode: 0o600 }); console.log(JSON.stringify(result));
} finally {
  if (browser) await browser.close(); if (server) await server.close();
  for (const id of ids) { try { await call({ method: "cancel", taskId: id }); } catch {} }
  if (ids.length && worker?.exitCode === null && worker?.signalCode === null) await until(async () => (await call({ method: "info" })).info.activeTaskCount === 0);
  if (worker && worker.exitCode === null && worker.signalCode === null) {
    let timer; const closed = new Promise((resolve) => worker.once("exit", resolve)); worker.kill("SIGINT");
    try { await Promise.race([closed, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error("isolated worker shutdown deadline")), 10000); })]); }
    catch (e) { worker.kill("SIGKILL"); await closed; throw e; } finally { clearTimeout(timer); }
  }
  await rm(run, { recursive: true, force: true });
}
