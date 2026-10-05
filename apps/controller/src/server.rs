use super::*;
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt},
    path::Path,
};

pub async fn serve(controller: Arc<Controller>) -> Result<()> {
    let root = &controller.config.state_directory;
    if !root.exists() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)
            .map_err(|e| format!("controller stateDirectory: {e}"))?;
    }
    let metadata = fs::symlink_metadata(root).map_err(|e| e.to_string())?;
    unsafe extern "C" {
        fn getuid() -> u32;
    }
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != unsafe { getuid() }
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(
            "controller stateDirectory must be a real private directory owned by this user (0700)"
                .into(),
        );
    }
    let listener = tokio::net::TcpListener::bind(controller.config.controller.listen_address)
        .await
        .map_err(|e| format!("controller listenAddress: {e}"))?;
    let address = endpoint(&controller.config);
    if let Ok(metadata) = fs::symlink_metadata(&address) {
        if !metadata.file_type().is_socket() {
            return Err(format!(
                "Controller IPC {address} is not a socket; existing file preserved"
            ));
        }
        match tokio::net::UnixStream::connect(&address).await {
            Ok(_) => return Err(format!("Controller IPC {address} is already active")),
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {}
            Err(e) => return Err(format!("Cannot verify stale controller IPC: {e}")),
        }
        fs::remove_file(&address).map_err(|e| e.to_string())?;
    }
    let ipc = tokio::net::UnixListener::bind(&address).map_err(|e| e.to_string())?;
    fs::set_permissions(&address, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    eprintln!("WinBridge controller ready: ws://{}; local IPC {address}; expected worker {} / instance {}", listener.local_addr().map_err(|e| e.to_string())?, controller.config.controller.worker_node_id, controller.config.instance_id);
    let network_limit = Arc::new(tokio::sync::Semaphore::new(4));
    let ipc_limit = Arc::new(tokio::sync::Semaphore::new(32));
    let mut connections = tokio::task::JoinSet::new();
    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            result = connections.join_next(), if !connections.is_empty() => {
                if let Some(Err(error)) = result { eprintln!("Controller connection: {error}"); }
            }
            result = listener.accept() => {
                let (stream, peer) = result.map_err(|e| e.to_string())?;
                let Ok(permit) = Arc::clone(&network_limit).try_acquire_owned() else { continue; };
                let controller = Arc::clone(&controller);
                connections.spawn(async move {
                    let _permit = permit;
                    if let Err(error) = controller.accept(stream, peer).await { eprintln!("Worker session {peer}: {error}"); }
                });
            }
            result = ipc.accept() => {
                let (mut stream, _) = result.map_err(|e| e.to_string())?;
                let Ok(permit) = Arc::clone(&ipc_limit).try_acquire_owned() else { continue; };
                let controller = Arc::clone(&controller);
                connections.spawn(async move {
                    let _permit = permit;
                    let operation = async {
                        let request = winbridge_transport::read_frame(&mut stream).await?;
                        let response = match serde_json::from_slice(&request) {
                            Ok(request) => controller.call(request).await,
                            Err(e) => LocalResponse::Error { message: format!("Invalid controller IPC request: {e}") },
                        };
                        let mut bytes = serde_json::to_vec(&response)?;
                        if bytes.len() > winbridge_transport::MAX_FRAME_BYTES {
                            bytes = serde_json::to_vec(&LocalResponse::Error { message: "Controller response exceeds 1 MiB; query an individual task".into() })?;
                        }
                        winbridge_transport::write_frame(&mut stream, &bytes).await
                    };
                    match tokio::time::timeout(winbridge_transport::IPC_TIMEOUT, operation).await {
                        Ok(Ok(())) => {},
                        result => eprintln!("Controller IPC request failed: {result:?}"),
                    }
                });
            }
        }
    }
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    fs::remove_file(Path::new(&address)).map_err(|e| e.to_string())?;
    Ok(())
}
