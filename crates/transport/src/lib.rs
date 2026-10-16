//! Bounded, one-request local IPC. UI lifetime never controls accepted tasks.
use std::{io, path::Path, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use winbridge_protocol::{
    FencedRequest, FencedResponse, LocalRequest, LocalResponse, WireRequest, WireResponse,
};

mod readiness;
pub mod websocket;
pub use readiness::netwatch_health;

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
async fn exchange<
    S: AsyncRead + AsyncWrite + Unpin,
    Q: serde::Serialize,
    R: serde::de::DeserializeOwned,
>(
    mut stream: S,
    request: &Q,
) -> io::Result<R> {
    write_frame(&mut stream, &serde_json::to_vec(request)?).await?;
    serde_json::from_slice(&read_frame(&mut stream).await?).map_err(io::Error::other)
}
pub async fn call(address: &str, request: &LocalRequest) -> io::Result<LocalResponse> {
    match call_wire(address, &WireRequest::Legacy(request.clone())).await? {
        WireResponse::Legacy(response) => Ok(*response),
        WireResponse::Fenced(_) => Err(io::Error::other(
            "Unexpected v2 response to a v1 request; no fallback or automatic retry",
        )),
    }
}
pub async fn call_fenced(address: &str, request: &FencedRequest) -> io::Result<FencedResponse> {
    match call_wire(address, &WireRequest::Fenced(Box::new(request.clone()))).await? {
        WireResponse::Fenced(response) => {
            response.validate_for(&request.context).map_err(io::Error::other)?;
            Ok(*response)
        },
        WireResponse::Legacy(_) => Err(io::Error::other("Peer did not return request contract 2; acceptance is unconfirmed, no fallback or automatic retry")),
    }
}
pub async fn call_wire(address: &str, request: &WireRequest) -> io::Result<WireResponse> {
    call_dto(
        address,
        request,
        "worker IPC timeout; query the same idempotency key before submitting a new task",
    )
    .await
}
pub async fn call_extension(
    address: &str,
    request: &winbridge_protocol::ExtensionRequest,
) -> io::Result<winbridge_protocol::ExtensionResponse> {
    let response: winbridge_protocol::ExtensionResponse = call_dto(
        address,
        request,
        "Readonly worker IPC timeout; saved page retained, no command submitted",
    )
    .await?;
    response.validate_for(request).map_err(io::Error::other)?;
    Ok(response)
}
pub async fn operation_page(
    address: &str,
    request: &winbridge_protocol::OperationPageRequest,
) -> Result<winbridge_protocol::OperationPage, String> {
    use winbridge_protocol::*;
    let LocalResponse::Info { info } = call(address, &LocalRequest::Info)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Err("Missing operation provider metadata".into());
    };
    if !matches!(interpret_catalog(&info),CatalogObservation::Known(c) if c.supports(EXTENSION_TRANSPORT,1)&&c.supports(OPERATION_READ_MODEL,1))
        || !interpret_contract_schemas(&info).is_some_and(|c| {
            c.schemas.iter().any(|s| {
                s.contract == operation_contract() && s.sha256 == operation_schema_sha256()
            })
        })
    {
        return Err(
            "Worker does not support this operation page contract/schema; no fallback".into(),
        );
    }
    if !info
        .fenced_requests
        .as_ref()
        .is_some_and(|f| f.context == request.context)
    {
        return Err("Operation query scope changed; cursor retained, no rebind".into());
    }
    let envelope = ExtensionRequest {
        extension_version: 1,
        contract: operation_contract(),
        context: request.context.clone(),
        payload: serde_json::to_value(&request.query).map_err(|e| e.to_string())?,
    };
    decode_operation_page(
        call_extension(address, &envelope)
            .await
            .map_err(|e| e.to_string())?,
        request,
    )
}
/// A distinct local control route; WebSocket and readonly extensions cannot dispatch it.
pub async fn queue_control(
    address: &str,
    request: &winbridge_protocol::QueueControlRequest,
) -> Result<winbridge_protocol::QueueControlResponse, String> {
    use winbridge_protocol::*;
    if request.queue_control_version != 1 {
        return Err("Unsupported queue control request version; no fallback".into());
    }
    let LocalResponse::Info { info } = call(address, &LocalRequest::Info)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Err("Queue provider unavailable; no change submitted".into());
    };
    let Some(view) = interpret_queue_control(&info) else {
        return Err("Queue control metadata not supported; no change submitted".into());
    };
    if view.context != request.context
        || !interpret_contract_schemas(&info).is_some_and(|s| {
            s.schemas.iter().any(|s| {
                s.contract.id == QUEUE_CONTROL_CONTRACT
                    && s.contract.version == 1
                    && s.sha256 == queue_control_schema_sha256()
            })
        })
    {
        return Err(
            "Queue scope/schema changed; retain original request, no fallback/rebind".into(),
        );
    }
    let message = if matches!(request.command, QueueControlCommand::Set { .. }) {
        "Queue change acknowledgement unconfirmed; observe original command ID/context before repeating only that exact request"
    } else {
        "Readonly queue observation timeout; no change submitted"
    };
    let response: QueueControlResponse = call_dto(address, request, message)
        .await
        .map_err(|e| e.to_string())?;
    response.validate_for(request)?;
    let observation = match &response.result {
        QueueControlResult::Applied { observation, .. }
        | QueueControlResult::Observed { observation, .. } => Some(observation),
        _ => None,
    };
    if observation.is_some_and(|o| Some(&o.provider) != info.build.as_ref()) {
        return Err(
            "Queue provider changed during request; outcome unconfirmed, retain original command"
                .into(),
        );
    }
    Ok(response)
}
async fn call_dto<Q: serde::Serialize, R: serde::de::DeserializeOwned>(
    address: &str,
    request: &Q,
    timeout_message: &str,
) -> io::Result<R> {
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
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, timeout_message))?
}

#[cfg(windows)]
pub fn create_pipe(
    address: &str,
    first: bool,
) -> io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
    windows::create_pipe(address, first)
}
