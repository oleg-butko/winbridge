//! Dispatch/correlation tests with synthetic process metadata; no OS custody proof.
use super::*;
fn query() -> KeeperProbeRequest {
    KeeperProbeRequest {
        keeper_probe_version: 1,
        request_id: uuid::Uuid::new_v4().to_string(),
        binding: crate::bootstrap::tests::fixture().previous,
    }
}
#[test]
fn probe_rejects_unbound_foreign_or_changed_peers_before_physical_queries() {
    let original = query().binding;
    for fault in 0..12 {
        let mut q = query();
        let mut peer = original.worker.process_id;
        match fault {
            0 => peer = original.coordinator.process_id,
            1 => q.keeper_probe_version = 2,
            2 => q.request_id = "invalid".into(),
            3 => q.binding.selection_sha256 = None,
            4 => q.binding.generation = uuid::Uuid::new_v4().to_string(),
            5 => q.binding.worker.birth = "999".into(),
            6 => q.binding.coordinator.birth = "999".into(),
            7 => q.binding.job_name.push_str("-other"),
            8 => q.binding.context.instance_id.push_str("-other"),
            9 => q.binding.finalization_command_sha256 = "a".repeat(64),
            10 => q.binding.worker.host_id = "0".repeat(64),
            _ => q.binding.context.journal_incarnation = uuid::Uuid::new_v4().to_string(),
        }
        let r = respond(Some(&original), Some(&q), peer, |_, _| {
            panic!("Untrusted metadata reached physical checks")
        });
        assert!(
            matches!(r.result, KeeperProbeResult::Rejected { .. }),
            "fault {fault}"
        );
    }
    assert!(matches!(
        respond(
            None,
            Some(&query()),
            original.worker.process_id,
            |_, _| panic!("Cold rows may not create custody")
        )
        .result,
        KeeperProbeResult::Rejected { .. }
    ));
    assert!(matches!(
        respond(
            Some(&original),
            None,
            original.worker.process_id,
            |_, _| panic!("Malformed DTO reached custody")
        )
        .result,
        KeeperProbeResult::Rejected { .. }
    ));
}
#[test]
fn accepted_probe_requires_the_original_physical_check_and_exact_response() {
    let q = query();
    let mut calls = 0;
    let r = respond(
        Some(&q.binding),
        Some(&q),
        q.binding.worker.process_id,
        |pid, binding| {
            calls += 1;
            assert_eq!(pid, q.binding.worker.process_id);
            assert_eq!(binding, &q.binding);
            Ok(()) // Dispatch semantics only, not an OS observation.
        },
    );
    assert_eq!(calls, 1);
    validate_response(&q, &r, 0, clock().unwrap()).unwrap();
    for fault in 0..5 {
        let mut bad = r.clone();
        match fault {
            0 => bad.request_id = None,
            1 => bad.keeper_probe_version = 2,
            2 => {
                if let KeeperProbeResult::Observed { binding, .. } = &mut bad.result {
                    binding.generation = uuid::Uuid::new_v4().to_string();
                }
            }
            3 => {
                if let KeeperProbeResult::Observed { observed_at_ms, .. } = &mut bad.result {
                    *observed_at_ms = u64::MAX;
                }
            }
            _ => {
                bad.result = KeeperProbeResult::Rejected {
                    message: "Unavailable".into(),
                }
            }
        }
        assert!(
            validate_response(&q, &bad, 0, clock().unwrap()).is_err(),
            "fault {fault}"
        );
    }
    let refused = respond(
        Some(&q.binding),
        Some(&q),
        q.binding.worker.process_id,
        |_, _| Err("Exact Job query failed".into()),
    );
    assert!(matches!(refused.result, KeeperProbeResult::Rejected { .. }));
}
#[tokio::test]
async fn another_process_cannot_use_saved_binding_to_contact_a_keeper() {
    let q = query();
    let root = tempfile::tempdir().unwrap();
    assert_ne!(std::process::id(), q.binding.worker.process_id);
    let e = observe_original_keeper(root.path(), &q.binding)
        .await
        .err()
        .unwrap();
    assert!(e.contains("Only the original activated worker"));
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::io::Read;
    use winbridge_platform::ManagedChild;
    const HELPER: &str = "supervisor::keeper_probe::tests::native::probe_worker_helper";
    #[tokio::test]
    #[ignore = "Owned helper child, launched only by the native peer/Job case"]
    async fn probe_worker_helper() {
        let input = fs::read("probe-input.json").unwrap();
        let (directory, binding): (PathBuf, KeeperBinding) =
            serde_json::from_slice(&input).unwrap();
        assert_eq!(std::process::id(), binding.worker.process_id);
        let endpoint = ipc::endpoint(&directory, &binding.launch_id).unwrap();
        let mut q = KeeperProbeRequest {
            keeper_probe_version: 1,
            request_id: uuid::Uuid::new_v4().to_string(),
            binding: binding.clone(),
        };
        q.binding.selection_sha256 = Some("a".repeat(64));
        let wrong: KeeperProbeResponse = ipc::call(&endpoint, binding.coordinator.process_id, &q)
            .await
            .unwrap();
        assert!(matches!(wrong.result, KeeperProbeResult::Rejected { .. }));
        struct Duplicate(KeeperProbeRequest);
        impl serde::Serialize for Duplicate {
            fn serialize<S: serde::Serializer>(
                &self,
                s: S,
            ) -> std::result::Result<S::Ok, S::Error> {
                use serde::ser::SerializeMap;
                let mut m = s.serialize_map(Some(4))?;
                m.serialize_entry("keeperProbeVersion", &1u16)?;
                m.serialize_entry("requestId", &self.0.request_id)?;
                m.serialize_entry("requestId", &self.0.request_id)?;
                m.serialize_entry("binding", &self.0.binding)?;
                m.end()
            }
        }
        q.binding = binding.clone();
        let malformed: KeeperProbeResponse =
            ipc::call(&endpoint, binding.coordinator.process_id, &Duplicate(q))
                .await
                .unwrap();
        assert!(matches!(
            malformed.result,
            KeeperProbeResult::Rejected { .. }
        ));
        assert!(malformed.request_id.is_none());
        let capture = observe_original_keeper(&directory, &binding).await.unwrap();
        assert_eq!(capture.binding(), &binding);
        capture.require_current().unwrap();
        tokio::time::sleep(ipc::TIMEOUT + Duration::from_millis(50)).await;
        assert!(capture.require_current().is_err());
        // Expiry still forbids a NEW admission/Start. It must not prevent an
        // original attempt's readonly continuation using these exact handles.
        capture.require_original_processes().unwrap();
        eprintln!(
            "WINBRIDGE_PROBE_CLIENT={}",
            serde_json::json!({"worker":binding.worker,"keeper":binding.coordinator,"changedBindingRejected":true,"duplicateFieldsRejected":true,"exactCaptureObserved":true,"captureExpired":true})
        );
    }
    #[tokio::test]
    #[ignore = "Real Windows IPC child/Job/handle check; isolated fixture, no updater SQL or network"]
    async fn original_child_probe_uses_real_peer_and_exact_handles() {
        let proof = PathBuf::from(
            std::env::var_os("WINBRIDGE_KEEPER_PROBE_NATIVE_PROOF")
                .expect("Explicit native proof path required"),
        );
        let directory = tempfile::tempdir().unwrap();
        let keeper = infrastructure::stamp(std::process::id()).unwrap();
        let job = format!("Local\\WinBridge-Probe-{}", uuid::Uuid::new_v4());
        let command = winbridge_protocol::ResolvedCommand {
            executable: std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            arguments: vec![
                "--exact".into(),
                HELPER.into(),
                "--ignored".into(),
                "--test-threads=1".into(),
                "--nocapture".into(),
            ],
            working_directory: directory.path().to_string_lossy().into_owned(),
            timeout_ms: 15000,
        };
        let (mut child, mut stdout, mut stderr) =
            ManagedChild::prepare(&command, Some(&job)).unwrap();
        let worker = infrastructure::stamp(child.id()).unwrap();
        let mut binding = query().binding;
        binding.launch_id = uuid::Uuid::new_v4().to_string();
        binding.coordinator = keeper.clone();
        binding.worker = worker.clone();
        binding.job_name = job;
        let endpoint = ipc::endpoint(directory.path(), &binding.launch_id).unwrap();
        let mut listener = ipc::Listener::bind(&endpoint).unwrap();
        fs::write(
            directory.path().join("probe-input.json"),
            serde_json::to_vec(&(directory.path(), &binding)).unwrap(),
        )
        .unwrap();
        let out = std::thread::spawn(move || {
            let mut b = Vec::new();
            stdout.read_to_end(&mut b).unwrap();
            b
        });
        let err = std::thread::spawn(move || {
            let mut b = Vec::new();
            stderr.read_to_end(&mut b).unwrap();
            b
        });
        eprintln!(
            "WINBRIDGE_PROBE_SERVER={}",
            serde_json::json!({"keeper":keeper,"worker":worker})
        );
        child.resume().unwrap();
        let custody =
            crate::activation::ActivationKeeperCustody::probe_test_custody(child, binding.clone())
                .unwrap();
        let server = tokio::spawn(async move {
            let mut observed = 0;
            for _ in 0..4 {
                let (mut stream, pid) =
                    tokio::time::timeout(Duration::from_secs(10), listener.accept())
                        .await
                        .unwrap()
                        .unwrap();
                let bytes = ipc::read_bytes(&mut stream).await.unwrap();
                let q = serde_json::from_slice::<KeeperProbeRequest>(&bytes).ok();
                let r = respond(
                    Some(custody.bootstrap_binding()),
                    q.as_ref(),
                    pid,
                    |pid, b| custody.require_bootstrap_peer(pid, b),
                );
                if matches!(r.result, KeeperProbeResult::Observed { .. }) {
                    observed += 1;
                }
                ipc::write(&mut stream, &r).await.unwrap();
            }
            (custody, observed)
        });
        // Same valid JSON from the parent has the wrong ACTUAL IPC peer PID.
        let q = KeeperProbeRequest {
            keeper_probe_version: 1,
            request_id: uuid::Uuid::new_v4().to_string(),
            binding: binding.clone(),
        };
        let wrong: KeeperProbeResponse = ipc::call(&endpoint, keeper.process_id, &q).await.unwrap();
        assert!(matches!(wrong.result, KeeperProbeResult::Rejected { .. }));
        let (mut custody, observed) = server.await.unwrap();
        assert_eq!(observed, 1);
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(s) = custody.physical_test_child().try_wait().unwrap() {
                break s;
            }
            assert!(Instant::now() < deadline, "Probe helper did not exit");
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        let stdout = String::from_utf8_lossy(&out.join().unwrap()).into_owned();
        let stderr = String::from_utf8_lossy(&err.join().unwrap()).into_owned();
        assert!(status.success(), "stdout={stdout}; stderr={stderr}");
        let drain_deadline = Instant::now() + Duration::from_secs(3);
        while !custody.physical_test_child().tree_is_empty().unwrap() {
            assert!(
                Instant::now() < drain_deadline,
                "Original Job did not become empty"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let marker = "WINBRIDGE_PROBE_CLIENT=";
        let start = stderr
            .find(marker)
            .expect("Missing real child capture result")
            + marker.len();
        let client: serde_json::Value =
            serde_json::from_str(stderr[start..].lines().next().unwrap()).unwrap();
        assert_eq!(client["worker"], serde_json::to_value(&worker).unwrap());
        assert_eq!(client["keeper"], serde_json::to_value(&keeper).unwrap());
        let report = serde_json::json!({"schemaVersion":1,"kind":"keeper_probe_native_unit_proof","platform":"windows","passed":true,"build":winbridge_protocol::build_identity(),"scope":"real OS peer/Job/capture; synthetic role history, no updater workflow","syntheticRoleHistory":true,"keeper":keeper,"worker":worker,"client":client,"wrongPeerRejected":true,"observedResponses":observed,"childExitCode":status.code(),"originalJobEmpty":true,"readersFinished":true,"runtimeApplied":false,"installedRuntimeApplied":false,"windowsOpened":false,"networkUsed":false});
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(proof)
            .unwrap();
        use std::io::Write;
        f.write_all(&serde_json::to_vec_pretty(&report).unwrap())
            .unwrap();
    }
}
