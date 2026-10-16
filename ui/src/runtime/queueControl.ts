import type { QueueControlRequest, QueueControlResponse, WorkerInfo } from "./contracts";
import { interpretQueueControl } from "./discovery";
export type QueueChange = QueueControlRequest & { command: Extract<QueueControlRequest["command"], { kind: "set" }> };
const object = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const keys = (v: Record<string, unknown>, allowed: string[]) => Object.keys(v).every((k) => allowed.includes(k));
export const queueRevision = (v: unknown): v is string => typeof v === "string" && /^(0|[1-9][0-9]*)$/.test(v) && v.length <= 19 && BigInt(v) <= 9223372036854775807n;
const modes = ["running", "admission_paused", "dispatch_paused", "paused"];
export function readQueueChange(raw: unknown): QueueChange | null {
  if (!object(raw) || !keys(raw, ["queueControlVersion", "context", "command"]) || raw.queueControlVersion !== 1 || !object(raw.context)
    || !keys(raw.context, ["instanceId", "journalIncarnation"]) || typeof raw.context.instanceId !== "string" || typeof raw.context.journalIncarnation !== "string"
    || !object(raw.command) || !keys(raw.command, ["kind", "commandId", "expectedRevision", "mode"]) || raw.command.kind !== "set"
    || typeof raw.command.commandId !== "string" || !/^[A-Za-z0-9_-]{1,128}$/.test(raw.command.commandId) || !queueRevision(raw.command.expectedRevision)
    || !modes.includes(raw.command.mode as string)) return null;
  return raw as unknown as QueueChange;
}
export function validateQueueResponse(reply: QueueControlResponse, request: QueueControlRequest, info: WorkerInfo): void {
  const invalid = () => new Error("Ответ очереди не подтверждён: сохраните исходные command ID, область и ревизию.");
  const sameScope = (a: { instanceId: string; journalIncarnation: string } | null) => a?.instanceId === request.context.instanceId && a.journalIncarnation === request.context.journalIncarnation;
  if (reply.queueControlVersion !== 1 || (reply.context !== null && !sameScope(reply.context))) throw invalid();
  if (reply.result.kind === "unconfirmed" || reply.result.kind === "rejected") return;
  if ((reply.result.kind === "applied") !== (request.command.kind === "set") || !sameScope(reply.context)) throw invalid();
  const observation = reply.result.observation;
  if (!sameScope(observation.context) || !interpretQueueControl({ ...info, contractCatalog: { ...(info.contractCatalog as object), queueControl: observation } })) throw invalid();
  const receipt = reply.result.receipt;
  if (!receipt) return;
  if (!sameScope(receipt.context) || !queueRevision(receipt.fromRevision) || !queueRevision(receipt.toRevision) || typeof receipt.changed !== "boolean"
    || !Number.isSafeInteger(receipt.changedAtMs) || receipt.changedAtMs < 0 || !modes.includes(receipt.mode)
    || BigInt(receipt.toRevision) > BigInt(observation.revision)
    || BigInt(receipt.toRevision) !== BigInt(receipt.fromRevision) + (receipt.changed ? 1n : 0n)) throw invalid();
  if (request.command.kind === "set") {
    if (receipt.commandId !== request.command.commandId || receipt.fromRevision !== request.command.expectedRevision || receipt.mode !== request.command.mode) throw invalid();
  } else if (request.command.commandId !== receipt.commandId) throw invalid();
}
