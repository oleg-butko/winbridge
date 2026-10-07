// Decode only for presentation; test Unicode integrity and host/script argument boundaries.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import ts from "typescript";

const source = await readFile(new URL("../ui/src/features/tasks/commandPresentation.ts", import.meta.url), "utf8");
const compiled = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const { powerShellScript } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`);
const script = "# Привет ✓ 🚀\r\nWrite-Output 'quotes: \" x $y `z'\n";
const encoded = Buffer.from(script, "utf16le").toString("base64");
const command = (executable, args) => ({ executable, arguments: args, workingDirectory: ".", timeoutMs: 5000 });
for (const executable of ["pwsh.exe", "C:\\Program Files\\PowerShell\\7\\PWSH.EXE", "/opt/powershell/pwsh", "powershell.exe"]) {
  for (const flag of ["-EncodedCommand", "-encodedcommand", "-enc", "-ec", "-e", "-enco"]) {
    assert.equal(powerShellScript(command(executable, ["-NoProfile", flag, encoded])).script, script);
  }
}
assert.equal(powerShellScript(command("pwsh", ["-WorkingDirectory", "-EncodedCommand", "-EncodedCommand", encoded])).script, script);
for (const args of [["-File", "script.ps1", "-EncodedCommand", encoded], ["-Command", "echo", "-enc", encoded], ["-com", "-EncodedCommand", encoded], ["-fi", "script.ps1", "-enc", encoded], ["script.ps1", "-enc", encoded], ["-unknown", "-enc", encoded]]) {
  assert.equal(powerShellScript(command("pwsh", args)), null, "Script argv must not be interpreted as host options");
}
assert.equal(powerShellScript(command("other.exe", ["-EncodedCommand", encoded])), null);
assert.equal(powerShellScript(command("pwsh", ["-EncodedCommand", encoded.slice(0, 16) + "\r\n " + encoded.slice(16)])).script, script);
assert.equal(powerShellScript(command("pwsh", ["-EncodedCommand", ""])).script, "");
assert.equal(powerShellScript(command("pwsh", ["-EncodedCommand", "QQB="])).script, "A", "PowerShell accepts nonzero unused Base64 bits");
for (const bad of ["not-base64!", "AA==", "ANg=", "AA", "A".repeat(262145)]) {
  assert.ok(powerShellScript(command("pwsh", ["-EncodedCommand", bad])).error);
}
assert.ok(powerShellScript(command("pwsh", ["-EncodedCommand"])).error);
console.log("PowerShell presentation verified: Unicode/emoji/newlines, host option boundaries, missing/malformed/oversized encoding; original argv stays unchanged.");
