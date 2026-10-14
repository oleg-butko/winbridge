//! An outer Job covers all managed children, independently of task rows.
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DomainIdentity {
    pub job_name: String,
    pub host_id: String,
    pub session_id: u32,
    pub owner_id: String,
}
impl DomainIdentity {
    pub fn validate(&self) -> io::Result<()> {
        let suffix = self.job_name.strip_prefix("Global\\WinBridge-Domain-");
        if !suffix.is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            || self.host_id.len() != 64
            || !self.host_id.bytes().all(|b| b.is_ascii_hexdigit())
            || self.owner_id.len() != 36
            || !self.owner_id.bytes().enumerate().all(|(i, b)| {
                if [8, 13, 18, 23].contains(&i) {
                    b == b'-'
                } else {
                    b.is_ascii_hexdigit() && !b.is_ascii_uppercase()
                }
            })
        {
            return Err(io::Error::other("Invalid execution domain identity"));
        }
        Ok(())
    }
}

pub struct ExecutionDomain {
    #[cfg(windows)]
    pub(crate) job: super::windows::Job,
    identity: DomainIdentity,
}
impl ExecutionDomain {
    pub fn identity_for(scope: &str, owner_id: &str) -> io::Result<Option<DomainIdentity>> {
        #[cfg(windows)]
        {
            let (host_id, session_id) = super::windows::host_binding()?;
            Ok(Some(DomainIdentity {
                job_name: Self::job_name(scope, session_id, owner_id),
                host_id,
                session_id,
                owner_id: owner_id.into(),
            }))
        }
        #[cfg(not(windows))]
        {
            let _ = (scope, owner_id);
            Ok(None)
        }
    }
    pub fn job_name(scope: &str, session_id: u32, owner_id: &str) -> String {
        use sha2::{Digest, Sha256};
        format!(
            "Global\\WinBridge-Domain-{:x}",
            Sha256::digest(format!("{scope}|session:{session_id}|owner:{owner_id}").as_bytes())
        )
    }
    pub fn open(identity: &DomainIdentity) -> io::Result<Self> {
        identity.validate()?;
        #[cfg(windows)]
        {
            let (host, session) = super::windows::host_binding()?;
            if host != identity.host_id || session != identity.session_id {
                return Err(io::Error::other(
                    "Execution domain belongs to another Windows host/logon session",
                ));
            }
            Ok(Self {
                job: super::windows::Job::open_domain(&identity.job_name)?,
                identity: identity.clone(),
            })
        }
        #[cfg(not(windows))]
        {
            Err(io::Error::other(
                "Windows execution domain is unavailable on this platform",
            ))
        }
    }
    pub fn identity(&self) -> &DomainIdentity {
        &self.identity
    }
}

pub fn observe_domain(identity: &DomainIdentity) -> super::process_identity::ProcessObservation {
    use super::process_identity::ProcessObservation::Unknown;
    if let Err(error) = identity.validate() {
        return Unknown(error.to_string());
    }
    #[cfg(windows)]
    {
        super::windows::observe_domain(identity)
    }
    #[cfg(not(windows))]
    {
        Unknown("Windows execution domain cannot be checked on this platform".into())
    }
}
