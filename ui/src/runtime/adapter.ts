import { invoke, isTauri } from "@tauri-apps/api/core";
import type { LocalRequest, LocalResponse } from "./contracts";

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
