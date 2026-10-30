//! Separate bounded same-user channel. It does not extend legacy worker RPC.
use serde::{de::DeserializeOwned, Serialize};
use std::{io, path::Path, time::Duration};
use tokio::io::{AsyncRead, AsyncWrite};
pub const TIMEOUT: Duration = Duration::from_secs(3);
/// Source verification/physical scans can exceed the short handoff deadline.
pub const CONTROL_TIMEOUT: Duration = Duration::from_secs(45);
#[cfg(unix)]
pub type Stream = tokio::net::UnixStream;
#[cfg(windows)]
pub type Stream = tokio::net::windows::named_pipe::NamedPipeServer;
#[cfg(unix)]
pub type ClientStream = tokio::net::UnixStream;
#[cfg(windows)]
pub type ClientStream = tokio::net::windows::named_pipe::NamedPipeClient;
pub struct Listener {
    #[cfg(unix)]
    inner: tokio::net::UnixListener,
    #[cfg(windows)]
    inner: Option<Stream>,
    #[cfg(windows)]
    address: String,
}
pub fn endpoint(directory: &Path, id: &str) -> io::Result<String> {
    if id.len() != 36 || !id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        return Err(io::Error::other("Invalid supervisor launch ID"));
    }
    #[cfg(windows)]
    {
        let _ = directory;
        Ok(format!(
            r"\\.\pipe\WinBridge-{}-update-{id}",
            super::windows::user_sid()?
        ))
    }
    #[cfg(unix)]
    {
        use sha2::{Digest, Sha256};
        let name = format!("s-{:x}.sock", Sha256::digest(id.as_bytes()));
        Ok(directory.join(&name[..27]).to_string_lossy().into_owned())
    }
}
impl Listener {
    pub fn bind(address: &str) -> io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let inner = tokio::net::UnixListener::bind(address)?;
            std::fs::set_permissions(address, std::fs::Permissions::from_mode(0o600))?;
            Ok(Self { inner })
        }
        #[cfg(windows)]
        {
            Ok(Self {
                inner: Some(super::create_pipe(address, true)?),
                address: address.into(),
            })
        }
    }
    pub async fn accept(&mut self) -> io::Result<(Stream, u32)> {
        #[cfg(unix)]
        {
            let (stream, _) = self.inner.accept().await?;
            let credential = stream
                .peer_cred()
                .map_err(|e| io::Error::new(io::ErrorKind::ConnectionAborted, e))?;
            if credential.uid() != unsafe { libc::geteuid() } {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Supervisor IPC user mismatch",
                ));
            }
            let pid = credential
                .pid()
                .and_then(|p| u32::try_from(p).ok())
                .ok_or_else(|| io::Error::other("Supervisor IPC peer PID missing"))?;
            Ok((stream, pid))
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            let result = self
                .inner
                .as_ref()
                .ok_or_else(|| io::Error::other("Supervisor listener is closed"))?
                .connect()
                .await;
            let old = self.inner.take().unwrap();
            // Keep the connected instance alive while creating the next listener.
            self.inner = Some(super::create_pipe(&self.address, false)?);
            result.map_err(|e| io::Error::new(io::ErrorKind::ConnectionAborted, e))?;
            let stream = old;
            let mut pid = 0;
            if unsafe {
                windows_sys::Win32::System::Pipes::GetNamedPipeClientProcessId(
                    stream.as_raw_handle(),
                    &mut pid,
                )
            } == 0
            {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    io::Error::last_os_error(),
                ));
            }
            Ok((stream, pid))
        }
    }
}
/// Bounded original frame for a dispatcher. Parse typed requests from THESE
/// bytes; a routing Value may collapse duplicates and cannot validate a DTO.
pub async fn read_bytes(stream: &mut Stream) -> io::Result<Vec<u8>> {
    tokio::time::timeout(TIMEOUT, async {
        let bytes = super::read_frame(stream).await?;
        if bytes.len() > 32768 {
            return Err(io::Error::other("Supervisor message exceeds 32 KiB"));
        }
        Ok(bytes)
    })
    .await
    .map_err(|_| io::Error::other("Supervisor read timeout"))?
}
pub async fn read<Q: DeserializeOwned>(stream: &mut Stream) -> io::Result<Q> {
    serde_json::from_slice(&read_bytes(stream).await?).map_err(io::Error::other)
}
pub async fn write<R: Serialize>(stream: &mut Stream, response: &R) -> io::Result<()> {
    tokio::time::timeout(
        TIMEOUT,
        super::write_frame(stream, &serde_json::to_vec(response)?),
    )
    .await
    .map_err(|_| io::Error::other("Supervisor write timeout; acceptance unknown"))?
}
async fn exchange<S: AsyncRead + AsyncWrite + Unpin, Q: Serialize, R: DeserializeOwned>(
    mut stream: S,
    request: &Q,
) -> io::Result<R> {
    let bytes = serde_json::to_vec(request)?;
    if bytes.len() > 32768 {
        return Err(io::Error::other("Supervisor request too large"));
    }
    super::write_frame(&mut stream, &bytes).await?;
    let bytes = super::read_frame(&mut stream).await?;
    if bytes.len() > 32768 {
        return Err(io::Error::other("Supervisor response too large"));
    }
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}
/// Wait only for an available connection, before any request bytes are sent.
/// A command exchange is never retried, including after an unknown ACK.
pub async fn connect(address: &str, expected_pid: u32) -> io::Result<ClientStream> {
    tokio::time::timeout(TIMEOUT, async {
        #[cfg(unix)]
        let stream = {
            let stream = tokio::net::UnixStream::connect(address).await?;
            let peer = stream.peer_cred()?;
            if peer.uid() != unsafe { libc::geteuid() }
                || peer.pid().and_then(|p| u32::try_from(p).ok()) != Some(expected_pid)
            {
                return Err(io::Error::other("Supervisor IPC server identity mismatch"));
            }
            stream
        };
        #[cfg(windows)]
        let stream = {
            use std::os::windows::io::AsRawHandle;
            let stream = loop {
                match tokio::net::windows::named_pipe::ClientOptions::new().open(address) {
                    Ok(stream) => break stream,
                    Err(e) if e.raw_os_error() == Some(231) => {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                    Err(e) => return Err(e),
                }
            };
            let mut pid = 0;
            if unsafe {
                windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId(
                    stream.as_raw_handle(),
                    &mut pid,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            if pid != expected_pid {
                return Err(io::Error::other("Supervisor IPC server PID mismatch"));
            }
            stream
        };
        Ok(stream)
    })
    .await
    .map_err(|_| io::Error::other("Supervisor IPC connect timeout; request not sent"))?
}
pub async fn call<Q: Serialize, R: DeserializeOwned>(
    address: &str,
    expected_pid: u32,
    request: &Q,
) -> io::Result<R> {
    call_with_timeout(address, expected_pid, request, TIMEOUT).await
}
pub async fn call_with_timeout<Q: Serialize, R: DeserializeOwned>(
    address: &str,
    expected_pid: u32,
    request: &Q,
    deadline: Duration,
) -> io::Result<R> {
    tokio::time::timeout(deadline, async {
        let stream = connect(address, expected_pid).await?;
        exchange(stream, request).await
    })
    .await
    .map_err(|_| io::Error::other("Supervisor IPC timeout; do not resend handoff or spawn"))?
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn busy_pipe_waits_without_sending() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            & 0xffffffffffff;
        let id = format!("{:08x}-0000-4000-8000-{nonce:012x}", std::process::id());
        let address = endpoint(Path::new("."), &id).unwrap();
        let mut listener = Listener::bind(&address).unwrap();
        // Occupy the only instance without letting the listener rotate it yet.
        let first = tokio::net::windows::named_pipe::ClientOptions::new()
            .open(&address)
            .unwrap();
        let mut pending = tokio::spawn(async move { connect(&address, std::process::id()).await });
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut pending)
                .await
                .is_err()
        );
        let (first_server, _) = listener.accept().await.unwrap();
        let client = pending.await.unwrap().unwrap();
        let (mut server, _) = listener.accept().await.unwrap();
        let mut byte = [0];
        assert!(
            tokio::time::timeout(Duration::from_millis(100), server.read_exact(&mut byte))
                .await
                .is_err()
        );
        drop((first, first_server, client, server));
    }
}
