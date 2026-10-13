//! Physical observations never infer tree ownership from a PID alone.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProcessIdentity {
    pub job_name: String,
    pub host_id: String,
    pub session_id: u32,
    pub process_id: u32,
    pub creation_time: u64,
}

#[derive(Debug)]
pub enum ProcessObservation {
    Empty(String),
    Occupied(String),
    Unknown(String),
}

pub fn observe_process(identity: &ProcessIdentity) -> ProcessObservation {
    #[cfg(windows)]
    {
        super::windows::observe(identity)
    }
    #[cfg(not(windows))]
    {
        let _ = identity;
        ProcessObservation::Unknown("Windows Job Object нельзя проверить на этой платформе".into())
    }
}
