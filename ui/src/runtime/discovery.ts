import type { BuildIdentity, ContractCatalog, ContractRef, WorkerInfo } from "./contracts";

export type CatalogObservation =
  | { kind: "missing" }
  | { kind: "unsupported"; reason: string }
  | { kind: "known"; catalog: ContractCatalog };

const object = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const keys = (value: Record<string, unknown>, allowed: string[]) =>
  Object.keys(value).every((key) => allowed.includes(key));
function build(value: unknown): value is BuildIdentity {
  return object(value) && typeof value.version === "string" && typeof value.buildId === "string"
    && typeof value.sourceDirty === "boolean"
    && (value.sourceCommit === null || typeof value.sourceCommit === "string")
    && (value.sourceManifestSha256 === null || typeof value.sourceManifestSha256 === "string");
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
    || !build(raw.provider) || !Array.isArray(raw.supported)) return invalid("Формат каталога неизвестен. Новые функции не включены.");
  const provider = raw.provider;
  if (!info.build || !provider.buildId || (["version", "sourceCommit", "sourceDirty", "sourceManifestSha256", "buildId"] as const)
    .some((key) => info.build![key] !== provider[key])) return invalid("Каталог относится к другой сборке worker.");
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
