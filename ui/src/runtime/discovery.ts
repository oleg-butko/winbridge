import type { BuildIdentity, ContractCatalog, ContractRef, ControllerReadinessProjection, ProjectPolicyCatalog, WorkerInfo, WorkSchedulingObservation, QueueControlObservation, QueueBlocker, ResourceSchedulingObservation } from "./contracts";

export type CatalogObservation =
  | { kind: "missing" }
  | { kind: "unsupported"; reason: string }
  | { kind: "known"; catalog: ContractCatalog };

const object = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const keys = (value: Record<string, unknown>, allowed: string[]) =>
  Object.keys(value).every((key) => allowed.includes(key));
function optionalString(value: unknown): value is string | null | undefined {
  return value === null || value === undefined || typeof value === "string";
}
function readBuild(value: unknown): BuildIdentity | null {
  if (!object(value) || typeof value.version !== "string" || typeof value.buildId !== "string"
    || typeof value.sourceDirty !== "boolean" || !optionalString(value.sourceCommit)
    || !optionalString(value.sourceManifestSha256)) return null;
  // Rust Option<String> reads an omitted field as None. Normalize both sides
  // identically before comparing build identities or returning typed metadata.
  return { version: value.version, buildId: value.buildId, sourceDirty: value.sourceDirty,
    sourceCommit: value.sourceCommit ?? null, sourceManifestSha256: value.sourceManifestSha256 ?? null };
}
function contract(value: unknown): value is ContractRef {
  return object(value) && keys(value, ["id", "version"]) && typeof value.id === "string"
    && value.id.length >= 1 && value.id.length <= 128 && !/[^A-Za-z0-9._/-]/.test(value.id)
    && typeof value.version === "number" && Number.isInteger(value.version) && value.version >= 1 && value.version <= 65535;
}

// Advertisement is information about a build, never a permission to execute.
export function interpretCatalog(info: Pick<WorkerInfo, "contractCatalog" | "build">): CatalogObservation {
  const raw = info.contractCatalog;
  if (raw === undefined) return { kind: "missing" };
  const invalid = (reason: string): CatalogObservation => ({ kind: "unsupported", reason });
  if (!object(raw) || !keys(raw, ["schemaVersion", "provider", "supported", "controllerProjection", "workScheduling", "queueControl", "resourceScheduling"]) || raw.schemaVersion !== 1
    || !Array.isArray(raw.supported)) return invalid("Формат каталога неизвестен. Новые функции не включены.");
  const provider = readBuild(raw.provider);
  const expected = readBuild(info.build);
  if (!provider || !expected || !provider.buildId || (["version", "sourceCommit", "sourceDirty", "sourceManifestSha256", "buildId"] as const)
    .some((key) => expected[key] !== provider[key])) return invalid("Каталог относится к другой сборке worker.");
  const seen = new Set<string>();
  if (raw.supported.length > 64 || raw.supported.some((entry) => {
    if (!contract(entry)) return true;
    const id = `${entry.id}@${entry.version}`;
    if (seen.has(id)) return true;
    seen.add(id);
    return false;
  })) return invalid("В каталоге некорректные или повторяющиеся контракты.");
  return { kind: "known", catalog: { schemaVersion: 1, provider, supported: raw.supported as ContractRef[], ...(Object.hasOwn(raw, "controllerProjection") ? { controllerProjection: raw.controllerProjection } : {}), ...(Object.hasOwn(raw, "workScheduling") ? { workScheduling: raw.workScheduling } : {}), ...(Object.hasOwn(raw, "queueControl") ? { queueControl: raw.queueControl } : {}), ...(Object.hasOwn(raw, "resourceScheduling") ? { resourceScheduling: raw.resourceScheduling } : {}) } };
}

export function interpretResourceScheduling(info: WorkerInfo): ResourceSchedulingObservation | null {
  const catalog = interpretCatalog(info);
  if (catalog.kind !== "known" || !catalog.catalog.supported.some((c) => c.id === "winbridge.resource-scheduling" && c.version === 1)) return null;
  const v = catalog.catalog.resourceScheduling;
  const count = (n: unknown): n is number => typeof n === "number" && Number.isInteger(n) && n >= 0 && n <= 0xffffffff;
  if (!object(v) || !keys(v, ["schemaVersion", "provider", "context", "policyGeneration", "enabled", "resources", "owners", "truncated", "observedAtMs", "readOnly"])
    || v.schemaVersion !== 1 || !sameBuild(v.provider, info.build) || !sameScope(v.context, info.fencedRequests?.context)
    || typeof v.policyGeneration !== "string" || !/^(0|[1-9][0-9]*)$/.test(v.policyGeneration) || v.policyGeneration.length > 19 || BigInt(v.policyGeneration) > 9223372036854775807n
    || typeof v.enabled !== "boolean" || typeof v.truncated !== "boolean" || typeof v.readOnly !== "boolean"
    || !Number.isSafeInteger(v.observedAtMs) || (v.observedAtMs as number) < 0 || !Array.isArray(v.resources) || v.resources.length > 128 || !Array.isArray(v.owners) || v.owners.length > 256) return null;
  const ids = new Set<string>();
  for (const r of v.resources) {
    if (!object(r) || !keys(r, ["id", "kind", "capacity", "usedUnits", "uncertainUnits"]) || typeof r.id !== "string" || !r.id || ids.has(r.id)
      || !["build", "cache", "binary", "ipc", "port", "interactive"].includes(r.kind as string) || !count(r.usedUnits) || !count(r.uncertainUnits) || r.uncertainUnits > r.usedUnits
      || (r.capacity != null && (!count(r.capacity) || r.capacity < 1 || r.capacity > 64 || (r.kind !== "build" && r.capacity !== 1)))) return null;
    ids.add(r.id);
  }
  const owners = new Set<string>();
  const totals = new Map<string, [number, number]>();
  for (const o of v.owners) {
    if (!object(o) || !keys(o, ["resourceId", "leaseId", "projectId", "operationKind", "operationId", "units", "state", "applicationLifetime"])
      || [o.resourceId, o.leaseId, o.projectId, o.operationId].some((s) => typeof s !== "string" || !s)
      || !count(o.units) || o.units < 1 || o.units > 64 || !["task", "import"].includes(o.operationKind as string)
      || !["held", "uncertain"].includes(o.state as string) || typeof o.applicationLifetime !== "boolean"
      || owners.has(JSON.stringify([o.leaseId, o.resourceId])) || (!v.truncated && !ids.has(o.resourceId as string))) return null;
    if (o.applicationLifetime && v.resources.some((r) => r.id === o.resourceId && ["build", "cache"].includes(r.kind))) return null;
    owners.add(JSON.stringify([o.leaseId, o.resourceId]));
    const total = totals.get(o.resourceId as string) ?? [0, 0];
    total[0] += o.units; if (o.state === "uncertain") total[1] += o.units;
    totals.set(o.resourceId as string, total);
  }
  for (const r of v.resources) {
    const total = totals.get(r.id) ?? [0, 0];
    if ((!v.enabled && r.capacity != null) || total[0] > r.usedUnits || total[1] > r.uncertainUnits
      || (!v.truncated && (total[0] !== r.usedUnits || total[1] !== r.uncertainUnits))) return null;
  }
  return { ...v, provider: readBuild(v.provider)!, resources: v.resources.map((r) => ({ ...r, capacity: r.capacity ?? null })) } as unknown as ResourceSchedulingObservation;
}

export function interpretWorkScheduling(info: WorkerInfo): WorkSchedulingObservation | null {
  const catalog = interpretCatalog(info);
  if (catalog.kind !== "known" || !catalog.catalog.supported.some((ref) => ref.id === "winbridge.work-scheduling" && ref.version === 1)) return null;
  const value = catalog.catalog.workScheduling;
  if (!object(value) || !keys(value, ["schemaVersion", "provider", "context", "maxConcurrentWork", "occupiedSlots", "uncertainSlots", "queuedOperations", "observedAtMs", "readOnly"])
    || value.schemaVersion !== 1 || !sameBuild(value.provider, info.build) || !sameScope(value.context, info.fencedRequests?.context)
    || typeof value.readOnly !== "boolean" || ![value.occupiedSlots, value.uncertainSlots, value.queuedOperations].every((n) => typeof n === "number" && Number.isInteger(n) && n >= 0 && n <= 0xffffffff)
    || (value.uncertainSlots as number) > (value.occupiedSlots as number)
    || typeof value.observedAtMs !== "number" || !Number.isSafeInteger(value.observedAtMs) || value.observedAtMs < 0
    || (value.maxConcurrentWork != null && (typeof value.maxConcurrentWork !== "number" || !Number.isInteger(value.maxConcurrentWork) || value.maxConcurrentWork < 1 || value.maxConcurrentWork > 32))) return null;
  return { ...value, provider: readBuild(value.provider)!, maxConcurrentWork: value.maxConcurrentWork ?? null } as unknown as WorkSchedulingObservation;
}

export function interpretQueueControl(info: WorkerInfo): QueueControlObservation | null {
  const catalog = interpretCatalog(info);
  if (catalog.kind !== "known" || !catalog.catalog.supported.some((c) => c.id === "winbridge.queue-control" && c.version === 1)) return null;
  const value = catalog.catalog.queueControl;
  const count = (n: unknown): n is number => typeof n === "number" && Number.isInteger(n) && n >= 0 && n <= 0xffffffff;
  const time = (n: unknown) => typeof n === "number" && Number.isSafeInteger(n) && n >= 0;
  if (!object(value) || !keys(value, ["schemaVersion", "provider", "context", "revision", "mode", "policy", "pendingRoots", "projects", "blockers", "lastCommandId", "changedAtMs", "observedAtMs", "readOnly"])
    || value.schemaVersion !== 1 || !sameBuild(value.provider, info.build) || !sameScope(value.context, info.fencedRequests?.context)
    || typeof value.revision !== "string" || !/^(0|[1-9][0-9]*)$/.test(value.revision) || value.revision.length > 19 || BigInt(value.revision) > 9223372036854775807n
    || !["running", "admission_paused", "dispatch_paused", "paused"].includes(value.mode as string)
    || !count(value.pendingRoots) || !Array.isArray(value.projects) || !Array.isArray(value.blockers)
    || !optionalString(value.lastCommandId) || typeof value.readOnly !== "boolean" || !time(value.changedAtMs) || !time(value.observedAtMs)) return null;
  const policy = value.policy;
  if (policy != null && (!object(policy) || !keys(policy, ["maxPendingRoots", "maxPendingRootsPerProject"])
    || !count(policy.maxPendingRoots) || policy.maxPendingRoots < 1 || policy.maxPendingRoots > 10000
    || (policy.maxPendingRootsPerProject != null && (!count(policy.maxPendingRootsPerProject) || policy.maxPendingRootsPerProject < 1 || policy.maxPendingRootsPerProject > policy.maxPendingRoots)))) return null;
  const seen = new Set<string>(); let total = 0;
  const blockers: QueueBlocker[] = [];
  if (["admission_paused", "paused"].includes(value.mode as string)) blockers.push({ code: "admission_paused" });
  if (["dispatch_paused", "paused"].includes(value.mode as string)) blockers.push({ code: "dispatch_paused" });
  if (object(policy) && value.pendingRoots >= (policy.maxPendingRoots as number)) blockers.push({ code: "global_limit", limit: policy.maxPendingRoots as number });
  for (const p of value.projects) {
    if (!object(p) || !keys(p, ["projectId", "pendingRoots"]) || typeof p.projectId !== "string" || !p.projectId || seen.has(p.projectId) || !count(p.pendingRoots)) return null;
    seen.add(p.projectId); total += p.pendingRoots;
    if (object(policy) && typeof policy.maxPendingRootsPerProject === "number" && p.pendingRoots >= policy.maxPendingRootsPerProject) blockers.push({ code: "project_limit", projectId: p.projectId, limit: policy.maxPendingRootsPerProject });
  }
  if (total !== value.pendingRoots || value.blockers.length !== blockers.length || value.blockers.some((b, i) => !object(b) || !keys(b, Object.keys(blockers[i])) || Object.entries(blockers[i]).some(([key, expected]) => b[key] !== expected))) return null;
  return { ...value, provider: readBuild(value.provider)!, policy: object(policy) ? { ...policy, maxPendingRootsPerProject: policy.maxPendingRootsPerProject ?? null } : null, lastCommandId: value.lastCommandId ?? null } as unknown as QueueControlObservation;
}

type ProjectionObservation = { kind: "missing" | "unsupported" } | { kind: "known"; projection: ControllerReadinessProjection };
const sameBuild = (a: unknown, b: unknown) => {
  const left = readBuild(a), right = readBuild(b);
  return left !== null && right !== null && (["version", "sourceCommit", "sourceDirty", "sourceManifestSha256", "buildId"] as const).every((key) => left[key] === right[key]);
};
const scope = (value: unknown): value is Record<string, string> => object(value)
  && keys(value, ["instanceId", "journalIncarnation"]) && typeof value.instanceId === "string" && typeof value.journalIncarnation === "string";
const sameScope = (a: unknown, b: unknown) => scope(a) && scope(b) && a.instanceId === b.instanceId && a.journalIncarnation === b.journalIncarnation;
function identity(value: unknown): value is Record<string, unknown> {
  return object(value) && keys(value, ["schemaVersion", "contract", "contractSchemaSha256", "consumer", "provider", "context", "routeTransport", "route", "projectPolicy"])
    && value.schemaVersion === 1 && contract(value.contract) && typeof value.contractSchemaSha256 === "string"
    && readBuild(value.consumer) !== null && readBuild(value.provider) !== null && scope(value.context)
    && typeof value.routeTransport === "string" && ["mcp_ipc_websocket", "cli_ipc_websocket"].includes(value.routeTransport)
    && object(value.projectPolicy) && keys(value.projectPolicy, ["projectId", "sha256"])
    && typeof value.projectPolicy.projectId === "string" && typeof value.projectPolicy.sha256 === "string"
    && Array.isArray(value.route) && value.route.length === 3 && value.route.every((hop) => object(hop)
      && keys(hop, ["role", "nodeId", "endpoint", "build"]) && typeof hop.role === "string" && ["adapter", "controller", "worker"].includes(hop.role)
      && typeof hop.nodeId === "string" && typeof hop.endpoint === "string" && readBuild(hop.build) !== null);
}
const reasons = ["unsupported_contract", "missing_identity", "invalid_identity", "missing_evidence", "invalid_evidence", "identity_changed", "schema_changed", "build_unverified", "pair_untested", "route_not_fresh", "policy_denied"];
function projectionView(view: unknown, raw: Record<string, unknown>): boolean {
  if (!object(view) || !keys(view, ["projectId", "contract", "identity", "readiness", "receiptId", "testProfile", "controllerGeneration", "sessionId", "detail"])
    || typeof view.projectId !== "string" || !contract(view.contract) || !optionalString(view.receiptId)
    || typeof view.testProfile !== "string" || typeof view.detail !== "string"
    || view.controllerGeneration !== raw.controllerGeneration || view.sessionId !== raw.sessionId || !object(view.readiness)) return false;
  const ready = view.readiness;
  if (!keys(ready, ["supported", "buildVerified", "pairTested", "enabled", "reasons"])
    || ["supported", "buildVerified", "pairTested", "enabled"].some((key) => typeof ready[key] !== "boolean")
    || !Array.isArray(ready.reasons) || ready.reasons.some((reason) => typeof reason !== "string" || !reasons.includes(reason))) return false;
  if (view.identity !== null && view.identity !== undefined) {
    const id = view.identity;
    if (!identity(id) || !sameScope(id.context, raw.context) || !sameBuild(id.provider, raw.providerBuild)
      || !object(id.projectPolicy) || id.projectPolicy.projectId !== view.projectId
      || !contract(id.contract) || id.contract.id !== view.contract.id || id.contract.version !== view.contract.version
      || !Array.isArray(id.route) || !object(id.route[1]) || id.route[1].role !== "controller" || !sameBuild(id.route[1].build, raw.controllerBuild)) return false;
  }
  return !ready.enabled || (view.identity != null && ready.supported === true && ready.buildVerified === true && ready.pairTested === true && ready.reasons.length === 0);
}
function verificationView(view: unknown): boolean {
  return object(view) && keys(view, ["verificationKey", "identity", "status", "stage", "taskIds", "receiptId", "detail"])
    && typeof view.verificationKey === "string" && identity(view.identity)
    && typeof view.status === "string" && ["accepted", "running", "succeeded", "failed", "indeterminate"].includes(view.status)
    && typeof view.stage === "string" && Array.isArray(view.taskIds) && view.taskIds.every((id) => typeof id === "string")
    && optionalString(view.receiptId) && typeof view.detail === "string";
}
export function interpretControllerProjection(info: WorkerInfo): ProjectionObservation {
  const catalog = interpretCatalog(info);
  if (catalog.kind !== "known" || !catalog.catalog.supported.some((c) => c.id === "winbridge.controller-readiness" && c.version === 1)) return { kind: "unsupported" };
  const raw = catalog.catalog.controllerProjection;
  if (raw === undefined) return { kind: "missing" };
  const connection = info.controllerConnection;
  if (!object(raw) || !keys(raw, ["schemaVersion", "controllerBuild", "controllerGeneration", "sessionId", "providerBuild", "providerGeneration", "context", "sequence", "views", "verifications", "truncated"])
    || raw.schemaVersion !== 1 || typeof raw.controllerGeneration !== "string" || typeof raw.sessionId !== "string"
    || readBuild(raw.controllerBuild) === null || !sameBuild(raw.providerBuild, info.build)
    || raw.providerGeneration !== info.generation || !connection || connection.state !== "connected"
    || raw.sessionId !== connection.sessionId || raw.controllerGeneration !== connection.controllerGeneration
    || !info.fencedRequests || info.fencedRequests.requestContractVersion !== 2 || !sameScope(raw.context, info.fencedRequests.context)
    || typeof raw.sequence !== "number" || !Number.isSafeInteger(raw.sequence) || raw.sequence <= 0
    || typeof raw.truncated !== "boolean" || !Array.isArray(raw.views) || raw.views.length > 10
    || !Array.isArray(raw.verifications) || raw.verifications.length > 10
    || !raw.views.every((view) => projectionView(view, raw)) || !raw.verifications.every(verificationView)) return { kind: "unsupported" };
  return { kind: "known", projection: raw as unknown as ControllerReadinessProjection };
}

export function catalogSummary(info: Pick<WorkerInfo, "contractCatalog" | "build">): string {
  const observation = interpretCatalog(info);
  if (observation.kind === "missing") return "Каталог совместимости не получен: worker или посредник использует прежний формат.";
  if (observation.kind === "unsupported") return observation.reason;
  const versions = observation.catalog.supported.filter((entry) => entry.id === "winbridge.worker-rpc").map((entry) => entry.version);
  return `Worker сообщает поддержку RPC: ${versions.length ? versions.join(", ") : "известные версии не указаны"}. Это не подтверждение проверки пары и не новое разрешение на запуск.`;
}


export function interpretProjectPolicies(info: Pick<WorkerInfo, "projectPolicies" | "build" | "projects">): ProjectPolicyCatalog | null {
  const raw = info.projectPolicies;
  if (!object(raw) || !keys(raw, ["schemaVersion", "provider", "projects"]) || raw.schemaVersion !== 1 || !Array.isArray(raw.projects)
    || raw.projects.length !== info.projects.length) return null;
  const provider = readBuild(raw.provider);
  const expected = readBuild(info.build);
  if (!provider || !expected || !provider.buildId || (["version", "sourceCommit", "sourceDirty", "sourceManifestSha256", "buildId"] as const)
    .some((key) => expected[key] !== provider[key])) return null;
  const seen = new Set<string>();
  if (raw.projects.some((entry) => {
    if (!object(entry) || !keys(entry, ["projectId", "sha256"]) || typeof entry.projectId !== "string"
      || typeof entry.sha256 !== "string" || !/^[a-f0-9]{64}$/.test(entry.sha256) || entry.sha256.length !== 64
      || seen.has(entry.projectId) || !info.projects.some((p) => p.id === entry.projectId)) return true;
    seen.add(entry.projectId);
    return false;
  })) return null;
  return { schemaVersion: 1, provider, projects: raw.projects as ProjectPolicyCatalog["projects"] };
}

export function projectPolicySummary(info: Pick<WorkerInfo, "projectPolicies" | "build" | "projects">): string {
  const policies = interpretProjectPolicies(info);
  return policies ? `Получены отпечатки действующих настроек для ${policies.projects.length} проектов. Изменение настроек потребует новой проверки пары.`
    : "Проверенные сведения о действующих настройках проектов ещё не получены. Готовность новых контрактов не подтверждена.";
}
