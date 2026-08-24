//! `contacts/<identity_id>.json`: the private note this node keeps on one
//! identity (proposal 003 section 1).
//!
//! Nothing here is signed and nothing here is synced. The store covers foreign
//! identities as well as this node's own, which is why it is deliberately not
//! part of [`IdentityMeta`]: that file describes identities this home
//! controls, and it is `deny_unknown_fields`.
//!
//! Decision 003 makes the chain the full history and proposal 001 section 1
//! makes ledgers public replicated data, so an address on a ledger is a
//! permanent publication. This is where the address goes instead.

use std::fs;
use std::path::{Path, PathBuf};

use mabel_core::IdentityId;
use serde::{Deserialize, Serialize};

use crate::atomic::{DATA_MODE, create_dir, write_atomic};
use crate::error::{Result, io_at, json_at};
use crate::home::NodeHome;
use crate::now_ms;

/// Directory of the contact store, under the node home.
pub const CONTACTS_DIR: &str = "contacts";

/// Bytes of UTF-8 a nickname may hold (proposal 003 section 1).
pub const MAX_NICKNAME_BYTES: usize = 64;

/// Bytes of UTF-8 a note may hold (proposal 003 section 1).
pub const MAX_NOTE_BYTES: usize = 512;

/// One private note: `contacts/<identity_id>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContactEntry {
    /// A private name for this identity.
    #[serde(default)]
    pub nickname: Option<String>,
    /// A private note about this identity.
    #[serde(default)]
    pub note: Option<String>,
    /// When this node last wrote the file.
    pub updated_at_ms: u64,
}

impl ContactEntry {
    /// A note stamped with `now_ms`.
    #[must_use]
    pub fn new(nickname: Option<String>, note: Option<String>, now_ms: u64) -> Self {
        Self {
            nickname,
            note,
            updated_at_ms: now_ms,
        }
    }

    /// Whether both fields are unset, which is what deletes the file.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nickname.is_none() && self.note.is_none()
    }
}

/// Why a nickname or a note was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContactTextError {
    /// The value is over the field's byte cap.
    #[error("{field} is at most {cap} bytes of UTF-8, and this is {len}")]
    TooLong {
        /// Which field.
        field: &'static str,
        /// Bytes the value holds.
        len: usize,
        /// Bytes the field allows.
        cap: usize,
    },
    /// The value holds a codepoint the policy of proposal 003 section 1
    /// forbids, or has whitespace at an edge.
    #[error("{field} is not valid text: {detail}")]
    Invalid {
        /// Which field.
        field: &'static str,
        /// What is wrong with it, in one clause.
        detail: &'static str,
    },
}

/// What a contact-field check returns.
pub type TextResult<T> = std::result::Result<T, ContactTextError>;

/// Checks one contact field against the byte cap and the codepoint policy.
///
/// The policy is the one proposal 003 section 1 fixes for `display_name`,
/// minus the identity-id rule: a nickname never leaves this node, so it
/// cannot be mistaken for someone else's identifier on another screen.
///
/// # Errors
///
/// Returns [`ContactTextError`] for a value over the cap or holding a control,
/// bidi or invisible-format character, or with whitespace at an edge.
pub fn check_text(field: &'static str, value: &str, cap: usize) -> TextResult<()> {
    if value.len() > cap {
        return Err(ContactTextError::TooLong {
            field,
            len: value.len(),
            cap,
        });
    }
    let invalid = |detail| ContactTextError::Invalid { field, detail };
    for character in value.chars() {
        match character {
            '\u{0}'..='\u{1f}' | '\u{7f}' => {
                return Err(invalid("it holds a C0 control character"));
            }
            '\u{80}'..='\u{9f}' => return Err(invalid("it holds a C1 control character")),
            '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' => {
                return Err(invalid("it holds a bidi control character"));
            }
            '\u{200b}'..='\u{200f}' | '\u{2060}'..='\u{2064}' | '\u{feff}' => {
                return Err(invalid(
                    "it holds a zero-width or invisible format character",
                ));
            }
            _ => {}
        }
    }
    if value.trim() != value {
        return Err(invalid("it has leading or trailing whitespace"));
    }
    Ok(())
}

/// Normalizes and checks one field: an empty value is unset, not empty text.
///
/// # Errors
///
/// Returns the errors of [`check_text`].
pub fn normalize(
    field: &'static str,
    value: Option<&str>,
    cap: usize,
) -> TextResult<Option<String>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    check_text(field, value, cap)?;
    Ok(Some(value.to_owned()))
}

/// The contact directory, one file per identity.
#[derive(Debug, Clone)]
pub struct ContactStore {
    dir: PathBuf,
}

impl ContactStore {
    /// The store under a node home.
    #[must_use]
    pub fn new(home: &NodeHome) -> Self {
        Self::at(home.contacts_dir())
    }

    /// The store in one directory.
    #[must_use]
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The directory holding the files.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// `contacts/<identity_id>.json`.
    #[must_use]
    pub fn path(&self, identity: IdentityId) -> PathBuf {
        self.dir.join(format!("{identity}.json"))
    }

    /// Reads the note for one identity, `None` when there is none.
    ///
    /// A malformed file is treated as absent: the store is a convenience, and
    /// a bad file must not fail the identity document that carries it.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] when the file exists and cannot be read.
    ///
    /// [`StorageError::Io`]: crate::StorageError::Io
    pub fn read(&self, identity: IdentityId) -> Result<Option<ContactEntry>> {
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
                    "discarding an unreadable contact file"
                );
                Ok(None)
            }
        }
    }

    /// Every identity this store holds a note on, sorted.
    ///
    /// A file name that is not an identity id is skipped rather than failing
    /// the listing: the caller wants the notes, not an audit of the directory.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] when the directory exists and cannot be
    /// listed. A directory that is not there yet holds no notes.
    ///
    /// [`StorageError::Io`]: crate::StorageError::Io
    pub fn identities(&self) -> Result<Vec<IdentityId>> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(io_at(&self.dir)(error)),
        };
        let mut identities = Vec::new();
        for entry in entries {
            let name = entry.map_err(io_at(&self.dir))?.file_name();
            let Some(stem) = name
                .to_string_lossy()
                .strip_suffix(".json")
                .map(str::to_owned)
            else {
                continue;
            };
            if let Ok(identity) = stem.parse::<IdentityId>() {
                identities.push(identity);
            }
        }
        identities.sort_unstable();
        Ok(identities)
    }

    /// Writes the note for one identity, or removes the file when both fields
    /// are unset.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] when the directory or the file cannot be
    /// written.
    ///
    /// [`StorageError::Io`]: crate::StorageError::Io
    pub fn write(
        &self,
        identity: IdentityId,
        entry: &ContactEntry,
    ) -> Result<Option<ContactEntry>> {
        if entry.is_empty() {
            self.remove(identity)?;
            return Ok(None);
        }
        create_dir(&self.dir)?;
        let path = self.path(identity);
        let mut bytes = serde_json::to_vec_pretty(entry).map_err(json_at(&path))?;
        bytes.push(b'\n');
        write_atomic(&path, &bytes, DATA_MODE)?;
        Ok(Some(entry.clone()))
    }

    /// Replaces the note for one identity, stamping it with the current time.
    ///
    /// # Errors
    ///
    /// As [`ContactStore::write`].
    pub fn replace(
        &self,
        identity: IdentityId,
        nickname: Option<String>,
        note: Option<String>,
    ) -> Result<Option<ContactEntry>> {
        self.write(identity, &ContactEntry::new(nickname, note, now_ms()))
    }

    /// Forgets the note for one identity.
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

    use super::{
        ContactEntry, ContactStore, ContactTextError, MAX_NICKNAME_BYTES, MAX_NOTE_BYTES, normalize,
    };

    fn identity() -> IdentityId {
        IdentityId::from_bytes([3u8; 32])
    }

    #[test]
    fn a_note_round_trips_through_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = ContactStore::at(dir.path().join("contacts"));
        assert_eq!(store.read(identity()).unwrap(), None);

        let written = store
            .replace(
                identity(),
                Some("bob at the print shop".to_owned()),
                Some("met at the zine fair".to_owned()),
            )
            .unwrap()
            .expect("both fields are set");
        let read = store.read(identity()).unwrap().expect("the file is there");
        assert_eq!(read, written);
        assert_eq!(read.nickname.as_deref(), Some("bob at the print shop"));
        assert_eq!(read.note.as_deref(), Some("met at the zine fair"));
    }

    #[test]
    fn clearing_both_fields_removes_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = ContactStore::at(dir.path());
        store
            .replace(identity(), Some("bob".to_owned()), None)
            .unwrap();
        assert!(store.path(identity()).is_file());

        assert_eq!(store.replace(identity(), None, None).unwrap(), None);
        assert!(!store.path(identity()).exists());
        assert_eq!(store.read(identity()).unwrap(), None);
    }

    #[test]
    fn a_malformed_contact_file_is_treated_as_absent() {
        let dir = tempfile::tempdir().unwrap();
        let store = ContactStore::at(dir.path());
        std::fs::write(store.path(identity()), b"{\"nickname\":").unwrap();
        assert_eq!(store.read(identity()).unwrap(), None);

        std::fs::write(store.path(identity()), b"{\"surprise\": true}").unwrap();
        assert_eq!(store.read(identity()).unwrap(), None);
    }

    #[test]
    fn an_empty_field_is_unset_and_a_long_one_is_refused() {
        assert_eq!(
            normalize("nickname", Some("   "), MAX_NICKNAME_BYTES).unwrap(),
            None
        );
        assert_eq!(
            normalize("nickname", Some(" bob "), MAX_NICKNAME_BYTES).unwrap(),
            Some("bob".to_owned())
        );
        let long = "n".repeat(MAX_NICKNAME_BYTES + 1);
        assert_eq!(
            normalize("nickname", Some(&long), MAX_NICKNAME_BYTES),
            Err(ContactTextError::TooLong {
                field: "nickname",
                len: MAX_NICKNAME_BYTES + 1,
                cap: MAX_NICKNAME_BYTES,
            })
        );
        assert!(normalize("note", Some(&"a".repeat(MAX_NOTE_BYTES)), MAX_NOTE_BYTES).is_ok());
    }

    #[test]
    fn a_forbidden_codepoint_is_refused() {
        for (value, detail) in [
            ("bob\u{7}", "it holds a C0 control character"),
            ("bob\u{80}", "it holds a C1 control character"),
            ("bob\u{202e}", "it holds a bidi control character"),
            (
                "bob\u{200b}",
                "it holds a zero-width or invisible format character",
            ),
        ] {
            assert_eq!(
                normalize("nickname", Some(value), MAX_NICKNAME_BYTES),
                Err(ContactTextError::Invalid {
                    field: "nickname",
                    detail,
                }),
                "{value:?}"
            );
        }
    }

    #[test]
    fn an_entry_with_no_fields_is_empty() {
        assert!(ContactEntry::new(None, None, 1).is_empty());
        assert!(!ContactEntry::new(Some("bob".to_owned()), None, 1).is_empty());
    }
}
