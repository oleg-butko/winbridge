// Production panel + checksummed actual Windows observations. No mutation endpoint.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { readFile, writeFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { createServer } from "vite";
import { chromium } from "playwright";

assert.ok(process.argv[2], "Usage: node scripts/validate-queue-control-installed-ui.mjs GATE_DIRECTORY");
const gate = resolve(process.argv[2]);
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
const modulePath = "/__installed_queue.tsx";
const source = `import React from 'react';import {createRoot} from 'react-dom/client';import {MantineProvider} from '@mantine/core';import '@mantine/core/styles.css';import {QueueControlPanel} from '/src/features/tasks/QueueControlPanel.tsx';const root=createRoot(document.getElementById('root'));window.__renderQueue=(info,connected)=>root.render(React.createElement(MantineProvider,null,React.createElement(QueueControlPanel,{key:info.instanceId+info.nodeId,info,connected})));`;
let server;
let browser;
try {
  server = await createServer({ plugins: [{ name: "installed-queue-panel", enforce: "pre",
    resolveId(id) { if (id === modulePath) return modulePath; }, load(id) { if (id === modulePath) return source; },
    configureServer(instance) { instance.middlewares.use(async (request, response, next) => {
      if (request.url !== "/__installed_queue") return next();
      try { const html = await instance.transformIndexHtml(request.url, `<html lang="ru"><head><meta charset="UTF-8"></head><body><div id="root"></div><script type="module" src="${modulePath}"></script></body></html>`); response.setHeader("Content-Type", "text/html; charset=utf-8"); response.end(html); } catch (error) { next(error); }
    }); },
  }], server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  browser = await chromium.launch({ executablePath: process.env.WINBRIDGE_BROWSER ?? (existsSync("/usr/bin/chromium") ? "/usr/bin/chromium" : undefined), chromiumSandbox: true });
  const page = await browser.newPage({ viewport: { width: 1250, height: 900 } });
  const errors = []; page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/__installed_queue`);
  await page.waitForFunction(() => typeof window.__renderQueue === "function");
  const panel = page.getByTestId("queue-control");
  async function render(info, connected = true) {
    assert.deepEqual(info.build, report.provider);
    await page.evaluate(({ info, connected }) => window.__renderQueue(info, connected), { info, connected });
    const v = info.contractCatalog.queueControl;
    const count = `Незавершённых запросов: ${v.pendingRoots}${v.policy ? ` / ${v.policy.maxPendingRoots}` : "; общий лимит не задан"}.`;
    await panel.getByText(count, { exact: true }).waitFor();
    await panel.getByText(new RegExp(v.context.journalIncarnation)).waitFor();
    const admission = v.mode === "paused" || v.mode === "admission_paused";
    const dispatch = v.mode === "paused" || v.mode === "dispatch_paused";
    for (const name of [admission ? "Возобновить приём" : "Приостановить приём", dispatch ? "Возобновить запуск" : "Приостановить запуск"]) {
      assert.equal(await panel.getByRole("button", { name, exact: true }).isDisabled(), true, "Browser renderer must have no mutation endpoint");
    }
  }
  for (const info of report.observations) await render(info);
  await render(report.observations[0]);
  await panel.getByText("Достигнут общий лимит 2.", { exact: true }).waitFor();
  await render(completed.finalWorkingInfo);
  await panel.getByText("Приём и запуск разрешены", { exact: true }).waitFor();
  await render(completed.finalWorkingInfo, false);
  await panel.getByText(/Нет свежей связи/).waitFor();
  await panel.screenshot({ path: join(gate, "installed-queue.png") });
  assert.deepEqual(errors, []);
  const result = { passed: true, provider: completed.provider, reportSha256: ready.sha256,
    checks: ["actual-installed-windows-observations", "separate-pause-labels", "root-count-and-blockers", "restart-same-scope", "working-default-mode", "offline-readonly", "browser-no-mutation"], nativeWindowAutomation: false };
  await writeFile(join(gate, "ui-completed.json"), JSON.stringify(result, null, 2), { flag: "wx", mode: 0o600 });
  console.log(JSON.stringify(result));
} finally {
  if (browser) await browser.close();
  if (server) await server.close();
}
