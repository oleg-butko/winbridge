//! Process ownership. No task policy, journals, or deployment paths.
use std::{
    io::{self, Read},
    process::{Child, Command, ExitStatus, Stdio},
};
use winbridge_protocol::ResolvedCommand;

pub mod reports;
#[cfg(windows)]
mod windows;

pub struct ManagedChild {
    child: Child,
    #[cfg(windows)]
    job: windows::Job,
}

pub type OutputReader = Box<dyn Read + Send>;

/// Reject linked/reparse/special paths before accessing managed state or project files.
pub fn require_safe_path(path: &std::path::Path) -> io::Result<()> {
    if !path.is_absolute() {
        return Err(io::Error::other("Filesystem path must be absolute"));
    }
    for ancestor in path.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                let mut linked = metadata.file_type().is_symlink();
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    linked |= metadata.file_attributes() & 0x400 != 0;
                }
                #[cfg(not(windows))]
                {
                    let _ = &mut linked;
                }
                if linked || !(metadata.is_dir() || metadata.is_file()) {
                    return Err(io::Error::other(format!(
                        "Linked/reparse/special path refused: {}",
                        ancestor.display()
                    )));
                }
                if metadata.is_file() {
                    require_single_link(ancestor)?;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(io::Error::other(format!("{}: {e}", ancestor.display()))),
        }
    }
    Ok(())
}

/// Stable identity of the opened file, independent of name and length.
pub fn file_identity(file: &std::fs::File) -> io::Result<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = file.metadata()?;
        Ok(format!("{}:{}", meta.dev(), meta.ino()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let mut meta: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut meta) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(format!(
            "{}:{}:{}",
            meta.dwVolumeSerialNumber, meta.nFileIndexHigh, meta.nFileIndexLow
        ))
    }
}

pub fn require_local_disk(path: &std::path::Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetVolumePathNameW};
        let mut parent = path;
        while !parent.exists() {
            parent = parent
                .parent()
                .ok_or_else(|| io::Error::other("local disk path has no existing parent"))?;
        }
        let canonical = std::fs::canonicalize(parent)?;
        let wide: Vec<u16> = canonical.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut root = vec![0u16; 32768];
        if unsafe { GetVolumePathNameW(wide.as_ptr(), root.as_mut_ptr(), root.len() as u32) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { GetDriveTypeW(root.as_ptr()) } != 3 {
            return Err(io::Error::other("state/workspace directory must use a local fixed disk; network/shared/removable storage is unsupported"));
        }
    }
    #[cfg(unix)]
    {
        let mut parent = path;
        while !parent.exists() {
            parent = parent
                .parent()
                .ok_or_else(|| io::Error::other("local disk path has no existing parent"))?;
        }
        let canonical = std::fs::canonicalize(parent)?;
        let mounts = std::fs::read_to_string("/proc/mounts")?;
        for line in mounts.lines() {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() > 2 && fields[2].contains("vmhgfs") && canonical.starts_with(fields[1])
            {
                return Err(io::Error::other(
                    "state/workspace directory cannot use VMware Shared Folders",
                ));
            }
        }
    }
    Ok(())
}

impl ManagedChild {
    pub fn spawn(command: &ResolvedCommand) -> io::Result<(Self, OutputReader, OutputReader)> {
        let mut builder = Command::new(&command.executable);
        builder
            .args(&command.arguments)
            .current_dir(&command.working_directory)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            builder.process_group(0);
        }
        #[cfg(windows)]
        let job = {
            use std::os::windows::process::CommandExt;
            use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED};
            // No user code runs before the process belongs to the worker's Job Object.
            builder.creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW);
            windows::Job::new()?
        };
        let mut child = builder.spawn()?;
        #[cfg(windows)]
        if let Err(error) = job.assign_and_resume(&child) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        let stdout = Box::new(
            child
                .stdout
                .take()
                .ok_or_else(|| io::Error::other("stdout pipe missing"))?,
        );
        let stderr = Box::new(
            child
                .stderr
                .take()
                .ok_or_else(|| io::Error::other("stderr pipe missing"))?,
        );
        Ok((
            Self {
                child,
                #[cfg(windows)]
                job,
            },
            stdout,
            stderr,
        ))
    }

    pub fn id(&self) -> u32 {
        self.child.id()
    }
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }
    pub fn terminate_tree(&mut self) -> io::Result<()> {
        #[cfg(windows)]
        {
            self.job.terminate()
        }
        #[cfg(unix)]
        {
            // Negative PID selects the process group created before exec.
            let result = unsafe { libc::kill(-(self.child.id() as i32), libc::SIGKILL) };
            if result != 0 && io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }
    pub fn tree_is_empty(&self) -> io::Result<bool> {
        #[cfg(windows)]
        {
            self.job.is_empty()
        }
        #[cfg(unix)]
        {
            Ok(true)
        } // SIGKILL plus joined pipe readers and reaped child is the portable test adapter.
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        let _ = self.terminate_tree();
        let _ = self.child.wait();
    }
}

/// Check regular-file hard links on the real filesystem, including NTFS.
pub fn require_single_link(path: &std::path::Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let file = std::fs::File::open(path)?;
        let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if information.nNumberOfLinks != 1 {
            return Err(io::Error::other("Hard-linked regular file refused"));
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if std::fs::symlink_metadata(path)?.nlink() != 1 {
            return Err(io::Error::other("Hard-linked regular file refused"));
        }
    }
    Ok(())
}
