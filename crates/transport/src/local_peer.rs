//! OS-derived local peer, never constructed from caller-supplied JSON/PID.
use std::io;

#[derive(Debug)]
pub struct LocalPeer {
    process_id: u32,
}
impl LocalPeer {
    pub fn process_id(&self) -> u32 {
        self.process_id
    }
}
mod sealed {
    pub trait Sealed {}
}
/// Only the actual accepted local stream implementations can attest a peer.
pub trait LocalPeerStream: sealed::Sealed {
    fn local_peer(&self) -> io::Result<LocalPeer>;
}

#[cfg(unix)]
impl sealed::Sealed for tokio::net::UnixStream {}
#[cfg(unix)]
impl LocalPeerStream for tokio::net::UnixStream {
    fn local_peer(&self) -> io::Result<LocalPeer> {
        let credential = self.peer_cred()?;
        if credential.uid() != unsafe { libc::geteuid() } {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Local IPC user mismatch",
            ));
        }
        let process_id = credential
            .pid()
            .and_then(|p| u32::try_from(p).ok())
            .filter(|p| *p != 0)
            .ok_or_else(|| io::Error::other("Local IPC peer PID missing"))?;
        Ok(LocalPeer { process_id })
    }
}
#[cfg(windows)]
impl sealed::Sealed for tokio::net::windows::named_pipe::NamedPipeServer {}
#[cfg(windows)]
impl LocalPeerStream for tokio::net::windows::named_pipe::NamedPipeServer {
    fn local_peer(&self) -> io::Result<LocalPeer> {
        use std::os::windows::io::AsRawHandle;
        let mut process_id = 0;
        if unsafe {
            windows_sys::Win32::System::Pipes::GetNamedPipeClientProcessId(
                self.as_raw_handle(),
                &mut process_id,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if process_id == 0 {
            return Err(io::Error::other("Local IPC peer PID missing"));
        }
        Ok(LocalPeer { process_id })
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[tokio::test]
    async fn peer_pid_comes_from_the_connected_socket_not_its_payload() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut caller, mut accepted) = tokio::net::UnixStream::pair().unwrap();
        caller.write_all(br#"{"processId":1}"#).await.unwrap();
        let peer = accepted.local_peer().unwrap();
        let mut payload = vec![0; 15];
        accepted.read_exact(&mut payload).await.unwrap();
        assert_eq!(peer.process_id(), std::process::id());
        assert_ne!(peer.process_id(), 1);
        assert_eq!(payload, br#"{"processId":1}"#);
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    #[tokio::test]
    async fn accepted_named_pipe_authenticates_the_real_client_pid() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let address = format!(
            r"\\.\pipe\WinBridge-peer-proof-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let accepted = crate::create_pipe(&address, true).unwrap();
        let client = tokio::spawn(async move {
            crate::supervisor::connect(&address, std::process::id())
                .await
                .unwrap()
        });
        accepted.connect().await.unwrap();
        let _caller = client.await.unwrap();
        assert_eq!(
            accepted.local_peer().unwrap().process_id(),
            std::process::id()
        );
    }
}
