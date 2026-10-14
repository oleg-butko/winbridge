import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import ts from "typescript";

const source = await readFile(new URL("../ui/src/runtime/discovery.ts", import.meta.url), "utf8");
const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const { interpretCatalog, catalogSummary } = await import(`data:text/javascript;base64,${Buffer.from(code).toString("base64")}`);
const fixture = JSON.parse(await readFile(new URL("../tests/fixtures/discovery/catalog-cases.json", import.meta.url), "utf8"));
for (const item of fixture.cases) {
  const info = { build: item.workerBuild, contractCatalog: item.catalog };
  assert.equal(interpretCatalog(info).kind, item.expected, item.name);
  if (item.expected === "known") assert.match(catalogSummary(info), /не подтверждение проверки пары/);
}
console.log(`Discovery: ${fixture.cases.length} shared Rust/TypeScript cases; unsupported metadata grants no new action`);
