use std::{
    io,
    mem::{size_of, zeroed},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    process::Child,
};
use windows_sys::Win32::{
    Foundation::{
        GetLastError, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, FILETIME, HANDLE,
        INVALID_HANDLE_VALUE,
    },
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
        },
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicAccountingInformation,
            JobObjectExtendedLimitInformation, OpenJobObjectW, QueryInformationJobObject,
            SetInformationJobObject, TerminateJobObject, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
        Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6464KEY},
        RemoteDesktop::ProcessIdToSessionId,
        SystemServices::JOB_OBJECT_QUERY,
        Threading::{
            GetCurrentProcessId, GetProcessTimes, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
        },
    },
};

pub struct Job(OwnedHandle);
fn owned(handle: HANDLE) -> io::Result<OwnedHandle> {
    if handle.is_null() || handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: this newly returned handle has exactly one owner.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}
impl Job {
    pub fn new(name: Option<&str>) -> io::Result<Self> {
        if name.is_some_and(|name| name.contains('\0') || name.encode_utf16().count() >= 260) {
            return Err(io::Error::other("Invalid Windows Job Object name"));
        }
        let name = name.map(wide);
        let job = Self(owned(unsafe {
            CreateJobObjectW(
                std::ptr::null(),
                name.as_ref().map_or(std::ptr::null(), |n| n.as_ptr()),
            )
        })?);
        if name.is_some() && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            return Err(io::Error::other(
                "Execution Job Object already exists; refusing to reuse it",
            ));
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = unsafe {
            SetInformationJobObject(
                job.0.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }
    pub fn assign(&self, child: &Child) -> io::Result<()> {
        if unsafe { AssignProcessToJobObject(self.0.as_raw_handle(), child.as_raw_handle()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    pub fn resume(&self, child: &Child) -> io::Result<()> {
        // std::process does not expose the primary thread handle. The process is
        // still suspended and has one thread; obtain it through documented APIs.
        let snapshot = owned(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) })?;
        let mut entry: THREADENTRY32 = unsafe { zeroed() };
        entry.dwSize = size_of::<THREADENTRY32>() as u32;
        let mut available = unsafe { Thread32First(snapshot.as_raw_handle(), &mut entry) };
        while available != 0 {
            if entry.th32OwnerProcessID == child.id() {
                let thread =
                    owned(unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) })?;
                if unsafe { ResumeThread(thread.as_raw_handle()) } == u32::MAX {
                    return Err(io::Error::last_os_error());
                }
                return Ok(());
            }
            available = unsafe { Thread32Next(snapshot.as_raw_handle(), &mut entry) };
        }
        Err(io::Error::other(
            "suspended primary thread was not found; process was not started",
        ))
    }
    pub fn terminate(&self) -> io::Result<()> {
        if unsafe { TerminateJobObject(self.0.as_raw_handle(), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    pub fn is_empty(&self) -> io::Result<bool> {
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
        if unsafe {
            QueryInformationJobObject(
                self.0.as_raw_handle(),
                JobObjectBasicAccountingInformation,
                &mut info as *mut _ as *mut _,
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(info.ActiveProcesses == 0)
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

pub fn host_binding() -> io::Result<(String, u32)> {
    use sha2::{Digest, Sha256};
    let mut guid = [0u16; 256];
    let mut size = std::mem::size_of_val(&guid) as u32;
    let code = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            wide("SOFTWARE\\Microsoft\\Cryptography").as_ptr(),
            wide("MachineGuid").as_ptr(),
            RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY,
            std::ptr::null_mut(),
            guid.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if code != 0 {
        return Err(io::Error::from_raw_os_error(code as i32));
    }
    let length = guid
        .iter()
        .position(|v| *v == 0)
        .ok_or_else(|| io::Error::other("MachineGuid is not terminated"))?;
    if length == 0 {
        return Err(io::Error::other("MachineGuid is empty"));
    }
    let host_id = format!(
        "{:x}",
        Sha256::digest(String::from_utf16_lossy(&guid[..length]).as_bytes())
    );
    let mut session = 0;
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((host_id, session))
}

pub fn creation_time(child: &Child) -> io::Result<u64> {
    let mut created: FILETIME = unsafe { zeroed() };
    let mut exited: FILETIME = unsafe { zeroed() };
    let mut kernel: FILETIME = unsafe { zeroed() };
    let mut user: FILETIME = unsafe { zeroed() };
    if unsafe {
        GetProcessTimes(
            child.as_raw_handle(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

/// Read-only observation. Never opens a PID for termination or changes a job.
pub fn observe(
    identity: &super::process_identity::ProcessIdentity,
) -> super::process_identity::ProcessObservation {
    use super::process_identity::ProcessObservation::{Empty, Occupied, Unknown};
    let (host, session) = match host_binding() {
        Ok(binding) => binding,
        Err(error) => {
            return Unknown(format!(
                "Не удалось проверить Windows host/session: {error}"
            ))
        }
    };
    if host != identity.host_id || session != identity.session_id {
        return Unknown(
            "Другой Windows host/session; прежнее дерево процессов не проверено".into(),
        );
    }
    let handle = unsafe { OpenJobObjectW(JOB_OBJECT_QUERY, 0, wide(&identity.job_name).as_ptr()) };
    if handle.is_null() {
        let error = io::Error::last_os_error();
        return if error.raw_os_error() == Some(ERROR_FILE_NOT_FOUND as i32) {
            Empty("Сохранённый Windows Job Object больше не существует".into())
        } else {
            Unknown(format!("Не удалось проверить Windows Job Object: {error}"))
        };
    }
    let job = match owned(handle) {
        Ok(handle) => Job(handle),
        Err(error) => return Unknown(error.to_string()),
    };
    match job.is_empty() {
        Ok(true) => Empty("В сохранённом Windows Job Object нет процессов".into()),
        Ok(false) => Occupied("В сохранённом Windows Job Object ещё есть процессы".into()),
        Err(error) => Unknown(format!("Не удалось прочитать Windows Job Object: {error}")),
    }
}
