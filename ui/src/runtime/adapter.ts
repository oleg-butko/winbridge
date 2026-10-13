import { invoke, isTauri } from "@tauri-apps/api/core";
import type { FencedResponse, LocalRequest, LocalResponse, RequestContext } from "./contracts";
import { bindWorkerRequest, requestEnvelope, scopedResult } from "./requestContext";
import type { BoundWorkerRequest } from "./requestContext";

export const nativeRuntime = isTauri();
export async function workerMutation(request: LocalRequest, context?: RequestContext): Promise<LocalResponse> {
  return workerBoundCall(bindWorkerRequest(request, context));
}
export async function workerBoundCall(bound: BoundWorkerRequest): Promise<LocalResponse> {
  if (!nativeRuntime) throw new Error("Запуск команд доступен в Windows desktop с подключённым worker");
  const envelope = requestEnvelope(bound);
  if (!("requestContractVersion" in envelope)) return workerCall(envelope);
  return scopedResult(await invoke<FencedResponse>("worker_rpc_fenced", { request: envelope }), envelope.context);
}
export async function workerCall(request: LocalRequest): Promise<LocalResponse> {
  if (!nativeRuntime) throw new Error("Запуск команд доступен в Windows desktop с подключённым worker");
  return invoke<LocalResponse>("worker_rpc", { request });
}
export function expectResult<T extends LocalResponse["kind"]>(response: LocalResponse, kind: T): Extract<LocalResponse, { kind: T }> {
  if (response.kind === "error") throw new Error(response.message);
  if (response.kind !== kind) throw new Error(`Несовместимый ответ worker: ${response.kind}`);
  return response as Extract<LocalResponse, { kind: T }>;
}
