use super::*;

fn fixture(file: RuntimeFile) -> FileExchangeRecord {
    let root = std::env::temp_dir().join("synthetic-exchange-runtime");
    let id = uuid::Uuid::new_v4().to_string();
    let image = |path: PathBuf, id: &str, sha: char| ImageEvidence {
        path: path.to_str().unwrap().into(),
        bytes: 1024,
        sha256: sha.to_string().repeat(64),
        file_identity: id.into(),
        query_supported: file != RuntimeFile::Manifest,
    };
    FileExchangeRecord::prepared(FileExchangeSpec {
        update_id: id.clone(),
        runtime_directory: root.to_str().unwrap().into(),
        file,
        old: image(root.join(file.name()), "old-independent-file", 'a'),
        prepared: image(
            root.join("update")
                .join(id)
                .join("prepared-runtime")
                .join(file.name()),
            "new-independent-file",
            'b',
        ),
    })
    .unwrap()
}
fn receipts(r: &FileExchangeRecord) -> (ImageEvidence, ImageEvidence) {
    let mut old = r.spec.old.clone();
    old.path = r.spec.retained_path().to_str().unwrap().into();
    let mut new = r.spec.prepared.clone();
    new.path = r.spec.installed_path().to_str().unwrap().into();
    (old, new)
}
#[test]
fn complete_exchange_requires_both_intents_and_keeps_exact_rename_identities() {
    for file in [
        RuntimeFile::Worker,
        RuntimeFile::Fixture,
        RuntimeFile::Desktop,
        RuntimeFile::Updater,
        RuntimeFile::Manifest,
    ] {
        let original = fixture(file);
        let (old, new) = receipts(&original);
        assert!(original.retired(old.clone()).is_err());
        assert!(original.issue_place().is_err());
        assert!(original.installed(new.clone()).is_err());
        let issued = original.issue_retire().unwrap();
        assert!(issued.issue_retire().is_err());
        let retired = issued.retired(old.clone()).unwrap();
        assert!(retired.retired(old).is_err());
        assert!(retired.installed(new.clone()).is_err());
        let placing = retired.issue_place().unwrap();
        assert!(placing.issue_place().is_err());
        let installed = placing.installed(new.clone()).unwrap();
        assert_eq!(installed.revision, 4);
        assert!(installed.installed(new).is_err());
        assert!(installed.indeterminate("late error".into()).is_err());
        installed.validate().unwrap();
    }
}
#[test]
fn copied_rebound_or_changed_file_cannot_substitute_for_same_file_move() {
    let r = fixture(RuntimeFile::Worker);
    let (old, new) = receipts(&r);
    for kind in 0..5 {
        let corrupt = |e: &ImageEvidence| {
            let mut e = e.clone();
            match kind {
                0 => e.file_identity = "copied-file-same-content".into(),
                1 => e.sha256 = "c".repeat(64),
                2 => e.bytes += 1,
                3 => e.path.push_str(".other"),
                _ => e.query_supported = false,
            }
            e
        };
        assert!(r.issue_retire().unwrap().retired(corrupt(&old)).is_err());
        assert!(r
            .issue_retire()
            .unwrap()
            .retired(old.clone())
            .unwrap()
            .issue_place()
            .unwrap()
            .installed(corrupt(&new))
            .is_err());
    }
}
#[test]
fn uncertain_outcome_retains_exact_prefix_and_never_reauthorizes_replay_after_json_reopen() {
    let r = fixture(RuntimeFile::Desktop);
    let (old, _) = receipts(&r);
    let prefixes = [
        r.clone(),
        r.issue_retire().unwrap(),
        r.issue_retire().unwrap().retired(old).unwrap(),
    ];
    let mut all = prefixes.to_vec();
    all.push(prefixes[2].issue_place().unwrap());
    for previous in all {
        let stopped = previous
            .indeterminate("Missing durable move receipt; inspect only".into())
            .unwrap();
        let decoded: FileExchangeRecord =
            serde_json::from_slice(&serde_json::to_vec(&stopped).unwrap()).unwrap();
        decoded.validate().unwrap();
        assert_eq!(decoded, stopped);
        assert_eq!(decoded.revision, previous.revision + 1);
        assert!(decoded.issue_retire().is_err());
        assert!(decoded.issue_place().is_err());
        assert!(decoded.indeterminate("retry".into()).is_err());
        let FileExchangeState::Indeterminate {
            previous: saved, ..
        } = decoded.state
        else {
            panic!()
        };
        assert_eq!(*saved, previous.state);
    }
}
#[test]
fn fixed_scope_independent_files_and_frozen_wire_shape_reject_forgery() {
    let r = fixture(RuntimeFile::Manifest);
    for kind in 0..7 {
        let mut changed = r.clone();
        match kind {
            0 => changed.spec.prepared.file_identity = changed.spec.old.file_identity.clone(),
            1 => changed.spec.old.path = changed.spec.prepared.path.clone(),
            2 => changed.spec.update_id = "../outside".into(),
            3 => changed.spec.prepared.query_supported = true,
            4 => changed.spec.runtime_directory = "relative".into(),
            5 => changed.revision = 4,
            _ => changed.schema_version = 2,
        }
        assert!(changed.validate().is_err());
    }
    let mut value = serde_json::to_value(&r).unwrap();
    value["state"]["canReplace"] = serde_json::json!(true);
    assert!(serde_json::from_value::<FileExchangeRecord>(value).is_err());
}

fn batch() -> FileExchangeBatch {
    let seed = fixture(RuntimeFile::Worker);
    let files = [
        RuntimeFile::Worker,
        RuntimeFile::Fixture,
        RuntimeFile::Desktop,
        RuntimeFile::Updater,
        RuntimeFile::Manifest,
    ]
    .into_iter()
    .map(|file| {
        let mut spec = seed.spec.clone();
        spec.file = file;
        spec.old.path = spec.installed_path().to_str().unwrap().into();
        spec.prepared.path = spec
            .run()
            .join("prepared-runtime")
            .join(file.name())
            .to_str()
            .unwrap()
            .into();
        spec.old.query_supported = file != RuntimeFile::Manifest;
        spec.prepared.query_supported = file != RuntimeFile::Manifest;
        spec.old.file_identity = format!("old-{file:?}");
        spec.prepared.file_identity = format!("new-{file:?}");
        FileExchangeRecord::prepared(spec).unwrap()
    })
    .collect();
    FileExchangeBatch {
        schema_version: 1,
        files,
    }
}
#[test]
fn manifest_is_last_and_batch_will_not_skip_causal_file_transitions() {
    let mut b = batch();
    assert!(b.advance(4, b.files[4].issue_retire().unwrap()).is_err());
    for i in 0..5 {
        assert_eq!(b.next_file().unwrap(), Some(i));
        let r = b.files[i].clone();
        let (old, new) = receipts(&r);
        let issued = r.issue_retire().unwrap();
        let retired = issued.retired(old).unwrap();
        let placing = retired.issue_place().unwrap();
        let installed = placing.installed(new).unwrap();
        assert!(b.advance(i, installed.clone()).is_err());
        for next in [issued, retired, placing, installed] {
            b = b.advance(i, next).unwrap();
        }
    }
    assert_eq!(b.next_file().unwrap(), None);
    assert!(b.advance(0, b.files[0].clone()).is_err());
}
#[test]
fn mixed_scope_alias_and_progress_after_uncertainty_are_rejected() {
    let b = batch();
    for kind in 0..4 {
        let mut corrupt = b.clone();
        match kind {
            0 => corrupt.files.swap(0, 4),
            1 => {
                corrupt.files[1].spec.prepared.file_identity =
                    corrupt.files[0].spec.old.file_identity.clone()
            }
            2 => corrupt.files[2] = corrupt.files[2].issue_retire().unwrap(),
            _ => {
                let replacement = batch();
                corrupt.files[1] = replacement.files[1].clone();
            }
        }
        assert!(corrupt.validate().is_err());
    }
    let stopped = b
        .advance(
            0,
            b.files[0]
                .indeterminate("Move outcome unknown".into())
                .unwrap(),
        )
        .unwrap();
    let reopened: FileExchangeBatch =
        serde_json::from_slice(&serde_json::to_vec(&stopped).unwrap()).unwrap();
    assert!(reopened
        .advance(1, reopened.files[1].issue_retire().unwrap())
        .is_err());
    assert!(reopened.files[0].issue_retire().is_err());
}
