use std::{fs, path::Path};
use ts_rs::{Config, TS};
use winbridge_protocol::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // serde_json encodes these bounded counters/timestamps as JSON numbers.
    // Only this generator process sets the binding; no global configuration changes.
    std::env::set_var("TS_RS_LARGE_INT", "number");
    let config = Config::from_env();
    let declarations = [
        RuntimeSettings::decl(&config),
        SaveRuntimeSettings::decl(&config),
        SettingsSaved::decl(&config),
        OutputCleanupQuery::decl(&config),
        OutputCleanupEntry::decl(&config),
        OutputCleanupPlan::decl(&config),
        OutputCleanupPreview::decl(&config),
        ApplyOutputCleanup::decl(&config),
        OutputCleanupResult::decl(&config),
        HistoryKind::decl(&config),
        HistoryCursor::decl(&config),
        HistoryQuery::decl(&config),
        HistoryEntry::decl(&config),
        HistoryPage::decl(&config),
        DiagnosticRequest::decl(&config),
        DiagnosticPreview::decl(&config),
        StorageUsage::decl(&config),
        SavedDiagnostic::decl(&config),
        AttentionSeverity::decl(&config),
        AttentionSubmission::decl(&config),
        AttentionRecord::decl(&config),
        AttentionPage::decl(&config),
        SnapshotSource::decl(&config),
        SnapshotFile::decl(&config),
        SnapshotReuse::decl(&config),
        SnapshotDelta::decl(&config),
        SnapshotDeltaSummary::decl(&config),
        SnapshotPayloadChunks::decl(&config),
        SnapshotTransfer::decl(&config),
        SnapshotManifest::decl(&config),
        SnapshotReady::decl(&config),
        ImportSubmission::decl(&config),
        ImportRecord::decl(&config),
        SnapshotPreview::decl(&config),
        PublishedSnapshot::decl(&config),
        PublishedSnapshotPage::decl(&config),
        VerifiedSnapshotRef::decl(&config),
        TaskStatus::decl(&config),
        CommandSource::decl(&config),
        TaskSubmission::decl(&config),
        ResolvedCommand::decl(&config),
        JournalEvent::decl(&config),
        TaskRecord::decl(&config),
        TaskSummary::decl(&config),
        OutputStream::decl(&config),
        OutputChunk::decl(&config),
        RecipeInfo::decl(&config),
        RecipeOrigin::decl(&config),
        RecipeProfileInfo::decl(&config),
        CommandHandlerInfo::decl(&config),
        ProjectInfo::decl(&config),
        ConnectionState::decl(&config),
        ControllerConnection::decl(&config),
        ActivityLogStatus::decl(&config),
        ControllerStatus::decl(&config),
        ResourceRecoveryEntry::decl(&config),
        ResourceRecovery::decl(&config),
        WorkerInfo::decl(&config),
        BuildIdentity::decl(&config),
        LocalRequest::decl(&config),
        LocalResponse::decl(&config),
        TaskReport::decl(&config),
        CommandDraft::decl(&config),
        DeliveryDraft::decl(&config),
        DesktopDraftState::decl(&config),
        NotificationPreferences::decl(&config),
        NotificationCursor::decl(&config),
        CompanionStatus::decl(&config),
        AttentionTarget::decl(&config),
        AttentionEvent::decl(&config),
        AttentionEventPage::decl(&config),
        DesktopValidationStatus::decl(&config),
        WorkflowProfile::decl(&config),
        WorkflowRecipePin::decl(&config),
        ReadinessProbe::decl(&config),
        NetwatchProbe::decl(&config),
        ArtifactSpec::decl(&config),
        BuildArtifact::decl(&config),
        WorkflowSubmission::decl(&config),
        WorkflowStepStatus::decl(&config),
        WorkflowStep::decl(&config),
        WorkflowRecord::decl(&config),
        ApplicationStatus::decl(&config),
        ApplicationRecord::decl(&config),
        LogCursor::decl(&config),
        LogPage::decl(&config),
    ];
    let text = format!(
        "// Generated from crates/protocol by npm run generate:contracts. Do not edit.\n{}\n",
        declarations
            .iter()
            .map(|d| {
                let declaration = format!("export {d}");
                declaration
                    .lines()
                    .map(str::trim_end)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect::<Vec<_>>()
            .join("\n")
    );
    if text.contains("bigint") {
        return Err("JSON bindings must use number for bounded counters/timestamps".into());
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/runtime/contracts.ts");
    if std::env::args().any(|arg| arg == "--check") {
        if fs::read_to_string(&path)? != text {
            return Err(
                "UI contracts differ from Rust DTOs; run npm run generate:contracts".into(),
            );
        }
        println!("UI contracts match Rust DTOs");
    } else {
        fs::write(path, text)?;
        println!("UI contracts generated from Rust DTOs");
    }
    Ok(())
}
