//! Fresh, process-authenticated observations for the trusted source driver.
//! Use on a blocking thread; no cached JSON or agent-supplied endpoint is accepted.
use crate::{sha256, source::SourceControl};
use std::path::{Path, PathBuf};
use tokio::runtime::Handle;
use winbridge_core::config::WorkerConfig;
use winbridge_platform::infrastructure;
use winbridge_protocol::{
    update::SupervisorWorkerBinding, LocalRequest, LocalResponse, WorkerInfo,
};
use winbridge_transport::{self as transport, supervisor as ipc};

type Result<T> = std::result::Result<T, String>;

pub struct WorkerSourceControl {
    binding: SupervisorWorkerBinding,
    config: PathBuf,
    endpoint: String,
    io: Handle,
}
impl WorkerSourceControl {
    pub(crate) fn io(&self) -> &Handle {
        &self.io
    }
    /// Binding comes from the immutable worker-owned supervisor spec. This is a
    /// readonly adapter; it cannot acquire or manufacture a source lease.
    pub fn new(binding: SupervisorWorkerBinding, config: &Path, io: Handle) -> Result<Self> {
        let bytes = crate::read(config, 64 * 1024)?;
        if sha256(&bytes) != binding.config_sha256 {
            return Err("Worker config differs from the frozen source binding".into());
        }
        let cfg: WorkerConfig = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        cfg.validate()?;
        if cfg.instance_id != binding.context.instance_id {
            return Err("Source observation config belongs to another instance".into());
        }
        let endpoint = transport::endpoint(&cfg.state_directory, &cfg.instance_id)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            binding,
            config: config.into(),
            endpoint,
            io,
        })
    }
    fn require_binding(&self) -> Result<()> {
        if sha256(&crate::read(&self.config, 64 * 1024)?) != self.binding.config_sha256
            || !infrastructure::matches(&self.binding.process).map_err(|e| e.to_string())?
        {
            return Err("Bound worker process/config changed; no automatic rebind".into());
        }
        Ok(())
    }
}
impl SourceControl for WorkerSourceControl {
    fn observe_worker(&mut self) -> Result<WorkerInfo> {
        self.require_binding()?;
        let info = self.io.block_on(async {
            tokio::time::timeout(transport::IPC_TIMEOUT, async {
                // Authenticate the actual pipe/socket server before sending the
                // ordinary readonly Info request. Its frame limit remains 1 MiB.
                let mut stream = ipc::connect(&self.endpoint, self.binding.process.process_id)
                    .await
                    .map_err(|e| e.to_string())?;
                transport::write_frame(
                    &mut stream,
                    &serde_json::to_vec(&LocalRequest::Info).map_err(|e| e.to_string())?,
                )
                .await
                .map_err(|e| e.to_string())?;
                let bytes = transport::read_frame(&mut stream)
                    .await
                    .map_err(|e| e.to_string())?;
                match serde_json::from_slice::<LocalResponse>(&bytes).map_err(|e| e.to_string())? {
                    LocalResponse::Info { info } => Ok(*info),
                    LocalResponse::Error { message } => Err(message),
                    _ => Err("Expected fresh worker Info for source effects".into()),
                }
            })
            .await
            .map_err(|_| "Worker source observation timed out; preserve lease".to_owned())?
        })?;
        self.require_binding()?;
        if info.instance_id != self.binding.context.instance_id
            || info.generation != self.binding.generation
            || info.build.as_ref() != Some(&self.binding.build)
            || info.fenced_requests.as_ref().map(|v| &v.context) != Some(&self.binding.context)
            || info.journal_recovery_required.is_some()
            || info.journal_inspection.is_some()
            || info.maintenance_error.is_some()
        {
            return Err(
                "Fresh worker observation scope/build/health changed; preserve lease".into(),
            );
        }
        Ok(info)
    }
}
