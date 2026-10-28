//! Infrastructure is outside the specified worker domain, never a breakaway task.
use crate::execution_domain::ExecutionDomain;
use std::{
    fs::File,
    io,
    path::Path,
    process::{Child, Command, Stdio},
};
pub use winbridge_protocol::update::SupervisorProcess as ProcessStamp;

/// A handle to the exact live Windows process, retained across shutdown. A PID
/// lookup after exit cannot recreate this capability or prove the old exit.
pub struct PinnedProcess {
    #[cfg(windows)]
    handle: std::os::windows::io::OwnedHandle,
    identity: ProcessStamp,
}
impl PinnedProcess {
    pub fn live(expected: &ProcessStamp) -> io::Result<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
            use windows_sys::Win32::{
                Foundation::FILETIME,
                System::Threading::{
                    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
                    PROCESS_SYNCHRONIZE,
                },
            };
            let raw = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                    0,
                    expected.process_id,
                )
            };
            if raw.is_null() {
                return Err(io::Error::last_os_error());
            }
            let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
            let (mut born, mut exit, mut kernel, mut user): (
                FILETIME,
                FILETIME,
                FILETIME,
                FILETIME,
            ) = unsafe { std::mem::zeroed() };
            if unsafe {
                GetProcessTimes(
                    handle.as_raw_handle(),
                    &mut born,
                    &mut exit,
                    &mut kernel,
                    &mut user,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            let (host_id, session_id) =
                crate::windows::host_binding_for_process(expected.process_id)?;
            let identity = ProcessStamp {
                process_id: expected.process_id,
                birth: (((born.dwHighDateTime as u64) << 32) | born.dwLowDateTime as u64)
                    .to_string(),
                host_id,
                session_id,
            };
            if identity != *expected {
                return Err(io::Error::other(
                    "Exact process birth/host/session differs; no shutdown pin",
                ));
            }
            let pin = Self { handle, identity };
            pin.require_live()?;
            Ok(pin)
        }
        #[cfg(not(windows))]
        {
            let _ = expected;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Exact shutdown process pin requires Windows",
            ))
        }
    }
    pub fn identity(&self) -> &ProcessStamp {
        &self.identity
    }
    pub fn require_live(&self) -> io::Result<()> {
        if self.exited()? {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Pinned process has exited",
            ))
        } else {
            Ok(())
        }
    }
    /// Query-only membership in an existing Job. Never create a Job, retain a
    /// child-side Job handle, alter limits, or infer ownership from its name.
    pub fn require_job_membership(&self, name: &str) -> io::Result<()> {
        self.require_live()?;
        #[cfg(windows)]
        {
            use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
            use windows_sys::Win32::System::{
                JobObjects::{IsProcessInJob, OpenJobObjectW},
                SystemServices::JOB_OBJECT_QUERY,
            };
            let wide: Vec<_> = name.encode_utf16().chain(Some(0)).collect();
            if name.is_empty() || name.contains('\0') {
                return Err(io::Error::other("Invalid Job observation name"));
            }
            let raw = unsafe { OpenJobObjectW(JOB_OBJECT_QUERY, 0, wide.as_ptr()) };
            if raw.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = unsafe { OwnedHandle::from_raw_handle(raw) };
            let mut member = 0;
            if unsafe {
                IsProcessInJob(
                    self.handle.as_raw_handle(),
                    job.as_raw_handle(),
                    &mut member,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            if member == 0 {
                return Err(io::Error::other(
                    "Pinned process is outside the original activation Job",
                ));
            }
            drop(job);
            self.require_live()
        }
        #[cfg(not(windows))]
        {
            let _ = name;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Exact Job membership requires Windows",
            ))
        }
    }
    /// Only the retained kernel object decides this result. Access denied and
    /// an invalid wait never become positive evidence of exit.
    pub fn exited(&self) -> io::Result<bool> {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::{
                Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
                System::Threading::WaitForSingleObject,
            };
            match unsafe { WaitForSingleObject(self.handle.as_raw_handle(), 0) } {
                WAIT_OBJECT_0 => Ok(true),
                WAIT_TIMEOUT => Ok(false),
                _ => Err(io::Error::last_os_error()),
            }
        }
        #[cfg(not(windows))]
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Exact shutdown process wait requires Windows",
        ))
    }
}

pub fn stamp(pid: u32) -> io::Result<ProcessStamp> {
    #[cfg(windows)]
    {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::{
            Foundation::{FILETIME, WAIT_OBJECT_0, WAIT_TIMEOUT},
            System::Threading::{
                GetProcessTimes, OpenProcess, WaitForSingleObject,
                PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
            },
        };
        let raw = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                pid,
            )
        };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        let h = unsafe { OwnedHandle::from_raw_handle(raw) };
        match unsafe { WaitForSingleObject(h.as_raw_handle(), 0) } {
            WAIT_TIMEOUT => {}
            WAIT_OBJECT_0 => {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "Infrastructure process exited",
                ))
            }
            _ => return Err(io::Error::last_os_error()),
        }
        let (mut born, mut exit, mut kernel, mut user): (FILETIME, FILETIME, FILETIME, FILETIME) =
            unsafe { std::mem::zeroed() };
        if unsafe {
            GetProcessTimes(
                h.as_raw_handle(),
                &mut born,
                &mut exit,
                &mut kernel,
                &mut user,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let (host, session) = crate::windows::host_binding_for_process(pid)?;
        Ok(ProcessStamp {
            process_id: pid,
            birth: (((born.dwHighDateTime as u64) << 32) | born.dwLowDateTime as u64).to_string(),
            host_id: host,
            session_id: session,
        })
    }
    #[cfg(target_os = "linux")]
    {
        use sha2::{Digest, Sha256};
        let value = std::fs::read_to_string(format!("/proc/{pid}/stat"))?;
        let fields = value
            .rsplit_once(')')
            .ok_or_else(|| io::Error::other("Process stat missing"))?
            .1
            .split_whitespace()
            .collect::<Vec<_>>();
        if fields.first().is_some_and(|s| matches!(*s, "Z" | "X")) {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Infrastructure process exited",
            ));
        }
        let birth = fields
            .get(19)
            .ok_or_else(|| io::Error::other("Process birth missing"))?
            .to_string();
        let session_id = fields
            .get(3)
            .ok_or_else(|| io::Error::other("Process session missing"))?
            .parse()
            .map_err(io::Error::other)?;
        let boot = std::fs::read("/proc/sys/kernel/random/boot_id")?;
        Ok(ProcessStamp {
            process_id: pid,
            birth,
            host_id: format!("{:x}", Sha256::digest(boot)),
            session_id,
        })
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = pid;
        Err(io::Error::other(
            "Infrastructure process identity unsupported",
        ))
    }
}
pub fn matches(expected: &ProcessStamp) -> io::Result<bool> {
    Ok(stamp(expected.process_id)? == *expected)
}
pub fn require_parent_outside(domain: Option<&ExecutionDomain>) -> io::Result<()> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        let domain = domain.ok_or_else(|| {
            io::Error::other("Worker execution domain required for infrastructure launch")
        })?;
        if domain.job.contains(unsafe { GetCurrentProcess() })? {
            return Err(io::Error::other(
                "Infrastructure launch from a managed worker domain is forbidden",
            ));
        }
    }
    #[cfg(not(windows))]
    let _ = domain;
    Ok(())
}
/// Uncommitted children are killed on drop. Detach before sending handoff, never
/// after its ACK: a lost ACK must not kill an accepted independent supervisor.
pub struct InfrastructureChild {
    child: Option<Child>,
    kill_on_drop: bool,
    pub identity: ProcessStamp,
}
impl InfrastructureChild {
    pub fn spawn(
        image: &Path,
        args: &[String],
        cwd: &Path,
        stdout: File,
        stderr: File,
        domain: Option<&ExecutionDomain>,
    ) -> io::Result<Self> {
        require_parent_outside(domain)?;
        crate::require_safe_path(image)?;
        crate::require_local_disk(image)?;
        let mut command = Command::new(image);
        command
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn()?;
        let identity = match stamp(child.id()) {
            Ok(v) => v,
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        };
        let owner = Self {
            child: Some(child),
            identity,
            kill_on_drop: true,
        };
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            if domain
                .unwrap()
                .job
                .contains(owner.child.as_ref().unwrap().as_raw_handle())?
            {
                return Err(io::Error::other(
                    "Infrastructure inherited the worker execution domain",
                ));
            }
        }
        Ok(owner)
    }
    pub fn detached(&mut self) {
        self.kill_on_drop = false;
    }
    pub fn exited(&mut self) -> io::Result<bool> {
        Ok(self.child.as_mut().unwrap().try_wait()?.is_some())
    }
}
impl Drop for InfrastructureChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if self.kill_on_drop {
                let _ = child.kill();
                let _ = child.wait();
            } else {
                #[cfg(unix)]
                {
                    let _ = std::thread::Builder::new()
                        .name("infrastructure-reaper".into())
                        .spawn(move || {
                            let _ = child.wait();
                        });
                }
                #[cfg(not(unix))]
                drop(child);
            }
        }
    }
}

/// Readonly environment observation, never authorization to leave a Job.
pub fn job_observation(expected: &ProcessStamp) -> io::Result<Option<bool>> {
    if !matches(expected)? {
        return Err(io::Error::other(
            "Process identity changed before Job observation",
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::System::{
            JobObjects::IsProcessInJob,
            Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
        };
        let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, expected.process_id) };
        if raw.is_null() {
            return Err(io::Error::last_os_error());
        }
        let h = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut member = 0;
        if unsafe { IsProcessInJob(h.as_raw_handle(), std::ptr::null_mut(), &mut member) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if !matches(expected)? {
            return Err(io::Error::other(
                "Process identity changed during Job observation",
            ));
        }
        Ok(Some(member != 0))
    }
    #[cfg(not(windows))]
    {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_identity_rejects_a_reused_or_forged_birth() {
        let current = stamp(std::process::id()).unwrap();
        assert!(matches(&current).unwrap());
        let mut wrong = current.clone();
        wrong.birth.push('0');
        assert!(!matches(&wrong).unwrap());
    }
    #[cfg(not(windows))]
    #[test]
    fn windows_process_pin_never_claims_exit_on_an_unsupported_platform() {
        assert!(PinnedProcess::live(&stamp(std::process::id()).unwrap()).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn pinned_process_requires_exact_live_identity_and_observes_owned_exit() {
        use std::time::{Duration, Instant};
        let root = tempfile::tempdir().unwrap();
        let command = winbridge_protocol::ResolvedCommand {
            executable: std::env::current_exe().unwrap().to_str().unwrap().into(),
            arguments: vec![
                "--exact".into(),
                "infrastructure::tests::unconfirmed_lifetime_helper".into(),
                "--ignored".into(),
            ],
            working_directory: root.path().to_str().unwrap().into(),
            timeout_ms: 10000,
        };
        let (mut child, _out, _err) = crate::ManagedChild::prepare(&command, None).unwrap();
        let identity = stamp(child.id()).unwrap();
        for field in ["birth", "host", "session"] {
            let mut wrong = identity.clone();
            match field {
                "birth" => wrong.birth.push('0'),
                "host" => wrong.host_id = "0".repeat(64),
                _ => wrong.session_id = wrong.session_id.wrapping_add(1),
            }
            assert!(PinnedProcess::live(&wrong).is_err(), "{field}");
        }
        let pin = PinnedProcess::live(&identity).unwrap();
        assert_eq!(pin.identity(), &identity);
        assert!(!pin.exited().unwrap());
        let job = child.identity().unwrap().job_name.clone();
        pin.require_job_membership(&job).unwrap();
        assert!(pin
            .require_job_membership(&format!(
                "Global\\WinBridge-Missing-{}-{}",
                identity.process_id, identity.birth
            ))
            .is_err());
        // Query failure does not alter the original membership or limits.
        pin.require_job_membership(&job).unwrap();
        child.resume().unwrap();
        pin.require_live().unwrap();
        child.terminate_tree().unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while !pin.exited().unwrap() {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(pin.require_live().is_err());
        assert!(PinnedProcess::live(&identity).is_err());
        while child.try_wait().unwrap().is_none() {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn unconfirmed_owned_cleanup_does_not_wait_for_process_lifetime() {
        use std::time::{Duration, Instant};
        let root = tempfile::tempdir().unwrap();
        let command = winbridge_protocol::ResolvedCommand {
            executable: std::env::current_exe().unwrap().to_str().unwrap().into(),
            arguments: [
                "--exact",
                "infrastructure::tests::unconfirmed_lifetime_helper",
                "--ignored",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            working_directory: root.path().to_str().unwrap().into(),
            timeout_ms: 1000,
        };
        let (child, _stdout, _stderr) = crate::ManagedChild::prepare(&command, None).unwrap();
        let identity = stamp(child.id()).unwrap();
        let at = Instant::now();
        child.close_unconfirmed();
        assert!(at.elapsed() < Duration::from_secs(2));
        let until = Instant::now() + Duration::from_secs(5);
        while matches(&identity).unwrap_or(false) {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    #[ignore = "Owned process lifetime fixture"]
    fn unconfirmed_lifetime_helper() {
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
    #[cfg(windows)]
    #[test]
    fn managed_domain_cannot_launch_infrastructure() {
        use crate::{ManagedChild, OutputReader};
        use winbridge_protocol::ResolvedCommand;
        let dir = tempfile::tempdir().unwrap();
        let owner = format!("00000000-0000-4000-8000-{:012x}", std::process::id());
        let identity = ExecutionDomain::identity_for(dir.path().to_str().unwrap(), &owner)
            .unwrap()
            .unwrap();
        let domain = ExecutionDomain::open(&identity).unwrap();
        std::fs::write(
            dir.path().join("domain.json"),
            serde_json::to_vec(&identity).unwrap(),
        )
        .unwrap();
        let cmd = ResolvedCommand {
            executable: std::env::current_exe().unwrap().to_str().unwrap().into(),
            arguments: vec![
                "--exact".into(),
                "infrastructure::tests::domain_helper".into(),
                "--ignored".into(),
                "--nocapture".into(),
            ],
            working_directory: dir.path().to_str().unwrap().into(),
            timeout_ms: 10000,
        };
        let (mut child, out, err) =
            ManagedChild::prepare_in_domain(&cmd, None, Some(&domain)).unwrap();
        let drain = |mut r: OutputReader| {
            std::thread::spawn(move || {
                let mut b = Vec::new();
                let _ = std::io::Read::read_to_end(&mut r, &mut b);
                b
            })
        };
        let stdout = drain(out);
        let stderr = drain(err);
        child.resume().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Some(exit) = child.try_wait().unwrap() {
                assert_eq!(exit.code(), Some(43));
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let _ = stdout.join();
        let _ = stderr.join();
    }
    #[cfg(windows)]
    #[test]
    #[ignore = "Owned managed-domain helper"]
    fn domain_helper() {
        let identity = serde_json::from_slice(
            &std::fs::read(std::env::current_dir().unwrap().join("domain.json")).unwrap(),
        )
        .unwrap();
        let domain = ExecutionDomain::open(&identity).unwrap();
        assert!(require_parent_outside(Some(&domain)).is_err());
        std::process::exit(43);
    }
}
