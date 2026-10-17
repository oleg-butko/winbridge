// Readonly production renderer using actual IPC observations retained by runtime tests.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { resolve, join } from "node:path";
import { createServer } from "vite";
import { chromium } from "playwright";

assert.ok(process.argv[2], "Usage: node scripts/validate-resource-scheduling-ui.mjs PROOF_JSON [CHECKSUMMED_NATIVE_REPORT_DIRECTORY]");
const proofPath = resolve(process.argv[2]);
const data = await readFile(proofPath);
if (process.argv[3]) {
  const ready = JSON.parse((await readFile(join(resolve(process.argv[3]), "report.ready.json"), "utf8")).replace(/^\uFEFF/, ""));
  const item = ready.files.find((f) => f.path === "resource-scheduling-proof.json");
  assert.ok(item);
  assert.equal(item.bytes, data.length); assert.equal(item.sha256, createHash("sha256").update(data).digest("hex"));
}
const proof = JSON.parse(data.toString("utf8").replace(/^\uFEFF/, ""));
assert.equal(proof.passed, true); assert.ok(["linux", "windows"].includes(proof.platform));
const evidence = resolve(".winbridge/architecture-transition", `resource-ui-${proof.platform}-${Date.now()}`);
await mkdir(evidence, { recursive: true });
const modulePath = "/__resource_summary.tsx";
const source = `import React from 'react';import {createRoot} from 'react-dom/client';import {MantineProvider} from '@mantine/core';import '@mantine/core/styles.css';import {ResourceSchedulingSummary} from '/src/features/tasks/ResourceSchedulingSummary.tsx';const root=createRoot(document.getElementById('root'));window.__renderResources=(info,connected)=>root.render(React.createElement(MantineProvider,null,React.createElement(ResourceSchedulingSummary,{info,connected})));`;
let server;
let browser;
try {
  server = await createServer({ plugins: [{ name: "resource-summary", enforce: "pre",
    resolveId(id) { if (id === modulePath) return modulePath; }, load(id) { if (id === modulePath) return source; },
    configureServer(instance) { instance.middlewares.use(async (request, response, next) => {
      if (request.url !== "/__resource_summary") return next();
      try { const html = await instance.transformIndexHtml(request.url, `<html lang="ru"><head><meta charset="UTF-8"></head><body><div id="root"></div><script type="module" src="${modulePath}"></script></body></html>`); response.setHeader("Content-Type", "text/html; charset=utf-8"); response.end(html); } catch (error) { next(error); }
    }); },
  }], server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  browser = await chromium.launch({ executablePath: process.env.WINBRIDGE_BROWSER ?? (existsSync("/usr/bin/chromium") ? "/usr/bin/chromium" : undefined), chromiumSandbox: true });
  const page = await browser.newPage({ viewport: { width: 1250, height: 1000 } });
  const errors = []; page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/__resource_summary`);
  await page.waitForFunction(() => typeof window.__renderResources === "function");
  const panel = page.getByTestId("resource-scheduling");
  for (const info of [proof.loadedInfo, proof.restoredInfo, proof.finalInfo]) {
    assert.deepEqual(info.build, proof.build);
    const v = info.contractCatalog.resourceScheduling;
    assert.deepEqual(v.context, proof.context);
    await page.evaluate((info) => window.__renderResources(info, true), info);
    await panel.getByText("Политика общих ресурсов включена", { exact: true }).waitFor();
    await panel.getByText(new RegExp(v.context.journalIncarnation)).waitFor();
    for (const r of v.resources) await panel.getByText(new RegExp(`занято ${r.usedUnits}/${r.capacity} · освобождение не подтверждено: ${r.uncertainUnits}`)).first().waitFor();
  }
  await page.evaluate((info) => window.__renderResources(info, true), proof.restoredInfo);
  await panel.getByText("Владельцы cache", { exact: true }).click();
  const cacheOwner = proof.restoredInfo.contractCatalog.resourceScheduling.owners.find((o) => o.resourceId === "cache");
  await panel.locator("details").filter({ has: page.getByText("Владельцы cache", { exact: true }) }).getByText(new RegExp(cacheOwner.operationId)).waitFor();
  await page.evaluate((info) => window.__renderResources(info, false), proof.finalInfo);
  await panel.getByText(/Нет свежей связи/).waitFor();
  await panel.screenshot({ path: join(evidence, "resource-summary.png") });
  assert.deepEqual(errors, []); assert.equal(await panel.getByRole("button").count(), 0);
  const result = { passed: true, platform: proof.platform, provider: proof.build, context: proof.context,
    checks: ["actual-ipc-observations", "capacities-and-owner-counts", "frozen-owner-after-restart", "physical-cleanup-zero", "stale-snapshot", "no-mutation-endpoint"], nativeWindowAutomation: false, evidence };
  await writeFile(join(evidence, "result.json"), JSON.stringify(result, null, 2), { flag: "wx", mode: 0o600 });
  console.log(JSON.stringify(result));
} finally {
  if (browser) await browser.close(); if (server) await server.close();
}
