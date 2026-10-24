//! Private startup before Store/instance lock/dispatch. Ready grants no writer.
use crate::Result;
use std::path::Path;

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
        activation::{channel, ActivationReady},
        journal::UpdateJournal,
    };

    pub(super) async fn receive(config: &Path, update_id: &str, attempt_id: &str) -> Result<()> {
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
        let mut stream =
            winbridge_transport::supervisor::connect(&address, parent.identity().process_id)
                .await
                .map_err(|_| "Private activation parent connection rejected")?;
        parent.require_live().map_err(|e| e.to_string())?;
        let timeout = Duration::from_millis(record.intent.context.startup_timeout_ms);
        let challenge: channel::Challenge = channel::read(&mut stream, timeout).await?;
        channel::validate_challenge(&challenge, &record)?;
        // Expensive verification precedes Ready. Never reopen the worker Store.
        guard.revalidate()?;
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
        channel::write(&mut stream, &ready, timeout).await?;
        // No state-transfer command exists yet. Keep resources and the private
        // channel alive; EOF/unknown input/parent death exits without any writer.
        // The bounded hold is independent of whether the parent consumes Ready.
        let until = Instant::now()
            .checked_add(timeout)
            .ok_or("Activation hold deadline overflow")?;
        tokio::select! {
            // Do not cancel/restart a partially read frame while polling the
            // parent. Whichever future ends terminates the entire receiver.
            _=channel::read::<_,serde::de::IgnoredAny>(&mut stream,timeout) =>
                Err("State handoff unavailable or parent channel closed; no writer opened".into()),
            outcome=async {
                loop {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    parent.require_live().map_err(|_|"Activation parent exited; no writer opened")?;
                    if Instant::now()>=until { return Err("Pre-writer activation hold expired; no writer opened".into()); }
                }
            } => outcome,
        }
    }
}
