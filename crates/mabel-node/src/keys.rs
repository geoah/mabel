//! Key files: 32 random bytes, stored as hex, mode 0600 (proposal 001
//! sections 4, 8 and 12).

use std::fs;
use std::path::Path;

use data_encoding::HEXLOWER;
use iroh_base::SecretKey;

use crate::atomic::{KEY_MODE, check_key_mode, tighten_key_mode, write_atomic};
use crate::error::{Result, StorageError, io_at};

/// Length of a secret key in bytes.
pub const SECRET_KEY_BYTES: usize = 32;

/// Generates a secret key from 32 operating-system random bytes.
///
/// # Errors
///
/// Returns [`StorageError::Random`] if the operating system will not produce
/// random bytes.
pub fn generate_secret_key() -> Result<SecretKey> {
    let mut bytes = [0u8; SECRET_KEY_BYTES];
    getrandom::fill(&mut bytes).map_err(|error| StorageError::Random(error.to_string()))?;
    Ok(SecretKey::from_bytes(&bytes))
}

/// Writes a secret key atomically, ending at mode 0600.
///
/// # Errors
///
/// Returns [`StorageError::Io`] if the write, the rename or the permission
/// change fails.
pub fn write_secret_key(path: &Path, key: &SecretKey) -> Result<()> {
    let mut text = HEXLOWER.encode(&key.to_bytes());
    text.push('\n');
    write_atomic(path, text.as_bytes(), KEY_MODE)?;
    // The rename keeps the temp file's mode, but an existing destination with
    // looser permissions must not survive a rewrite either.
    tighten_key_mode(path)
}

/// Reads a secret key, refusing a group- or world-accessible file.
///
/// `allow_insecure` is the `--allow-insecure-permissions` flag; without it a
/// loose mode is [`StorageError::InsecurePermissions`], exit code 60.
///
/// # Errors
///
/// Returns [`StorageError::InsecurePermissions`] for a loose mode,
/// [`StorageError::MalformedKey`] if the file does not hold 32 hex bytes and
/// [`StorageError::Io`] if it cannot be read.
pub fn read_secret_key(path: &Path, allow_insecure: bool) -> Result<SecretKey> {
    check_key_mode(path, allow_insecure)?;
    let text = fs::read_to_string(path).map_err(io_at(path))?;
    let decoded =
        HEXLOWER
            .decode(text.trim().as_bytes())
            .map_err(|error| StorageError::MalformedKey {
                path: path.to_path_buf(),
                message: error.to_string(),
            })?;
    let bytes: [u8; SECRET_KEY_BYTES] =
        decoded
            .as_slice()
            .try_into()
            .map_err(|_| StorageError::MalformedKey {
                path: path.to_path_buf(),
                message: format!("{} bytes, expected {SECRET_KEY_BYTES}", decoded.len()),
            })?;
    Ok(SecretKey::from_bytes(&bytes))
}

#[cfg(test)]
mod tests {
    use super::{generate_secret_key, read_secret_key, write_secret_key};

    #[test]
    fn generated_keys_differ() {
        let one = generate_secret_key().unwrap();
        let two = generate_secret_key().unwrap();
        assert_ne!(one.to_bytes(), two.to_bytes());
        assert_ne!(one.to_bytes(), [0u8; 32]);
    }

    #[test]
    fn a_key_round_trips_through_its_file() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("node.key");
        let key = generate_secret_key().unwrap();
        write_secret_key(&path, &key).unwrap();
        let read = read_secret_key(&path, false).unwrap();
        assert_eq!(read.to_bytes(), key.to_bytes());
        assert_eq!(read.public(), key.public());
    }

    #[test]
    fn a_short_key_file_is_malformed() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("node.key");
        std::fs::write(&path, "aabb\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let error = read_secret_key(&path, false).expect_err("2 bytes is not a key");
        assert_eq!(error.exit_code(), 10);
    }
}
