//! Atomic file writes and the permission rules (proposal 001 section 8).
//!
//! Every write goes to a temp file in the destination directory, is flushed
//! with `fsync`, is renamed over the destination and is followed by an
//! `fsync` of the directory, so a reader sees either the old file or the
//! whole new one.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{Result, StorageError, io_at};

/// Mode of every directory in the node home.
pub const DIR_MODE: u32 = 0o700;

/// Mode of every key file in the node home.
pub const KEY_MODE: u32 = 0o600;

/// Mode of the files that are not key material.
pub const DATA_MODE: u32 = 0o644;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Creates a directory and its parents with mode 0700.
pub(crate) fn create_dir(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(DIR_MODE);
    }
    builder.create(path).map_err(io_at(path))
}

/// Writes `contents` to `path` atomically, leaving the file at `mode`.
pub(crate) fn write_atomic(path: &Path, contents: &[u8], mode: u32) -> Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let temp = temp_path(path);
    match write_temp(&temp, contents, mode) {
        Ok(()) => {}
        Err(error) => {
            let _ = fs::remove_file(&temp);
            return Err(error);
        }
    }
    if let Err(error) = fs::rename(&temp, path).map_err(io_at(path)) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    sync_dir(dir)
}

fn write_temp(temp: &Path, contents: &[u8], mode: u32) -> Result<()> {
    let mut file = File::create(temp).map_err(io_at(temp))?;
    set_mode(&file, temp, mode)?;
    file.write_all(contents).map_err(io_at(temp))?;
    file.sync_all().map_err(io_at(temp))
}

fn temp_path(path: &Path) -> PathBuf {
    let name = path.file_name().map_or_else(
        || String::from("file"),
        |n| n.to_string_lossy().into_owned(),
    );
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = format!(".{name}.tmp.{}.{counter}", process::id());
    path.parent().unwrap_or_else(|| Path::new(".")).join(temp)
}

#[cfg(unix)]
fn set_mode(file: &File, path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(mode))
        .map_err(io_at(path))
}

#[cfg(not(unix))]
fn set_mode(_file: &File, _path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

/// Flushes a directory entry so a rename survives a crash.
#[cfg(unix)]
pub(crate) fn sync_dir(dir: &Path) -> Result<()> {
    File::open(dir)
        .and_then(|handle| handle.sync_all())
        .map_err(io_at(dir))
}

#[cfg(not(unix))]
pub(crate) fn sync_dir(_dir: &Path) -> Result<()> {
    Ok(())
}

/// Refuses a key file that the group or the world can reach.
///
/// `allow_insecure` is the `--allow-insecure-permissions` flag (proposal 001
/// sections 8 and 9); when it is set the mode is only logged.
#[cfg(unix)]
pub(crate) fn check_key_mode(path: &Path, allow_insecure: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = fs::metadata(path)
        .map_err(io_at(path))?
        .permissions()
        .mode()
        & 0o777;
    if mode & 0o077 == 0 {
        return Ok(());
    }
    if allow_insecure {
        tracing::warn!(path = %path.display(), mode = format!("{mode:04o}"), "insecure key file permissions allowed by flag");
        return Ok(());
    }
    Err(StorageError::InsecurePermissions {
        path: path.to_path_buf(),
        mode,
    })
}

#[cfg(not(unix))]
pub(crate) fn check_key_mode(_path: &Path, _allow_insecure: bool) -> Result<()> {
    Ok(())
}

/// Tightens an existing key file to 0600.
#[cfg(unix)]
pub(crate) fn tighten_key_mode(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(KEY_MODE)).map_err(io_at(path))
}

#[cfg(not(unix))]
pub(crate) fn tighten_key_mode(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::{DIR_MODE, KEY_MODE, check_key_mode, create_dir, write_atomic};

    fn mode_of(path: &std::path::Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn created_directories_are_0700() {
        let home = tempfile::tempdir().unwrap();
        let nested = home.path().join("a/b/c");
        create_dir(&nested).unwrap();
        assert_eq!(mode_of(&nested), DIR_MODE);
        assert_eq!(mode_of(&home.path().join("a")), DIR_MODE);
    }

    #[test]
    fn atomic_write_leaves_no_temp_files() {
        let home = tempfile::tempdir().unwrap();
        let target = home.path().join("thing.json");
        write_atomic(&target, b"{}", KEY_MODE).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"{}");
        assert_eq!(mode_of(&target), KEY_MODE);

        let entries: Vec<_> = std::fs::read_dir(home.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(entries, vec![std::ffi::OsString::from("thing.json")]);
    }

    #[test]
    fn atomic_write_replaces_the_previous_contents() {
        let home = tempfile::tempdir().unwrap();
        let target = home.path().join("thing.json");
        write_atomic(&target, b"old", KEY_MODE).unwrap();
        write_atomic(&target, b"new", KEY_MODE).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
    }

    #[test]
    fn a_0644_key_file_is_refused_unless_the_flag_is_set() {
        let home = tempfile::tempdir().unwrap();
        let key = home.path().join("node.key");
        write_atomic(&key, b"secret", 0o644).unwrap();

        let error = check_key_mode(&key, false).expect_err("0644 is refused");
        assert!(error.is_insecure_permissions());
        assert_eq!(error.exit_code(), 60);
        check_key_mode(&key, true).expect("the flag allows it");

        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(KEY_MODE)).unwrap();
        check_key_mode(&key, false).expect("0600 passes");
    }
}
