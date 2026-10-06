//! Isolated Netwatch JSON-line IPC; readiness must belong to the owned process.
use std::{io, path::Path, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use winbridge_protocol::NetwatchProbe;

async fn health<S: AsyncRead + AsyncWrite + Unpin>(mut stream: S, pid: u32) -> io::Result<String> {
    stream.write_all(b"{\"command\":\"health\"}\n").await?;
    let mut body = Vec::new();
    loop {
        let byte = stream.read_u8().await?;
        if byte == b'\n' {
            break;
        }
        if body.len() >= 4096 {
            return Err(io::Error::other(
                "Netwatch health response exceeds 4096 bytes",
            ));
        }
        body.push(byte);
    }
    let value: serde_json::Value = serde_json::from_slice(&body)?;
    if value["ok"] != true
        || value["data"]["status"] != "ok"
        || value["data"]["service"] != "netwatch"
        || value["data"]["pid"].as_u64() != Some(pid as u64)
    {
        return Err(io::Error::other(
            "Netwatch health must confirm service/status and the owned application PID",
        ));
    }
    Ok(format!(
        "Netwatch IPC health confirmed: owned PID {pid}, version {}",
        value["data"]["version"].as_str().unwrap_or("unknown")
    ))
}

pub async fn netwatch_health(
    root: &Path,
    probe: &NetwatchProbe,
    pid: u32,
) -> Result<String, String> {
    // Admission validates the relative path and isolated instance prefix.
    let directory = root.join(probe.runtime_directory.replace('\\', "/"));
    winbridge_platform::require_safe_path(&directory).map_err(|e| e.to_string())?;
    tokio::time::timeout(Duration::from_millis(500), async {
        #[cfg(unix)]
        {
            let address = directory.join(".netwatch/agent.sock");
            winbridge_platform::require_safe_path(address.parent().unwrap())?;
            if std::fs::symlink_metadata(&address)?
                .file_type()
                .is_symlink()
            {
                return Err(io::Error::other("Linked Netwatch IPC endpoint refused"));
            }
            let stream = tokio::net::UnixStream::connect(address).await?;
            if stream.peer_cred()?.pid().map(|p| p as u32) != Some(pid) {
                return Err(io::Error::other(
                    "Netwatch IPC server PID differs from owned application",
                ));
            }
            health(stream, pid).await
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use tokio::net::windows::named_pipe::ClientOptions;
            use windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId;
            let address = format!(
                r"\\.\pipe\netwatch-agent-{}-{}",
                super::windows::user_sid()?,
                probe.instance_id
            );
            let stream = ClientOptions::new().open(address)?;
            let mut server_pid = 0;
            if unsafe { GetNamedPipeServerProcessId(stream.as_raw_handle(), &mut server_pid) } == 0
            {
                return Err(io::Error::last_os_error());
            }
            if server_pid != pid {
                return Err(io::Error::other(
                    "Netwatch named pipe server PID differs from owned application",
                ));
            }
            health(stream, pid).await
        }
    })
    .await
    .map_err(|_| "Netwatch health request exceeded 500 ms".to_string())?
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn readiness_checks_pid_service_and_bounded_json_line() {
        for body in [
            b"{\"ok\":true,\"data\":{\"status\":\"ok\",\"service\":\"netwatch\",\"pid\":123}}\n"
                .to_vec(),
            b"{\"ok\":true,\"data\":{\"status\":\"ok\",\"service\":\"netwatch\",\"pid\":124}}\n"
                .to_vec(),
            b"{\"ok\":false}\n".to_vec(),
            vec![b'x'; 4097],
        ] {
            let expected = body.windows(9).any(|v| v == b"pid\":123}");
            let (client, mut server) = tokio::io::duplex(8192);
            let job = tokio::spawn(async move {
                let mut request = [0; 21];
                server.read_exact(&mut request).await.unwrap();
                server.write_all(&body).await.unwrap();
            });
            assert_eq!(health(client, 123).await.is_ok(), expected);
            job.await.unwrap();
        }
    }

    #[tokio::test]
    async fn ipc_owner_must_match_the_owned_application_pid() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("runtime/.netwatch");
        std::fs::create_dir_all(&directory).unwrap();
        let instance = format!(
            "winbridge-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let probe = NetwatchProbe {
            instance_id: instance.clone(),
            runtime_directory: "runtime".into(),
        };
        #[cfg(unix)]
        let listener = tokio::net::UnixListener::bind(directory.join("agent.sock")).unwrap();
        #[cfg(windows)]
        let listener = super::super::create_pipe(
            &format!(
                r"\\.\pipe\netwatch-agent-{}-{instance}",
                super::super::windows::user_sid().unwrap()
            ),
            true,
        )
        .unwrap();
        let server = tokio::spawn(async move {
            #[cfg(unix)]
            let (mut stream, _) = listener.accept().await.unwrap();
            #[cfg(windows)]
            let mut stream = {
                listener.connect().await.unwrap();
                listener
            };
            let mut request = [0; 21];
            if stream.read_exact(&mut request).await.is_ok() {
                let body = format!("{{\"ok\":true,\"data\":{{\"status\":\"ok\",\"service\":\"netwatch\",\"pid\":{}}}}}\n", std::process::id() + 1);
                let _ = stream.write_all(body.as_bytes()).await;
            }
        });
        // Even a self-reported matching PID is rejected if the OS reports another server.
        let result = netwatch_health(temp.path(), &probe, std::process::id() + 1).await;
        assert!(result.unwrap_err().contains("server PID"));
        server.await.unwrap();
    }
}
