// Production panel + real MCP/Windows pages; the browser bridge has no mutation API.
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { randomUUID } from "node:crypto";
import { existsSync } from "node:fs";
import { readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { parseArgs, promisify } from "node:util";
import { createServer } from "vite";
import { chromium } from "playwright";

const { values } = parseArgs({ options: { "controller-config": { type: "string" }, gate: { type: "string" } } });
assert.ok(values["controller-config"] && values.gate);
const root = resolve(import.meta.dirname, "..");
const directory = resolve(values.gate);
const info = JSON.parse(await readFile(join(directory, "ui-info.json"), "utf8"));
const prepared = JSON.parse(await readFile(join(directory, "prepared.json"), "utf8"));
const opened = [];
const queries = [];
const modulePath = "/__operations_validation.tsx";
const source = `
import React from 'react';
import {createRoot} from 'react-dom/client';
import {MantineProvider} from '@mantine/core';
import '@mantine/core/styles.css';
import {OperationPagePanel} from '/src/features/history/OperationPagePanel.tsx';
const root=createRoot(document.getElementById('root'));
window.__renderOperations=(connected)=>root.render(React.createElement(MantineProvider,null,React.createElement(OperationPagePanel,{info:${JSON.stringify(info)},connected,onOpen:entry=>window.operationOpened(entry)})));
window.__renderOperations(true);`;
let server;
let browser;
try {
  server = await createServer({ plugins: [{ name: "winbridge-readonly-operation-test", enforce: "pre",
    resolveId(id) { if (id === modulePath) return modulePath; },
    load(id) { if (id === modulePath) return source; },
    configureServer(instance) {
      instance.middlewares.use(async (request, response, next) => {
        if (request.url !== "/__operations_validation") return next();
        try {
          const html = await instance.transformIndexHtml(request.url, `<html lang="ru"><head><meta charset="UTF-8"></head><body><div id="root"></div><script type="module" src="${modulePath}"></script></body></html>`);
          response.setHeader("Content-Type", "text/html; charset=utf-8"); response.end(html);
        } catch (error) { next(error); }
      });
    },
  }], server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  browser = await chromium.launch({ executablePath: process.env.WINBRIDGE_BROWSER ?? (existsSync("/usr/bin/chromium") ? "/usr/bin/chromium" : undefined), chromiumSandbox: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1100 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.exposeFunction("operationRead", async (request) => {
    assert.deepEqual(request.context, info.fencedRequests.context);
    const file = join(directory, `browser-query-${randomUUID()}.json`);
    await writeFile(file, JSON.stringify(request), { flag: "wx", mode: 0o600 });
    const { stdout } = await promisify(execFile)("python3", [join(root, "scripts/validate-operation-pages.py"),
      "--controller-config", resolve(values["controller-config"]), "--query", file], { cwd: root, timeout: 30000, maxBuffer: 1024 * 1024 });
    const response = JSON.parse(stdout);
    queries.push({ request, ids: response.records.map((r) => r.reference.operationId) });
    return response;
  });
  await page.exposeFunction("operationOpened", (entry) => opened.push(entry));
  await page.addInitScript(() => {
    window.__TAURI_INTERNALS__ = { invoke: (command, args) => {
      if (command !== "operation_page") throw new Error(`No mutation endpoint: ${command}`);
      return window.operationRead(args.request);
    } };
  });
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/__operations_validation`);
  const panel = page.getByTestId("operation-pages");
  const refresh = panel.getByRole("button", { name: "Обновить общий журнал", exact: true });
  await refresh.click();
  await panel.getByRole("button", { name: `Открыть операцию ${prepared.taskId}`, exact: true }).waitFor();
  assert.match(await panel.innerText(), /Ресурс проекта: изменение проекта; занят/);
  const firstIds = queries.at(-1).ids;
  const next = panel.getByRole("button", { name: "Следующая страница общего журнала", exact: true });
  assert.equal(await next.isEnabled(), true);
  await next.click();
  // Owner IDs may occur in resources on many rows; wait for the actual row button.
  await panel.getByRole("button", { name: `Открыть операцию ${firstIds[0]}`, exact: true }).waitFor({ state: "detached" });
  assert.equal(queries.length, 2);
  assert.equal(queries[1].request.query.cursor.context.journalIncarnation, info.fencedRequests.context.journalIncarnation);
  assert.ok(queries[1].ids.every((id) => !firstIds.includes(id)));
  await panel.getByRole("textbox", { name: "Поиск по ID, проекту и названию", exact: true }).fill(prepared.taskId);
  await panel.getByText(/Фильтры изменены/).waitFor();
  await refresh.click();
  const open = panel.getByRole("button", { name: `Открыть операцию ${prepared.taskId}`, exact: true });
  await open.waitFor();
  assert.equal(await panel.getByRole("button", { name: /^Открыть операцию / }).count(), 1);
  await open.click();
  assert.equal(opened.length, 1);
  assert.equal(opened[0].operationId, prepared.taskId);
  assert.equal(opened[0].kind, "task");
  assert.equal(queries.at(-1).request.query.cursor, null);
  assert.match(await panel.innerText(), /revision \d+ · событие \d+/);
  await panel.screenshot({ path: join(directory, "browser-operation-page.png") });
  const count = queries.length;
  await page.evaluate(() => window.__renderOperations(false));
  await panel.getByText(/Нет свежей связи с worker/).waitFor();
  assert.equal(await refresh.isDisabled(), true);
  assert.equal(await open.isDisabled(), true);
  assert.equal(queries.length, count);
  assert.deepEqual(errors, []);
  const result = { passed: true, platform: "windows-data/debian-browser", taskId: prepared.taskId,
    checks: ["actual-mcp-windows-pages", "real-panel-resource-and-revision", "cursor-next-page", "filter-explicit-refresh",
      "exact-task-navigation", "stale-local-freshness-disabled"], readonlyQueries: queries.length };
  await writeFile(join(directory, "browser-result.json"), JSON.stringify(result, null, 2), { flag: "wx", mode: 0o600 });
  console.log(JSON.stringify(result));
} finally {
  if (browser) await browser.close();
  if (server) await server.close();
}
