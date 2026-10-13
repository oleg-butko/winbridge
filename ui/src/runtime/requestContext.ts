import type { FencedRequest, FencedResponse, LocalRequest, LocalResponse, RequestContext } from "./contracts";

export type BoundWorkerRequest<T extends LocalRequest = LocalRequest> = {
  request: T;
  context?: RequestContext;
};

// Save this value with a pending logical operation. Sending it never reads Info
// or changes its contract when configuration/connection identity changes.
export function bindWorkerRequest<T extends LocalRequest>(request: T, context?: RequestContext): BoundWorkerRequest<T> {
  return { request: structuredClone(request), context: context ? { ...context } : undefined };
}

export function requestEnvelope(bound: BoundWorkerRequest): LocalRequest | FencedRequest {
  return bound.context ? { requestContractVersion: 2, context: { ...bound.context }, request: bound.request } : bound.request;
}

export function scopedResult(response: FencedResponse, expected: RequestContext): LocalResponse {
  if (response.requestContractVersion !== 2) throw new Error("Несовместимый контракт ответа; автоматического повтора нет");
  const sameScope = response.context?.instanceId === expected.instanceId
    && response.context?.journalIncarnation === expected.journalIncarnation;
  if (response.response.kind === "error" && (!sameScope || response.outcome === "unconfirmed")) throw new Error(response.response.message);
  if (response.response.kind !== "error" && (!sameScope || response.outcome !== "responded")) {
    throw new Error("Ответ принадлежит другому журналу. Исходный запрос не повторён в новой области.");
  }
  return response.response;
}
