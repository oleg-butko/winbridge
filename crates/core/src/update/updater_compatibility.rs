//! Pure production policy. A compatible report still grants no physical authority.
use super::valid_hash;
use winbridge_protocol::{update::*, ArtifactRole};

/// Full same-schema install policy. Required unimplemented drivers stay blockers:
/// a candidate claiming a future contract cannot enable code absent locally.
pub fn assess_updater_compatibility(
    support: &UpdaterRuntimeSupport,
    expected_updater_journal_schema: u32,
) -> UpdaterCompatibilityAssessment {
    use UpdaterCompatibilityBlocker as B;
    use UpdaterExecutionContract as C;
    let mut blockers = vec![];
    if support.schema_version != 1
        || support.kind != "winbridge_updater_runtime_support"
        || support.query_contract != 1
    {
        blockers.push(B::UnsupportedQuery {});
    }
    let descriptor = &support.descriptor;
    if descriptor.schema_version != 1
        || descriptor.query_contract != 1
        || descriptor.kind != "winbridge_artifact"
        || descriptor.role != ArtifactRole::Updater
        || descriptor.target_os != "windows"
        || descriptor.target_arch != "x86_64"
    {
        blockers.push(B::InvalidDescriptor {});
    }
    if descriptor.build.source_dirty
        || !descriptor
            .build
            .source_commit
            .as_ref()
            .is_some_and(|s| valid_hash(s, &[40, 64]))
        || !descriptor
            .build
            .source_manifest_sha256
            .as_ref()
            .is_some_and(|s| valid_hash(s, &[64]))
    {
        blockers.push(B::UncommittedBuild {});
    }
    if support.build_flavor != UpdaterBuildFlavor::Standard {
        blockers.push(B::ValidationBuild {});
    }
    if expected_updater_journal_schema == 0
        || support.updater_journal_schema != expected_updater_journal_schema
    {
        blockers.push(B::JournalSchemaMismatch {
            expected: expected_updater_journal_schema,
            actual: support.updater_journal_schema,
        });
    }
    let requirements = [
        (
            C::FinalStateBackup,
            Some(15),
            support.final_state_backup_contract,
        ),
        (
            C::RuntimePreparation,
            Some(16),
            support.runtime_preparation_contract,
        ),
        (
            C::RuntimeReplacement,
            Some(17),
            support.runtime_replacement_contract,
        ),
        (C::Activation, None, support.activation_contract),
        (
            C::ControllerConfirmation,
            None,
            support.controller_confirmation_contract,
        ),
        (C::DesktopUpdate, None, support.desktop_update_contract),
    ];
    for (contract, implemented, candidate) in requirements {
        if candidate.is_none() {
            blockers.push(B::MissingContract { contract });
        }
        match (implemented, candidate) {
            (None, _) => blockers.push(B::DriverNotImplemented { contract }),
            (Some(expected), Some(actual)) if actual != expected => {
                blockers.push(B::ContractVersionMismatch {
                    contract,
                    expected,
                    actual,
                });
            }
            _ => {}
        }
    }
    UpdaterCompatibilityAssessment {
        schema_version: 1,
        policy: UpdaterCompatibilityPolicy::SameSchemaInstallV1,
        expected_updater_journal_schema,
        blockers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> UpdaterRuntimeSupport {
        let mut descriptor = winbridge_protocol::artifact_descriptor(ArtifactRole::Updater);
        descriptor.target_os = "windows".into();
        descriptor.target_arch = "x86_64".into();
        descriptor.build.source_dirty = false;
        descriptor.build.source_commit = Some("a".repeat(40));
        descriptor.build.source_manifest_sha256 = Some("b".repeat(64));
        UpdaterRuntimeSupport {
            schema_version: 1,
            kind: "winbridge_updater_runtime_support".into(),
            query_contract: 1,
            descriptor,
            process: SupervisorProcess {
                process_id: 1,
                birth: "1".into(),
                host_id: "c".repeat(64),
                session_id: 1,
            },
            updater_journal_schema: 15,
            build_flavor: UpdaterBuildFlavor::Standard,
            final_state_backup_contract: Some(15),
            runtime_preparation_contract: Some(16),
            runtime_replacement_contract: Some(17),
            activation_contract: None,
            controller_confirmation_contract: None,
            desktop_update_contract: None,
        }
    }
    #[test]
    fn matching_schema_does_not_hide_missing_candidate_and_local_drivers() {
        let assessment = assess_updater_compatibility(&fixture(), 15);
        assert_eq!(assessment.blockers.len(), 6);
        for contract in [
            UpdaterExecutionContract::Activation,
            UpdaterExecutionContract::ControllerConfirmation,
            UpdaterExecutionContract::DesktopUpdate,
        ] {
            assert!(assessment
                .blockers
                .contains(&UpdaterCompatibilityBlocker::MissingContract { contract }));
            assert!(assessment
                .blockers
                .contains(&UpdaterCompatibilityBlocker::DriverNotImplemented { contract }));
        }
    }
    #[test]
    fn claimed_future_contracts_never_enable_unimplemented_drivers() {
        let mut support = fixture();
        support.activation_contract = Some(18);
        support.controller_confirmation_contract = Some(19);
        support.desktop_update_contract = Some(20);
        let assessment = assess_updater_compatibility(&support, 15);
        assert_eq!(assessment.blockers.len(), 3);
        assert!(assessment
            .blockers
            .iter()
            .all(|b| matches!(b, UpdaterCompatibilityBlocker::DriverNotImplemented { .. })));
    }
    #[test]
    fn validation_dirty_wrong_target_and_schema_never_qualify_as_production() {
        let mut support = fixture();
        support.build_flavor = UpdaterBuildFlavor::Validation;
        support.descriptor.build.source_dirty = true;
        support.descriptor.target_os = "linux".into();
        let assessment = assess_updater_compatibility(&support, 16);
        for expected in [
            UpdaterCompatibilityBlocker::ValidationBuild {},
            UpdaterCompatibilityBlocker::UncommittedBuild {},
            UpdaterCompatibilityBlocker::InvalidDescriptor {},
            UpdaterCompatibilityBlocker::JournalSchemaMismatch {
                expected: 16,
                actual: 15,
            },
        ] {
            assert!(assessment.blockers.contains(&expected));
        }
    }
    #[test]
    fn unknown_query_and_contract_versions_have_explicit_blockers() {
        let mut support = fixture();
        support.query_contract = 2;
        support.runtime_replacement_contract = Some(18);
        let assessment = assess_updater_compatibility(&support, 15);
        assert!(assessment
            .blockers
            .contains(&UpdaterCompatibilityBlocker::UnsupportedQuery {}));
        assert!(assessment.blockers.contains(
            &UpdaterCompatibilityBlocker::ContractVersionMismatch {
                contract: UpdaterExecutionContract::RuntimeReplacement,
                expected: 17,
                actual: 18
            }
        ));
    }

    #[test]
    fn committed_source_identity_matches_existing_sha1_and_sha256_manifest_policy() {
        for length in [40, 64] {
            let mut support = fixture();
            support.descriptor.build.source_commit = Some("a".repeat(length));
            assert!(!assess_updater_compatibility(&support, 15)
                .blockers
                .contains(&UpdaterCompatibilityBlocker::UncommittedBuild {}));
        }
        let mut support = fixture();
        support.descriptor.build.source_commit = None;
        assert!(assess_updater_compatibility(&support, 15)
            .blockers
            .contains(&UpdaterCompatibilityBlocker::UncommittedBuild {}));
    }
}
