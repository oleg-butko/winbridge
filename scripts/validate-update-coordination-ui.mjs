// Production readonly renderer with actual isolated worker IPC observations.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { resolve, join } from "node:path";
import { createServer } from "vite";
import { chromium } from "playwright";

assert.ok(process.argv[2], "Usage: node scripts/validate-update-coordination-ui.mjs PROOF_JSON [CHECKSUMMED_NATIVE_REPORT_DIRECTORY]");
const data = await readFile(resolve(process.argv[2]));
if (process.argv[3]) {
  const ready = JSON.parse((await readFile(join(resolve(process.argv[3]), "report.ready.json"), "utf8")).replace(/^\uFEFF/, ""));
  assert.equal(ready.platform, "windows"); assert.equal(ready.passed, true);
  const item = ready.files.find((f) => f.path === "update-coordination-proof.json");
  assert.ok(item); assert.equal(item.bytes, data.length); assert.equal(item.sha256, createHash("sha256").update(data).digest("hex"));
}
const proof = JSON.parse(data.toString("utf8").replace(/^\uFEFF/, ""));
assert.equal(proof.passed, true); assert.ok(["linux", "windows"].includes(proof.platform)); assert.equal(proof.observations.length, 6);
const evidence = resolve(".winbridge/update-preparation", `coordination-ui-${proof.platform}-${Date.now()}`);
await mkdir(evidence, { recursive: true });
const modulePath = "/__update_summary.tsx";
const source = `import React from 'react';import {createRoot} from 'react-dom/client';import {MantineProvider} from '@mantine/core';import '@mantine/core/styles.css';import {UpdateCoordinationSummary} from '/src/features/tasks/UpdateCoordinationSummary.tsx';const root=createRoot(document.getElementById('root'));window.__renderUpdate=(info,connected)=>root.render(React.createElement(MantineProvider,null,React.createElement(UpdateCoordinationSummary,{info,connected})));`;
let server; let browser;
try {
  server = await createServer({ plugins: [{ name: "update-summary", enforce: "pre",
    resolveId(id) { if (id === modulePath) return modulePath; }, load(id) { if (id === modulePath) return source; },
    configureServer(instance) { instance.middlewares.use(async (request, response, next) => {
      if (request.url !== "/__update_summary") return next();
      try { const html = await instance.transformIndexHtml(request.url, `<html lang="ru"><head><meta charset="UTF-8"></head><body><div id="root"></div><script type="module" src="${modulePath}"></script></body></html>`); response.setHeader("Content-Type", "text/html; charset=utf-8"); response.end(html); } catch (error) { next(error); }
    }); },
  }], server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  browser = await chromium.launch({ executablePath: process.env.WINBRIDGE_BROWSER ?? (existsSync("/usr/bin/chromium") ? "/usr/bin/chromium" : undefined), chromiumSandbox: true });
  const page = await browser.newPage({ viewport: { width: 1250, height: 1000 } });
  const errors = []; page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/__update_summary`);
  await page.waitForFunction(() => typeof window.__renderUpdate === "function");
  const panel = page.getByTestId("update-coordination");
  const labels = ["Ожидание исходников", "Исходники заняты обновлением", "Ожидание завершения работы", "Проверка остановки исполнителей", "Worker готов к передаче управления"];
  for (const [index, info] of proof.observations.slice(0, 5).entries()) {
    const lease = info.updateCoordination.lease;
    await page.evaluate((info) => window.__renderUpdate(info, true), info);
    await panel.getByText(labels[index], { exact: true }).waitFor();
    await panel.getByText(`Исходники: ${lease.spec.sourceDirectory}`, { exact: true }).waitFor();
    await panel.getByText(new RegExp(lease.spec.updateId)).waitFor();
    assert.equal(await panel.getByRole("button").count(), 0);
  }
  await panel.getByText(/Установка новой версии ещё не подтверждена/).waitFor();
  await page.evaluate((info) => window.__renderUpdate(info, false), proof.observations[4]);
  await panel.getByText(/Нет свежей связи/).waitFor();
  await panel.screenshot({ path: join(evidence, "coordination-summary.png") });
  await page.evaluate((info) => window.__renderUpdate(info, true), proof.observations[5]);
  await panel.waitFor({ state: "detached" });
  // Synthetic compatibility guards use a real observation as their base.
  for (const change of [
    (v) => { v.updateCoordination.schemaVersion = 2; },
    (v) => { v.updateCoordination.lease.spec.journalIncarnation = "00000000-0000-0000-0000-000000000000"; },
    (v) => { v.updateCoordination.lease.spec.workerGeneration = "00000000-0000-0000-0000-000000000000"; },
    (v) => { v.updateCoordination.lease.revision = -1; },
    (v) => { v.updateCoordination.lease.phase = "installed"; },
    (v) => { v.updateCoordination.install = true; },
  ]) {
    const info = structuredClone(proof.observations[0]); change(info);
    await page.evaluate((info) => window.__renderUpdate(info, true), info);
    await panel.getByText("Состояние обновления не распознано", { exact: true }).waitFor();
  }
  const readonly = structuredClone(proof.observations[0]); readonly.updateCoordination.readOnly = true;
  readonly.generation = "00000000-0000-0000-0000-000000000000";
  await page.evaluate((info) => window.__renderUpdate(info, true), readonly);
  await panel.getByText(/Журнал только для чтения/).waitFor();
  assert.deepEqual(errors, []); assert.equal(await panel.getByRole("button").count(), 0);
  const result = { passed: true, platform: proof.platform, provider: proof.observations[0].build,
    checks: ["five-actual-ipc-phases", "source-project-scope", "quiesced-is-not-installed", "released-metadata-disappears", "offline-stale-state", "unknown-version-and-scope-refusal", "readonly-projection", "no-mutation-controls"], nativeWindowAutomation: false, evidence };
  await writeFile(join(evidence, "result.json"), JSON.stringify(result, null, 2), { flag: "wx", mode: 0o600 });
  console.log(JSON.stringify(result));
} finally { if (browser) await browser.close(); if (server) await server.close(); }
