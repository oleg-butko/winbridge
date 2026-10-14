//! Create-only physical coverage outside the database being restored.
use crate::{recovery, store::Store, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
use winbridge_core::config::WorkerConfig;
use winbridge_platform::execution_domain::{DomainIdentity, ExecutionDomain};
use winbridge_protocol::{RecoveryCoverageInfo, RequestContext};

pub(crate) const WITNESS: &str = "execution-domain.json";
const MEMBERS: &str = "execution-domains";
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Coverage {
    FreshJournal,
    QuiescentCurrentJournal,
    LegacyUncovered,
    UnsupportedPlatform,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Witness {
    pub format_version: u16,
    pub context: RequestContext,
    pub state_directory: std::path::PathBuf,
    pub identity: Option<DomainIdentity>,
    pub coverage: Coverage,
}
impl Witness {
    fn scope(&self) -> Result<String> {
        serde_json::to_string(&(self.context.clone(), self.state_directory.clone()))
            .map_err(|e| e.to_string())
    }
    pub fn complete(&self) -> bool {
        matches!(
            self.coverage,
            Coverage::FreshJournal | Coverage::QuiescentCurrentJournal
        ) && self.identity.is_some()
    }
    pub fn info(&self) -> RecoveryCoverageInfo {
        RecoveryCoverageInfo {
            complete: self.complete(),
            reason: match self.coverage {
                Coverage::FreshJournal => "Вся область исполнения нового журнала учтена отдельно от SQLite; её состояние проверяется при восстановлении",
                Coverage::QuiescentCurrentJournal => "Область исполнения учтена после проверки текущего журнала без незавершённых владельцев; восстановление SQLite требует отдельной проверки этой области",
                Coverage::LegacyUncovered => "Прежние владельцы не подтверждены. Архив доступен, автоматическая активация новой пары запрещена",
                Coverage::UnsupportedPlatform => "Это Unix адаптер разработки; состояние Windows процессов здесь подтвердить нельзя",
            }.into(),
        }
    }
}
pub(crate) fn context(config: &WorkerConfig, store: &Store) -> RequestContext {
    RequestContext {
        instance_id: config.instance_id.clone(),
        journal_incarnation: store.journal_authority().incarnation.clone(),
    }
}
pub(crate) fn load(state: &Path, expected: &RequestContext) -> Result<Witness> {
    let path = state.join(WITNESS);
    winbridge_platform::require_safe_path(&path).map_err(|e| e.to_string())?;
    let witness: Witness =
        serde_json::from_slice(&winbridge_workspace_sync::read_bounded(&path, 8192)?)
            .map_err(|_| "Invalid execution-domain witness; recovery admission is blocked")?;
    if witness.format_version != 1
        || &witness.context != expected
        || fs::canonicalize(state).map_err(|e| e.to_string())? != witness.state_directory
    {
        return Err("Execution-domain witness belongs to another journal/state directory; recovery admission is blocked".into());
    }
    if let Some(identity) = &witness.identity {
        identity.validate().map_err(|e| e.to_string())?;
        if identity.job_name
            != ExecutionDomain::job_name(&witness.scope()?, identity.session_id, &identity.owner_id)
        {
            return Err("Physical domain scope/name mismatch".into());
        }
    }
    if (cfg!(windows) && witness.identity.is_none())
        || (witness.coverage == Coverage::UnsupportedPlatform && witness.identity.is_some())
    {
        return Err("Physical witness/platform shape is invalid; no execution allowed".into());
    }
    all_identities(&witness)?;
    Ok(witness)
}
pub(crate) fn all_identities(witness: &Witness) -> Result<Vec<DomainIdentity>> {
    let mut result = witness.identity.iter().cloned().collect::<Vec<_>>();
    let directory = witness.state_directory.join(MEMBERS);
    winbridge_platform::require_safe_path(&directory).map_err(|e| e.to_string())?;
    if directory.try_exists().map_err(|e| e.to_string())? {
        for entry in fs::read_dir(&directory).map_err(|e| e.to_string())? {
            if result.len() >= 4096 {
                return Err(
                    "Execution-domain registry exceeds 4096 owners; recovery is blocked".into(),
                );
            }
            let path = entry.map_err(|e| e.to_string())?.path();
            winbridge_platform::require_safe_path(&path).map_err(|e| e.to_string())?;
            let identity: DomainIdentity =
                serde_json::from_slice(&winbridge_workspace_sync::read_bounded(&path, 8192)?)
                    .map_err(|_| "Incomplete execution-domain member; no effects allowed")?;
            identity.validate().map_err(|e| e.to_string())?;
            if path.file_name().and_then(|n| n.to_str())
                != Some(format!("owner-{}.json", identity.owner_id).as_str())
                || identity.job_name
                    != ExecutionDomain::job_name(
                        &witness.scope()?,
                        identity.session_id,
                        &identity.owner_id,
                    )
                || witness
                    .identity
                    .as_ref()
                    .is_none_or(|first| first.host_id != identity.host_id)
                || result.iter().any(|old| old.owner_id == identity.owner_id)
            {
                return Err("Foreign/duplicate execution-domain member; no effects allowed".into());
            }
            result.push(identity);
        }
    }
    result.sort_by(|a, b| a.owner_id.cmp(&b.owner_id));
    Ok(result)
}
pub(crate) fn fingerprint(state: &Path, context: &RequestContext) -> Result<String> {
    use sha2::{Digest, Sha256};
    let witness = load(state, context)?;
    let identities = all_identities(&witness)?;
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(witness, identities)).map_err(|e| e.to_string())?)
    ))
}
pub(crate) fn fresh_witness(
    config: &WorkerConfig,
    store: &Store,
    coverage: Coverage,
) -> Result<Witness> {
    let context = context(config, store);
    let state_directory = fs::canonicalize(&config.state_directory).map_err(|e| e.to_string())?;
    let scope = serde_json::to_string(&(context.clone(), state_directory.clone()))
        .map_err(|e| e.to_string())?;
    let identity = ExecutionDomain::identity_for(&scope, &store.journal_authority().authority_id)
        .map_err(|e| e.to_string())?;
    Ok(Witness {
        format_version: 1,
        context,
        state_directory,
        coverage: if identity.is_none() {
            Coverage::UnsupportedPlatform
        } else {
            coverage
        },
        identity,
    })
}
pub(crate) fn initialize(
    config: &WorkerConfig,
    store: &Store,
    fresh: bool,
) -> Result<(Option<ExecutionDomain>, RecoveryCoverageInfo)> {
    let path = config.state_directory.join(WITNESS);
    let witness = if path.try_exists().map_err(|e| e.to_string())? {
        load(&config.state_directory, &context(config, store))?
    } else {
        // Normal, non-restored startup only, after reconciliation and before IPC.
        let coverage = if fresh {
            Coverage::FreshJournal
        } else if store.inspection_resource_summary()?.is_none()
            && store.applications(false)?.is_empty()
        {
            Coverage::QuiescentCurrentJournal
        } else {
            Coverage::LegacyUncovered
        };
        let witness = fresh_witness(config, store, coverage)?;
        recovery::persist_new_json(&path, &witness)?;
        witness
    };
    let current =
        ExecutionDomain::identity_for(&witness.scope()?, &store.journal_authority().authority_id)
            .map_err(|e| e.to_string())?;
    if let Some(identity) = &current {
        if witness
            .identity
            .as_ref()
            .is_none_or(|first| first.host_id != identity.host_id)
        {
            return Err(
                "Physical registry belongs to another Windows host; no effects allowed".into(),
            );
        }
        if !all_identities(&witness)?.iter().any(|old| old == identity) {
            let directory = config.state_directory.join(MEMBERS);
            winbridge_platform::require_safe_path(&directory).map_err(|e| e.to_string())?;
            if !directory.exists() {
                let builder = fs::DirBuilder::new();
                #[cfg(unix)]
                let mut builder = builder;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    builder.mode(0o700);
                }
                builder.create(&directory).map_err(|e| e.to_string())?;
            }
            recovery::persist_new_json(
                &directory.join(format!("owner-{}.json", identity.owner_id)),
                identity,
            )?;
        }
    }
    let owner = current
        .as_ref()
        .map(ExecutionDomain::open)
        .transpose()
        .map_err(|e| format!("Execution domain: {e}"))?;
    Ok((owner, witness.info()))
}
