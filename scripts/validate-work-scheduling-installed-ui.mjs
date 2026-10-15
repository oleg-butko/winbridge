// Production renderer using checksum-verified observations from the installed Windows exe.
// No submit/cancel/config endpoint is exposed to the browser.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { readFile, writeFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { createServer } from "vite";
import { chromium } from "playwright";

const gate = resolve(process.argv[2] ?? "");
assert.ok(process.argv[2], "Usage: node scripts/validate-work-scheduling-installed-ui.mjs GATE_DIRECTORY");
const completed = JSON.parse(await readFile(join(gate, "completed.json"), "utf8"));
assert.equal(completed.passed, true);
const ready = JSON.parse((await readFile(join(completed.reportDirectory, "report.ready.json"), "utf8")).replace(/^\uFEFF/, ""));
const bytes = await readFile(join(completed.reportDirectory, "report.json"));
assert.equal(bytes.length, ready.bytes);
assert.equal(createHash("sha256").update(bytes).digest("hex"), ready.sha256);
assert.equal(ready.sha256, completed.reportSha256);
assert.equal(ready.passed, true);
const report = JSON.parse(bytes.toString("utf8").replace(/^\uFEFF/, ""));
assert.equal(report.passed, true);
assert.deepEqual(report.provider, completed.provider);
const modulePath = "/__installed_work_summary.tsx";
const source = `import React from 'react';import {createRoot} from 'react-dom/client';import {MantineProvider} from '@mantine/core';import '@mantine/core/styles.css';import {WorkSchedulingSummary} from '/src/features/tasks/WorkSchedulingSummary.tsx';const root=createRoot(document.getElementById('root'));window.__renderWorkSummary=(info,connected)=>root.render(React.createElement(MantineProvider,null,React.createElement(WorkSchedulingSummary,{info,connected})));`;
let server;
let browser;
try {
  server = await createServer({ plugins: [{ name: "installed-work-summary", enforce: "pre",
    resolveId(id) { if (id === modulePath) return modulePath; }, load(id) { if (id === modulePath) return source; },
    configureServer(instance) { instance.middlewares.use(async (request, response, next) => {
      if (request.url !== "/__installed_work_summary") return next();
      try { const html = await instance.transformIndexHtml(request.url, `<html lang="ru"><head><meta charset="UTF-8"></head><body><div id="root"></div><script type="module" src="${modulePath}"></script></body></html>`); response.setHeader("Content-Type", "text/html; charset=utf-8"); response.end(html); } catch (error) { next(error); }
    }); },
  }], server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  browser = await chromium.launch({ executablePath: process.env.WINBRIDGE_BROWSER ?? (existsSync("/usr/bin/chromium") ? "/usr/bin/chromium" : undefined), chromiumSandbox: true });
  const page = await browser.newPage({ viewport: { width: 1250, height: 680 } });
  const errors = []; page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/__installed_work_summary`);
  await page.waitForFunction(() => typeof window.__renderWorkSummary === "function");
  const panel = page.getByTestId("work-scheduling");
  async function render(info, connected = true) {
    assert.deepEqual(info.build, report.provider);
    await page.evaluate(({ info, connected }) => window.__renderWorkSummary(info, connected), { info, connected });
    const view = info.contractCatalog.workScheduling;
    const label = view.maxConcurrentWork === null ? "Общий лимит не включён" : `Работы: ${view.occupiedSlots}/${view.maxConcurrentWork}`;
    await panel.getByText(label, { exact: true }).waitFor();
    await panel.getByText(new RegExp(`Занято слотов: ${view.occupiedSlots}; ожидают исполнения: ${view.queuedOperations};`)).waitFor();
    await panel.getByText(new RegExp(view.context.journalIncarnation)).waitFor();
  }
  for (const scenario of report.scenarios) {
    await render(scenario.initialInfo);
    if (scenario.afterCancelInfo) await render(scenario.afterCancelInfo);
  }
  await render(completed.finalWorkingInfo);
  await render(completed.finalWorkingInfo, false);
  await panel.getByText(/Нет свежей связи/).waitFor();
  await panel.screenshot({ path: join(gate, "installed-summary.png") });
  assert.deepEqual(errors, []);
  const result = { passed: true, provider: completed.provider, reportSha256: ready.sha256,
    checks: ["actual-installed-windows-observations", "three-policy-summaries", "queued-after-cancel", "scope-visible", "working-default-mode", "stale-snapshot"], nativeWindowAutomation: false };
  await writeFile(join(gate, "ui-completed.json"), JSON.stringify(result, null, 2), { flag: "wx", mode: 0o600 });
  console.log(JSON.stringify(result));
} finally {
  if (browser) await browser.close();
  if (server) await server.close();
}
