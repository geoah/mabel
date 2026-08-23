//! Reading and writing the three file artifacts of proposal 001 section 3.8.
//!
//! A file handed to the CLI is peer input. Every read checks the artifact's
//! size cap against the file length *before* it reads the bytes in, so an
//! oversize file is refused without allocating in proportion to it (pitfall
//! 7); `mabel-core` then runs the same wire-format validator and field table
//! an event from the network runs.
//!
//! A cap, a malformed encoding or a file that is not the artifact it claims to
//! be exits 10 with the `Schema error:` prefix. A well-formed file whose
//! events do not fold is a different failure: it exits 20 with the reason the
//! fold gave, because the bytes are an artifact and the ledger inside it is
//! the thing that is wrong.

use std::path::Path;

use mabel_core::artifacts::{AcceptanceFile, ArtifactError, IdentityDescriptor, InvitationBundle};
use mabel_core::{
    MAX_ACCEPTANCE_FILE_BYTES, MAX_IDENTITY_DESCRIPTOR_BYTES, MAX_INVITATION_BUNDLE_BYTES,
};

use crate::error::{CliError, Result};

/// One artifact kind: the name its descriptor carries and the cap it is read
/// under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A ledger's events `0..=invitation`, at most 1 MiB.
    InvitationBundle,
    /// An invitee's signed acceptance, at most 4 KiB.
    AcceptanceFile,
    /// An identity's inception and witnesses, at most 64 KiB.
    IdentityDescriptor,
}

impl Kind {
    /// The message name the field table and every error message use.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::InvitationBundle => "InvitationBundle",
            Self::AcceptanceFile => "AcceptanceFile",
            Self::IdentityDescriptor => "IdentityDescriptor",
        }
    }

    /// The size cap of proposal 001 section 3.8.
    #[must_use]
    pub const fn cap(self) -> usize {
        match self {
            Self::InvitationBundle => MAX_INVITATION_BUNDLE_BYTES,
            Self::AcceptanceFile => MAX_ACCEPTANCE_FILE_BYTES,
            Self::IdentityDescriptor => MAX_IDENTITY_DESCRIPTOR_BYTES,
        }
    }
}

/// Reads an `InvitationBundle` file.
///
/// # Errors
///
/// Returns code 10 for a file over 1 MiB or one the validator refuses, and
/// code 2 for a path that is not there.
pub fn read_invitation_bundle(path: &Path) -> Result<InvitationBundle> {
    let kind = Kind::InvitationBundle;
    let bytes = read_capped(kind, path)?;
    InvitationBundle::read(&bytes).map_err(|error| failure(kind, &error, path))
}

/// Reads an `AcceptanceFile`, whose signature `mabel-core` checks on read.
///
/// # Errors
///
/// Returns code 10 for a file over 4 KiB, one the validator refuses or one
/// whose signature does not verify, and code 2 for a path that is not there.
pub fn read_acceptance_file(path: &Path) -> Result<AcceptanceFile> {
    let kind = Kind::AcceptanceFile;
    let bytes = read_capped(kind, path)?;
    AcceptanceFile::read(&bytes).map_err(|error| failure(kind, &error, path))
}

/// Reads an `IdentityDescriptor` and folds its inception.
///
/// # Errors
///
/// Returns code 10 for a file over 64 KiB or one the validator refuses, code
/// 20 when the inception does not fold, and code 2 for a path that is not
/// there.
pub fn read_identity_descriptor(path: &Path) -> Result<IdentityDescriptor> {
    let kind = Kind::IdentityDescriptor;
    let bytes = read_capped(kind, path)?;
    IdentityDescriptor::read(&bytes).map_err(|error| failure(kind, &error, path))
}

/// Writes an artifact, returning its length.
///
/// # Errors
///
/// Returns code 1 when the path cannot be written.
pub fn write(path: &Path, bytes: &[u8]) -> Result<u64> {
    std::fs::write(path, bytes).map_err(|error| {
        CliError::internal(
            "io_error",
            format!("cannot write {}: {error}", path.display()),
        )
        .with_detail("path", path.display().to_string())
    })?;
    Ok(bytes.len() as u64)
}

/// Turns a refusal from `mabel-core` into the envelope and exit code the file
/// deserves.
///
/// A prefix or an inception that does not fold carries the fold's own reason
/// and exits 20; everything else is a malformed artifact and exits 10.
pub fn failure(kind: Kind, error: &ArtifactError, path: &Path) -> CliError {
    let violation = match error {
        ArtifactError::Prefix(violation) | ArtifactError::Inception(violation) => Some(violation),
        _ => None,
    };
    let failure = match violation {
        Some(violation) => {
            CliError::from(&violation.reason).with_detail("failed_at_seq", violation.seq)
        }
        None => CliError::schema(error.code(), error.to_string()),
    };
    failure
        .with_detail("artifact", kind.name())
        .with_detail("path", path.display().to_string())
}

/// Reads a file whose length the cap already accepted.
///
/// The length is taken from the directory entry, so a file over the cap is
/// refused before its bytes are read.
fn read_capped(kind: Kind, path: &Path) -> Result<Vec<u8>> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            return CliError::usage("no_such_file", format!("no file at {}", path.display()))
                .with_detail("path", path.display().to_string());
        }
        CliError::internal(
            "io_error",
            format!("cannot read {}: {error}", path.display()),
        )
        .with_detail("path", path.display().to_string())
    })?;
    let cap = kind.cap();
    if metadata.len() > cap as u64 {
        return Err(too_large(kind, metadata.len(), path));
    }
    let bytes = std::fs::read(path).map_err(|error| {
        CliError::internal(
            "io_error",
            format!("cannot read {}: {error}", path.display()),
        )
        .with_detail("path", path.display().to_string())
    })?;
    // A file that grew between the two calls is refused on the same rule.
    if bytes.len() > cap {
        return Err(too_large(kind, bytes.len() as u64, path));
    }
    Ok(bytes)
}

/// The code 10 envelope for an over-cap file, worded as
/// `WireError::MessageTooLarge` words it.
fn too_large(kind: Kind, len: u64, path: &Path) -> CliError {
    CliError::schema(
        "message_too_large",
        format!(
            "{} is {len} bytes, over the {}-byte cap",
            kind.name(),
            kind.cap()
        ),
    )
    .with_detail("artifact", kind.name())
    .with_detail("path", path.display().to_string())
    .with_detail("bytes", len)
    .with_detail("cap", kind.cap())
}
