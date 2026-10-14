import type { BuildIdentity, ContractCatalog, ContractRef, ControllerReadinessProjection, ProjectPolicyCatalog, WorkerInfo } from "./contracts";

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
  if (!object(raw) || !keys(raw, ["schemaVersion", "provider", "supported", "controllerProjection"]) || raw.schemaVersion !== 1
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
  return { kind: "known", catalog: { schemaVersion: 1, provider, supported: raw.supported as ContractRef[], ...(Object.hasOwn(raw, "controllerProjection") ? { controllerProjection: raw.controllerProjection } : {}) } };
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
