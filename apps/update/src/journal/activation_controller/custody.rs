//! Current LocalVerified/root/owner checks; historical inspection is separate.
use super::*;
use crate::activation::controller::{ActivationControllerSnapshot, ActivationControllerState};
use winbridge_protocol::update::{SupervisorPhase, SupervisorProcess};

fn local_edge(
    local: &ActivationRecord,
    service: &ActivationServiceRecord,
    root: &UpdateRecord,
    edge: &UpdateTransition,
) -> Result<()> {
    let source = &local.intent.source;
    let command = edge
        .command
        .as_ref()
        .ok_or_else(|| bad("Local root command missing"))?;
    if local.revision != 7
        || local.state != (ActivationState::LocalVerified {})
        || root.update_id != source.update_id
        || root.phase != UpdatePhase::AwaitingController
        || source.parent_revision.checked_add(1) != Some(root.revision)
        || root.request.expected.installation_id != source.installation_id
        || root.request.expected.scope_sha256 != source.scope_sha256
        || edge.update_id != root.update_id
        || edge.revision != root.revision
        || edge.previous_phase != Some(UpdatePhase::VerifyingLocal)
        || edge.phase != root.phase
        || edge.occurred_at_ms != root.updated_at_ms
        || root.updated_at_ms != local.updated_at_ms
        || edge.owner_id != source.owner_id
        || edge.owner_epoch != source.owner_epoch
        || command.event != winbridge_protocol::update::UpdateEvent::LocalHealthVerified
        || command.expected_revision != source.parent_revision
        || command.idempotency_key
            != format!("local-service-verified-{}", service.command.command_id)
        || root.reason.as_deref() != Some(command.reason.as_str())
    {
        return Err(bad(
            "Controller requires exact atomic LocalVerified7/AwaitingController root edge",
        ));
    }
    Ok(())
}

pub(super) fn receiver_scope(
    c: &Connection,
    id: &str,
    attempt: &str,
    actual: &SupervisorProcess,
    graph: &Graph,
) -> Result<()> {
    let local = &graph.local;
    let source = &local.intent.source;
    if schema(c)? != UPDATE_SCHEMA_VERSION
        || local.intent.context.updater_journal_schema != UPDATE_SCHEMA_VERSION
        || local.intent.attempt_id != attempt
        || source.update_id != id
        || local.created.as_ref().map(|p| &p.process) != Some(actual)
        || local.ready.as_ref().map(|p| &p.identity.process) != Some(actual)
        || local.state_owned.as_ref().map(|p| &p.process) != Some(actual)
        || activation::read(c, id)?.as_ref() != Some(local)
    {
        return Err(bad("Controller receiver requires current LocalVerified child/schema; no adoption from history"));
    }
    let root = record(c, id)?;
    let raw: String = c
        .query_row(
            "SELECT transition FROM transitions WHERE update_id=?1 AND revision=?2",
            params![id, root.revision as i64],
            |r| r.get(0),
        )
        .map_err(storage)?;
    let edge: UpdateTransition = decode(&raw)?;
    local_edge(local, &graph.service, &root, &edge)?;
    let installation = info(c)?;
    let owner: String = c
        .query_row(
            "SELECT owner FROM installation WHERE singleton=1",
            [],
            |r| r.get(0),
        )
        .map_err(storage)?;
    let s = supervisor::read(c, &source.launch_id)?;
    if installation.active_update_id.as_deref() != Some(id)
        || installation.owner_epoch != source.owner_epoch
        || owner != source.owner_id
        || s.phase != SupervisorPhase::Active
        || s.owner_id.as_deref() != Some(source.owner_id.as_str())
        || s.owner_epoch != Some(source.owner_epoch)
        || s.spec.update_id != id
        || s.process.as_ref() != Some(&local.intent.context.supervisor)
    {
        return Err(bad("Controller child has stale root/owner/supervisor"));
    }
    Ok(())
}

#[cfg(windows)]
pub(super) fn owned_local(
    c: &Connection,
    a: &UpdateAuthority,
    s: &winbridge_protocol::update::SupervisorRecord,
    preparation: &crate::activation::ActivationPreparation,
    local: &ActivationRecord,
) -> Result<Graph> {
    check_authority(c, a)?;
    runtime_baseline::current_supervisor(c, a, s)?;
    let graph = Graph::load(c, &s.spec.update_id)?;
    let actual = &local
        .state_owned
        .as_ref()
        .ok_or_else(|| bad("Controller physical ownership missing"))?
        .process;
    receiver_scope(
        c,
        &s.spec.update_id,
        &local.intent.attempt_id,
        actual,
        &graph,
    )?;
    if graph.local != *local
        || local.intent != preparation.record().intent
        || local.intent.source.owner_id != a.owner_id()
        || local.intent.source.owner_epoch != a.epoch()
        || local.intent.source.launch_id != s.spec.launch_id
        || crate::sha256(preparation.channel_nonce.as_bytes())
            != local.intent.context.channel_nonce_sha256
        || s.process.as_ref()
            != Some(
                &winbridge_platform::infrastructure::stamp(std::process::id()).map_err(storage)?,
            )
    {
        return Err(bad(
            "Controller requires original resident nonce, supervisor and local7 owner",
        ));
    }
    Ok(graph)
}

impl UpdateJournal {
    /// Data for the already OS-authenticated child; the receiver must still own
    /// the original RunningLocalService and enforce its monotonic deadline.
    pub fn inspect_activation_controller_receiver(
        directory: &Path,
        id: &str,
        attempt: &str,
        actual: &SupervisorProcess,
    ) -> Result<ActivationControllerSnapshot> {
        check_files(directory)?;
        let mut c = connection(&directory.join("updates.sqlite"), true)?;
        if schema(&c)? != UPDATE_SCHEMA_VERSION {
            return Err(bad(
                "Controller receiver requires exact current updater schema",
            ));
        }
        let tx = c.transaction().map_err(storage)?;
        let graph = Graph::load(&tx, id)?;
        receiver_scope(&tx, id, attempt, actual, &graph)?;
        let controller = ledger(&tx, id, &graph)?;
        if controller.state != (ActivationControllerState::Issued {}) {
            return Err(bad(
                "Controller delivery is not original Issued; no startup from retained result",
            ));
        }
        let result = ActivationControllerSnapshot {
            physical: graph.physical,
            writer: graph.writer,
            service: graph.service,
            local: graph.local,
            controller,
        };
        tx.commit().map_err(storage)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
