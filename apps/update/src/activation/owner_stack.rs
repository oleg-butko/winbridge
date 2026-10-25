//! Owned input survives ordinary startup errors; IO stays on the parent driver.
use std::{future::Future, sync::Mutex};
#[cfg(any(windows, test))]
pub(crate) fn on_original_io<T, R, F, U>(input: T, create: F) -> Result<R, (T, &'static str)>
where
    T: Send,
    R: Send,
    F: FnOnce(T) -> U + Send,
    U: Future<Output = R>,
{
    let io = match tokio::runtime::Handle::try_current() {
        Ok(handle) => handle,
        Err(_) => {
            return Err((
                input,
                "Original supervisor IO runtime is missing; resources retained",
            ))
        }
    };
    let slot = Mutex::new(Some(input));
    std::thread::scope(|scope| {
        let held = &slot;
        let thread = std::thread::Builder::new()
            .name("activation-launcher".into())
            .stack_size(8 * 1024 * 1024)
            .spawn_scoped(scope, move || {
                let input = held
                    .lock()
                    .unwrap()
                    .take()
                    .expect("One original activation owner");
                io.block_on(Box::pin(create(input)))
            });
        match thread {
            Ok(thread) => match thread.join() {
                Ok(result) => Ok(result),
                Err(panic) => std::panic::resume_unwind(panic),
            },
            Err(_) => Err((
                slot.lock()
                    .unwrap()
                    .take()
                    .expect("Unstarted original activation owner"),
                "Activation launcher owner thread could not start; resources retained",
            )),
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::Duration,
    };
    #[test]
    fn missing_runtime_returns_same_input_without_executing_factory() {
        let input = Arc::new(());
        let called = AtomicBool::new(false);
        let result = on_original_io(input.clone(), |_| {
            called.store(true, Ordering::SeqCst);
            async {}
        });
        let Err((retained, _)) = result else {
            panic!("No runtime must not dispatch")
        };
        assert!(Arc::ptr_eq(&retained, &input));
        assert!(!called.load(Ordering::SeqCst));
    }
    #[tokio::test]
    async fn returned_io_works_after_owner_thread_has_retired() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use winbridge_transport::supervisor as ipc;
        let temp = tempfile::tempdir().unwrap();
        let address = ipc::endpoint(temp.path(), &uuid::Uuid::new_v4().to_string()).unwrap();
        let input = address.clone();
        let mut listener = tokio::task::spawn_blocking(move || {
            on_original_io(
                input,
                |address| async move { ipc::Listener::bind(&address) },
            )
        })
        .await
        .unwrap()
        .unwrap()
        .unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            let (client, server) = tokio::join!(
                ipc::connect(&address, std::process::id()),
                listener.accept()
            );
            let mut client = client.unwrap();
            let (mut server, pid) = server.unwrap();
            assert_eq!(pid, std::process::id());
            client.write_all(b"retained").await.unwrap();
            let mut bytes = [0; 8];
            server.read_exact(&mut bytes).await.unwrap();
            assert_eq!(&bytes, b"retained");
        })
        .await
        .expect("The ORIGINAL IO driver must survive launcher thread retirement");
    }
}
