//! The full debug fixture driver must not poll on a small executor stack.
//! Construct and poll it here, retaining the original IO/timer runtime.
use crate::Result;
use std::future::Future;

/// One bounded thread per explicitly enabled Main fixture. The returned handle
/// joins it; no runtime, global stack setting, detached task or retry is added.
#[inline(never)]
pub(super) fn spawn<F, U>(create: F) -> Result<tokio::task::JoinHandle<Result<()>>>
where
    F: FnOnce() -> U + Send + 'static,
    U: Future<Output = Result<()>>,
{
    let io = tokio::runtime::Handle::try_current()
        .map_err(|_| "Main validation requires its original IO runtime")?;
    // MSVC debug Main26 drive::poll alone used 1,268,096 bytes before nested
    // control/serde calls. Keep that diagnostic chain off Tokio worker stacks.
    // This module is absent from ordinary builds (update-validation only).
    let owner = std::thread::Builder::new()
        .name("winbridge-main-validation".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || io.block_on(Box::pin(create())))
        .map_err(|e| format!("Cannot start Main validation owner: {e}"))?;
    Ok(tokio::task::spawn_blocking(move || {
        owner.join().map_err(|_| {
            "Main validation owner panicked; preserve its original attempt".to_string()
        })?
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use winbridge_transport::supervisor as ipc;

    #[test]
    fn missing_io_runtime_does_not_construct_or_run_the_driver() {
        let called = Arc::new(AtomicBool::new(false));
        let check = called.clone();
        let result = spawn(move || {
            called.store(true, Ordering::SeqCst);
            async { Ok(()) }
        });
        assert!(result.is_err());
        assert!(!check.load(Ordering::SeqCst));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn named_owner_uses_original_pipe_io_and_timer_and_is_joined_on_error() {
        let root = tempfile::tempdir().unwrap();
        let address = ipc::endpoint(root.path(), &uuid::Uuid::new_v4().to_string()).unwrap();
        let mut listener = ipc::Listener::bind(&address).unwrap();
        let handle = spawn(move || async move {
            assert_eq!(
                std::thread::current().name(),
                Some("winbridge-main-validation")
            );
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            let response: serde_json::Value = ipc::call(
                &address,
                std::process::id(),
                &serde_json::json!({"query":1}),
            )
            .await
            .map_err(|e| e.to_string())?;
            assert_eq!(response, serde_json::json!({"observed":1}));
            Err("original diagnostic failure".into())
        })
        .unwrap();
        let (mut stream, pid) = listener.accept().await.unwrap();
        assert_eq!(pid, std::process::id());
        let request: serde_json::Value = ipc::read(&mut stream).await.unwrap();
        assert_eq!(request, serde_json::json!({"query":1}));
        ipc::write(&mut stream, &serde_json::json!({"observed":1}))
            .await
            .unwrap();
        assert_eq!(
            handle.await.unwrap().unwrap_err(),
            "original diagnostic failure"
        );
    }
}
