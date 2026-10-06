// Generated from crates/protocol by npm run generate:contracts. Do not edit.
export type SnapshotSource = { "kind": "git_commit", commit: string, };
export type SnapshotFile = {
/**
 * Canonical UTF-8 path with '/' separators, relative to the payload root.
 */
path: string, bytes: number, sha256: string, executable: boolean, };
export type SnapshotManifest = { schemaVersion: number, projectId: string, snapshotId: string, source: SnapshotSource,
/**
 * payload.zip is fixed by the format; no caller-supplied filesystem path.
 */
payloadBytes: number, payloadSha256: string, totalBytes: number, files: Array<SnapshotFile>, };
export type SnapshotReady = { schemaVersion: number, projectId: string, snapshotId: string, manifestSha256: string, payloadSha256: string, payloadBytes: number, };
export type ImportSubmission = { protocolVersion: number, requestId: string, projectId: string, snapshotId: string, manifestSha256: string, idempotencyKey: string, timeoutMs: number, };
export type ImportRecord = { importId: string, submission: ImportSubmission, status: TaskStatus, phase: string, acceptedAtMs: number, finishedAtMs: number | null, cancelRequested: boolean, completedFiles: number, completedBytes: number, totalFiles: number | null, totalBytes: number | null, commit: string | null, sharedPath: string, workspacePath: string, reason: string | null, events: Array<JournalEvent>, };
export type SnapshotPreview = { projectId: string, snapshotId: string, manifestSha256: string, source: SnapshotSource, totalFiles: number, totalBytes: number, payloadBytes: number, sharedPath: string, workspacePath: string, files: Array<SnapshotFile>, nextOffset: number | null, payloadVerified: boolean, peerSharedPath: string | null, };
export type PublishedSnapshot = { snapshotId: string, source: SnapshotSource | null, totalFiles: number | null, totalBytes: number | null, payloadBytes: number | null, metadataError: string | null, };
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
export type TaskRecord = { taskId: string, submission: TaskSubmission, resolvedCommand: ResolvedCommand, snapshot: VerifiedSnapshotRef | null, status: TaskStatus, acceptedAtMs: number, startedAtMs: number | null, finishedAtMs: number | null, processId: number | null, exitCode: number | null, reason: string | null, cancelRequested: boolean, outputBytes: number, outputTruncated: boolean, events: Array<JournalEvent>, };
export type TaskSummary = { taskId: string, projectId: string, status: TaskStatus, acceptedAtMs: number, };
export type OutputStream = "stdout" | "stderr";
export type OutputChunk = { sequence: number, stream: OutputStream, offset: number,
/**
 * Raw bytes: UI decodes each stream incrementally, without splitting UTF-8 characters.
 */
bytes: Array<number>, };
export type RecipeInfo = { id: string, command: ResolvedCommand, };
export type ProjectInfo = { id: string, workspaceRoot: string, allowDirectCommands: boolean, recipes: Array<RecipeInfo>, };
export type ConnectionState = "connecting" | "connected" | "disconnected";
export type ControllerConnection = { url: string, state: ConnectionState, sessionId: string | null, controllerNodeId: string | null, controllerGeneration: string | null, lastConfirmedAtMs: number | null, error: string | null, };
export type ActivityLogStatus = { path: string, maxFileBytes: number, retainedArchives: number, droppedRecords: number, lastError: string | null, };
export type ControllerStatus = { instanceId: string, nodeId: string, generation: string, listenAddress: string, connected: boolean, sessionId: string | null, workerNodeId: string | null, workerGeneration: string | null, workerAddress: string | null, lastError: string | null, activityLog: ActivityLogStatus | null, };
export type WorkerInfo = { protocolVersion: number, instanceId: string, nodeId: string, generation: string, platform: string, runtimeValidation: string, projects: Array<ProjectInfo>, controllerConnection: ControllerConnection | null, snapshotImportAvailable: boolean, workflowAvailable: boolean, };
export type LocalRequest = { "method": "submit_workflow", request: WorkflowSubmission, } | { "method": "list_workflows" } | { "method": "get_workflow", workflowId: string, } | { "method": "cancel_workflow", workflowId: string, } | { "method": "list_applications" } | { "method": "get_application", applicationId: string, } | { "method": "stop_application", applicationId: string, } | { "method": "read_application_log", applicationId: string, cursor: LogCursor | null, maxBytes: number, } | { "method": "list_snapshots", projectId: string, offset: number, } | { "method": "import_snapshot", request: ImportSubmission, } | { "method": "list_imports" } | { "method": "get_import", importId: string, } | { "method": "cancel_import", importId: string, } | { "method": "preview_snapshot", projectId: string, snapshotId: string, offset: number, } | { "method": "info" } | { "method": "controller_status" } | { "method": "submit", request: TaskSubmission, } | { "method": "list_tasks" } | { "method": "get_task", taskId: string, } | { "method": "read_output", taskId: string, afterSequence: number, } | { "method": "cancel", taskId: string, };
export type LocalResponse = { "kind": "workflow", workflow: WorkflowRecord, } | { "kind": "workflows", workflows: Array<WorkflowRecord>, } | { "kind": "application", application: ApplicationRecord, } | { "kind": "applications", applications: Array<ApplicationRecord>, } | { "kind": "log", page: LogPage, } | { "kind": "snapshots", page: PublishedSnapshotPage, } | { "kind": "import", import: ImportRecord, } | { "kind": "imports", imports: Array<ImportRecord>, } | { "kind": "snapshot_preview", preview: SnapshotPreview, } | { "kind": "info", info: WorkerInfo, } | { "kind": "task", task: TaskRecord, } | { "kind": "tasks", tasks: Array<TaskSummary>, } | { "kind": "output", chunks: Array<OutputChunk>, } | { "kind": "error", message: string, } | { "kind": "controller_status", status: ControllerStatus, };
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
drafts: Array<CommandDraft>, deliveryDrafts: Array<DeliveryDraft>, };
export type ReadinessProbe = { stdoutContains: string, timeoutMs: number, };
export type WorkflowSubmission = { protocolVersion: number, requestId: string, idempotencyKey: string, name: string, projectId: string, snapshotId: string, manifestSha256: string, timeoutMs: number, build: CommandSource, start: CommandSource, readiness: ReadinessProbe,
/**
 * UTF-8 application log, relative to the registered project root, outside snapshots.
 */
logPath: string, };
export type WorkflowStepStatus = "pending" | "running" | "succeeded" | "failed" | "cancelled" | "skipped" | "indeterminate";
export type WorkflowStep = { stepId: string, name: string, status: WorkflowStepStatus, operationId: string | null, startedAtMs: number | null, finishedAtMs: number | null, reason: string | null, };
export type WorkflowRecord = { workflowId: string, submission: WorkflowSubmission, status: TaskStatus, acceptedAtMs: number, finishedAtMs: number | null, cancelRequested: boolean, reason: string | null, applicationId: string | null, steps: Array<WorkflowStep>, events: Array<JournalEvent>, };
export type ApplicationStatus = "starting" | "ready" | "stopping" | "stopped" | "exited" | "failed" | "indeterminate";
export type ApplicationRecord = { applicationId: string, workflowId: string, projectId: string, taskId: string, status: ApplicationStatus, processId: number | null, createdAtMs: number, readyAtMs: number | null, finishedAtMs: number | null, logPath: string, reason: string | null, failureReason: string | null, };
export type LogCursor = { fileIdentity: string, offset: number, prefixBytes: number, prefixSha256: string, };
export type LogPage = { applicationId: string, path: string, encoding: string, bytes: Array<number>, cursor: LogCursor, resetReason: string | null, endOfFile: boolean, };
