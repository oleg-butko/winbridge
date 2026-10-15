// The real ReadinessPanel renders metadata read from the installed Windows worker.
// This localhost page has no worker command endpoint and performs no Windows effects.
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { existsSync } from "node:fs";
import { readFile, writeFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { parseArgs, promisify } from "node:util";
import { createServer } from "vite";
import { chromium } from "playwright";

const { values } = parseArgs({ options: { "controller-config": { type: "string" }, "pair-gate": { type: "string" } } });
assert.ok(values["controller-config"] && values["pair-gate"], "Supply --controller-config and --pair-gate");
const root = resolve(import.meta.dirname, "..");
const directory = resolve(values["pair-gate"]);
const completed = JSON.parse(await readFile(join(directory, "completed.json"), "utf8"));
assert.equal(completed.passed, true);
const { stdout } = await promisify(execFile)("python3", ["-c", `
import json,sys,importlib.util
from pathlib import Path
s=importlib.util.spec_from_file_location('n','scripts/validate-workflow-native.py');m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
c=json.loads(Path(sys.argv[1]).read_text());e=Path(c['stateDirectory'])/('controller-'+c['instanceId']+'.sock');i=m.call(e,{'method':'info'})['info']
print(json.dumps({k:i[k] for k in ['instanceId','nodeId','build','generation','contractCatalog','fencedRequests','controllerConnection']}))
`, values["controller-config"]], { cwd: root, maxBuffer: 1024 * 1024 });
const info = JSON.parse(stdout);
const receipt = completed.view.receiptId;
assert.ok(info.contractCatalog.controllerProjection.views.some((view) => view.receiptId === receipt && view.readiness.enabled));
const modulePath = "/__projection_validation.tsx";
const source = `
import React from 'react';
import {createRoot} from 'react-dom/client';
import {MantineProvider} from '@mantine/core';
import '@mantine/core/styles.css';
import {ReadinessPanel} from '/src/features/tasks/ReadinessPanel.tsx';
const info=${JSON.stringify(info)};
const root=createRoot(document.getElementById('root'));
window.__openedTasks=[];
window.__renderReadiness=(connected)=>root.render(React.createElement(MantineProvider,null,React.createElement(ReadinessPanel,{info,connected,onTask:id=>window.__openedTasks.push(id)})));
window.__renderReadiness(true);`;
let server;
let browser;
try {
  server = await createServer({ plugins: [{ name: "winbridge-readonly-projection-test", enforce: "pre",
    resolveId(id) { if (id === modulePath) return modulePath; },
    load(id) { if (id === modulePath) return source; },
    configureServer(instance) {
      instance.middlewares.use(async (request, response, next) => {
        if (request.url !== "/__projection_validation") return next();
        try {
          const html = await instance.transformIndexHtml(request.url, `<html lang="ru"><head><meta charset="UTF-8"></head><body><div id="root"></div><script type="module" src="${modulePath}"></script></body></html>`);
          response.setHeader("Content-Type", "text/html; charset=utf-8"); response.end(html);
        } catch (error) { next(error); }
      });
    },
  }], server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  const address = server.httpServer.address();
  browser = await chromium.launch({ executablePath: process.env.WINBRIDGE_BROWSER ?? (existsSync("/usr/bin/chromium") ? "/usr/bin/chromium" : undefined), chromiumSandbox: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1200 } });
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(`http://127.0.0.1:${address.port}/__projection_validation`);
  const panel = page.getByTestId("contract-readiness");
  try { await panel.waitFor(); } catch (error) { throw new Error(`Readiness renderer unavailable: ${errors.join("; ")} / ${(await page.locator("body").innerText()).slice(0, 2000)}`, { cause: error }); }
  const text = await panel.innerText();
  for (const label of ["Поддержка", "Проверены сборки", "Проверена пара", "Готовность профиля"]) assert.match(text, new RegExp(label + ": да", "i"));
  assert.ok(text.includes(receipt), "Current real receipt is missing");
  await page.getByText(`Проверка ${completed.view.verificationKey}: Пройдена`, { exact: true }).click();
  const selected = completed.view.taskIds[1];
  await page.getByRole("button", { name: `Открыть задачу 2: ${selected}`, exact: true }).click();
  assert.deepEqual(await page.evaluate(() => window.__openedTasks), [selected]);
  await panel.screenshot({ path: join(directory, "browser-ready.png") });
  await page.evaluate(() => window.__renderReadiness(false));
  await page.getByText("Последние известные данные. Текущая готовность не подтверждена.", { exact: true }).waitFor();
  assert.doesNotMatch(await panel.innerText(), /Готовность профиля: да/i);
  assert.deepEqual(errors, []);
  const result = { passed: true, platform: "windows-data/debian-browser", receiptId: receipt,
    checks: ["actual-windows-info", "real-panel-four-facts", "exact-saved-task-navigation", "stale-local-connection-no-enable"],
    scope: info.fencedRequests.context, sessionId: info.controllerConnection.sessionId };
  await writeFile(join(directory, "browser-result.json"), JSON.stringify(result, null, 2), { flag: "wx", mode: 0o600 });
  console.log(JSON.stringify(result));
} finally {
  if (browser) await browser.close();
  if (server) await server.close();
}
