//! Bounded, one-request local IPC. UI lifetime never controls accepted tasks.
use std::{io, path::Path, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use winbridge_protocol::{LocalRequest, LocalResponse};

pub mod websocket;

#[cfg(windows)]
mod windows;

pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
pub const IPC_TIMEOUT: Duration = Duration::from_secs(10);

pub fn endpoint(state_directory: &Path, instance_id: &str) -> io::Result<String> {
    if instance_id.is_empty()
        || instance_id.len() > 80
        || !instance_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(io::Error::other("invalid instance ID"));
    }
    #[cfg(windows)]
    {
        let _ = state_directory;
        Ok(format!(
            r"\\.\pipe\WinBridge-{}-{instance_id}",
            windows::user_sid()?
        ))
    }
    #[cfg(unix)]
    {
        Ok(state_directory
            .join(format!("worker-{instance_id}.sock"))
            .to_string_lossy()
            .into_owned())
    }
}

pub async fn read_frame<S: AsyncRead + Unpin>(stream: &mut S) -> io::Result<Vec<u8>> {
    let length = stream.read_u32_le().await? as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(io::Error::other("IPC frame must contain 1–1048576 bytes"));
    }
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).await?;
    Ok(bytes)
}
pub async fn write_frame<S: AsyncWrite + Unpin>(stream: &mut S, bytes: &[u8]) -> io::Result<()> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(io::Error::other("IPC response exceeds the 1 MiB limit"));
    }
    stream.write_u32_le(bytes.len() as u32).await?;
    stream.write_all(bytes).await?;
    stream.flush().await
}
async fn exchange<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    request: &LocalRequest,
) -> io::Result<LocalResponse> {
    write_frame(&mut stream, &serde_json::to_vec(request)?).await?;
    serde_json::from_slice(&read_frame(&mut stream).await?).map_err(io::Error::other)
}
pub async fn call(address: &str, request: &LocalRequest) -> io::Result<LocalResponse> {
    tokio::time::timeout(IPC_TIMEOUT, async {
        #[cfg(unix)]
        {
            exchange(tokio::net::UnixStream::connect(address).await?, request).await
        }
        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ClientOptions;
            use windows_sys::Win32::Foundation::ERROR_PIPE_BUSY;
            let stream = loop {
                match ClientOptions::new().open(address) {
                    Ok(stream) => break stream,
                    Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                        tokio::time::sleep(Duration::from_millis(25)).await
                    }
                    Err(error) => return Err(error),
                }
            };
            exchange(stream, request).await
        }
    })
    .await
    .map_err(|_| {
        io::Error::new(
            io::ErrorKind::TimedOut,
            "worker IPC timeout; query the same idempotency key before submitting a new task",
        )
    })?
}

#[cfg(windows)]
pub fn create_pipe(
    address: &str,
    first: bool,
) -> io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
    windows::create_pipe(address, first)
}
