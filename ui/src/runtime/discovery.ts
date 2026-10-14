import type { BuildIdentity, ContractCatalog, ContractRef, ProjectPolicyCatalog, WorkerInfo } from "./contracts";

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
  if (!object(raw) || !keys(raw, ["schemaVersion", "provider", "supported"]) || raw.schemaVersion !== 1
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
  return { kind: "known", catalog: { schemaVersion: 1, provider, supported: raw.supported as ContractRef[] } };
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
