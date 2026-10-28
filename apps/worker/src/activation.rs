//! Private startup before Store/instance lock/dispatch. Ready grants no writer.
use crate::Result;
use std::path::Path;
#[cfg(any(windows, test))]
fn verification_remaining(
    deadline: std::time::Instant,
    now: std::time::Instant,
    wall_ms: u64,
    admitted_ms: u64,
    timeout_ms: u64,
) -> Result<std::time::Duration> {
    let end = admitted_ms
        .checked_add(timeout_ms)
        .ok_or("Activation deadline overflow")?;
    if wall_ms < admitted_ms || wall_ms >= end {
        return Err("Original activation window expired or clock moved backwards".into());
    }
    let monotonic = deadline
        .checked_duration_since(now)
        .filter(|d| !d.is_zero())
        .ok_or_else(|| "Original monotonic activation window expired".to_string())?;
    Ok(monotonic.min(std::time::Duration::from_millis(end - wall_ms)))
}
#[cfg(windows)]
#[path = "activation_controller.rs"]
mod controller;
#[cfg(windows)]
#[path = "activation_lifetime.rs"]
mod lifetime;
#[cfg(windows)]
#[path = "activation_service.rs"]
mod service;

/// Construct the graph-heavy startup future on its own stack, as the original
/// supervisor does. Do not enlarge the ordinary worker main coroutine frame.
pub fn run(config: &Path, update_id: &str, attempt_id: &str) -> Result<()> {
    #[cfg(not(windows))]
    {
        let _ = (config, update_id, attempt_id);
        Err("Private activation receiver requires Windows; no worker journal opened".into())
    }
    #[cfg(windows)]
    {
        let (config, update, attempt) = (
            config.to_owned(),
            update_id.to_owned(),
            attempt_id.to_owned(),
        );
        std::thread::Builder::new()
            .name("activation-receiver".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| e.to_string())?
                    .block_on(Box::pin(receive(&config, &update, &attempt)))
            })
            .map_err(|_| "Activation receiver owner thread could not start")?
            .join()
            .map_err(|_| "Activation receiver owner thread stopped; no retry")?
    }
}
pub async fn receive(config: &Path, update_id: &str, attempt_id: &str) -> Result<()> {
    std::hint::black_box(winbridge_update::activation::channel::RECEIVER_MARKER);
    std::hint::black_box(winbridge_update::activation::channel::HANDOFF_MARKER);
    std::hint::black_box(winbridge_update::activation::channel::WRITER_MARKER);
    std::hint::black_box(winbridge_update::activation::channel::SERVICE_MARKER);
    std::hint::black_box(winbridge_update::activation::channel::CONTROLLER_MARKER);
    #[cfg(windows)]
    std::hint::black_box(winbridge_protocol::update::ACTIVATION_LIFETIME_MARKER);
    #[cfg(not(windows))]
    {
        let _ = (config, update_id, attempt_id);
        Err("Private activation receiver requires Windows; no worker journal opened".into())
    }
    #[cfg(windows)]
    {
        native::receive(config, update_id, attempt_id).await
    }
}

#[cfg(any(windows, test))]
fn inspect_state_journal(
    path: &Path,
    context: &winbridge_protocol::RequestContext,
    schema: u32,
    expected: &winbridge_protocol::update::WorkerShutdownReceipt,
) -> Result<()> {
    use crate::store::{Store, JOURNAL_SCHEMA_VERSION};
    use winbridge_protocol::update::WorkerUpdatePhase;
    if schema != JOURNAL_SCHEMA_VERSION
        || expected.lease.phase != WorkerUpdatePhase::Quiesced
        || context.instance_id != expected.lease.spec.instance_id
        || context.journal_incarnation != expected.lease.spec.journal_incarnation
    {
        return Err("Physical state journal schema/quiesced logical scope differs".into());
    }
    // Exact schema, query_only and existing file: no migration or owner epoch.
    let store = Store::open_read_only(path)?;
    if store.journal_authority().incarnation != context.journal_incarnation
        || store.worker_update_lease(&expected.update_id)? != expected.lease
        || store
            .worker_shutdown_receipt(&expected.update_id, &expected.command)?
            .as_ref()
            != Some(expected)
    {
        return Err("Physical state journal differs from the accepted shutdown/lease".into());
    }
    Ok(())
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::{
        fs::{File, OpenOptions},
        os::windows::fs::OpenOptionsExt,
        time::{Duration, Instant},
    };
    use winbridge_platform::infrastructure::PinnedProcess;
    use winbridge_update::{
        activation::{channel, ActivationReady, ActivationRecord, ActivationStateOwned},
        journal::UpdateJournal,
    };

    pub(super) async fn receive(config: &Path, update_id: &str, attempt_id: &str) -> Result<()> {
        let mut timing = winbridge_update::activation::ActivationTiming::new("receiver", None);
        for id in [update_id, attempt_id] {
            if uuid::Uuid::parse_str(id)
                .ok()
                .map(|id| id.to_string())
                .as_deref()
                != Some(id)
            {
                return Err("Private activation requires canonical update/attempt UUIDs".into());
            }
        }
        winbridge_platform::require_local_path_prefix(config).map_err(|e| e.to_string())?;
        winbridge_platform::require_safe_path(config).map_err(|e| e.to_string())?;
        winbridge_platform::require_local_disk(config).map_err(|e| e.to_string())?;
        let config = config.canonicalize().map_err(|e| e.to_string())?;
        // Deny both writes and deletes throughout the private startup session.
        let config_pin: File = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&config)
            .map_err(|e| e.to_string())?; // FILE_SHARE_READ
        let guard = crate::runtime_guard::prepare_runtime_guard(&config)?
            .ok_or("Private activation requires a known installed set; no journal opened")?;
        let identity = guard.identity().clone();
        timing.mark("runtime_pinned", None, None);
        let directory = Path::new(&identity.image.path)
            .parent()
            .ok_or("Loaded worker has no runtime")?
            .join("update-state");
        let record = UpdateJournal::inspect_activation_receiver(
            &directory,
            update_id,
            attempt_id,
            &identity.process,
        )
        .map_err(|e| e.to_string())?;
        // Freeze the child's remaining original window once. Receiving another
        // message or a wall-clock rollback must not start another timeout.
        let handoff_deadline = original_handoff_deadline(&record)?;
        let remaining = || remaining_handoff(&record, handoff_deadline);
        timing.observe_deadline(handoff_deadline);
        timing.mark("initial_graph_checked", None, None);
        let check = crate::compatibility::check_config(
            &config,
            &record.intent.context.worker_config.sha256,
        )?;
        if check.config != record.intent.context.worker_config
            || winbridge_platform::file_identity(&config_pin).map_err(|e| e.to_string())?
                != check.config.file_identity
            || check.worker_config_schema != record.intent.context.worker_config_schema
        {
            return Err(
                "Private activation config identity/schema differs; no journal opened".into(),
            );
        }
        let parent =
            PinnedProcess::live(&record.intent.context.supervisor).map_err(|e| e.to_string())?;
        let address = channel::endpoint(&directory, attempt_id)?;
        let mut stream = tokio::time::timeout(
            remaining()?,
            winbridge_transport::supervisor::connect(&address, parent.identity().process_id),
        )
        .await
        .map_err(|_| "Original activation window expired during parent connection")?
        .map_err(|_| "Private activation parent connection rejected")?;
        parent.require_live().map_err(|e| e.to_string())?;
        let challenge: channel::Challenge = channel::read(&mut stream, remaining()?).await?;
        channel::validate_challenge(&challenge, &record)?;
        timing.mark("challenge_received", None, None);
        // Expensive verification precedes Ready. Never reopen the worker Store.
        guard.revalidate()?;
        timing.mark("runtime_revalidated_for_ready", None, None);
        parent.require_live().map_err(|e| e.to_string())?;
        if UpdateJournal::inspect_activation_receiver(
            &directory,
            update_id,
            attempt_id,
            &identity.process,
        )
        .map_err(|e| e.to_string())?
            != record
        {
            return Err("Private activation changed before Ready; no writer opened".into());
        }
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis();
        let at = u64::try_from(at).map_err(|_| "Activation clock overflow")?;
        let ready = channel::Ready {
            contract_version: channel::CONTRACT,
            ready: ActivationReady {
                attempt_id: attempt_id.into(),
                identity,
                worker_config_sha256: check.config.sha256,
                observed_at_ms: at,
            },
        };
        channel::validate_ready(&ready, &record)?;
        channel::write(&mut stream, &ready, remaining()?).await?;
        timing.mark("ready_sent", None, None);
        // One bounded read remains live while checking the original parent.
        // V1 Ready is unchanged; V2 is a separate typed handoff, never a writer.
        let request: channel::StateHandoff = tokio::select! {
            value=channel::read(&mut stream,remaining()?) => value?,
            outcome=parent_alive(&parent, remaining()?) => return outcome,
        };
        timing.mark("handoff_received", None, None);
        let snapshot = UpdateJournal::inspect_activation_state_handoff(
            &directory,
            update_id,
            attempt_id,
            &ready.ready.identity.process,
        )
        .map_err(|e| e.to_string())?;
        let issued = &snapshot.record;
        timing.mark("handoff_graph_checked", None, None);
        channel::validate_state_handoff(&request, issued)?;
        if issued.intent != record.intent || issued.ready.as_ref() != Some(&ready.ready) {
            return Err("State handoff changed the authenticated Ready attempt".into());
        }
        let cfg = winbridge_core::config::WorkerConfig::load(&config)?;
        let pins = winbridge_update::shutdown::ShutdownStatePins::capture(&cfg.state_directory)
            .map_err(|e| e.to_string())?;
        if cfg.instance_id != issued.intent.context.worker_context.instance_id
            || pins.observation() != &issued.intent.context.worker_state
        {
            return Err("State handoff directory/journal/lock identities changed".into());
        }
        guard.revalidate()?;
        timing.mark("runtime_revalidated_for_handoff", None, None);
        parent.require_live().map_err(|e| e.to_string())?;
        if UpdateJournal::inspect_activation_state_handoff(
            &directory,
            update_id,
            attempt_id,
            &ready.ready.identity.process,
        )
        .map_err(|e| e.to_string())?
            != snapshot
        {
            return Err("State handoff changed before acquiring the existing lock".into());
        }
        require_handoff_window(issued, handoff_deadline)?;
        let state = pins
            .try_lock()
            .map_err(|e| format!("Exact state acquisition failed; no retry: {e}"))?;
        timing.mark("instance_lock_acquired", None, None);
        super::inspect_state_journal(
            &cfg.state_directory.join("journal.sqlite"),
            &issued.intent.context.worker_context,
            issued.intent.context.worker_journal_schema,
            &snapshot.shutdown_receipt,
        )?;
        state.revalidate().map_err(|e| e.to_string())?;
        parent.require_live().map_err(|e| e.to_string())?;
        if UpdateJournal::inspect_activation_state_handoff(
            &directory,
            update_id,
            attempt_id,
            &ready.ready.identity.process,
        )
        .map_err(|e| e.to_string())?
            != snapshot
        {
            return Err("State handoff changed before the physical ownership receipt".into());
        }
        let owned = channel::StateOwned {
            contract_version: channel::HANDOFF_CONTRACT,
            state_owned: ActivationStateOwned {
                attempt_id: attempt_id.into(),
                process: ready.ready.identity.process.clone(),
                context: issued.intent.context.worker_context.clone(),
                generation: uuid::Uuid::new_v4().to_string(),
                state: state.observation().clone(),
                observed_at_ms: now()?,
            },
        };
        channel::validate_state_owned(&owned, issued)?;
        channel::write(&mut stream, &owned, remaining()?).await?;
        timing.mark("state_owned_sent", None, None);
        // V3 is a separate private command, not permission inferred from V2.
        let request: channel::WriterCommand = tokio::select! {
            value=channel::read(&mut stream,remaining()?)=>value?,
            outcome=parent_alive(&parent,remaining()?)=>return outcome,
        };
        timing.mark("writer_received", None, None);
        let writer_snapshot = UpdateJournal::inspect_activation_writer_receiver(
            &directory,
            update_id,
            attempt_id,
            &ready.ready.identity.process,
        )
        .map_err(|e| e.to_string())?;
        timing.mark("writer_graph_checked", None, None);
        channel::validate_writer_command(&request, &writer_snapshot.writer.command)?;
        if writer_snapshot.activation.intent != record.intent
            || writer_snapshot.activation.state_owned.as_ref() != Some(&owned.state_owned)
            || request.command.binding.physical != owned.state_owned
        {
            return Err("Writer changed the original same-child physical ownership".into());
        }
        let mut checkpoint_timing = winbridge_update::activation::ActivationTiming::new(
            "receiver_writer_checkpoint",
            Some(handoff_deadline),
        );
        let mut checkpoint = || {
            parent.require_live().map_err(|e| e.to_string())?;
            guard.revalidate()?;
            checkpoint_timing.mark("runtime_checked", None, None);
            require_handoff_window(&writer_snapshot.activation, handoff_deadline)?;
            let current = UpdateJournal::inspect_activation_writer_receiver(
                &directory,
                update_id,
                attempt_id,
                &ready.ready.identity.process,
            )
            .map_err(|e| e.to_string())?;
            if current != writer_snapshot {
                return Err(
                    "Writer admission changed during existing-state preparation/commit".into(),
                );
            }
            checkpoint_timing.mark("graph_checked", None, None);
            require_handoff_window(&writer_snapshot.activation, handoff_deadline)
        };
        let mut journal = crate::store::ActivationJournal::open(
            state,
            request.command.clone(),
            writer_snapshot.shutdown_receipt.clone(),
            &mut checkpoint,
        )
        .map_err(|e| e.to_string())?;
        timing.mark("existing_journal_opened", None, None);
        journal.constrain_startup_deadline(handoff_deadline);
        let crate::store::JournalActivationAcceptance::Fresh(fresh) =
            journal.activate(&mut checkpoint)?
        else {
            return Err(
                "Observed journal receipt grants no fresh private runtime ownership".into(),
            );
        };
        timing.mark("authority_committed", None, None);
        let response = channel::WriterReceipt {
            contract_version: channel::WRITER_CONTRACT,
            receipt: fresh.receipt().clone(),
        };
        channel::validate_writer_receipt(&response, &request.command)?;
        channel::write(&mut stream, &response, remaining()?).await?;
        service::Receiver {
            config,
            directory,
            writer: writer_snapshot,
            parent,
            guard,
            journal,
            fresh,
            deadline: handoff_deadline,
            stream,
        }
        .run()
        .await
    }

    fn now() -> Result<u64> {
        u64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_millis(),
        )
        .map_err(|_| "Activation clock overflow".into())
    }
    fn original_handoff_deadline(record: &ActivationRecord) -> Result<Instant> {
        let end = record
            .intent
            .admitted_at_ms
            .checked_add(record.intent.context.startup_timeout_ms)
            .ok_or("State handoff deadline overflow")?;
        let remaining = end
            .saturating_sub(now()?)
            .min(record.intent.context.startup_timeout_ms);
        Instant::now()
            .checked_add(Duration::from_millis(remaining))
            .ok_or_else(|| "State handoff monotonic deadline overflow".into())
    }
    pub(super) fn require_handoff_window(
        record: &ActivationRecord,
        deadline: Instant,
    ) -> Result<()> {
        remaining_handoff(record, deadline).map(|_| ())
    }
    fn remaining_handoff(record: &ActivationRecord, deadline: Instant) -> Result<Duration> {
        super::verification_remaining(
            deadline,
            Instant::now(),
            now()?,
            record.intent.admitted_at_ms,
            record.intent.context.startup_timeout_ms,
        )
    }
    pub(super) async fn parent_alive(parent: &PinnedProcess, timeout: Duration) -> Result<()> {
        let until = Instant::now()
            .checked_add(timeout)
            .ok_or("Activation hold deadline overflow")?;
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            parent
                .require_live()
                .map_err(|_| "Activation parent exited; no further startup action")?;
            if Instant::now() >= until {
                return Err("Private activation hold expired; no further startup action".into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verification_io_uses_remaining_original_budget_and_refuses_clock_rollback() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        let deadline = now + Duration::from_secs(10);
        assert_eq!(
            verification_remaining(deadline, now, 240500, 1000, 240000).unwrap(),
            Duration::from_millis(500),
            "A forward wall-clock jump must reduce IO budget too"
        );
        assert!(
            verification_remaining(deadline, now, 121000, 1000, 120000).is_err(),
            "A retained 120-second attempt must not receive a new 240-second budget"
        );
        assert_eq!(
            verification_remaining(deadline, now, 1001, 1000, 240000).unwrap(),
            Duration::from_secs(10)
        );
        assert_eq!(
            verification_remaining(deadline, now + Duration::from_secs(3), 4001, 1000, 240000)
                .unwrap(),
            Duration::from_secs(7)
        );
        for (at, wall, admitted, budget) in [
            (deadline, 1001, 1000, 240000),
            (now, 999, 1000, 240000),
            (now, 241000, 1000, 240000),
            (now, u64::MAX, u64::MAX, 1),
        ] {
            assert!(verification_remaining(deadline, at, wall, admitted, budget).is_err());
        }
    }
    use crate::store::{shutdown_metadata_fixture, ShutdownAcceptance, JOURNAL_SCHEMA_VERSION};
    use winbridge_protocol::RequestContext;

    #[test]
    fn physical_journal_readonly_requires_the_exact_accepted_quiesced_shutdown() {
        // Metadata only. No physical ownership or permission is claimed here.
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("journal.sqlite");
        let (mut store, pending, _) = shutdown_metadata_fixture(&path);
        let context = RequestContext {
            instance_id: pending.lease.spec.instance_id.clone(),
            journal_incarnation: pending.lease.spec.journal_incarnation.clone(),
        };
        assert!(inspect_state_journal(&path, &context, JOURNAL_SCHEMA_VERSION, &pending).is_err());
        let ShutdownAcceptance::Fresh(permit) = store.accept_worker_shutdown(pending).unwrap()
        else {
            panic!("fresh receipt")
        };
        let accepted = permit.receipt().clone();
        let authority = store.journal_authority().clone();
        inspect_state_journal(&path, &context, JOURNAL_SCHEMA_VERSION, &accepted).unwrap();
        assert_eq!(store.journal_authority(), &authority);
        assert_eq!(
            store.active_worker_update().unwrap().as_ref(),
            Some(&accepted.lease)
        );
        for index in 0..5 {
            let mut ctx = context.clone();
            let mut receipt = accepted.clone();
            match index {
                0 => ctx.instance_id.push('x'),
                1 => ctx.journal_incarnation = uuid::Uuid::new_v4().to_string(),
                2 => receipt.process.birth.push('0'),
                3 => receipt.lease.spec.updater_owner_id = uuid::Uuid::new_v4().to_string(),
                _ => receipt.command.command_id.push('x'),
            }
            assert!(inspect_state_journal(&path, &ctx, JOURNAL_SCHEMA_VERSION, &receipt).is_err());
        }
        assert!(
            inspect_state_journal(&path, &context, JOURNAL_SCHEMA_VERSION - 1, &accepted).is_err()
        );
        assert_eq!(store.journal_authority(), &authority);
        assert_eq!(
            store
                .worker_shutdown_receipt(&accepted.update_id, &accepted.command)
                .unwrap(),
            Some(accepted)
        );
    }
    #[test]
    fn physical_journal_readonly_never_migrates_an_old_or_future_schema() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("journal.sqlite");
        let (mut store, pending, _) = shutdown_metadata_fixture(&path);
        let context = RequestContext {
            instance_id: pending.lease.spec.instance_id.clone(),
            journal_incarnation: pending.lease.spec.journal_incarnation.clone(),
        };
        let ShutdownAcceptance::Fresh(permit) = store.accept_worker_shutdown(pending).unwrap()
        else {
            panic!("fresh receipt")
        };
        let accepted = permit.receipt();
        for version in [JOURNAL_SCHEMA_VERSION - 1, JOURNAL_SCHEMA_VERSION + 1] {
            store
                .connection
                .pragma_update(None, "user_version", version)
                .unwrap();
            assert!(
                inspect_state_journal(&path, &context, JOURNAL_SCHEMA_VERSION, accepted).is_err()
            );
            let actual: u32 = store
                .connection
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap();
            assert_eq!(actual, version);
        }
    }
}
