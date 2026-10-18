//! Atomic executable/Job creation on stable Rust. No shell or PATH interpretation.
use crate::{execution_domain::ExecutionDomain, windows::Job, OutputReader, SpawnFailure};
use std::{
    ffi::c_void,
    fs::File,
    io,
    marker::PhantomData,
    mem::size_of,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::ExitStatusExt,
    },
    path::Path,
    process::ExitStatus,
};
use windows_sys::Win32::{
    Foundation::{
        SetHandleInformation, ERROR_INSUFFICIENT_BUFFER, FILETIME, GENERIC_READ, HANDLE,
        HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    Security::SECURITY_ATTRIBUTES,
    Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    },
    System::{
        Pipes::CreatePipe,
        Threading::{
            CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess, GetProcessTimes,
            InitializeProcThreadAttributeList, ResumeThread, UpdateProcThreadAttribute,
            WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT,
            EXTENDED_STARTUPINFO_PRESENT, INFINITE, LPPROC_THREAD_ATTRIBUTE_LIST,
            PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_JOB_LIST,
            STARTF_USESTDHANDLES, STARTUPINFOEXW,
        },
    },
};

pub(crate) struct NativeChild {
    process: OwnedHandle,
    thread: OwnedHandle,
    pid: u32,
}
impl NativeChild {
    pub fn id(&self) -> u32 {
        self.pid
    }
    pub fn try_wait(&self) -> io::Result<Option<ExitStatus>> {
        match unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => self.exit_status().map(Some),
            _ => Err(io::Error::last_os_error()),
        }
    }
    pub fn wait(&self) -> io::Result<ExitStatus> {
        if unsafe { WaitForSingleObject(self.process.as_raw_handle(), INFINITE) } != WAIT_OBJECT_0 {
            return Err(io::Error::last_os_error());
        }
        self.exit_status()
    }
    fn exit_status(&self) -> io::Result<ExitStatus> {
        let mut code = 0;
        if unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(ExitStatus::from_raw(code))
    }
    pub fn resume(&self) -> io::Result<()> {
        match unsafe { ResumeThread(self.thread.as_raw_handle()) } {
            1 => Ok(()),
            u32::MAX => Err(io::Error::last_os_error()),
            count => Err(io::Error::other(format!(
                "Unexpected primary-thread suspend count {count}; cleanup required"
            ))),
        }
    }
    pub fn creation_time(&self) -> io::Result<u64> {
        let (mut born, mut exit, mut kernel, mut user): (FILETIME, FILETIME, FILETIME, FILETIME) =
            unsafe { std::mem::zeroed() };
        if unsafe {
            GetProcessTimes(
                self.process.as_raw_handle(),
                &mut born,
                &mut exit,
                &mut kernel,
                &mut user,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok((u64::from(born.dwHighDateTime) << 32) | u64::from(born.dwLowDateTime))
    }
}
fn owned(h: HANDLE) -> io::Result<OwnedHandle> {
    if h.is_null() || h == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: caller transfers one newly created Win32 handle.
    Ok(unsafe { OwnedHandle::from_raw_handle(h) })
}
fn pipe(security: &SECURITY_ATTRIBUTES) -> io::Result<(OwnedHandle, OwnedHandle)> {
    let (mut read, mut write) = (std::ptr::null_mut(), std::ptr::null_mut());
    if unsafe { CreatePipe(&mut read, &mut write, security, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let pair = (owned(read)?, owned(write)?);
    if unsafe { SetHandleInformation(pair.0.as_raw_handle(), HANDLE_FLAG_INHERIT, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(pair)
}
struct Attributes<'a> {
    words: Vec<usize>,
    _values: PhantomData<&'a [HANDLE]>,
}
impl Drop for Attributes<'_> {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.pointer()) };
    }
}
impl<'a> Attributes<'a> {
    fn pointer(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.words.as_mut_ptr().cast()
    }
    fn new(jobs: &'a [HANDLE], handles: &'a [HANDLE]) -> io::Result<Self> {
        let mut bytes = 0;
        let result =
            unsafe { InitializeProcThreadAttributeList(std::ptr::null_mut(), 2, 0, &mut bytes) };
        if result != 0
            || io::Error::last_os_error().raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32)
            || bytes == 0
        {
            return Err(io::Error::other("Cannot size process attributes"));
        }
        let mut words = vec![0usize; bytes.div_ceil(size_of::<usize>())];
        if unsafe { InitializeProcThreadAttributeList(words.as_mut_ptr().cast(), 2, 0, &mut bytes) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut list = Self {
            words,
            _values: PhantomData,
        };
        for (kind, values) in [
            (PROC_THREAD_ATTRIBUTE_JOB_LIST, jobs),
            (PROC_THREAD_ATTRIBUTE_HANDLE_LIST, handles),
        ] {
            if unsafe {
                UpdateProcThreadAttribute(
                    list.pointer(),
                    0,
                    kind as usize,
                    values.as_ptr().cast::<c_void>(),
                    std::mem::size_of_val(values),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(list)
    }
}
fn wide(text: &std::ffi::OsStr) -> io::Result<Vec<u16>> {
    let mut v: Vec<u16> = text.encode_wide().collect();
    if v.contains(&0) {
        return Err(io::Error::other("NUL in executable command"));
    }
    v.push(0);
    Ok(v)
}
// Standard CRT argv quoting; each input remains exactly one argument.
fn quote(value: &str) -> io::Result<String> {
    if value.contains('\0') {
        return Err(io::Error::other("NUL in executable argument"));
    }
    let mut result = String::from("\"");
    let mut slashes = 0;
    for c in value.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        result.extend(std::iter::repeat_n(
            '\\',
            if c == '"' { slashes * 2 + 1 } else { slashes },
        ));
        slashes = 0;
        result.push(c);
    }
    result.extend(std::iter::repeat_n('\\', slashes * 2));
    result.push('"');
    Ok(result)
}
pub(crate) fn spawn(
    command: &winbridge_protocol::ResolvedCommand,
    job: &Job,
    domain: Option<&ExecutionDomain>,
) -> Result<(NativeChild, OutputReader, OutputReader), SpawnFailure> {
    let image = Path::new(&command.executable);
    let cwd = Path::new(&command.working_directory);
    if !image.is_absolute()
        || !image
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
        || !cwd.is_absolute()
    {
        return Err(io::Error::other("Atomic launch requires an absolute .exe and cwd; scripts/PATH use the standard adapter").into());
    }
    // Reject UNC/device syntax before any filesystem access to that path.
    crate::require_local_path_prefix(image)?;
    crate::require_local_path_prefix(cwd)?;
    crate::require_safe_path(image)?;
    crate::require_local_disk(image)?;
    crate::require_safe_path(cwd)?;
    crate::require_local_disk(cwd)?;
    let app = wide(image.as_os_str())?;
    let directory = wide(cwd.as_os_str())?;
    let mut command_line = quote(&command.executable)?;
    for argument in &command.arguments {
        command_line.push(' ');
        command_line.push_str(&quote(argument)?);
    }
    let mut line = wide(std::ffi::OsStr::new(&command_line))?;
    if line.len() > 32767 {
        return Err(
            io::Error::other("Windows executable command exceeds 32767 UTF-16 units").into(),
        );
    }
    let security = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: 1,
    };
    let (stdout_read, stdout_write) = pipe(&security)?;
    let (stderr_read, stderr_write) = pipe(&security)?;
    let nul = wide(std::ffi::OsStr::new("NUL"))?;
    let stdin = owned(unsafe {
        CreateFileW(
            nul.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            &security,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        )
    })?;
    let mut jobs = Vec::new();
    if let Some(domain) = domain {
        jobs.push(domain.job.raw());
    }
    jobs.push(job.raw());
    let handles = [
        stdin.as_raw_handle(),
        stdout_write.as_raw_handle(),
        stderr_write.as_raw_handle(),
    ];
    let mut attributes = Attributes::new(&jobs, &handles)?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = handles[0];
    startup.StartupInfo.hStdOutput = handles[1];
    startup.StartupInfo.hStdError = handles[2];
    startup.lpAttributeList = attributes.pointer();
    let mut info = PROCESS_INFORMATION::default();
    crate::trace_process("atomic-create.begin", None);
    if unsafe {
        CreateProcessW(
            app.as_ptr(),
            line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
            CREATE_SUSPENDED
                | CREATE_NO_WINDOW
                | CREATE_UNICODE_ENVIRONMENT
                | EXTENDED_STARTUPINFO_PRESENT,
            std::ptr::null(),
            directory.as_ptr(),
            &startup.StartupInfo,
            &mut info,
        )
    } == 0
    {
        return Err(io::Error::last_os_error().into());
    }
    // CreateProcess succeeded: ownership failures from here cannot claim no process.
    // The Job already covers the suspended root, even before a PID receipt exists.
    #[cfg(test)]
    if let Some(root) = std::env::var_os("WINBRIDGE_ATOMIC_CRASH_ROOT") {
        tests::crash_after_create(Path::new(&root), info.dwProcessId);
    }
    let process = owned(info.hProcess).map_err(|error| SpawnFailure {
        no_process_created: false,
        error,
    })?;
    let thread = owned(info.hThread).map_err(|error| SpawnFailure {
        no_process_created: false,
        error,
    })?;
    let child = NativeChild {
        process,
        thread,
        pid: info.dwProcessId,
    };
    crate::trace_process("atomic-create.assigned", Some(child.pid));
    if !job
        .contains(child.process.as_raw_handle())
        .map_err(|error| SpawnFailure {
            no_process_created: false,
            error,
        })?
        || domain.is_some_and(|d| {
            !d.job
                .contains(child.process.as_raw_handle())
                .unwrap_or(false)
        })
    {
        return Err(SpawnFailure {
            no_process_created: false,
            error: io::Error::other("Atomic Job membership could not be verified; no resume"),
        });
    }
    drop(attributes);
    drop(stdout_write);
    drop(stderr_write);
    drop(stdin);
    Ok((
        child,
        Box::new(File::from(stdout_read)),
        Box::new(File::from(stderr_read)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        infrastructure,
        process_identity::{observe_process, ProcessIdentity, ProcessObservation},
        ManagedChild, RootChild,
    };
    use std::{
        fs,
        io::Read,
        path::PathBuf,
        process::{Command, Stdio},
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    use winbridge_protocol::ResolvedCommand;

    fn nonce() -> String {
        format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }
    fn name() -> String {
        format!("Global\\WinBridge-Atomic-{}", nonce())
    }
    fn fixture(root: &Path) -> PathBuf {
        let source = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("winbridge-fixture.exe");
        assert!(
            source.is_file(),
            "Build winbridge-worker --bin winbridge-fixture before Windows platform tests"
        );
        let dest = root.join("tool with spaces.exe");
        fs::copy(source, &dest).unwrap();
        dest
    }
    fn command(image: &Path, root: &Path, args: Vec<String>) -> ResolvedCommand {
        ResolvedCommand {
            executable: image.to_str().unwrap().into(),
            arguments: args,
            working_directory: root.to_str().unwrap().into(),
            timeout_ms: 5000,
        }
    }
    fn finish(
        mut child: ManagedChild,
        mut out: OutputReader,
        mut err: OutputReader,
    ) -> (Vec<u8>, Vec<u8>) {
        child.resume().unwrap();
        let stdout = std::thread::spawn(move || {
            let mut b = Vec::new();
            out.read_to_end(&mut b).unwrap();
            b
        });
        let stderr = std::thread::spawn(move || {
            let mut b = Vec::new();
            err.read_to_end(&mut b).unwrap();
            b
        });
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(exit) = child.try_wait().unwrap() {
                assert_eq!(exit.code(), Some(0));
                break;
            }
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(10));
        }
        child.terminate_tree().unwrap();
        assert!(child.tree_is_empty().unwrap());
        (stdout.join().unwrap(), stderr.join().unwrap())
    }
    fn proof(key: &str, value: serde_json::Value) {
        if let Some(path) = std::env::var_os("WINBRIDGE_ATOMIC_GATE_REPORT") {
            let mut v=fs::read(&path).ok().map(|b|serde_json::from_slice(&b).unwrap()).unwrap_or_else(||serde_json::json!({"schemaVersion":1,"kind":"atomic_executable_proof","platform":"windows","build":winbridge_protocol::build_identity(),"runtimeApplied":false,"windowsOpened":false}));
            v[key] = value;
            fs::write(path, serde_json::to_vec_pretty(&v).unwrap()).unwrap();
        }
    }
    #[test]
    fn atomic_argv_stdio_and_membership_before_resume() {
        let root = tempfile::tempdir().unwrap();
        let image = fixture(root.path());
        let expected = vec![
            "".to_string(),
            "a b".into(),
            "a\"b".into(),
            "C:\\path with space\\".into(),
            "\\\\".into(),
            "Привет ✓ 🚀".into(),
            "line\nbreak".into(),
            "%PATH% & | < > ^ !".into(),
        ];
        let mut args = vec!["arguments".into()];
        args.extend(expected.clone());
        let (child, out, err) = ManagedChild::prepare_atomic_executable(
            &command(&image, root.path(), args),
            Some(&name()),
            None,
        )
        .unwrap();
        let identity = child.identity().unwrap().clone();
        assert!(matches!(
            observe_process(&identity),
            ProcessObservation::Occupied(_)
        ));
        assert!(!root.path().join("runs.log").exists());
        let (stdout, stderr) = finish(child, out, err);
        assert!(stderr.is_empty());
        let data = String::from_utf8(stdout).unwrap();
        let actual: Vec<String> =
            serde_json::from_str(data.lines().find(|l| l.starts_with('[')).unwrap()).unwrap();
        assert_eq!(actual, expected);
        assert!(matches!(
            observe_process(&identity),
            ProcessObservation::Empty(_)
        ));
        proof(
            "argvAndSuspended",
            serde_json::json!({"passed":true,"processId":identity.process_id,"birth":identity.creation_time.to_string(),"jobName":identity.job_name,"argumentCount":expected.len()}),
        );
    }
    #[test]
    fn atomic_close_before_resume_never_executes_user_code() {
        let root = tempfile::tempdir().unwrap();
        let image = fixture(root.path());
        let (child, out, err) = ManagedChild::prepare_atomic_executable(
            &command(&image, root.path(), vec!["success".into()]),
            Some(&name()),
            None,
        )
        .unwrap();
        let identity = child.identity().unwrap().clone();
        child.close_unconfirmed();
        drop(out);
        drop(err);
        let until = Instant::now() + Duration::from_secs(5);
        while !matches!(observe_process(&identity), ProcessObservation::Empty(_)) {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!root.path().join("runs.log").exists());
        proof("noResume", serde_json::json!({"passed":true}));
    }
    #[test]
    fn atomic_domain_and_private_job_are_both_assigned_without_post_create_mutation() {
        let root = tempfile::tempdir().unwrap();
        let image = fixture(root.path());
        let owner = format!(
            "00000000-0000-4000-8000-{:012x}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                & 0xffffffffffff
        );
        let id = ExecutionDomain::identity_for(root.path().to_str().unwrap(), &owner)
            .unwrap()
            .unwrap();
        let domain = ExecutionDomain::open(&id).unwrap();
        let cmd = command(&image, root.path(), vec!["long".into()]);
        let (mut other, _out, _err) =
            ManagedChild::prepare_in_domain(&cmd, Some(&name()), Some(&domain)).unwrap();
        let (mut child, out, err) =
            ManagedChild::prepare_atomic_executable(&cmd, Some(&name()), Some(&domain)).unwrap();
        let RootChild::Atomic(native) = &child.child else {
            panic!("Atomic child")
        };
        assert!(domain.job.contains(native.process.as_raw_handle()).unwrap());
        child.terminate_tree().unwrap();
        let _ = child.child.wait();
        assert!(child.tree_is_empty().unwrap());
        drop(out);
        drop(err);
        assert!(matches!(
            crate::execution_domain::observe_domain(&id),
            ProcessObservation::Occupied(_)
        ));
        other.terminate_tree().unwrap();
        let _ = other.child.wait();
        assert!(matches!(
            crate::execution_domain::observe_domain(&id),
            ProcessObservation::Empty(_)
        ));
        assert!(!root.path().join("runs.log").exists());
        proof("domainAndPrivateJob", serde_json::json!({"passed":true}));
    }
    #[test]
    fn atomic_invalid_commands_fail_before_creating_process() {
        let root = tempfile::tempdir().unwrap();
        let image = fixture(root.path());
        let remote = Path::new(r"\\invalid-winbridge-test-host\share\tool.exe");
        for cmd in [
            command(remote, root.path(), vec![]),
            command(&image, remote, vec![]),
        ] {
            let e = ManagedChild::prepare_atomic_executable(&cmd, Some(&name()), None)
                .err()
                .unwrap();
            assert!(e.no_process_created);
            assert!(e.to_string().contains("local disk path"));
        }
        for args in [vec!["bad\0arg".into()], vec!["x".repeat(32768)]] {
            let e = ManagedChild::prepare_atomic_executable(
                &command(&image, root.path(), args),
                Some(&name()),
                None,
            )
            .err()
            .unwrap();
            assert!(e.no_process_created);
        }
        let script = root.path().join("not-executable.cmd");
        fs::write(&script, "echo bad").unwrap();
        assert!(
            ManagedChild::prepare_atomic_executable(
                &command(&script, root.path(), vec![]),
                Some(&name()),
                None
            )
            .err()
            .unwrap()
            .no_process_created
        );
        assert!(!root.path().join("runs.log").exists());
        proof("rejectedBeforeCreate", serde_json::json!({"passed":true}));
    }
    #[test]
    fn atomic_extra_suspend_count_is_not_resumed_twice() {
        let root = tempfile::tempdir().unwrap();
        let image = fixture(root.path());
        let (mut child, out, err) = ManagedChild::prepare_atomic_executable(
            &command(&image, root.path(), vec![]),
            Some(&name()),
            None,
        )
        .unwrap();
        let RootChild::Atomic(native) = &child.child else {
            panic!("Atomic child")
        };
        assert_eq!(
            unsafe {
                windows_sys::Win32::System::Threading::SuspendThread(native.thread.as_raw_handle())
            },
            1
        );
        assert!(child.resume().unwrap_err().to_string().contains("count 2"));
        child.terminate_tree().unwrap();
        let _ = child.child.wait();
        drop(out);
        drop(err);
        assert!(!root.path().join("runs.log").exists());
        proof("suspendCount", serde_json::json!({"passed":true}));
    }
    pub(super) fn crash_after_create(root: &Path, pid: u32) -> ! {
        let process = infrastructure::stamp(pid).unwrap();
        fs::write(
            root.join("observed.json"),
            serde_json::to_vec(&process).unwrap(),
        )
        .unwrap();
        std::process::exit(87);
    }
    #[test]
    #[ignore = "Owned atomic creation crash helper"]
    fn atomic_crash_helper() {
        let root = PathBuf::from(std::env::var_os("WINBRIDGE_ATOMIC_CRASH_ROOT").unwrap());
        let image = fixture(&root);
        let job = name();
        let (host, session) = crate::windows::host_binding().unwrap();
        fs::write(
            root.join("intent.json"),
            serde_json::to_vec(&serde_json::json!({"jobName":job,"host":host,"session":session}))
                .unwrap(),
        )
        .unwrap();
        let _ = ManagedChild::prepare_atomic_executable(
            &command(&image, &root, vec![]),
            Some(&job),
            None,
        );
        panic!("Crash hook not reached");
    }
    #[test]
    fn atomic_owner_crash_immediately_after_create_leaves_no_orphan_or_user_effect() {
        let root = tempfile::tempdir().unwrap();
        let exit = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "windows_atomic::tests::atomic_crash_helper",
                "--ignored",
                "--nocapture",
            ])
            .env("WINBRIDGE_ATOMIC_CRASH_ROOT", root.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert_eq!(exit.code(), Some(87));
        let data: serde_json::Value =
            serde_json::from_slice(&fs::read(root.path().join("intent.json")).unwrap()).unwrap();
        let child: winbridge_protocol::update::SupervisorProcess =
            serde_json::from_slice(&fs::read(root.path().join("observed.json")).unwrap()).unwrap();
        let identity = ProcessIdentity {
            job_name: data["jobName"].as_str().unwrap().into(),
            host_id: data["host"].as_str().unwrap().into(),
            session_id: data["session"].as_u64().unwrap() as u32,
            process_id: child.process_id,
            creation_time: child.birth.parse().unwrap(),
        };
        let until = Instant::now() + Duration::from_secs(5);
        while !matches!(observe_process(&identity), ProcessObservation::Empty(_))
            || !original_root_exited(&child)
        {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!root.path().join("runs.log").exists());
        proof(
            "ownerCrashBeforePidReceipt",
            serde_json::json!({"passed":true,"child":child,"jobEmpty":true,"rootExited":true,"userCodeNotRun":true}),
        );
    }
    fn original_root_exited(process: &winbridge_protocol::update::SupervisorProcess) -> bool {
        match infrastructure::matches(process) {
            Ok(matches) => !matches,
            Err(error)
                if error.kind() == io::ErrorKind::NotFound || error.raw_os_error() == Some(87) =>
            {
                true
            } // Absent, nonzero PID.
            Err(error) => panic!("Root process observation unavailable: {error}"),
        }
    }
    #[test]
    #[ignore = "Owned handle inheritance helper"]
    fn handle_helper() {
        use windows_sys::Win32::{
            Foundation::{CompareObjectHandles, GetHandleInformation},
            System::Threading::{OpenEventW, SYNCHRONIZATION_SYNCHRONIZE},
        };
        let args: Vec<_> = std::env::args().collect();
        let values: Vec<_> = args
            .windows(2)
            .filter(|a| a[0] == "--skip")
            .map(|a| a[1].clone())
            .collect();
        let raw = values[0].parse::<usize>().unwrap() as HANDLE;
        let mut flags = 0;
        let inherited = if unsafe { GetHandleInformation(raw, &mut flags) } == 0 {
            false
        } else {
            let name = wide(std::ffi::OsStr::new(&values[1])).unwrap();
            let event = owned(unsafe { OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, 0, name.as_ptr()) })
                .unwrap();
            (unsafe { CompareObjectHandles(raw, event.as_raw_handle()) }) != 0
        };
        let mut input = Vec::new();
        std::io::stdin().read_to_end(&mut input).unwrap();
        assert!(input.is_empty());
        println!(
            "handle-probe {}",
            serde_json::json!({"inherited":inherited,"stdinEof":true})
        );
    }
    #[test]
    fn atomic_only_inherits_declared_stdio_handles() {
        let root = tempfile::tempdir().unwrap();
        let security = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: std::ptr::null_mut(),
            bInheritHandle: 1,
        };
        let event_name = format!("Global\\WinBridge-AtomicEvent-{}", nonce());
        let w = wide(std::ffi::OsStr::new(&event_name)).unwrap();
        let event = owned(unsafe {
            windows_sys::Win32::System::Threading::CreateEventW(&security, 1, 1, w.as_ptr())
        })
        .unwrap();
        let args = vec![
            "--exact".into(),
            "windows_atomic::tests::handle_helper".into(),
            "--ignored".into(),
            "--nocapture".into(),
            "--skip".into(),
            (event.as_raw_handle() as usize).to_string(),
            "--skip".into(),
            event_name,
        ];
        let cmd = command(&std::env::current_exe().unwrap(), root.path(), args);
        let (child, out, err) =
            ManagedChild::prepare_atomic_executable(&cmd, Some(&name()), None).unwrap();
        let (out, _) = finish(child, out, err);
        let text = String::from_utf8(out).unwrap();
        let value: serde_json::Value = serde_json::from_str(
            text.lines()
                .find_map(|l| l.split_once("handle-probe ").map(|(_, json)| json))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(value["inherited"], false);
        assert_eq!(value["stdinEof"], true);
        proof(
            "stdioOnly",
            serde_json::json!({"passed":true,"unlistedHandleInherited":false,"stdinEof":true}),
        );
    }
}
