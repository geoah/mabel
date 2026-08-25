//! `verification/<identity_id>.json`: the advisory verdict and when it was
//! taken (proposal 003 section 2).
//!
//! The cache is rebuildable. Deleting it costs one lookup, so a file that
//! does not parse is treated as absent rather than as an error the identity
//! document has to carry.

use std::fs;
use std::path::{Path, PathBuf};

use mabel_core::IdentityId;
use serde::{Deserialize, Serialize};

use super::verify::{VerificationOutcome, VerificationStatus};
use crate::atomic::{DATA_MODE, create_dir, write_atomic};
use crate::error::{Result, io_at, json_at};
use crate::home::NodeHome;

/// Directory of the verification cache, under the node home.
pub const VERIFICATION_DIR: &str = "verification";

/// How long a result is fresh: 24 hours (proposal 003 section 2).
pub const FRESH_FOR_MS: u64 = 24 * 60 * 60 * 1000;

/// One cached verdict, bound to the hostname it verified.
///
/// `checked_at_ms` belongs to the result in `status`: an `unreachable`
/// re-check of a decisive result lands in [`VerificationEntry::unreachable`]
/// and leaves the rest alone (proposal 003 section 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationEntry {
    /// The hostname this verdict is about. A profile naming a different
    /// hostname makes the entry absent.
    pub hostname: String,
    /// The verdict.
    pub status: VerificationStatus,
    /// When the lookup behind `status` ran.
    pub checked_at_ms: u64,
    /// When this hostname last verified, kept across later verdicts.
    #[serde(default)]
    pub last_verified_at_ms: Option<u64>,
    /// One sentence naming what was queried and what came back.
    pub detail: String,
    /// The last failed re-check, recorded beside a decisive result.
    #[serde(default)]
    pub unreachable: Option<UnreachableCheck>,
}

/// A re-check that did not answer, kept beside the decisive result it could
/// not refresh.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnreachableCheck {
    /// When the failed re-check ran.
    pub checked_at_ms: u64,
    /// Why it did not answer.
    pub detail: String,
}

impl VerificationEntry {
    /// A first entry for one outcome.
    #[must_use]
    pub fn first(outcome: &VerificationOutcome, now_ms: u64) -> Self {
        Self {
            hostname: outcome.hostname.clone(),
            status: outcome.status,
            checked_at_ms: now_ms,
            last_verified_at_ms: (outcome.status == VerificationStatus::Verified).then_some(now_ms),
            detail: outcome.detail.clone(),
            unreachable: None,
        }
    }

    /// How long ago the result in `status` was taken.
    #[must_use]
    pub fn age_ms(&self, now_ms: u64) -> u64 {
        now_ms.saturating_sub(self.checked_at_ms)
    }

    /// True once the result is over 24 hours old, which the UI renders as
    /// `stale` and the single-identity GET answers with while it refreshes.
    #[must_use]
    pub fn is_stale(&self, now_ms: u64) -> bool {
        self.age_ms(now_ms) > FRESH_FOR_MS
    }

    /// True when this entry is about `hostname`.
    #[must_use]
    pub fn covers(&self, hostname: &str) -> bool {
        self.hostname.eq_ignore_ascii_case(hostname)
    }
}

/// Folds one outcome into the entry the cache should hold.
///
/// Three rules from proposal 003 section 2, in order: an entry about another
/// hostname is absent, an `unreachable` re-check never overwrites a decisive
/// result, and `last_verified_at_ms` survives every later verdict.
#[must_use]
pub fn merge(
    previous: Option<VerificationEntry>,
    outcome: &VerificationOutcome,
    now_ms: u64,
) -> VerificationEntry {
    let previous = previous.filter(|entry| entry.covers(&outcome.hostname));
    let Some(previous) = previous else {
        return VerificationEntry::first(outcome, now_ms);
    };

    if outcome.status == VerificationStatus::Unreachable && previous.status.is_decisive() {
        return VerificationEntry {
            unreachable: Some(UnreachableCheck {
                checked_at_ms: now_ms,
                detail: outcome.detail.clone(),
            }),
            ..previous
        };
    }

    let last_verified_at_ms = if outcome.status == VerificationStatus::Verified {
        Some(now_ms)
    } else {
        previous.last_verified_at_ms
    };
    VerificationEntry {
        hostname: outcome.hostname.clone(),
        status: outcome.status,
        checked_at_ms: now_ms,
        last_verified_at_ms,
        detail: outcome.detail.clone(),
        unreachable: None,
    }
}

/// Whether the single-identity GET should start one background refresh
/// (proposal 003 section 2).
///
/// Only an entry this node already holds, gone over 24 hours old, refreshes.
/// A claim with no entry does not: the first sight of a stranger's hostname
/// must not query their zone, because reading a card is not asking to be
/// announced to the name on it (decision 018, issue 042). A check is something
/// a person asks for, and the forced check is where they ask. No timer
/// refreshes anything either.
#[must_use]
pub fn should_refresh(entry: Option<&VerificationEntry>, now_ms: u64) -> bool {
    entry.is_some_and(|entry| entry.is_stale(now_ms))
}

/// The verification cache directory, one file per identity.
#[derive(Debug, Clone)]
pub struct VerificationStore {
    dir: PathBuf,
}

impl VerificationStore {
    /// The store under a node home.
    #[must_use]
    pub fn new(home: &NodeHome) -> Self {
        Self::at(home.root().join(VERIFICATION_DIR))
    }

    /// The store in one directory.
    #[must_use]
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The directory holding the cache files.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// `verification/<identity_id>.json`.
    #[must_use]
    pub fn path(&self, identity: IdentityId) -> PathBuf {
        self.dir.join(format!("{identity}.json"))
    }

    /// Reads the entry for one identity, whatever hostname it is about.
    ///
    /// A missing or malformed file answers `None`: the cache is advisory and
    /// rebuildable, so a bad file costs one lookup, not a failed request.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] when the file exists and cannot be read.
    ///
    /// [`StorageError::Io`]: crate::StorageError::Io
    pub fn read(&self, identity: IdentityId) -> Result<Option<VerificationEntry>> {
        let path = self.path(identity);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_at(&path)(error)),
        };
        match serde_json::from_slice(&bytes) {
            Ok(entry) => Ok(Some(entry)),
            Err(error) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %error,
                    "discarding an unreadable verification cache entry"
                );
                Ok(None)
            }
        }
    }

    /// Reads the entry for one identity, absent unless it is about
    /// `hostname` (proposal 003 section 2, hostname binding).
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] when the file exists and cannot be read.
    ///
    /// [`StorageError::Io`]: crate::StorageError::Io
    pub fn read_bound(
        &self,
        identity: IdentityId,
        hostname: &str,
    ) -> Result<Option<VerificationEntry>> {
        Ok(self.read(identity)?.filter(|entry| entry.covers(hostname)))
    }

    /// Writes the entry for one identity.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] when the directory or the file cannot be
    /// written.
    ///
    /// [`StorageError::Io`]: crate::StorageError::Io
    pub fn write(&self, identity: IdentityId, entry: &VerificationEntry) -> Result<()> {
        create_dir(&self.dir)?;
        let path = self.path(identity);
        let mut bytes = serde_json::to_vec_pretty(entry).map_err(json_at(&path))?;
        bytes.push(b'\n');
        write_atomic(&path, &bytes, DATA_MODE)
    }

    /// Folds one outcome into the stored entry and writes the result.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] when the file cannot be read or written.
    ///
    /// [`StorageError::Io`]: crate::StorageError::Io
    pub fn record(
        &self,
        identity: IdentityId,
        outcome: &VerificationOutcome,
        now_ms: u64,
    ) -> Result<VerificationEntry> {
        let entry = merge(self.read(identity)?, outcome, now_ms);
        self.write(identity, &entry)?;
        Ok(entry)
    }

    /// Forgets the entry for one identity.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] when the file exists and cannot be
    /// removed.
    ///
    /// [`StorageError::Io`]: crate::StorageError::Io
    pub fn remove(&self, identity: IdentityId) -> Result<()> {
        let path = self.path(identity);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_at(&path)(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use mabel_core::IdentityId;

    use super::super::verify::{VerificationOutcome, VerificationStatus};
    use super::{FRESH_FOR_MS, VerificationEntry, VerificationStore, merge, should_refresh};

    const HOUR: u64 = 60 * 60 * 1000;

    fn identity() -> IdentityId {
        IdentityId::from_bytes([7u8; 32])
    }

    fn outcome(hostname: &str, status: VerificationStatus) -> VerificationOutcome {
        VerificationOutcome {
            hostname: hostname.to_owned(),
            status,
            detail: format!("{hostname} is {status}"),
        }
    }

    fn verified(now_ms: u64) -> VerificationEntry {
        VerificationEntry::first(
            &outcome("alice.example", VerificationStatus::Verified),
            now_ms,
        )
    }

    #[test]
    fn a_first_verified_entry_stamps_both_timestamps() {
        let entry = verified(1_000);
        assert_eq!(entry.status, VerificationStatus::Verified);
        assert_eq!(entry.checked_at_ms, 1_000);
        assert_eq!(entry.last_verified_at_ms, Some(1_000));
        assert!(entry.unreachable.is_none());

        let unverified =
            VerificationEntry::first(&outcome("bob.example", VerificationStatus::Unverified), 5);
        assert_eq!(unverified.last_verified_at_ms, None);
    }

    #[test]
    fn an_entry_about_another_hostname_is_treated_as_absent() {
        let previous = verified(1_000);
        let entry = merge(
            Some(previous),
            &outcome("carol.example", VerificationStatus::Unverified),
            2_000,
        );

        assert_eq!(entry.hostname, "carol.example");
        assert_eq!(entry.status, VerificationStatus::Unverified);
        assert_eq!(entry.checked_at_ms, 2_000);
        assert_eq!(entry.last_verified_at_ms, None);
    }

    #[test]
    fn an_unreachable_recheck_leaves_a_decisive_result_in_place() {
        for decisive in [VerificationStatus::Verified, VerificationStatus::Mismatched] {
            let previous = VerificationEntry::first(&outcome("alice.example", decisive), 1_000);
            let entry = merge(
                Some(previous.clone()),
                &outcome("alice.example", VerificationStatus::Unreachable),
                9_000,
            );

            assert_eq!(entry.status, decisive);
            assert_eq!(entry.checked_at_ms, 1_000);
            assert_eq!(entry.detail, previous.detail);
            assert_eq!(entry.last_verified_at_ms, previous.last_verified_at_ms);
            let beside = entry.unreachable.expect("the failed re-check is recorded");
            assert_eq!(beside.checked_at_ms, 9_000);
            assert_eq!(beside.detail, "alice.example is unreachable");
        }
    }

    #[test]
    fn an_unreachable_recheck_overwrites_an_indecisive_result() {
        let previous =
            VerificationEntry::first(&outcome("alice.example", VerificationStatus::Unverified), 1);
        let entry = merge(
            Some(previous),
            &outcome("alice.example", VerificationStatus::Unreachable),
            9_000,
        );

        assert_eq!(entry.status, VerificationStatus::Unreachable);
        assert_eq!(entry.checked_at_ms, 9_000);
        assert!(entry.unreachable.is_none());
    }

    #[test]
    fn a_decisive_recheck_clears_the_failed_one_and_keeps_the_last_verified_time() {
        let verified = verified(1_000);
        let after_failure = merge(
            Some(verified),
            &outcome("alice.example", VerificationStatus::Unreachable),
            2_000,
        );
        let after_mismatch = merge(
            Some(after_failure),
            &outcome("alice.example", VerificationStatus::Mismatched),
            3_000,
        );

        assert_eq!(after_mismatch.status, VerificationStatus::Mismatched);
        assert_eq!(after_mismatch.checked_at_ms, 3_000);
        assert_eq!(after_mismatch.last_verified_at_ms, Some(1_000));
        assert!(after_mismatch.unreachable.is_none());

        let verified_again = merge(
            Some(after_mismatch),
            &outcome("alice.example", VerificationStatus::Verified),
            4_000,
        );
        assert_eq!(verified_again.last_verified_at_ms, Some(4_000));
    }

    #[test]
    fn a_verified_result_goes_stale_after_twenty_four_hours() {
        let entry = verified(0);
        assert!(!entry.is_stale(FRESH_FOR_MS));
        assert_eq!(entry.age_ms(FRESH_FOR_MS), FRESH_FOR_MS);
        assert!(entry.is_stale(FRESH_FOR_MS + 1));
        assert!(entry.is_stale(25 * HOUR));
    }

    #[test]
    fn only_a_stale_entry_refreshes_and_a_missing_one_never_does() {
        let entry = verified(0);
        // A hostname this node has never checked stays unchecked until a
        // person asks. Refreshing here would query a stranger's zone on the
        // first sight of their card (decision 018, issue 042).
        assert!(!should_refresh(None, 0));
        assert!(!should_refresh(None, 25 * HOUR));
        assert!(!should_refresh(Some(&entry), 23 * HOUR));
        assert!(should_refresh(Some(&entry), 25 * HOUR));
    }

    #[test]
    fn an_entry_round_trips_through_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = VerificationStore::at(dir.path().join("verification"));
        assert_eq!(store.read(identity()).unwrap(), None);

        let entry = store
            .record(
                identity(),
                &outcome("alice.example", VerificationStatus::Verified),
                1_000,
            )
            .unwrap();
        assert_eq!(store.read(identity()).unwrap(), Some(entry.clone()));
        assert!(
            store
                .path(identity())
                .ends_with(format!("{}.json", identity()))
        );

        let json = std::fs::read_to_string(store.path(identity())).unwrap();
        assert!(json.contains("\"status\": \"verified\""), "{json}");
        assert!(json.contains("\"last_verified_at_ms\": 1000"), "{json}");

        store.remove(identity()).unwrap();
        store.remove(identity()).unwrap();
        assert_eq!(store.read(identity()).unwrap(), None);
    }

    #[test]
    fn the_store_binds_an_entry_to_its_hostname() {
        let dir = tempfile::tempdir().unwrap();
        let store = VerificationStore::at(dir.path());
        store
            .record(
                identity(),
                &outcome("alice.example", VerificationStatus::Verified),
                1_000,
            )
            .unwrap();

        assert!(
            store
                .read_bound(identity(), "alice.example")
                .unwrap()
                .is_some()
        );
        assert_eq!(store.read_bound(identity(), "carol.example").unwrap(), None);
    }

    #[test]
    fn a_malformed_cache_file_is_treated_as_absent() {
        let dir = tempfile::tempdir().unwrap();
        let store = VerificationStore::at(dir.path());
        std::fs::write(store.path(identity()), b"{\"hostname\":").unwrap();
        assert_eq!(store.read(identity()).unwrap(), None);

        std::fs::write(store.path(identity()), b"{\"surprise\": true}").unwrap();
        assert_eq!(store.read(identity()).unwrap(), None);
    }
}
