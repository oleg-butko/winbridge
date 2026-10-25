//! Selective release preserves names/identities, never grants a journal writer.
use super::{LockedShutdownState, ShutdownStatePins};
use std::{fs::File, io};
use winbridge_protocol::update::WorkerShutdownState;

/// The original handles remain resident, but their instance lock was released.
/// There is no conversion back to locked custody or into a startup permission.
pub struct ReleasedShutdownState {
    pins: ShutdownStatePins,
}
impl std::fmt::Debug for ReleasedShutdownState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReleasedShutdownState")
            .field("observation", self.observation())
            .finish_non_exhaustive()
    }
}
impl ReleasedShutdownState {
    pub fn observation(&self) -> &WorkerShutdownState {
        self.pins.observation()
    }
    /// File identities only, not current lock ownership or database contents.
    pub fn revalidate(&self) -> io::Result<()> {
        self.pins.revalidate()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShutdownStateReleasePhase {
    BeforeRelease,
    ReleaseUnconfirmed,
    AfterRelease,
}
enum FailedCustody {
    Locked(LockedShutdownState),
    Unconfirmed(ShutdownStatePins),
    Released(ReleasedShutdownState),
}
/// Keep this owner after failure. Neither data nor an error recreates a release
/// attempt, acquires a successor lock, opens SQLite, or clears update barriers.
pub struct ShutdownStateReleaseFailure {
    reason: io::Error,
    custody: Box<FailedCustody>,
}
impl std::fmt::Debug for ShutdownStateReleaseFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShutdownStateReleaseFailure")
            .field("reason", &self.reason)
            .field("phase", &self.phase())
            .finish_non_exhaustive()
    }
}
impl std::fmt::Display for ShutdownStateReleaseFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Instance lock handoff stopped: {}", self.reason)
    }
}
impl std::error::Error for ShutdownStateReleaseFailure {}
impl ShutdownStateReleaseFailure {
    fn new(reason: io::Error, custody: FailedCustody) -> Self {
        Self {
            reason,
            custody: Box::new(custody),
        }
    }
    pub fn reason(&self) -> &io::Error {
        &self.reason
    }
    pub fn phase(&self) -> ShutdownStateReleasePhase {
        match self.custody.as_ref() {
            FailedCustody::Locked(_) => ShutdownStateReleasePhase::BeforeRelease,
            FailedCustody::Unconfirmed(_) => ShutdownStateReleasePhase::ReleaseUnconfirmed,
            FailedCustody::Released(_) => ShutdownStateReleasePhase::AfterRelease,
        }
    }
    /// Knowledge of this owner's release, never observation of a successor.
    /// An OS unlock error cannot prove that the lock remains held or is free.
    pub fn original_lock_held(&self) -> Option<bool> {
        match self.custody.as_ref() {
            FailedCustody::Locked(_) => Some(true),
            FailedCustody::Unconfirmed(_) => None,
            FailedCustody::Released(_) => Some(false),
        }
    }
    pub fn observation(&self) -> &WorkerShutdownState {
        match self.custody.as_ref() {
            FailedCustody::Locked(state) => state.observation(),
            FailedCustody::Unconfirmed(pins) => pins.observation(),
            FailedCustody::Released(state) => state.observation(),
        }
    }
}
impl LockedShutdownState {
    /// Trusted caller must first persist HandoffIssued, authenticate the exact
    /// successor, and check live authority/deadline in the checkpoint. This
    /// adapter only unlocks its own existing file, keeping all original pins.
    /// Failure is not rolled back by reacquiring the lock or replaying release.
    pub fn release_for_handoff_checked(
        self,
        checkpoint: impl FnMut() -> io::Result<()>,
    ) -> Result<ReleasedShutdownState, ShutdownStateReleaseFailure> {
        self.release_with(checkpoint, fs2::FileExt::unlock)
    }
    fn release_with(
        self,
        mut checkpoint: impl FnMut() -> io::Result<()>,
        unlock: impl FnOnce(&File) -> io::Result<()>,
    ) -> Result<ReleasedShutdownState, ShutdownStateReleaseFailure> {
        // The caller checks its original deadline after potentially slow I/O,
        // immediately before the single external release effect.
        if let Err(reason) = self.revalidate().and_then(|_| checkpoint()) {
            return Err(ShutdownStateReleaseFailure::new(
                reason,
                FailedCustody::Locked(self),
            ));
        }
        let pins = self.pins;
        if let Err(reason) = unlock(&pins.instance) {
            return Err(ShutdownStateReleaseFailure::new(
                reason,
                FailedCustody::Unconfirmed(pins),
            ));
        }
        let state = ReleasedShutdownState { pins };
        // Facts after release do not grant another effect. Preserve the same
        // handles on any error, including a name rebound during a checkpoint.
        if let Err(reason) = state
            .revalidate()
            .and_then(|_| checkpoint())
            .and_then(|_| state.revalidate())
        {
            return Err(ShutdownStateReleaseFailure::new(
                reason,
                FailedCustody::Released(state),
            ));
        }
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path};

    fn fixture(root: &Path) -> LockedShutdownState {
        fs::write(root.join("journal.sqlite"), b"original journal bytes").unwrap();
        fs::write(root.join("worker.lock"), b"").unwrap();
        ShutdownStatePins::capture(root)
            .unwrap()
            .try_lock()
            .unwrap()
    }
    fn rejected() -> io::Error {
        io::Error::other("Original authority/deadline rejected")
    }
    #[test]
    fn release_allows_exact_successor_lock_and_preserves_original_names_and_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let owner = fixture(temp.path());
        let expected = owner.observation().clone();
        let mut calls = 0;
        let state = owner
            .release_for_handoff_checked(|| {
                calls += 1;
                let next = ShutdownStatePins::capture(temp.path())?.try_lock();
                if calls == 1 {
                    assert!(next.is_err());
                } else {
                    assert_eq!(next?.observation(), &expected);
                }
                Ok(())
            })
            .unwrap();
        assert_eq!(calls, 2);
        assert_eq!(state.observation(), &expected);
        state.revalidate().unwrap();
        assert_eq!(
            fs::read(temp.path().join("journal.sqlite")).unwrap(),
            b"original journal bytes"
        );
        let next = ShutdownStatePins::capture(temp.path())
            .unwrap()
            .try_lock()
            .unwrap();
        assert_eq!(next.observation(), &expected);
        drop(state);
        assert!(ShutdownStatePins::capture(temp.path())
            .unwrap()
            .try_lock()
            .is_err());
        next.revalidate().unwrap();
    }
    #[test]
    fn rejection_before_unlock_retains_original_exclusion_until_failure_owner_drops() {
        let temp = tempfile::tempdir().unwrap();
        let owner = fixture(temp.path());
        let expected = owner.observation().clone();
        let failure = owner
            .release_for_handoff_checked(|| Err(rejected()))
            .unwrap_err();
        assert_eq!(failure.phase(), ShutdownStateReleasePhase::BeforeRelease);
        assert_eq!(failure.original_lock_held(), Some(true));
        assert_eq!(failure.observation(), &expected);
        assert!(ShutdownStatePins::capture(temp.path())
            .unwrap()
            .try_lock()
            .is_err());
        drop(failure);
        ShutdownStatePins::capture(temp.path())
            .unwrap()
            .try_lock()
            .unwrap();
    }
    #[test]
    fn rejection_after_unlock_never_claims_the_original_lock_or_reacquires_it() {
        let temp = tempfile::tempdir().unwrap();
        let owner = fixture(temp.path());
        let expected = owner.observation().clone();
        let mut calls = 0;
        let failure = owner
            .release_for_handoff_checked(|| {
                calls += 1;
                if calls == 2 {
                    Err(rejected())
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        assert_eq!(failure.phase(), ShutdownStateReleasePhase::AfterRelease);
        assert_eq!(failure.original_lock_held(), Some(false));
        assert_eq!(failure.observation(), &expected);
        let next = ShutdownStatePins::capture(temp.path())
            .unwrap()
            .try_lock()
            .unwrap();
        assert_eq!(next.observation(), &expected);
        drop(failure);
        next.revalidate().unwrap();
    }
    #[test]
    fn unlock_error_keeps_uncertainty_for_both_actual_held_and_actual_released_outcomes() {
        for actually_release in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let owner = fixture(temp.path());
            let expected = owner.observation().clone();
            let failure = owner
                .release_with(
                    || Ok(()),
                    |file| {
                        if actually_release {
                            fs2::FileExt::unlock(file)?;
                        }
                        Err(io::Error::other("Injected uncertain OS result"))
                    },
                )
                .unwrap_err();
            assert_eq!(
                failure.phase(),
                ShutdownStateReleasePhase::ReleaseUnconfirmed
            );
            assert_eq!(failure.original_lock_held(), None);
            assert_eq!(failure.observation(), &expected);
            let next = ShutdownStatePins::capture(temp.path()).unwrap().try_lock();
            assert_eq!(next.is_ok(), actually_release);
        }
    }
    #[cfg(unix)]
    #[test]
    fn equal_bytes_under_a_rebound_journal_name_cannot_release_the_original_lock() {
        let temp = tempfile::tempdir().unwrap();
        let owner = fixture(temp.path());
        fs::rename(
            temp.path().join("journal.sqlite"),
            temp.path().join("journal.old"),
        )
        .unwrap();
        fs::write(
            temp.path().join("journal.sqlite"),
            b"original journal bytes",
        )
        .unwrap();
        let failure = owner
            .release_for_handoff_checked(|| panic!("No authority check for rebound state"))
            .unwrap_err();
        assert_eq!(failure.original_lock_held(), Some(true));
        assert!(ShutdownStatePins::capture(temp.path())
            .unwrap()
            .try_lock()
            .is_err());
    }
    #[cfg(unix)]
    #[test]
    fn rebound_name_after_release_retains_failed_pins_without_adopting_new_state() {
        let temp = tempfile::tempdir().unwrap();
        let owner = fixture(temp.path());
        let mut calls = 0;
        let failure = owner
            .release_for_handoff_checked(|| {
                calls += 1;
                if calls == 2 {
                    fs::rename(
                        temp.path().join("journal.sqlite"),
                        temp.path().join("journal.old"),
                    )?;
                    fs::write(
                        temp.path().join("journal.sqlite"),
                        b"original journal bytes",
                    )?;
                }
                Ok(())
            })
            .unwrap_err();
        assert_eq!(failure.phase(), ShutdownStateReleasePhase::AfterRelease);
        assert_eq!(failure.original_lock_held(), Some(false));
        assert_ne!(
            failure.observation().journal_identity,
            ShutdownStatePins::capture(temp.path())
                .unwrap()
                .observation()
                .journal_identity
        );
    }
    #[cfg(windows)]
    #[test]
    fn released_and_failed_windows_pins_still_deny_rename_delete() {
        for fail_after in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let owner = fixture(temp.path());
            let mut calls = 0;
            let result = owner.release_for_handoff_checked(|| {
                calls += 1;
                if fail_after && calls == 2 {
                    Err(rejected())
                } else {
                    Ok(())
                }
            });
            for name in ["journal.sqlite", "worker.lock"] {
                let path = temp.path().join(name);
                assert!(fs::rename(&path, path.with_extension("moved")).is_err());
                assert!(fs::remove_file(&path).is_err());
            }
            drop(result);
            fs::rename(
                temp.path().join("worker.lock"),
                temp.path().join("worker.moved"),
            )
            .unwrap();
        }
    }
}
