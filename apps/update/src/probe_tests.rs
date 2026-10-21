//! Actual owned process hooks, not journal/health approval fixtures.
use super::*;
struct Fixture {
    root: tempfile::TempDir,
    image: std::path::PathBuf,
    canary: std::path::PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::Builder::new()
            .prefix("wb-probe-")
            .tempdir_in(if cfg!(unix) {
                std::path::PathBuf::from("/var/tmp")
            } else {
                std::env::temp_dir()
            })
            .unwrap();
        let image = root.path().join("probe.exe");
        std::fs::copy(std::env::current_exe().unwrap(), &image).unwrap();
        let canary = root.path().join("ran");
        Self {
            root,
            image,
            canary,
        }
    }
    fn args(&self, long: bool) -> Vec<String> {
        let mut args = vec![
            "--exact".into(),
            "artifact::probe_tests::owned_probe_child".into(),
            "--ignored".into(),
            "--nocapture".into(),
            "--skip".into(),
            format!("probe-canary={}", self.canary.display()),
            "--skip".into(),
            format!(
                "probe-release={}",
                self.root.path().join("release").display()
            ),
        ];
        if long {
            args.extend(["--skip".into(), "probe-long=true".into()]);
        }
        args
    }
    fn observer(&self) -> Observer {
        Observer {
            resume_release: Some(self.root.path().join("release")),
            ..Default::default()
        }
    }
    fn job(&self) -> String {
        format!(
            "Global\\WinBridge-ProbeTest-{}",
            crate::sha256(self.root.path().to_string_lossy().as_bytes())
        )
    }
}
#[test]
#[ignore = "Owned private probe helper; parent tests invoke only a copied image"]
fn owned_probe_child() {
    let args: Vec<_> = std::env::args().collect();
    let canary = args
        .iter()
        .find_map(|a| a.strip_prefix("probe-canary="))
        .expect("Canary path required");
    // Unix prepare is not suspended. Keep the helper alive until the parent
    // has captured its identity and run the resumed callback. A quick exit
    // before PID capture is legitimately unconfirmed by the product adapter.
    let release = args
        .iter()
        .find_map(|a| a.strip_prefix("probe-release="))
        .expect("Release path required");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !std::path::Path::new(release).exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "Parent did not confirm resume"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    std::fs::write(canary, b"actual child ran").unwrap();
    if args.iter().any(|a| a == "probe-long=true") {
        std::thread::sleep(Duration::from_secs(20));
    }
}
#[derive(Default)]
struct Observer {
    fail: Option<&'static str>,
    resume_release: Option<std::path::PathBuf>,
    events: Vec<&'static str>,
    cleanup: Option<ProbeCleanup>,
}
impl ProbeObserver for Observer {
    fn before_create(&mut self) -> Result<()> {
        self.events.push("intent");
        if self.fail == Some("create") {
            Err("intent rejected".into())
        } else {
            Ok(())
        }
    }
    fn before_resume(
        &mut self,
        process: &winbridge_protocol::update::SupervisorProcess,
        job: &str,
    ) -> Result<()> {
        self.events.push("pid");
        assert!(process.process_id > 0);
        assert!(!job.is_empty());
        assert!(winbridge_platform::infrastructure::matches(process).unwrap());
        if self.fail == Some("pid") {
            Err("PID receipt rejected".into())
        } else {
            Ok(())
        }
    }
    fn resumed(&mut self) -> Result<()> {
        self.events.push("resumed");
        if let Some(path) = &self.resume_release {
            std::fs::write(path, b"parent captured PID and resumed").map_err(error)?;
        }
        Ok(())
    }
    fn poll(&mut self) -> Result<()> {
        if self.fail == Some("poll") {
            Err("owner phase changed".into())
        } else {
            Ok(())
        }
    }
    fn finished(&mut self, receipt: &ProbeCleanup) -> Result<()> {
        self.events.push("cleanup");
        self.cleanup = Some(receipt.clone());
        Ok(())
    }
}
#[test]
fn rejected_intent_does_not_create_or_resume_a_probe() {
    let f = Fixture::new();
    let p = pin(&f.image).unwrap();
    let mut o = Observer {
        fail: Some("create"),
        ..f.observer()
    };
    assert!(
        run_probe_observed(&p, &f.args(false), 5000, &f.job(), &mut o)
            .unwrap_err()
            .contains("intent rejected")
    );
    assert_eq!(o.events, ["intent", "cleanup"]);
    let r = o.cleanup.unwrap();
    assert!(r.no_process_created && r.tree_empty && r.capture_complete);
    assert!(!r.creation_confirmed && !r.resume_requested);
    assert!(r.process.is_none());
    assert!(!f.canary.exists());
}
#[cfg(windows)]
#[test]
fn rejected_pid_receipt_never_resumes_windows_and_still_reports_cleanup() {
    let f = Fixture::new();
    let p = pin(&f.image).unwrap();
    let mut o = Observer {
        fail: Some("pid"),
        ..f.observer()
    };
    assert!(
        run_probe_observed(&p, &f.args(false), 5000, &f.job(), &mut o)
            .unwrap_err()
            .contains("PID receipt rejected")
    );
    assert_eq!(o.events, ["intent", "pid", "cleanup"]);
    let r = o.cleanup.unwrap();
    assert!(r.creation_confirmed && r.tree_empty && r.capture_complete);
    assert!(!r.no_process_created && !r.resume_requested);
    assert!(r.process.is_some());
    assert!(!f.canary.exists());
}
#[test]
fn changed_owner_poll_terminates_only_owned_probe_and_reports_no_success() {
    let f = Fixture::new();
    let p = pin(&f.image).unwrap();
    let mut o = Observer {
        fail: Some("poll"),
        ..f.observer()
    };
    assert!(
        run_probe_observed(&p, &f.args(true), 5000, &f.job(), &mut o)
            .unwrap_err()
            .contains("owner phase changed")
    );
    assert_eq!(o.events, ["intent", "pid", "resumed", "cleanup"]);
    let r = o.cleanup.unwrap();
    assert!(r.creation_confirmed && r.resume_requested && r.tree_empty && r.capture_complete);
    assert!(r.failure.is_some());
}
#[test]
fn successful_probe_reports_exact_process_and_terminal_cleanup_after_hooks() {
    let f = Fixture::new();
    let p = pin(&f.image).unwrap();
    let mut o = f.observer();
    let (_, _, process) = run_probe_observed(&p, &f.args(false), 5000, &f.job(), &mut o).unwrap();
    assert_eq!(o.events, ["intent", "pid", "resumed", "cleanup"]);
    let r = o.cleanup.unwrap();
    assert_eq!(r.process, Some(process));
    assert_eq!(r.exit_code, Some(0));
    assert!(r.creation_confirmed && r.resume_requested && r.tree_empty && r.capture_complete);
    assert!(!r.output_exceeded);
    assert!(r.failure.is_none());
    assert_eq!(std::fs::read(&f.canary).unwrap(), b"actual child ran");
}
