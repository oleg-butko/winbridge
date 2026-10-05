import { invoke, isTauri } from "@tauri-apps/api/core";
import type { LocalRequest, LocalResponse, OutputChunk } from "./contracts";

export const nativeRuntime = isTauri();
export async function workerCall(request: LocalRequest): Promise<LocalResponse> {
  if (!nativeRuntime) throw new Error("Запуск команд доступен в Windows desktop с подключённым worker");
  return invoke<LocalResponse>("worker_rpc", { request });
}
export function expectResult<T extends LocalResponse["kind"]>(response: LocalResponse, kind: T): Extract<LocalResponse, { kind: T }> {
  if (response.kind === "error") throw new Error(response.message);
  if (response.kind !== kind) throw new Error(`Несовместимый ответ worker: ${response.kind}`);
  return response as Extract<LocalResponse, { kind: T }>;
}
export function decodeOutput(chunks: OutputChunk[], final: boolean): { stdout: string; stderr: string } {
  const decoder = { stdout: new TextDecoder(), stderr: new TextDecoder() };
  const output = { stdout: "", stderr: "" };
  for (const chunk of chunks) output[chunk.stream] += decoder[chunk.stream].decode(new Uint8Array(chunk.bytes), { stream: true });
  if (final) for (const stream of ["stdout", "stderr"] as const) output[stream] += decoder[stream].decode();
  return output;
}
