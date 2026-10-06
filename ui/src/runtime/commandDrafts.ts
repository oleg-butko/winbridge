import { invoke } from "@tauri-apps/api/core";
import type { CommandDraft, DeliveryDraft, DesktopDraftState, WorkflowProfile } from "./contracts";

export type CommandForm = Pick<CommandDraft, "snapshotId" | "recipeId" | "executable" | "argumentsJson" | "workingDirectory" | "timeoutSeconds">;
export const emptyCommandForm: CommandForm = { recipeId: null, executable: "", argumentsJson: "[]", workingDirectory: ".", timeoutSeconds: "60" };

export function loadCommandDrafts(): Promise<DesktopDraftState> {
  return invoke<DesktopDraftState>("load_command_drafts");
}
export function saveCommandDraft(draft: CommandDraft): Promise<void> {
  return invoke<void>("save_command_draft", { draft });
}
export function saveDeliveryDraft(draft: DeliveryDraft): Promise<void> {
  return invoke<void>("save_delivery_draft", { draft });
}
export function saveWorkflowProfile(profile: WorkflowProfile): Promise<void> {
  return invoke<void>("save_workflow_profile", { profile });
}
export function sameDraftScope(left: CommandDraft, right: Pick<CommandDraft, "workerInstanceId" | "projectId" | "workspaceRoot">): boolean {
  return left.workerInstanceId === right.workerInstanceId && left.projectId === right.projectId && left.workspaceRoot === right.workspaceRoot;
}
