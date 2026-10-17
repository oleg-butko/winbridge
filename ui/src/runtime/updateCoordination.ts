import type { WorkerInfo, WorkerUpdateLease } from "./contracts";

export type UpdateCoordinationView =
  | { kind: "missing" }
  | { kind: "unsupported" }
  | { kind: "known"; lease: WorkerUpdateLease; readOnly: boolean; observedAtMs: number };
const object = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const keys = (v: Record<string, unknown>, allowed: string[]) => Object.keys(v).every((k) => allowed.includes(k));
const uuid = (v: unknown): v is string => typeof v === "string" && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(v);
const count = (v: unknown): v is number => typeof v === "number" && Number.isSafeInteger(v) && v >= 0;
const id = (v: unknown): v is string => typeof v === "string" && /^[A-Za-z0-9_-]{1,80}$/.test(v);
const hash = (v: unknown) => typeof v === "string" && /^[0-9a-f]{64}$/.test(v);

// Optional readonly metadata. It grants no source/install authority or controls.
export function interpretUpdateCoordination(info: WorkerInfo): UpdateCoordinationView {
  const v = info.updateCoordination;
  if (v == null) return { kind: "missing" };
  const bad: UpdateCoordinationView = { kind: "unsupported" };
  if (!object(v) || !keys(v, ["schemaVersion", "lease", "readOnly", "observedAtMs"]) || v.schemaVersion !== 1
    || typeof v.readOnly !== "boolean" || !count(v.observedAtMs) || !object(v.lease)) return bad;
  const l = v.lease;
  if (!keys(l, ["spec", "revision", "phase", "acceptedAtMs", "updatedAtMs", "reason"]) || !object(l.spec)
    || !count(l.revision) || !["source_pending", "source_held", "draining", "quiescing", "quiesced", "released"].includes(l.phase as string)
    || !count(l.acceptedAtMs) || !count(l.updatedAtMs) || l.acceptedAtMs > l.updatedAtMs || l.updatedAtMs > v.observedAtMs
    || typeof l.reason !== "string" || !l.reason || l.reason.length > 4096) return bad;
  const s = l.spec;
  if (!keys(s, ["updateId", "instanceId", "journalIncarnation", "workerGeneration", "updaterOwnerId", "updaterEpoch", "sourceDirectory", "baselineManifestSha256", "targetManifestSha256", "projects"])
    || !uuid(s.updateId) || !uuid(s.journalIncarnation) || !uuid(s.workerGeneration) || !uuid(s.updaterOwnerId)
    || !id(s.instanceId) || !count(s.updaterEpoch) || s.updaterEpoch < 1
    || !hash(s.baselineManifestSha256) || !hash(s.targetManifestSha256)
    || typeof s.sourceDirectory !== "string" || !s.sourceDirectory || s.sourceDirectory.length > 4096
    || !Array.isArray(s.projects) || s.projects.length > 128 || s.projects.some((p, i, a) => !id(p) || (i > 0 && a[i - 1] >= p))
    || s.instanceId !== info.instanceId || s.journalIncarnation !== info.fencedRequests?.context.journalIncarnation
    || s.instanceId !== info.fencedRequests?.context.instanceId || (!v.readOnly && s.workerGeneration !== info.generation)) return bad;
  return { kind: "known", lease: l as unknown as WorkerUpdateLease, readOnly: v.readOnly, observedAtMs: v.observedAtMs };
}
