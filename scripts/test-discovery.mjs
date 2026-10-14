import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import ts from "typescript";

const source = await readFile(new URL("../ui/src/runtime/discovery.ts", import.meta.url), "utf8");
const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const { interpretCatalog, catalogSummary, interpretProjectPolicies, projectPolicySummary, interpretControllerProjection } = await import(`data:text/javascript;base64,${Buffer.from(code).toString("base64")}`);
const fixture = JSON.parse(await readFile(new URL("../tests/fixtures/discovery/catalog-cases.json", import.meta.url), "utf8"));
for (const item of fixture.cases) {
  const info = { build: item.workerBuild, contractCatalog: item.catalog };
  assert.equal(interpretCatalog(info).kind, item.expected, item.name);
  if (item.expected === "known") assert.match(catalogSummary(info), /не подтверждение проверки пары/);
}
console.log(`Discovery: ${fixture.cases.length} shared Rust/TypeScript cases; unsupported metadata grants no new action`);

const policyFixture = JSON.parse(await readFile(new URL("../tests/fixtures/readiness/policy-cases.json", import.meta.url), "utf8"));
for (const item of policyFixture.cases) {
  const info = { build: item.workerBuild, projectPolicies: item.policies, projects: item.projects };
  assert.equal(interpretProjectPolicies(info) !== null, item.known, item.name);
  assert.match(projectPolicySummary(info), item.known ? /отпечатки действующих настроек/ : /не подтверждена/);
}
console.log(`Project policies: ${policyFixture.cases.length} shared Rust/TypeScript cases`);
const projectionFixture = JSON.parse(await readFile(new URL("../tests/fixtures/readiness/projection-cases.json", import.meta.url), "utf8"));
for (const item of projectionFixture.cases) {
  const info = structuredClone(projectionFixture.baseInfo);
  if (item.pointer) {
    const fields = item.pointer.slice(1).split("/");
    const key = fields.pop();
    const parent = fields.reduce((value, field) => value[field], info);
    if (item.remove) delete parent[key];
    else parent[key] = item.repeat ? Array.from({ length: item.repeat }, () => structuredClone(parent[key])).flat() : item.value;
  }
  assert.equal(interpretControllerProjection(info).kind, item.expected, item.name);
}
console.log(`Controller projection: ${projectionFixture.cases.length} shared Rust/TypeScript cases`);
