import type { ResolvedCommand } from "../../runtime/contracts";

export type PowerShellScript = { script: string; argumentIndex: number } | { error: string };

// Presentation only: these bytes never replace the accepted argv or execute a script.
export function powerShellScript(command: ResolvedCommand): PowerShellScript | null {
  const name = command.executable.split(/[\\/]/).at(-1)?.toLowerCase();
  if (!["pwsh", "pwsh.exe", "powershell", "powershell.exe"].includes(name ?? "")) return null;
  const args = command.arguments;
  for (let i = 0; i < args.length; i += 1) {
    const option = args[i].toLowerCase();
    // Everything after Command/File belongs to the script, not to the host CLI.
    if (option === "--" || (option.length >= 2 && ["-command", "-commandwithargs", "-file"].some((full) => full.startsWith(option))) || option === "-cwa") return null;
    if (["-workingdirectory", "-wd", "-executionpolicy", "-ep", "-inputformat", "-inp", "-outputformat", "-o", "-windowstyle", "-w", "-configurationname", "-config", "-settingsfile", "-version", "-v"].includes(option)) { i += 1; continue; }
    if (!(option === "-ec" || (option.length >= 2 && "-encodedcommand".startsWith(option)))) {
      // Unknown options may consume a value or select an implicit script file.
      // Do not mistake its argv for an encoded host command.
      if (!["-noprofile", "-nop", "-nologo", "-nol", "-noninteractive", "-noni", "-noexit", "-noe", "-sta", "-mta", "-login", "-l"].includes(option)) return null;
      continue;
    }
    const encoded = args[i + 1];
    if (encoded === undefined) return { error: "После EncodedCommand отсутствует сценарий Base64." };
    if (encoded.length > 262144) return { error: "Сценарий слишком большой для декодирования в карточке (лимит 256 KiB Base64). Исходные аргументы доступны ниже." };
    try {
      const compact = encoded.replace(/[\t\n\r ]/g, "");
      // Reject forgiving browser decoding (bad padding/trailing bits) rather than invent text.
      const binary = atob(compact);
      if (btoa(binary) !== compact || binary.length % 2 !== 0) throw new Error("invalid encoding");
      const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
      const script = new TextDecoder("utf-16le", { fatal: true, ignoreBOM: true }).decode(bytes);
      return { script, argumentIndex: i + 1 };
    } catch {
      return { error: "Не удалось декодировать EncodedCommand: ожидается корректный Base64 со сценарием UTF-16LE. Точные аргументы сохранены ниже." };
    }
  }
  return null;
}
