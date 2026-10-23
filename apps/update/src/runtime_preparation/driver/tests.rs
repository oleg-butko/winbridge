//! Real local filesystem, synthetic observer only; never an installation permit.
use super::*;
struct TestObserver {
    fault: u8,
    injected: bool,
}
impl Observer for TestObserver {
    fn checkpoint(&mut self, r: &RuntimePreparationRecord) -> Result<()> {
        r.validate()
    }
    fn pulse(&mut self, r: &RuntimePreparationRecord) -> Result<()> {
        if self.fault == 1
            && !self.injected
            && matches!(r.files[1], PreparationFileState::Issued {})
        {
            fs::write(&r.plan.files[1].destination, b"occupied sentinel").map_err(error)?;
            self.injected = true;
        }
        if self.fault == 4
            && matches!(r.files[0], PreparationFileState::Issued {})
            && fs::metadata(&r.plan.files[0].destination).is_ok_and(|m| m.len() >= 65536)
        {
            return Err("Owner fence lost during stream".into());
        }
        Ok(())
    }
    fn save(
        &mut self,
        old: &RuntimePreparationRecord,
        next: RuntimePreparationRecord,
    ) -> Result<RuntimePreparationRecord> {
        old.validate_step(&next)?;
        if self.fault == 2 && next.revision == 2 {
            return Err("Directory receipt commit failed".into());
        }
        if self.fault == 3 && next.revision == 4 {
            return Err("Copy receipt commit failed".into());
        }
        // Assert the journal intent precedes the observable filesystem effect.
        if next.phase == PreparationPhase::DirectoryIssued {
            assert!(!next.plan.directory().exists());
        }
        for (index, (a, b)) in old.files.iter().zip(&next.files).enumerate() {
            if a != b && matches!(b, PreparationFileState::Issued {}) {
                assert!(!Path::new(&next.plan.files[index].destination).exists());
            }
        }
        Ok(next)
    }
}
fn fixture() -> (tempfile::TempDir, RuntimePreparationRecord) {
    #[cfg(unix)]
    let temp = {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/runtime-preparation-test-temp");
        fs::create_dir_all(&base).unwrap();
        tempfile::tempdir_in(base).unwrap()
    };
    #[cfg(not(unix))]
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap().join("runtime");
    let mut p = super::super::tests::fixture().plan;
    p.runtime_directory = root.to_str().unwrap().into();
    let parent = root.join("update").join(&p.update_id);
    let payload = parent.join("candidate").join("payload");
    fs::create_dir_all(&payload).unwrap();
    p.runtime_identity = directory_identity(&root).unwrap();
    p.parent_identity = directory_identity(&parent).unwrap();
    for index in 0..5 {
        let role = p.files[index].file;
        let old_path = root.join(role.name());
        let mut old = format!("old runtime fixture {}", role.name()).into_bytes();
        if role != RuntimeFile::Manifest {
            old.extend_from_slice(winbridge_protocol::ARTIFACT_QUERY_MARKER);
        }
        fs::write(&old_path, old).unwrap();
        p.files[index].old = artifact::inspect_image(&old_path).unwrap();
        p.files[index].destination = parent
            .join("prepared-runtime")
            .join(role.name())
            .to_str()
            .unwrap()
            .into();
        if role != RuntimeFile::Manifest {
            let mut raw = vec![b'a'; 150000];
            raw.extend_from_slice(winbridge_protocol::ARTIFACT_QUERY_MARKER);
            raw.extend_from_slice(role.name().as_bytes());
            let path = payload.join(role.name());
            fs::write(&path, raw).unwrap();
            let image = artifact::inspect_image(&path).unwrap();
            p.files[index].bytes = image.bytes;
            p.files[index].sha256 = image.sha256.clone();
            p.files[index].source = Some(image.clone());
            p.manifest.artifacts[index].bytes = image.bytes;
            p.manifest.artifacts[index].sha256 = image.sha256;
        }
    }
    let raw = p.manifest_bytes().unwrap();
    p.files[4].bytes = raw.len() as u64;
    p.files[4].sha256 = crate::sha256(&raw);
    p.available_bytes_before = fs2::available_space(&root).unwrap();
    let r = RuntimePreparationRecord::planned(p).unwrap();
    (temp, r)
}
fn source_bytes(r: &RuntimePreparationRecord) -> Vec<Vec<u8>> {
    r.plan
        .files
        .iter()
        .flat_map(|f| {
            std::iter::once(f.old.path.as_str()).chain(f.source.as_ref().map(|s| s.path.as_str()))
        })
        .map(|p| fs::read(p).unwrap())
        .collect()
}
#[test]
fn preparation_driver_creates_five_independent_files_with_intents_and_preserves_sources() {
    let (_root, mut r) = fixture();
    let before = source_bytes(&r);
    let mut observer = TestObserver {
        fault: 0,
        injected: false,
    };
    copy_set(&mut observer, &mut r).unwrap();
    assert_eq!(r.phase, PreparationPhase::Prepared);
    assert_eq!(r.revision, 12);
    assert_eq!(source_bytes(&r), before);
    r.exchange_batch().unwrap().validate().unwrap();
    for state in &r.files {
        let PreparationFileState::Verified { image } = state else {
            panic!("Not prepared")
        };
        assert_eq!(
            artifact::inspect_image(Path::new(&image.path)).unwrap(),
            *image
        );
    }
    assert_eq!(
        fs::read(r.plan.directory().join(RuntimeFile::Manifest.name())).unwrap(),
        r.plan.manifest_bytes().unwrap()
    );
    assert!(copy_set(&mut observer, &mut r).is_err());
}
#[test]
fn preparation_collision_keeps_existing_file_prefix_and_does_not_replay() {
    let (_root, mut r) = fixture();
    let before = source_bytes(&r);
    let mut observer = TestObserver {
        fault: 1,
        injected: false,
    };
    assert!(copy_set(&mut observer, &mut r).is_err());
    assert!(matches!(r.files[0], PreparationFileState::Verified { .. }));
    assert!(matches!(r.files[1], PreparationFileState::Issued {}));
    assert_eq!(
        fs::read(&r.plan.files[1].destination).unwrap(),
        b"occupied sentinel"
    );
    assert_eq!(source_bytes(&r), before);
    assert!(copy_set(&mut observer, &mut r).is_err());
    assert_eq!(
        fs::read(&r.plan.files[1].destination).unwrap(),
        b"occupied sentinel"
    );
}
#[test]
fn preparation_failed_receipt_preserves_created_directory_or_file_without_adoption() {
    for fault in [2, 3] {
        let (_root, mut r) = fixture();
        let before = source_bytes(&r);
        let mut observer = TestObserver {
            fault,
            injected: false,
        };
        assert!(copy_set(&mut observer, &mut r).is_err());
        assert!(r.plan.directory().is_dir());
        if fault == 2 {
            assert_eq!(r.phase, PreparationPhase::DirectoryIssued);
            assert_eq!(fs::read_dir(r.plan.directory()).unwrap().count(), 0);
        } else {
            assert!(matches!(r.files[0], PreparationFileState::Issued {}));
            assert!(Path::new(&r.plan.files[0].destination).is_file());
        }
        assert!(copy_set(&mut observer, &mut r).is_err());
        assert_eq!(source_bytes(&r), before);
    }
}
#[test]
fn preparation_stream_fence_keeps_partial_copy_and_issued_intent() {
    let (_root, mut r) = fixture();
    let before = source_bytes(&r);
    let mut observer = TestObserver {
        fault: 4,
        injected: false,
    };
    assert!(copy_set(&mut observer, &mut r)
        .unwrap_err()
        .contains("Owner fence"));
    assert!(matches!(r.files[0], PreparationFileState::Issued {}));
    let bytes = fs::metadata(&r.plan.files[0].destination).unwrap().len();
    assert_eq!(bytes, 65536);
    assert!(bytes < r.plan.files[0].bytes);
    assert_eq!(source_bytes(&r), before);
    assert!(copy_set(&mut observer, &mut r).is_err());
}
#[test]
fn preparation_existing_directory_is_preserved_before_any_directory_effect() {
    let (_root, mut r) = fixture();
    fs::create_dir(r.plan.directory()).unwrap();
    let sentinel = r.plan.directory().join("sentinel");
    fs::write(&sentinel, b"keep").unwrap();
    let mut observer = TestObserver {
        fault: 0,
        injected: false,
    };
    assert!(copy_set(&mut observer, &mut r).is_err());
    assert_eq!(r.phase, PreparationPhase::Planned);
    assert_eq!(fs::read(sentinel).unwrap(), b"keep");
}
