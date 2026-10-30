//! Close the writable connection while its actual authority is still borrowed.
//! Errors return the same journal; a keeper must retain custody and ownership.
use super::*;

#[must_use = "Close failure retains the live writable journal; keep authority and physical custody"]
pub struct JournalCloseFailure {
    journal: Box<UpdateJournal>,
    error: JournalError,
}
impl JournalCloseFailure {
    pub fn error(&self) -> &JournalError {
        &self.error
    }
    pub fn into_parts(self) -> (UpdateJournal, JournalError) {
        (*self.journal, self.error)
    }
}
impl fmt::Debug for JournalCloseFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JournalCloseFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}
impl fmt::Display for JournalCloseFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}
impl std::error::Error for JournalCloseFailure {}

impl UpdateJournal {
    /// Successful return proves that this writable connection closed, NOT
    /// that the caller released all authority references or retired its role.
    /// The borrow keeps the OS lock alive through SQLite close. On every
    /// failure both the connection and its directory are returned unchanged.
    pub fn close_owned(
        self,
        authority: &UpdateAuthority,
    ) -> std::result::Result<(), JournalCloseFailure> {
        if let Err(error) = self
            .check_directory(authority)
            .and_then(|_| check_authority(&self.connection, authority))
        {
            return Err(JournalCloseFailure {
                journal: Box::new(self),
                error,
            });
        }
        // sqlite3_close can succeed by silently rolling back an outstanding
        // transaction. A retirement must never hide that unresolved outcome.
        if !self.connection.is_autocommit() {
            return Err(JournalCloseFailure {
                journal: Box::new(self),
                error: fail(
                    JournalErrorCode::PreconditionsChanged,
                    "Updater writer still has a live transaction; retain authority and custody",
                ),
            });
        }
        let Self {
            connection,
            directory,
        } = self;
        connection
            .close()
            .map_err(|(connection, error)| JournalCloseFailure {
                journal: Box::new(Self {
                    connection,
                    directory,
                }),
                error: storage(error),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_close_failure_returns_the_same_journal_and_does_not_release_authority() {
        let root = tempfile::tempdir().unwrap();
        let mut journal = UpdateJournal::open(root.path(), &"a".repeat(64)).unwrap();
        let authority = journal.acquire_owner().unwrap();
        let before = journal.info().unwrap();
        let mut statement = std::ptr::null_mut();
        // A deliberately unfinalized native statement reproduces SQLITE_BUSY
        // at sqlite3_close. No raw handle survives the successful retry below.
        let status = unsafe {
            rusqlite::ffi::sqlite3_prepare_v2(
                journal.connection.handle(),
                c"SELECT 1".as_ptr(),
                -1,
                &mut statement,
                std::ptr::null_mut(),
            )
        };
        assert_eq!(status, rusqlite::ffi::SQLITE_OK);
        assert!(!statement.is_null());
        let failure = journal.close_owned(&authority).unwrap_err();
        assert_eq!(failure.error().code, JournalErrorCode::Storage);
        let (mut journal, _) = failure.into_parts();
        assert_eq!(journal.info().unwrap(), before);
        let mut other = UpdateJournal::open(root.path(), &"a".repeat(64)).unwrap();
        assert_eq!(
            other.acquire_owner().err().unwrap().code,
            JournalErrorCode::OwnerBusy
        );
        // The failed close returned a live connection owning this statement.
        assert_eq!(
            unsafe { rusqlite::ffi::sqlite3_finalize(statement) },
            rusqlite::ffi::SQLITE_OK
        );
        journal.close_owned(&authority).unwrap();
        assert_eq!(
            other.acquire_owner().err().unwrap().code,
            JournalErrorCode::OwnerBusy
        );
        drop(authority);
        let next = other.acquire_owner().unwrap();
        assert_eq!(next.epoch(), before.owner_epoch + 1);
    }

    #[test]
    fn stale_authority_close_refusal_preserves_the_readable_connection_and_rows() {
        let root = tempfile::tempdir().unwrap();
        let mut journal = UpdateJournal::open(root.path(), &"a".repeat(64)).unwrap();
        let authority = journal.acquire_owner().unwrap();
        journal
            .connection
            .execute("UPDATE installation SET epoch=epoch+1", [])
            .unwrap();
        let before = journal.info().unwrap();
        let failure = journal.close_owned(&authority).unwrap_err();
        assert_eq!(failure.error().code, JournalErrorCode::StaleOwner);
        let (mut journal, _) = failure.into_parts();
        assert_eq!(journal.info().unwrap(), before);
    }

    #[test]
    fn live_transaction_cannot_be_silently_rolled_back_by_writer_retirement() {
        let root = tempfile::tempdir().unwrap();
        let mut journal = UpdateJournal::open(root.path(), &"a".repeat(64)).unwrap();
        let authority = journal.acquire_owner().unwrap();
        journal.connection.execute_batch(
            "BEGIN IMMEDIATE; CREATE TABLE close_pending(value INTEGER); INSERT INTO close_pending VALUES(7);",
        ).unwrap();
        let failure = journal.close_owned(&authority).unwrap_err();
        assert_eq!(failure.error().code, JournalErrorCode::PreconditionsChanged);
        let (journal, _) = failure.into_parts();
        assert!(!journal.connection.is_autocommit());
        let value: i64 = journal
            .connection
            .query_row("SELECT value FROM close_pending", [], |r| r.get(0))
            .unwrap();
        assert_eq!(value, 7, "Refusal must retain the unresolved transaction");
        journal.connection.execute_batch("ROLLBACK").unwrap();
        journal.close_owned(&authority).unwrap();
    }
}
