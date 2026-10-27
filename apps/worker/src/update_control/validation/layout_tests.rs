use super::*;
use serde_json::json;

struct Fixture {
    // Release the exclusive Windows lease before TempDir tries to remove it.
    _lease: File,
    root: PathBuf,
    runtime: PathBuf,
    build: BuildIdentity,
    raw: serde_json::Value,
    _temp: tempfile::TempDir,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("run/None");
        let runtime = temp.path().join("stable");
        let production = temp.path().join("production");
        for path in [&root, &runtime, &production] {
            std::fs::create_dir_all(path).unwrap();
        }
        let mut opts = OpenOptions::new();
        opts.write(true).read(true).create_new(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            opts.share_mode(0);
        }
        let lease = opts
            .open(runtime.join(".winbridge-test-runtime.lock"))
            .unwrap();
        fs2::FileExt::lock_exclusive(&lease).unwrap();
        let mut build = winbridge_protocol::build_identity();
        build.source_dirty = false;
        build.source_commit = Some("a".repeat(40));
        build.source_manifest_sha256 = Some("b".repeat(64));
        let raw = json!({"schemaVersion":1,"kind":"winbridge_activation_test_reservation",
            "reservationId":uuid::Uuid::new_v4().to_string(),"runtimeDirectory":runtime,
            "runDirectory":root.parent().unwrap(),"installedWorker":production.join("winbridge-worker.exe"),
            "sourceCommit":build.source_commit,"sourceManifestSha256":build.source_manifest_sha256,
            "initialWorkerSha256":"c".repeat(64),"createdAtUtc":"2026-10-26T00:00:00Z"});
        Self {
            _temp: temp,
            root,
            runtime,
            build,
            raw,
            _lease: lease,
        }
    }
    fn write(&self, raw: &serde_json::Value) -> FixtureRuntimePin {
        let bytes = serde_json::to_vec(raw).unwrap();
        std::fs::write(self.runtime.join(MARKER), &bytes).unwrap();
        FixtureRuntimePin {
            directory: self.runtime.clone(),
            marker_sha256: winbridge_update::sha256(&bytes),
        }
    }
}
#[test]
fn legacy_fixture_retains_the_original_layout_without_a_marker_or_lease() {
    println!(
        "FIXTURE_LAYOUT_BUILD {}",
        serde_json::to_string(&winbridge_protocol::build_identity()).unwrap()
    );
    let f = Fixture::new();
    std::fs::create_dir(f.root.join("runtime")).unwrap();
    let layout = FixtureLayout::bind(&f.root, None, &f.build).unwrap();
    assert_eq!(&*layout, f.root.canonicalize().unwrap());
    assert_eq!(
        layout.runtime(),
        f.root.join("runtime").canonicalize().unwrap()
    );
    assert!(layout.initial_worker_sha256().is_none());
}
#[test]
fn external_layout_requires_matching_pins_and_a_real_busy_lease() {
    let f = Fixture::new();
    let pin = f.write(&f.raw);
    let layout = FixtureLayout::bind(&f.root, Some(&pin), &f.build).unwrap();
    assert_eq!(layout.runtime(), f.runtime.canonicalize().unwrap());
    assert_eq!(layout.join("state"), f.root.join("state"));
    assert_eq!(
        layout.initial_worker_sha256(),
        Some("c".repeat(64).as_str())
    );
    drop(layout);
    // The same saved marker without the original script lease is not enough.
    fs2::FileExt::unlock(&f._lease).unwrap();
    drop(f._lease);
    assert!(FixtureLayout::bind(&f.root, Some(&pin), &f.build).is_err());
}
#[test]
fn declaration_cannot_redirect_source_case_or_production() {
    let f = Fixture::new();
    for n in 0..9 {
        let mut raw = f.raw.clone();
        match n {
            0 => raw["schemaVersion"] = json!(2),
            1 => raw["reservationId"] = json!("invalid"),
            2 => raw["sourceCommit"] = json!("d".repeat(40)),
            3 => raw["sourceManifestSha256"] = json!("e".repeat(64)),
            4 => raw["runtimeDirectory"] = json!(f.root),
            5 => raw["runDirectory"] = json!(f._temp.path()),
            6 => raw["installedWorker"] = json!(f.runtime.join("winbridge-worker.exe")),
            7 => raw["initialWorkerSha256"] = json!("invalid"),
            _ => raw["unknownGrant"] = json!(true),
        }
        let pin = f.write(&raw);
        assert!(
            FixtureLayout::bind(&f.root, Some(&pin), &f.build).is_err(),
            "mutation {n}"
        );
    }
    let mut pin = f.write(&f.raw);
    pin.marker_sha256 = "f".repeat(64);
    assert!(FixtureLayout::bind(&f.root, Some(&pin), &f.build).is_err());
    let pin = f.write(&f.raw);
    let mut build = f.build.clone();
    build.source_dirty = true;
    assert!(FixtureLayout::bind(&f.root, Some(&pin), &build).is_err());
    assert!(FixtureLayout::bind(
        &f.root.parent().unwrap().join("other"),
        Some(&pin),
        &f.build
    )
    .is_err());
}
#[test]
fn missing_oversized_or_traversing_declarations_are_not_a_layout() {
    let f = Fixture::new();
    let pin = FixtureRuntimePin {
        directory: f.runtime.clone(),
        marker_sha256: "a".repeat(64),
    };
    assert!(FixtureLayout::bind(&f.root, Some(&pin), &f.build).is_err());
    let bytes = vec![b' '; 65537];
    std::fs::write(f.runtime.join(MARKER), &bytes).unwrap();
    let mut pin = FixtureRuntimePin {
        marker_sha256: winbridge_update::sha256(&bytes),
        ..pin
    };
    assert!(FixtureLayout::bind(&f.root, Some(&pin), &f.build).is_err());
    pin.directory = f.runtime.join("../stable");
    assert!(FixtureLayout::bind(&f.root, Some(&pin), &f.build).is_err());
}

#[test]
fn main_fixture_contract_keeps_v1_local_and_requires_explicit_v2_layout() {
    use super::super::Input;
    let raw = json!({"schemaVersion":1,"kind":"winbridge_shutdown_main_validation","root":"fixture",
        "workerConfigSha256":"a".repeat(64),"fault":"none",
        "request":{"contractVersion":1,"idempotencyKey":"fixture","source":{
            "archiveName":"winbridge-fixture.zip","archiveSha256":"b".repeat(64),
            "manifestSha256":"c".repeat(64),"sourceCommit":"d".repeat(40)},
            "expected":{"installationId":"fixture","scopeSha256":"e".repeat(64),
                "workerBuildId":"fixture","workerConfigSha256":"a".repeat(64),
                "baselineManifestSha256":"f".repeat(64)}}});
    let mut input: Input = serde_json::from_value(raw).unwrap();
    input.validate_layout_contract().unwrap();
    let value = serde_json::to_value(&input).unwrap();
    assert!(value.get("runtime").is_none() && value.get("controllerUrl").is_none());
    input.runtime = Some(FixtureRuntimePin {
        directory: PathBuf::from("stable"),
        marker_sha256: "a".repeat(64),
    });
    assert!(input.validate_layout_contract().is_err());
    input.schema_version = 2;
    input.validate_layout_contract().unwrap();
    for url in ["ws://192.168.44.203:1144", "ws://127.0.0.1:1144/"] {
        input.controller_url = Some(url.into());
        input.validate_layout_contract().unwrap();
    }
    for url in [
        "ws://192.168.44.203:1143",
        "ws://8.8.8.8:1144",
        "wss://127.0.0.1:1144",
        "ws://127.0.0.1:1144/extra",
    ] {
        input.controller_url = Some(url.into());
        assert!(input.validate_layout_contract().is_err());
    }
    input.controller_url = None;
    input.runtime = None;
    assert!(input.validate_layout_contract().is_err());
    for schema in [0, 3, 25] {
        input.schema_version = schema;
        assert!(input.validate_layout_contract().is_err());
    }
}
