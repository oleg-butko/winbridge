//! Synthetic query receipts exercise readonly contracts; no fake image is executed.
use super::*;
use serde_json::{json, Value};
use std::fs;
struct Fixture {
    _root: tempfile::TempDir,
    source: PathBuf,
    payload: PathBuf,
    verified: crate::VerifiedSource,
    report: Value,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("src");
        let stage = root.path().join("candidate");
        let payload = stage.join("payload");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&stage).unwrap();
        fs::create_dir(&payload).unwrap();
        fs::write(source.join("Cargo.toml"), b"source").unwrap();
        let m = json!({"schemaVersion":1,"sourceCommit":"a".repeat(40),"dirty":false,"createdAt":"2026-10-18T00:00:00Z","includesBuiltUi":false,"files":[{"path":"Cargo.toml","bytes":6,"sha256":crate::sha256(b"source")}]});
        let manifest = serde_json::to_vec(&m).unwrap();
        fs::write(source.join("winbridge-source-manifest.json"), &manifest).unwrap();
        let verified = crate::verify_source(&source, &crate::sha256(&manifest)).unwrap();
        let build = BuildIdentity {
            version: "0.1.0".into(),
            source_commit: Some(verified.source_commit.clone()),
            source_dirty: false,
            source_manifest_sha256: Some(verified.manifest_sha256.clone()),
            build_id: "123".into(),
        };
        let mut summaries = vec![];
        let mut queries = vec![];
        for (name, role) in ARTIFACTS {
            let mut bytes = b"synthetic artifact metadata fixture; never execute".to_vec();
            bytes.extend_from_slice(winbridge_protocol::ARTIFACT_QUERY_MARKER);
            bytes.extend_from_slice(name.as_bytes());
            fs::write(payload.join(name), bytes).unwrap();
            let image = crate::artifact::inspect_image(&payload.join(name)).unwrap();
            summaries.push(json!({"name":name,"bytes":image.bytes,"sha256":image.sha256}));
            queries.push(json!({"name":name,"query":{"schemaVersion":1,"kind":"artifact_query","readOnly":true,"candidateVerified":false,"image":image,"descriptor":{"schemaVersion":1,"kind":"winbridge_artifact","queryContract":1,"role":role,"build":build,"targetOs":std::env::consts::OS,"targetArch":std::env::consts::ARCH},"process":{"processId":123,"birth":"456","hostId":"b".repeat(64),"sessionId":1},"cleanupVerified":true}}));
        }
        let report = json!({"schemaVersion":1,"kind":"winbridge-update-candidate","status":"built","runtimeApplied":false,"candidateVerified":false,"sourceDirectory":crate::preparation::tool_path(&source.canonicalize().unwrap()).unwrap(),"manifestSha256":verified.manifest_sha256,"storage":{},"commands":[{"name":"rust","executable":"cargo.exe","arguments":["build"],"exitCode":0,"log":"rust.log"}],"artifacts":summaries,"error":null,"source":verified,"workerBuild":build,"artifactIdentities":queries,"artifactIdentityChecked":true});
        Self {
            _root: root,
            source,
            payload,
            verified,
            report,
        }
    }
    fn check(&self) -> Result<Vec<CandidateArtifact>> {
        Ok(check_report(
            &self.verified,
            &self.source.canonicalize().unwrap(),
            &self.payload,
            &serde_json::to_vec(&self.report).unwrap(),
        )?
        .0)
    }
}
#[test]
fn exact_four_files_source_roles_and_query_identities_are_required_without_execution() {
    let f = Fixture::new();
    let artifacts = f.check().unwrap();
    assert_eq!(artifacts.len(), 4);
    for (a, (name, role)) in artifacts.iter().zip(ARTIFACTS) {
        assert_eq!(a.name, name);
        assert_eq!(a.descriptor.role, role);
    }
    assert_eq!(fs::read_dir(&f.payload).unwrap().count(), 4);
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend_from_slice(&serde_json::to_vec(&f.report).unwrap());
    assert!(check_report(
        &f.verified,
        &f.source.canonicalize().unwrap(),
        &f.payload,
        &bytes
    )
    .is_ok());
}
#[test]
fn failed_dirty_wrong_target_or_unfinished_reports_cannot_become_artifact_sets() {
    for (path, value) in [
        ("/status", json!("failed")),
        ("/runtimeApplied", json!(true)),
        ("/candidateVerified", json!(true)),
        ("/error", json!("compile error")),
        ("/workerBuild/sourceDirty", json!(true)),
        ("/commands/0/exitCode", json!(101)),
        ("/artifactIdentityChecked", json!(false)),
        ("/source/manifestSha256", json!("c".repeat(64))),
        ("/artifactIdentities/0/query/cleanupVerified", json!(false)),
        ("/artifactIdentities/0/query/candidateVerified", json!(true)),
        (
            "/artifactIdentities/0/query/descriptor/targetArch",
            json!("foreign"),
        ),
        (
            "/artifactIdentities/0/query/descriptor/build/sourceCommit",
            json!("f".repeat(40)),
        ),
        (
            "/artifactIdentities/0/query/descriptor/role",
            json!("desktop"),
        ),
        ("/artifactIdentities/0/query/process/processId", json!(0)),
        (
            "/artifactIdentities/0/query/image/fileIdentity",
            json!("wrong"),
        ),
    ] {
        let mut f = Fixture::new();
        *f.report.pointer_mut(path).unwrap() = value;
        assert!(f.check().is_err(), "{path}");
    }
}
#[test]
fn missing_duplicate_extra_and_foreign_paths_are_rejected_without_following_report_paths() {
    for mode in [
        "duplicate",
        "missing",
        "traversal",
        "foreign-source",
        "foreign-image",
        "unknown-field",
        "extra-file",
    ] {
        let mut f = Fixture::new();
        match mode {
            "duplicate" => f.report["artifacts"][1] = f.report["artifacts"][0].clone(),
            "missing" => {
                f.report["artifactIdentities"].as_array_mut().unwrap().pop();
            }
            "traversal" => f.report["artifacts"][0]["name"] = json!("../winbridge-worker.exe"),
            "foreign-source" => {
                f.report["sourceDirectory"] = json!(f._root.path().join("not-read"))
            }
            "foreign-image" => {
                f.report["artifactIdentities"][0]["query"]["image"]["path"] =
                    json!(f._root.path().join("not-read.exe"))
            }
            "unknown-field" => f.report["futureInstallPermission"] = json!(true),
            "extra-file" => fs::write(f.payload.join("unknown.exe"), b"extra").unwrap(),
            _ => unreachable!(),
        }
        assert!(f.check().is_err(), "{mode}");
        assert!(!f._root.path().join("not-read").exists());
    }
}
#[test]
fn same_size_payload_edit_replacement_and_hard_link_alias_never_match_old_query_receipts() {
    for mode in ["edit", "replace", "hard-link", "missing"] {
        let f = Fixture::new();
        let path = f.payload.join("winbridge-worker.exe");
        let bytes = fs::read(&path).unwrap();
        match mode {
            "edit" => {
                let mut b = bytes.clone();
                b[0] = b'X';
                fs::write(&path, b).unwrap();
            }
            "replace" => {
                fs::rename(&path, f._root.path().join("old.exe")).unwrap();
                fs::write(&path, &bytes).unwrap();
            }
            "hard-link" => fs::hard_link(&path, f._root.path().join("alias.exe")).unwrap(),
            "missing" => fs::remove_file(&path).unwrap(),
            _ => unreachable!(),
        }
        assert!(f.check().is_err(), "{mode}");
    }
}
#[test]
fn standalone_inspection_verifies_exact_report_bytes_without_creating_authority() {
    let f = Fixture::new();
    let stage = f.payload.parent().unwrap();
    let bytes = serde_json::to_vec(&f.report).unwrap();
    fs::write(stage.join("candidate-report.json"), &bytes).unwrap();
    let hash = crate::sha256(&bytes);
    let inspection =
        inspect_candidate_files(&f.source, &f.verified.manifest_sha256, stage, &hash).unwrap();
    assert!(inspection.read_only);
    assert!(!inspection.journal_bound);
    assert!(!inspection.runtime_applied);
    assert!(!inspection.candidate_verified);
    assert_eq!(inspection.artifacts.len(), 4);
    assert_eq!(inspection.report.sha256, hash);
    assert!(!stage.join("update-state").exists());
    assert_eq!(
        fs::read(stage.join("candidate-report.json")).unwrap(),
        bytes
    );
    assert!(inspect_candidate_files(
        &f.source,
        &f.verified.manifest_sha256,
        stage,
        &"0".repeat(64)
    )
    .is_err());
    assert!(inspect_candidate_files(
        Path::new("relative"),
        &f.verified.manifest_sha256,
        stage,
        &hash
    )
    .is_err());
}
