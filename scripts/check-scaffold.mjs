import assert from "node:assert/strict";
import { readFileSync, existsSync } from "node:fs";
import { dirname, isAbsolute, resolve, relative } from "node:path";
import { spawnSync } from "node:child_process";

const root = resolve(import.meta.dirname, "..");
const listing = spawnSync("git", ["ls-files", "-z", "--cached", "--others", "--exclude-standard"], { cwd: root, encoding: "utf8" });
if (listing.error) throw listing.error;
if (listing.status !== 0) throw new Error(listing.stderr || "git file listing failed");
const files = [...new Set(listing.stdout.split("\0").filter(Boolean))];
let markdownCount = 0;
for (const file of files.filter((name) => name.endsWith(".md"))) {
  markdownCount += 1;
  const path = resolve(root, file);
  const markdown = readFileSync(path, "utf8");
  for (const match of markdown.matchAll(/\[[^\]]+\]\(([^)]+)\)/g)) {
    const target = match[1].split("#", 1)[0];
    if (!target || /^[a-z][a-z0-9+.-]*:/i.test(target)) continue;
    assert(!isAbsolute(target), `${file}: local links must be repository-relative`);
    const linked = resolve(dirname(path), target);
    assert(!relative(root, linked).startsWith(".."), `${file}: link escapes the repository: ${target}`);
    assert(existsSync(linked), `${file}: missing link ${target}`);
  }
}
const json = (file) => JSON.parse(readFileSync(resolve(root, file), "utf8"));
const controller = json("examples/controller.config.json");
const worker = json("examples/worker.config.json");
assert.equal(controller.schemaVersion, 1);
assert.equal(worker.schemaVersion, 1);
assert.equal(controller.delivery.adapter, "shared-folder");
assert.equal(worker.delivery.adapter, "shared-folder");
assert.equal(controller.delivery.namespace, worker.delivery.namespace);
assert.equal(new URL(worker.worker.controllerUrl).protocol, "ws:");
assert(worker.projects.every((project) => project.allowDirectCommands === true));
const controllerProjects = new Set(controller.projects.map((project) => project.id));
const workerProjects = new Set(worker.projects.map((project) => project.id));
for (const name of ["task.direct.json", "task.recipe.json"]) {
  const request = json(`examples/${name}`);
  assert(controllerProjects.has(request.projectId) && workerProjects.has(request.projectId));
}
const normalizedWindowsPath = (path) => path.replace(/[\\/]+/g, "\\").replace(/\\$/, "").toLowerCase();
const shared = normalizedWindowsPath(worker.delivery.root);
for (const project of worker.projects) {
  const workspace = normalizedWindowsPath(project.workspaceRoot);
  assert(workspace !== shared && !workspace.startsWith(`${shared}\\`), "build workspace must be outside shared delivery folder");
}
const cargo = readFileSync(resolve(root, "Cargo.toml"), "utf8");
const version = cargo.match(/\[workspace\.package\]\s*\nversion\s*=\s*"([^"]+)"/)?.[1];
assert.equal(json("package.json").version, version);
console.log(`Scaffold checked: ${markdownCount} Markdown files, local links, config/task examples and application version.`);
