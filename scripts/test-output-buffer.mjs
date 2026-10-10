import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import ts from "typescript";

const source = await readFile(new URL("../ui/src/features/tasks/outputBuffer.ts", import.meta.url), "utf8");
const compiled = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const { OutputBuffer, OUTPUT_CHARACTER_LIMIT } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`);
const buffer = new OutputBuffer();
const bytes = Buffer.from("Привет ✓ 🚀");
// Each Unicode code point crosses page boundaries; stderr has its own decoder.
for (let i = 0; i < bytes.length; i++) {
  buffer.append([{ sequence: i * 2 + 1, stream: "stdout", offset: i, bytes: [bytes[i]] }, { sequence: i * 2 + 2, stream: "stderr", offset: i, bytes: [bytes[i]] }]);
}
buffer.finish(); buffer.finish();
assert.equal(buffer.snapshot().stdout, "Привет ✓ 🚀");
assert.equal(buffer.snapshot().stderr, "Привет ✓ 🚀");
assert.equal(buffer.snapshot().loadedBytes, bytes.length * 2);
assert.equal(buffer.snapshot().hasGaps, false);
assert.throws(() => buffer.append([{ sequence: 100, stream: "stdout", offset: bytes.length, bytes: [65] }]), /подтверждённого конца/);
const long = new OutputBuffer();
let offset = 0;
for (let sequence = 1; sequence <= 300; sequence++) {
  const data = Buffer.from("🚀\n".repeat(1000));
  long.append([{ sequence, stream: "stdout", offset, bytes: [...data] }]); offset += data.length;
}
assert.equal(Array.from(long.snapshot().stdout).length, OUTPUT_CHARACTER_LIMIT);
assert.equal(long.snapshot().omittedCharacters.stdout, 600000 - OUTPUT_CHARACTER_LIMIT);
assert.equal(long.snapshot().loadedBytes, offset);
assert.equal(long.snapshot().hasGaps, false);
const gap = new OutputBuffer();
gap.append([{ sequence: 1, stream: "stdout", offset: 0, bytes: [0xe2] }]);
gap.append([{ sequence: 2, stream: "stdout", offset: 2, bytes: [0xac, 65] }]);
gap.finish();
assert.ok(gap.snapshot().hasGaps);
assert.ok(gap.snapshot().stdout.includes("[пропуск вывода]"));
assert.ok(!gap.snapshot().stdout.includes("€"));
const retry = new OutputBuffer();
const before = retry.snapshot();
assert.throws(() => retry.append([{ sequence: 1, stream: "stdout", offset: 0, bytes: [0xe2] }, { sequence: 1, stream: "stderr", offset: 0, bytes: [65] }]));
assert.deepEqual(retry.snapshot(), before, "Malformed page must not move decoder/cursor");
retry.append([{ sequence: 1, stream: "stdout", offset: 0, bytes: [0xe2, 0x82, 0xac] }]);
assert.equal(retry.snapshot().stdout, "€");
retry.append([{ sequence: 3, stream: "stderr", offset: 0, bytes: [65] }]);
assert.ok(retry.snapshot().hasGaps);
console.log("Output buffer verified: split UTF-8, independent streams, bounded Unicode tails, exact byte/cursor counts, gaps, transactional page validation and final flush.");
