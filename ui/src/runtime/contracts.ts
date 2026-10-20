// Generated from crates/protocol by npm run generate:contracts. Do not edit.
export type ArtifactRole = "worker" | "fixture" | "desktop" | "updater";
export type ArtifactDescriptor = { schemaVersion: number, kind: string, queryContract: number, role: ArtifactRole, build: BuildIdentity, targetOs: string, targetArch: string, };
export type RuntimeImageEvidence = { path: string, bytes: number, sha256: string, fileIdentity: string, querySupported: boolean, };
export type LoadedWorkerIdentity = { schemaVersion: number, kind: string, process: SupervisorProcess, runtimeIdentity: string, setId: string, manifestSha256: string, descriptor: ArtifactDescriptor, image: RuntimeImageEvidence, };
export type WorkerRuntimeCompatibility = { schemaVersion: number, kind: string, identity: LoadedWorkerIdentity, context: RequestContext, generation: string, workerConfigSha256: string, workerConfigSchema: number, workerJournalSchema: number, updaterJournalSchema: number, protocolVersion: number, observedAtMs: number, };
export type WorkerConfigCheckReport = { schemaVersion: number, kind: string, descriptor: ArtifactDescriptor, process: SupervisorProcess, config: RuntimeImageEvidence, workerConfigSchema: number, instanceId: string, workerJournalOpened: boolean, schedulerStarted: boolean, candidateVerified: boolean, runtimeApplied: boolean, };
export type WorkerRehearsalReport = { schemaVersion: number, kind: string, contractVersion: number, descriptor: ArtifactDescriptor, process: SupervisorProcess, directory: string, directoryIdentity: string, instanceId: string, generation: string, journalIncarnation: string, workerJournalSchema: number, updaterJournalSchema: number, workerConfigSchema: number, protocolVersion: number, freshJournal: boolean, ipcReady: boolean, serverStopped: boolean, controllerEnabled: boolean, directCommandsEnabled: boolean, productionStateOpened: boolean, candidateVerified: boolean, runtimeApplied: boolean, };
export type SourceManifest = { schemaVersion: number, sourceCommit: string, dirty: boolean, createdAt: string, includesBuiltUi: boolean, files: Array<SourceFile>, };
export type SourceFile = { path: string, bytes: number, sha256: string, };
export type SourceAction = "add" | "replace" | "remove" | "keep";
export type SourceChange = { path: string, action: SourceAction, previous: SourceFile | null, incoming: SourceFile | null, };
export type SourceBlockerCode = "local_modified" | "local_missing" | "unmanaged_file" | "path_type_conflict" | "case_conflict" | "layout_change";
export type SourceBlocker = { path: string, code: SourceBlockerCode, message: string, };
export type SourcePlan = { changes: Array<SourceChange>, blockers: Array<SourceBlocker>,
/**
 * Source preconditions only. Never authorizes filesystem writes or shutdown.
 */
canApply: boolean, };
export type UpdatePhase = "requested" | "preparing" | "source_applying" | "building" | "validating_candidate" | "candidate_verified" | "draining" | "quiesced" | "stopping" | "replacing" | "verifying_local" | "awaiting_controller" | "aborting" | "committed" | "deferred" | "failed_before_cutover" | "manual_intervention";
export type UpdateEvent = { "kind": "runtime_baseline_admitted" } | { "kind": "runtime_baseline_verified" } | { "kind": "runtime_preimages_admitted" } | { "kind": "runtime_preimages_verified" } | { "kind": "begin_preparation" } | { "kind": "begin_source_apply" } | { "kind": "source_verified" } | { "kind": "build_succeeded" } | { "kind": "candidate_health_admitted" } | { "kind": "candidate_accepted" } | { "kind": "begin_drain" } | { "kind": "quiesce", "data": { dispatchClosed: boolean, acceptedOperations: number, physicalOwners: number, uncertainOwners: number, } } | { "kind": "begin_shutdown" } | { "kind": "shutdown_verified" } | { "kind": "files_verified" } | { "kind": "local_health_verified" } | { "kind": "controller_confirmed" } | { "kind": "begin_abort" } | { "kind": "abort_completed" } | { "kind": "defer" } | { "kind": "fail" };
export type UpdateSourcePin = {
/**
 * Filename only. The installed policy chooses the source delivery directory.
 */
archiveName: string, archiveSha256: string, manifestSha256: string, sourceCommit: string, };
export type UpdateInstallationPin = { installationId: string, scopeSha256: string, workerBuildId: string, workerConfigSha256: string, baselineManifestSha256: string, };
export type UpdateRequest = { contractVersion: number, idempotencyKey: string, source: UpdateSourcePin, expected: UpdateInstallationPin, };
export type UpdateRecord = { updateId: string, revision: number, phase: UpdatePhase, request: UpdateRequest, acceptedAtMs: number, updatedAtMs: number, reason: string | null, };
export type UpdateCommand = { idempotencyKey: string, expectedRevision: number, event: UpdateEvent, reason: string, };
export type UpdateTransition = {
/**
 * Installation-wide order; a revision is scoped to one update.
 */
sequence: number, updateId: string, revision: number, previousPhase: UpdatePhase | null, phase: UpdatePhase, occurredAtMs: number, ownerEpoch: number, ownerId: string, command: UpdateCommand | null, };
export type WorkerUpdateSpec = { updateId: string, instanceId: string, journalIncarnation: string, workerGeneration: string, updaterOwnerId: string, updaterEpoch: number, sourceDirectory: string, baselineManifestSha256: string, targetManifestSha256: string,
/**
 * Derived by worker from registered workspace overlap, never caller assertions.
 */
projects: Array<string>, };
export type WorkerUpdatePhase = "source_pending" | "source_held" | "draining" | "quiescing" | "quiesced" | "released";
export type WorkerUpdateLease = { spec: WorkerUpdateSpec, revision: number, phase: WorkerUpdatePhase, acceptedAtMs: number, updatedAtMs: number, reason: string, };
export type WorkerUpdateAction = "acquire_source" | "begin_drain" | "close_dispatch" | "confirm_quiesced" | "release";
export type WorkerUpdateCommand = { commandId: string, expectedRevision: number, action: WorkerUpdateAction, sourceManifestSha256: string | null, reason: string, };
export type WorkerUpdateBlockers = { acceptedOperations: number, logicalOwners: number, inFlightEffects: number, physicalOwners: number, uncertainOwners: number, };
export type WorkerDrainObservation = { schemaVersion: number, context: RequestContext, workerGeneration: string, lease: WorkerUpdateLease | null, acceptedOperations: number, logicalOwners: number, inFlightEffects: number, queueRevision: string, queueMode: QueueMode, observedAtMs: number, };
export type UpdateBuildSpec = { attemptId: string, updateId: string, requestKey: string, platform: string, ownerId: string, ownerEpoch: number, workerSourceSpec: WorkerUpdateSpec, workerLeaseRevision: number, sourceDirectory: string, runtimeDirectory: string, sourceManifestSha256: string, command: ResolvedCommand, jobName: string | null, sourceAvailableBytesBefore: number, runtimeAvailableBytesBefore: number, acceptedAtMs: number, };
export type UpdateBuildPhase = "intent" | "spawned" | "resume_granted" | "running" | "completed" | "indeterminate";
export type UpdateBuildOutcome = "succeeded" | "failed" | "timed_out" | "spawn_failed";
export type UpdateBuildOutput = { retainedBytes: number, observedBytes: number, sha256: string, error: string | null, };
export type UpdateCompilerOutputRequest = { attemptId: string, stream: OutputStream, offset: number, limitBytes: number, fileIdentity: string | null, };
export type UpdateCompilerOutputPage = { schemaVersion: number, kind: string, readOnly: boolean, updateId: string, attemptId: string, buildRevision: number, observedAtMs: number, phase: UpdateBuildPhase, stream: OutputStream, offset: number, nextOffset: number, availableBytes: number, fileIdentity: string | null, bytes: Array<number>, hasMore: boolean, captureComplete: boolean, proofVerified: boolean, observedBytes: number | null, quotaTruncated: boolean | null, outputError: string | null, };
export type UpdateBuildRecord = { schemaVersion: number, spec: UpdateBuildSpec, revision: number, phase: UpdateBuildPhase, process: SupervisorProcess | null, outcome: UpdateBuildOutcome | null, exitCode: number | null, cleanupVerified: boolean, stdout: UpdateBuildOutput | null, stderr: UpdateBuildOutput | null, updatedAtMs: number, reason: string, };
export type SupervisorProcess = { processId: number, birth: string, hostId: string, sessionId: number, };
export type SupervisorPhase = "intent" | "spawned" | "ready" | "active" | "stopped" | "indeterminate";
export type SupervisorSpec = { launchId: string, updateId: string, parent: SupervisorProcess, parentOwnerId: string, parentEpoch: number, tokenSha256: string, configPath: string, configSha256: string, runtimeDirectory: string, imagePath: string, imageSha256: string, imageIdentity: string, excludedDomain: string | null, endpoint: string, acceptedAtMs: number, worker?: SupervisorWorkerBinding | null, };
export type SupervisorRecord = { schemaVersion: number, spec: SupervisorSpec, phase: SupervisorPhase, revision: number, process: SupervisorProcess | null, ownerId: string | null, ownerEpoch: number | null, updatedAtMs: number, reason: string, };
export type SupervisorAction = "observe" | "handoff" | "stop" | "worker_control" | "prepare_source" | "observe_preparation" | "build_candidate" | "abort_before_cutover" | "verify_candidate_artifacts" | "observe_worker_runtime" | "capture_runtime_baseline" | "observe_runtime_baseline" | "archive_runtime_preimages" | "observe_runtime_preimages" | "observe_worker_compatibility" | "validate_candidate_health" | "observe_candidate_health" | "observe_worker_receipt" | "observe_worker_drain" | "drain_candidate" | "observe_candidate_drain";
export type SupervisorRequest = { contractVersion: number, launchId: string, expectedRevision: number, action: SupervisorAction, token: string, workerCommand?: WorkerUpdateControlCommand | null, };
export type SupervisorResponse = { contractVersion: number, launchId: string, record: SupervisorRecord | null, error: string | null, workerControl?: WorkerUpdateControlResponse | null, preparation?: UpdatePreparationView | null, };
export type UpdatePreparationView = { schemaVersion: number, observedAtMs: number, update: UpdateRecord, source: SourceTransactionRecord | null, sourceAppliedFiles: number, build: UpdateBuildRecord | null,
/**
 * Observation of this supervisor's blocking driver, separate from saved state.
 */
driverActive: boolean, driverError: string | null,
/**
 * V7+ replies include this projection; v1-v6 keep their JSON shape.
 */
runtimeBaseline?: RuntimeBaselineView | null,
/**
 * V8/view schema 3 only; older contracts omit the field entirely.
 */
runtimePreimages?: RuntimePreimagesView | null,
/**
 * V10/view schema 4; older contracts omit it entirely.
 */
candidateHealth?: CandidateHealthView | null,
/**
 * V13/view schema 5 only; older replies omit this field.
 */
candidateDrain?: CandidateDrainView | null, };
export type RuntimeBaselineView = { "phase": "not_admitted" } | { "phase": "pending", intentRevision: number, admittedAtMs: number, } | { "phase": "verified", intentRevision: number, receiptRevision: number, observedAtMs: number, setId: string, workerProcess: SupervisorProcess, workerImageSha256: string, };
export type RuntimePreimagesView = { "phase": "not_admitted" } | { "phase": "pending", intentRevision: number, admittedAtMs: number, directory: string, } | { "phase": "verified", intentRevision: number, receiptRevision: number, completedAtMs: number, directory: string, fileCount: number, totalBytes: number, };
export type CandidateHealthPhase = "not_admitted" | "pending" | "verified" | "rejected" | "cancelled" | "indeterminate";
export type CandidateHealthView = { phase: CandidateHealthPhase, revision: number | null, attemptId: string | null, configProcess: SupervisorProcess | null, rehearsalProcess: SupervisorProcess | null, cleanupVerified: boolean, reason: string | null, };
export type CandidateDrainPhase = "not_admitted" | "starting" | "waiting_accepted" | "closing_dispatch" | "confirming_quiescence" | "quiesced" | "indeterminate" | "aborted";
export type CandidateDrainView = { phase: CandidateDrainPhase, attemptId: string | null, confirmedCommands: number, quiescedRevision: number | null,
/**
 * Optional live sample from a separate worker snapshot, never atomic with
 * updater evidence. Absent after supervisor restart; not an approval.
 */
workerObservation: WorkerDrainObservation | null, };
export type SupervisorWorkerBinding = { context: RequestContext, generation: string, build: BuildIdentity, process: SupervisorProcess, configSha256: string, endpoint: string, };
export type WorkerUpdateControlCommand = { "action": "observe" } | { "action": "observe_runtime" } | { "action": "observe_compatibility" } | { "action": "observe_receipt", command: WorkerUpdateCommand, } | { "action": "observe_drain" } | { "action": "begin_source" } | { "action": "advance", command: WorkerUpdateCommand, };
export type WorkerUpdateControlRequest = { controlVersion: number, binding: SupervisorWorkerBinding, updateId: string, launchId: string, supervisor: SupervisorProcess, updaterOwnerId: string, updaterEpoch: number, command: WorkerUpdateControlCommand, };
export type WorkerUpdateControlResponse = { controlVersion: number, binding: SupervisorWorkerBinding, updateId: string, launchId: string, result: WorkerUpdateControlResult, };
export type WorkerUpdateControlResult = { "kind": "snapshot", lease: WorkerUpdateLease | null, } | { "kind": "runtime_identity", identity: LoadedWorkerIdentity, } | { "kind": "runtime_compatibility", compatibility: WorkerRuntimeCompatibility, } | { "kind": "command_receipt", command: WorkerUpdateCommand, receipt: WorkerUpdateLease | null, observedAtMs: number, } | { "kind": "drain_observation", observation: WorkerDrainObservation, } | { "kind": "error", message: string, };
export type SourceTransactionPhase = "prepared" | "applying" | "verified" | "blocked" | "aborted";
export type SourceFileState = "planned" | "intent" | "applied";
export type SourceTransactionRecord = { schemaVersion: number, updateId: string, sourceDirectory: string, runtimeDirectory: string, stageDirectory: string, transactionDirectory: string, sourceDirectoryIdentity: string, stageDirectoryIdentity: string, transactionDirectoryIdentity: string, workerSourceSpec: WorkerUpdateSpec, workerLeaseRevision: number, planSha256: string, totalFiles: number, revision: number, phase: SourceTransactionPhase, acceptedAtMs: number, updatedAtMs: number, reason: string | null, };
export type SourceFileObservation = { ordinal: number, change: SourceChange, state: SourceFileState, };
export type SourceTransactionStatus = { schemaVersion: number, kind: string, readOnly: boolean, transaction: SourceTransactionRecord, appliedFiles: number, files: Array<SourceFileObservation>, hasMore: boolean, observedAtMs: number, };
export type OperationRef = { context: RequestContext, kind: HistoryKind, operationId: string, };
export type AdmissionWatermarks = { tasks: string, imports: string, workflows: string, applications: string, };
export type OperationCursor = { context: RequestContext, querySha256: string, watermarks: AdmissionWatermarks, before: HistoryCursor, };
export type OperationQuery = { projectId: string | null, kind: HistoryKind | null, search: string, limit: number, cursor: OperationCursor | null, };
export type OperationPageRequest = { context: RequestContext, query: OperationQuery, };
export type OperationResource = { leaseId: string, owner: OperationRef, state: ObservedLeaseState, scope: ObservedLeaseScope, revision: string, reason: string | null, };
export type OperationObservation = { reference: OperationRef, projectId: string, state: OperationState, revision: string, label: string, reason: string | null, textTruncated: boolean, acceptedAtMs: number, finishedAtMs: number | null, lastEventSequence: string | null, related: Array<OperationRef>, resources: Array<OperationResource>, resourcesTruncated: boolean, };
export type OperationPage = { schemaVersion: number, context: RequestContext, observationId: string, observedAtMs: number, records: Array<OperationObservation>, nextCursor: OperationCursor | null, hasMore: boolean, readOnly: boolean, };
export type WorkSchedulingPolicy = { maxConcurrentWork: number, };
export type WorkSchedulingObservation = { schemaVersion: number, provider: BuildIdentity, context: RequestContext, maxConcurrentWork: number | null, occupiedSlots: number, uncertainSlots: number, queuedOperations: number, observedAtMs: number, readOnly: boolean, };
export type QueuePolicy = { maxPendingRoots: number, maxPendingRootsPerProject: number | null, };
export type QueueProjectPending = { projectId: string, pendingRoots: number, };
export type QueueControlObservation = { schemaVersion: number, provider: BuildIdentity, context: RequestContext, revision: string, mode: QueueMode, policy: QueuePolicy | null, pendingRoots: number, projects: Array<QueueProjectPending>, blockers: Array<QueueBlocker>, lastCommandId: string | null, changedAtMs: number, observedAtMs: number, readOnly: boolean, };
export type QueueMode = "running" | "admission_paused" | "dispatch_paused" | "paused";
export type QueueBlocker = { "code": "admission_paused" } | { "code": "dispatch_paused" } | { "code": "global_limit", limit: number, } | { "code": "project_limit", projectId: string, limit: number, };
export type QueueControlCommand = { "kind": "observe", commandId: string | null, } | { "kind": "set", commandId: string, expectedRevision: string, mode: QueueMode, };
export type QueueControlRequest = { queueControlVersion: number, context: RequestContext, command: QueueControlCommand, };
export type QueueControlReceipt = { context: RequestContext, commandId: string, fromRevision: string, toRevision: string, mode: QueueMode, changed: boolean, changedAtMs: number, };
export type QueueControlResponse = { queueControlVersion: number, context: RequestContext | null, result: QueueControlResult, };
export type QueueControlResult = { "kind": "observed", observation: QueueControlObservation, receipt: QueueControlReceipt | null, } | { "kind": "applied", observation: QueueControlObservation, receipt: QueueControlReceipt, } | { "kind": "rejected", code: QueueControlError, message: string, } | { "kind": "unconfirmed", message: string, };
export type QueueControlError = "unsupported_version" | "invalid_request" | "scope_mismatch" | "read_only" | "revision_conflict" | "key_conflict" | "unavailable";
export type ResourceKind = "build" | "cache" | "binary" | "ipc" | "port" | "interactive";
export type ResourceDefinition = { id: string, kind: ResourceKind, capacity: number, };
export type ResourceRequirement = { resourceId: string, units: number, };
export type ProjectResourceBindings = { projectId: string, task: Array<ResourceRequirement>, import: Array<ResourceRequirement>, build: Array<ResourceRequirement>, start: Array<ResourceRequirement>, };
export type ResourceSchedulingPolicy = { resources: Array<ResourceDefinition>, projects: Array<ProjectResourceBindings>, };
export type ResourceCapacityObservation = { id: string, kind: ResourceKind, capacity: number | null, usedUnits: number, uncertainUnits: number, };
export type ResourceOwnerObservation = { resourceId: string, leaseId: string, projectId: string, operationKind: HistoryKind, operationId: string, units: number, state: ObservedLeaseState, applicationLifetime: boolean, };
export type ResourceSchedulingObservation = { schemaVersion: number, provider: BuildIdentity, context: RequestContext, policyGeneration: string, enabled: boolean, resources: Array<ResourceCapacityObservation>, owners: Array<ResourceOwnerObservation>, truncated: boolean, observedAtMs: number, readOnly: boolean, };
export type OperationState = { "family": "execution", status: TaskStatus, } | { "family": "application", status: ApplicationStatus, };
export type ObservedLeaseState = "held" | "uncertain";
export type ObservedLeaseScope = "project_mutation" | "application_lifetime";
export type ExtensionRequest = { extensionVersion: number, contract: ContractRef, context: RequestContext, payload: unknown, };
export type ExtensionResponse = { extensionVersion: number, contract: ContractRef, context: RequestContext | null, result: ExtensionResult, };
export type ExtensionResult = { "kind": "data", payload: unknown, } | { "kind": "error", code: ExtensionError, message: string, };
export type ExtensionError = "unsupported_contract" | "invalid_request" | "scope_mismatch" | "query_failed";
export type ControllerReadinessProjection = { schemaVersion: number, controllerBuild: BuildIdentity, controllerGeneration: string, sessionId: string, providerBuild: BuildIdentity, providerGeneration: string, context: RequestContext, sequence: number, views: Array<ContractReadinessView>, verifications: Array<VerificationView>, truncated: boolean, };
export type RuntimeSettings = { generation: string, path: string, activeJson: string, savedJson: string, savedSha256: string, restartRequired: boolean, };
export type SaveRuntimeSettings = { generation: string, requestId: string, expectedSha256: string, json: string, };
export type SettingsSaved = { settings: RuntimeSettings, backupPath: string | null, };
export type OutputCleanupQuery = { olderThanDays: number, projectId: string | null, };
export type OutputCleanupEntry = { taskId: string, projectId: string, bytes: number, finishedAtMs: number, };
export type OutputCleanupPlan = { beforeMs: number, projectId: string | null, taskIds: Array<string>, sha256: string, };
export type OutputCleanupPreview = { plan: OutputCleanupPlan, entries: Array<OutputCleanupEntry>, bytes: number, hasMore: boolean, observedAtMs: number, };
export type ApplyOutputCleanup = { idempotencyKey: string, plan: OutputCleanupPlan, };
export type OutputCleanupResult = { idempotencyKey: string, removedTasks: number, removedBytes: number, occurredAtMs: number, };
export type HistoryKind = "task" | "import" | "workflow" | "application";
export type HistoryCursor = { createdAtMs: number, kind: HistoryKind, operationId: string, };
export type HistoryQuery = { projectId: string | null, kind: HistoryKind | null, status: string | null, search: string, before: HistoryCursor | null, };
export type HistoryEntry = { operationId: string, kind: HistoryKind, projectId: string, label: string, status: string, createdAtMs: number, finishedAtMs: number | null, workflowId: string | null, };
export type HistoryPage = { records: Array<HistoryEntry>, nextCursor: HistoryCursor | null, hasMore: boolean, observedAtMs: number, };
export type DiagnosticRequest = { taskId: string, includeCommand: boolean, includeOutput: boolean, redactValues: Array<string>, };
export type DiagnosticPreview = { taskId: string, observedAtMs: number, json: string, commandIncluded: boolean, outputIncluded: boolean, outputTruncated: boolean, };
export type StorageUsage = { taskCount: number, importCount: number, workflowCount: number, applicationCount: number, attentionCount: number, pendingAttentionCount: number, storedOutputBytes: number, journalBytes: number, walBytes: number, outputLimitBytesPerTask: number, };
export type SavedDiagnostic = { path: string, bytes: number, sha256: string, };
export type AttentionSeverity = "info" | "warning" | "error";
export type AttentionSubmission = { requestId: string, idempotencyKey: string, taskId: string, severity: AttentionSeverity, message: string, };
export type AttentionRecord = { attentionId: string, sequence: number, projectId: string, taskId: string, severity: AttentionSeverity, message: string,
/**
 * "agent_request" or "task_outcome"; never claims authenticated identity.
 */
origin: string, createdAtMs: number, acknowledgedAtMs: number | null, };
export type AttentionPage = { records: Array<AttentionRecord>, nextSequence: number, hasMore: boolean, pendingCount: number, };
export type SnapshotSource = { "kind": "git_commit", commit: string, };
export type SnapshotFile = {
/**
 * Canonical UTF-8 path with '/' separators, relative to the payload root.
 */
path: string, bytes: number, sha256: string, executable: boolean, };
export type SnapshotReuse = { path: string, basePath: string, };
export type SnapshotDelta = { snapshotId: string, manifestSha256: string, reusedFiles: Array<SnapshotReuse>, };
export type SnapshotDeltaSummary = { snapshotId: string, manifestSha256: string, reusedFiles: number, reusedBytes: number, payloadBytes: number, };
export type SnapshotPayloadChunks = { chunkBytes: number, sha256: Array<string>, };
export type SnapshotTransfer = { payloadBytes: number, chunkBytes: number, reusedBytes: number, sharedBytes: number, };
export type SnapshotManifest = { schemaVersion: number, projectId: string, snapshotId: string, source: SnapshotSource,
/**
 * payload.zip is fixed by the format; no caller-supplied filesystem path.
 */
payloadBytes: number, payloadSha256: string, payloadChunks?: SnapshotPayloadChunks, totalBytes: number, delta?: SnapshotDelta, files: Array<SnapshotFile>, };
export type SnapshotReady = { schemaVersion: number, projectId: string, snapshotId: string, manifestSha256: string, payloadSha256: string, payloadBytes: number, };
export type ImportSubmission = { protocolVersion: number, requestId: string, projectId: string, snapshotId: string, manifestSha256: string, idempotencyKey: string, timeoutMs: number, };
export type ImportRecord = { importId: string, submission: ImportSubmission, status: TaskStatus, phase: string, delta?: SnapshotDeltaSummary, transfer?: SnapshotTransfer, acceptedAtMs: number, finishedAtMs: number | null, cancelRequested: boolean, completedFiles: number, completedBytes: number, totalFiles: number | null, totalBytes: number | null, commit: string | null, sharedPath: string, workspacePath: string, reason: string | null, events: Array<JournalEvent>, };
export type SnapshotPreview = { resumable: boolean, projectId: string, snapshotId: string, manifestSha256: string, source: SnapshotSource, totalFiles: number, totalBytes: number, payloadBytes: number, sharedPath: string, workspacePath: string, files: Array<SnapshotFile>, nextOffset: number | null, payloadVerified: boolean, peerSharedPath: string | null, delta?: SnapshotDeltaSummary, };
export type PublishedSnapshot = { resumable: boolean, snapshotId: string, source: SnapshotSource | null, totalFiles: number | null, totalBytes: number | null, payloadBytes: number | null, metadataError: string | null, delta?: SnapshotDeltaSummary, };
export type PublishedSnapshotPage = { projectId: string, sharedPath: string, snapshots: Array<PublishedSnapshot>, nextOffset: number | null, };
export type VerifiedSnapshotRef = { snapshotId: string, manifestSha256: string, source: SnapshotSource, };
export type TaskStatus = "queued" | "running" | "succeeded" | "failed" | "cancelled" | "interrupted" | "indeterminate";
export type CommandSource = { "kind": "recipe", recipeId: string, arguments: Array<string>, } | { "kind": "direct", executable: string, arguments: Array<string>,
/**
 * Relative path inside the registered project. Validation belongs to core/worker.
 */
workingDirectory: string, };
export type TaskSubmission = { protocolVersion: number, requestId: string, projectId: string, idempotencyKey: string, timeoutMs: number, snapshotId?: string, command: CommandSource, };
export type ResolvedCommand = { executable: string, arguments: Array<string>, workingDirectory: string, timeoutMs: number, };
export type JournalEvent = { sequence: number, occurredAtMs: number, status: TaskStatus, summary: string, };
export type TaskRecord = { taskId: string, submission: TaskSubmission, resolvedCommand: ResolvedCommand, recipeOrigin?: RecipeOrigin, snapshot: VerifiedSnapshotRef | null, status: TaskStatus, acceptedAtMs: number, startedAtMs: number | null, finishedAtMs: number | null, processId: number | null, exitCode: number | null, reason: string | null, cancelRequested: boolean, outputBytes: number, outputTruncated: boolean, outputRemovedAtMs: number | null, removedOutputBytes: number, events: Array<JournalEvent>, };
export type TaskSummary = { taskId: string, projectId: string, status: TaskStatus, acceptedAtMs: number, };
export type OutputStream = "stdout" | "stderr";
export type OutputChunk = { sequence: number, stream: OutputStream, offset: number,
/**
 * Raw bytes: UI decodes each stream incrementally, without splitting UTF-8 characters.
 */
bytes: Array<number>, };
export type RecipeInfo = { id: string, command: ResolvedCommand, origin?: RecipeOrigin, };
export type RecipeOrigin = { recipeId: string, profileId: string | null, profileLabel: string | null, };
export type RecipeProfileInfo = { id: string, label: string, description: string, };
export type CommandHandlerInfo = { id: string, label: string, description: string, };
export type ProjectInfo = { id: string, workspaceRoot: string, allowDirectCommands: boolean, recipes: Array<RecipeInfo>, recipeProfile?: RecipeProfileInfo, };
export type ConnectionState = "connecting" | "connected" | "disconnected";
export type ControllerConnection = { url: string, state: ConnectionState, sessionId: string | null, controllerNodeId: string | null, controllerGeneration: string | null, lastConfirmedAtMs: number | null, error: string | null, };
export type ActivityLogStatus = { path: string, maxFileBytes: number, retainedArchives: number, droppedRecords: number, lastError: string | null, };
export type ControllerStatus = { instanceId: string, nodeId: string, generation: string, listenAddress: string, connected: boolean, sessionId: string | null, workerNodeId: string | null, workerGeneration: string | null, workerAddress: string | null, lastError: string | null, activityLog: ActivityLogStatus | null, };
export type ResourceRecoveryEntry = { projectId: string, operationId: string, operationKind: string, physicalBinding: boolean, reason: string, };
export type ResourceRecovery = { unresolvedCount: number, truncated: boolean, resources: Array<ResourceRecoveryEntry>, };
export type JournalInspection = { schemaVersion: number, reason: string, };
export type RequestContext = { instanceId: string, journalIncarnation: string, };
export type FencedRequest = { requestContractVersion: number, context: RequestContext, request: LocalRequest, };
export type FencedResponse = { requestContractVersion: number, outcome: FencedReplyOutcome, context: RequestContext | null, response: LocalResponse, };
export type FencedReplyOutcome = "responded" | "rejected_before_dispatch" | "unconfirmed";
export type FencedRequestInfo = { requestContractVersion: number, context: RequestContext, legacyMutationsAllowed: boolean, };
export type RecoveryCoverageInfo = { complete: boolean, reason: string, };
export type ContractRef = { id: string, version: number, };
export type ContractCatalog = { schemaVersion: number, provider: BuildIdentity, supported: Array<ContractRef>, controllerProjection?: unknown, workScheduling?: unknown, queueControl?: unknown, resourceScheduling?: unknown, };
export type ContractSchemaFingerprint = { contract: ContractRef, sha256: string, };
export type ContractSchemaCatalog = { schemaVersion: number, provider: BuildIdentity, schemas: Array<ContractSchemaFingerprint>, };
export type ProjectPolicyFingerprint = { projectId: string, sha256: string, };
export type ProjectPolicyCatalog = { schemaVersion: number, provider: BuildIdentity, projects: Array<ProjectPolicyFingerprint>, };
export type RouteRole = "adapter" | "controller" | "worker";
export type RouteTransport = "mcp_ipc_websocket" | "cli_ipc_websocket";
export type RouteHop = {
/**
 * The ordered route is adapter -> controller -> worker.
 */
role: RouteRole, nodeId: string, endpoint: string, build: BuildIdentity, };
export type PairIdentity = { schemaVersion: number, contract: ContractRef, contractSchemaSha256: string, consumer: BuildIdentity, provider: BuildIdentity, context: RequestContext, routeTransport: RouteTransport,
/**
 * Session UUID is freshness, and deliberately absent from durable identity.
 */
route: Array<RouteHop>, projectPolicy: ProjectPolicyFingerprint, };
export type BuildVerification = { build: BuildIdentity, platform: string, testSuite: string, reportSha256: string, };
export type PairVerification = { schemaVersion: number, identity: PairIdentity, consumerVerification: BuildVerification, providerVerification: BuildVerification, testProfile: string, reportSha256: string, checks: Array<string>, };
export type ReadinessReason = "unsupported_contract" | "missing_identity" | "invalid_identity" | "missing_evidence" | "invalid_evidence" | "identity_changed" | "schema_changed" | "build_unverified" | "pair_untested" | "route_not_fresh" | "policy_denied";
export type ContractReadiness = { supported: boolean, buildVerified: boolean, pairTested: boolean, enabled: boolean, reasons: Array<ReadinessReason>, };
export type ControllerAdapter = "mcp" | "cli";
export type ControllerApiRequest = { controllerContractVersion: number, adapter: BuildIdentity, adapterKind: ControllerAdapter, request: ControllerOperation, };
export type ControllerOperation = { "method": "operation_page", request: OperationPageRequest, } | { "method": "contract_readiness", query: ContractReadinessQuery, } | { "method": "verify_pair", request: VerifyPairRequest, } | { "method": "get_verification", verificationKey: string, };
export type ContractReadinessQuery = { projectId: string, contract: ContractRef, receiptId: string | null, };
export type ContractReadinessView = { projectId: string, contract: ContractRef, identity: PairIdentity | null, readiness: ContractReadiness, receiptId: string | null, testProfile: string, controllerGeneration: string, sessionId: string | null, detail: string, };
export type ControllerApiResult = { "kind": "operation_page", page: OperationPage, } | { "kind": "contract_readiness", view: ContractReadinessView, } | { "kind": "verification", view: VerificationView, } | { "kind": "error", message: string, };
export type ControllerApiResponse = { controllerContractVersion: number, result: ControllerApiResult, };
export type VerifyPairRequest = { verificationKey: string,
/**
 * Copy identity from the readiness observation when creating the operation;
 * never replace it on retry or after reconnect/recovery.
 */
identity: PairIdentity, };
export type VerificationStatus = "accepted" | "running" | "succeeded" | "failed" | "indeterminate";
export type VerificationView = { verificationKey: string, identity: PairIdentity, status: VerificationStatus, stage: string, taskIds: Array<string>, receiptId: string | null, detail: string, };
export type WorkerInfo = { protocolVersion: number,
/**
 * Open read-only update coordination metadata; it is not an install capability.
 */
updateCoordination?: unknown, contractSchemas?: unknown,
/**
 * Read-only policy fingerprints. Unknown schemas stay opaque on older hops.
 */
projectPolicies?: unknown,
/**
 * Open read-only metadata. Interpret known versions separately; a future
 * catalog must not make the entire legacy Info reply unreadable.
 */
contractCatalog?: unknown, fencedRequests?: FencedRequestInfo, recoveryCoverage?: RecoveryCoverageInfo, instanceId: string, nodeId: string, generation: string, platform: string, runtimeValidation: string, projects: Array<ProjectInfo>, commandHandlers?: Array<CommandHandlerInfo>, controllerConnection: ControllerConnection | null, snapshotImportAvailable: boolean, snapshotDeltaAvailable: boolean, snapshotResumeAvailable: boolean, workflowAvailable: boolean, workflowFeatures: Array<string>, attentionAvailable: boolean, attentionTargetsAvailable: boolean, historyAvailable: boolean, maintenanceAvailable: boolean, maintenanceError: string | null, journalRecoveryRequired: string | null,
/**
 * Physical ownership still unresolved; scoped to projects, not a global
 * journal write failure. Absent in older workers and when no lease is uncertain.
 */
resourceRecovery?: ResourceRecovery,
/**
 * Explicit read-only inspection. Persisted statuses are historical evidence,
 * not observations of live processes; restart does not enable execution.
 */
journalInspection?: JournalInspection,
/**
 * Queued/running tasks across the entire journal; None when unavailable or stale.
 */
activeTaskCount: number | null, build: BuildIdentity | null, };
export type BuildIdentity = { version: string, sourceCommit: string | null, sourceDirty: boolean, sourceManifestSha256: string | null, buildId: string, };
export type LocalRequest = { "method": "get_runtime_settings" } | { "method": "save_runtime_settings", request: SaveRuntimeSettings, } | { "method": "preview_output_cleanup", query: OutputCleanupQuery, } | { "method": "apply_output_cleanup", request: ApplyOutputCleanup, } | { "method": "history", query: HistoryQuery, } | { "method": "prepare_task_diagnostic", request: DiagnosticRequest, } | { "method": "storage_usage" } | { "method": "list_attention_events", afterSequence: number, pendingOnly: boolean, } | { "method": "acknowledge_attention_event", attentionId: string, } | { "method": "request_attention", request: AttentionSubmission, } | { "method": "list_attention", afterSequence: number, pendingOnly: boolean, } | { "method": "acknowledge_attention", attentionId: string, } | { "method": "submit_workflow", request: WorkflowSubmission, } | { "method": "list_workflows" } | { "method": "get_workflow", workflowId: string, } | { "method": "cancel_workflow", workflowId: string, } | { "method": "list_applications" } | { "method": "get_application", applicationId: string, } | { "method": "stop_application", applicationId: string, } | { "method": "read_application_log", applicationId: string, cursor: LogCursor | null, maxBytes: number, } | { "method": "list_snapshots", projectId: string, offset: number, } | { "method": "import_snapshot", request: ImportSubmission, } | { "method": "import_delta_snapshot", request: ImportSubmission, } | { "method": "list_imports" } | { "method": "get_import", importId: string, } | { "method": "cancel_import", importId: string, } | { "method": "preview_snapshot", projectId: string, snapshotId: string, offset: number, } | { "method": "info" } | { "method": "controller_status" } | { "method": "submit", request: TaskSubmission, } | { "method": "list_tasks" } | { "method": "get_task", taskId: string, } | { "method": "read_output", taskId: string, afterSequence: number, } | { "method": "cancel", taskId: string, };
export type LocalResponse = { "kind": "runtime_settings", settings: RuntimeSettings, } | { "kind": "settings_saved", saved: SettingsSaved, } | { "kind": "output_cleanup_preview", preview: OutputCleanupPreview, } | { "kind": "output_cleanup_result", result: OutputCleanupResult, } | { "kind": "history", page: HistoryPage, } | { "kind": "diagnostic", preview: DiagnosticPreview, } | { "kind": "storage_usage", usage: StorageUsage, } | { "kind": "attention_event", event: AttentionEvent, } | { "kind": "attention_events", page: AttentionEventPage, } | { "kind": "attention", attention: AttentionRecord, } | { "kind": "attention_list", page: AttentionPage, } | { "kind": "workflow", workflow: WorkflowRecord, } | { "kind": "workflows", workflows: Array<WorkflowRecord>, } | { "kind": "application", application: ApplicationRecord, } | { "kind": "applications", applications: Array<ApplicationRecord>, } | { "kind": "log", page: LogPage, } | { "kind": "snapshots", page: PublishedSnapshotPage, } | { "kind": "import", import: ImportRecord, } | { "kind": "imports", imports: Array<ImportRecord>, } | { "kind": "snapshot_preview", preview: SnapshotPreview, } | { "kind": "info", info: WorkerInfo, } | { "kind": "task", task: TaskRecord, } | { "kind": "tasks", tasks: Array<TaskSummary>, } | { "kind": "output", chunks: Array<OutputChunk>, } | { "kind": "error", message: string, } | { "kind": "controller_status", status: ControllerStatus, };
export type TaskReport = { schemaVersion: number, platform: string, task: TaskRecord, output: Array<OutputChunk>, outputComplete: boolean, };
export type CommandDraft = { workerInstanceId: string, projectId: string, workspaceRoot: string, recipeId: string | null, snapshotId?: string, executable: string,
/**
 * Preserve the text exactly, including an incomplete JSON edit.
 */
argumentsJson: string, workingDirectory: string, timeoutSeconds: string, };
export type DeliveryDraft = { workerInstanceId: string, projectId: string, workspaceRoot: string, snapshotId: string | null, };
export type DesktopDraftState = { schemaVersion: number,
/**
 * Oldest first; the last matching worker entry restores its project choice.
 */
drafts: Array<CommandDraft>, deliveryDrafts: Array<DeliveryDraft>, workflowProfiles?: Array<WorkflowProfile>, notificationPreferences: NotificationPreferences, notificationCursors?: Array<NotificationCursor>, };
export type NotificationPreferences = { popupEnabled: boolean, soundEnabled: boolean, volumePercent: number, minimumSeverity: AttentionSeverity, minimumIntervalSeconds: number, quietHoursEnabled: boolean, quietStartMinute: number, quietEndMinute: number, };
export type NotificationCursor = { workerInstanceId: string, nodeId: string, afterSequence: number, };
export type CompanionStatus = { trayReady: boolean, workerConnected: boolean, lastConfirmedAtMs: number | null, pendingCount: number | null, activeTaskCount: number | null, error: string | null, latestAttention: AttentionEvent | null, openTaskId: string | null, openTarget: AttentionTarget | null, openTaskSequence: number, quitRequestSequence: number, preferences: NotificationPreferences, };
export type AttentionTarget = { "kind": "task", taskId: string, } | { "kind": "workflow", workflowId: string, } | { "kind": "application", applicationId: string, workflowId: string, };
export type AttentionEvent = { attentionId: string, sequence: number, projectId: string, target: AttentionTarget, severity: AttentionSeverity, message: string, origin: string, createdAtMs: number, acknowledgedAtMs: number | null, };
export type AttentionEventPage = { records: Array<AttentionEvent>, nextSequence: number, hasMore: boolean, pendingCount: number, };
export type DesktopValidationStatus = { enabled: boolean, phase: string, step: string, completed: boolean, error: string | null, };
export type WorkflowProfile = { workerInstanceId: string, projectId: string, workspaceRoot: string, name: string, parametersJson: string, };
export type WorkflowRecipePin = { stepName: string, recipeId: string,
/**
 * Definition at acceptance. Cwd is relative until the generation is imported.
 */
command: ResolvedCommand, origin: RecipeOrigin, };
export type ReadinessProbe = { stdoutContains?: string, timeoutMs: number, netwatch?: NetwatchProbe, };
export type NetwatchProbe = { instanceId: string, runtimeDirectory: string, };
export type ArtifactSpec = { name: string,
/**
 * Relative to the project root, outside immutable snapshots.
 */
path: string, };
export type BuildArtifact = { name: string, path: string, bytes: number, sha256: string, sourceCommit: string, manifestSha256: string, buildTaskId: string, recordedAtMs: number, };
export type WorkflowSubmission = { protocolVersion: number, requestId: string, idempotencyKey: string, name: string, projectId: string, snapshotId: string, manifestSha256: string, timeoutMs: number, build: CommandSource, start: CommandSource, readiness: ReadinessProbe,
/**
 * UTF-8 application log, relative to the registered project root, outside snapshots.
 */
logPath: string, artifacts?: Array<ArtifactSpec>, };
export type WorkflowStepStatus = "pending" | "running" | "succeeded" | "failed" | "cancelled" | "skipped" | "indeterminate";
export type WorkflowStep = { stepId: string, name: string, status: WorkflowStepStatus, operationId: string | null, startedAtMs: number | null, finishedAtMs: number | null, reason: string | null, };
export type WorkflowRecord = { workflowId: string, submission: WorkflowSubmission, recipePins?: Array<WorkflowRecipePin>, status: TaskStatus, acceptedAtMs: number, finishedAtMs: number | null, cancelRequested: boolean, reason: string | null, applicationId: string | null, steps: Array<WorkflowStep>, events: Array<JournalEvent>, artifacts: Array<BuildArtifact>, };
export type ApplicationStatus = "starting" | "ready" | "stopping" | "stopped" | "exited" | "failed" | "indeterminate";
export type ApplicationRecord = { applicationId: string, workflowId: string, projectId: string, taskId: string, status: ApplicationStatus, processId: number | null, createdAtMs: number, readyAtMs: number | null, finishedAtMs: number | null, logPath: string, reason: string | null, failureReason: string | null, };
export type LogCursor = { fileIdentity: string, offset: number, prefixBytes: number, prefixSha256: string, };
export type LogPage = { applicationId: string, path: string, encoding: string, bytes: Array<number>, cursor: LogCursor, resetReason: string | null, endOfFile: boolean, };
