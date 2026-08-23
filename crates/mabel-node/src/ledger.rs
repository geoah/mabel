//! One directory per ledger: event files, the head cache, provenance and
//! fork records (proposal 001 section 8).
//!
//! Event files are named by zero-padded sequence, so directory order is chain
//! order and the only two access patterns, "read all" and "read from seq N",
//! are served by a sorted listing. There is no database and no other index.
//!
//! Stored bytes are the signed object. Nothing here decodes an event and
//! re-encodes it; a decode happens only to read the `body` field when the
//! head cache is rebuilt (proposal 001 section 3.1, byte authority).

use std::fs;
use std::path::{Path, PathBuf};

use iroh_base::EndpointId;
use mabel_core::{EventId, LedgerId, event_id};
use mabel_proto::prost::Message;
use mabel_proto::v0::SignedEvent;
use serde::{Deserialize, Serialize};

use crate::atomic::{DATA_MODE, create_dir, write_atomic};
use crate::error::{Result, StorageError, io_at, json_at};
use crate::now_ms;

/// Digits in an event file name, so directory order is chain order.
pub const SEQ_DIGITS: usize = 12;

/// Extension of an event file.
pub const EVENT_EXT: &str = "ev";

/// Extension of a fork record file.
pub const FORK_EXT: &str = "fork";

/// Name of the head cache file.
pub const HEAD_FILE: &str = "head.json";

/// Name of the ledger provenance file.
pub const META_FILE: &str = "meta.json";

/// The head cache, rebuildable from the event files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Head {
    /// Sequence of the last event.
    pub seq: u64,
    /// Id of the last event.
    pub event_id: EventId,
    /// When this cache was written.
    pub updated_ms: u64,
}

/// Where a ledger came from. Provenance, never authorization (proposal 001
/// section 4).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerMeta {
    /// The endpoint the first event arrived from, absent for a local ledger.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_endpoint: Option<EndpointId>,
    /// When this node first stored an event of this ledger.
    pub first_seen_ms: u64,
}

/// An event offered to [`LedgerStore::append`].
///
/// `bytes` is the encoded `SignedEvent` exactly as the signer or the peer
/// produced it, and is what lands on disk.
#[derive(Debug, Clone, Copy)]
pub struct NewEvent<'a> {
    /// Position in the chain.
    pub seq: u64,
    /// Id of the event, `BLAKE3(EVENT_ID_DOMAIN || body)`.
    pub event_id: EventId,
    /// The encoded `SignedEvent`.
    pub bytes: &'a [u8],
}

/// An event read back from disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredEvent {
    /// Position in the chain.
    pub seq: u64,
    /// The stored `SignedEvent` bytes, unmodified.
    pub bytes: Vec<u8>,
}

/// A fork record file, named by the conflicting event's id (proposal 001,
/// clarifications).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkFile {
    /// Sequence the two events contend for.
    pub seq: u64,
    /// Id of the event that lost, which is not in the ledger directory.
    pub conflicting: EventId,
    /// Path of the `.fork` file.
    pub path: PathBuf,
}

/// The files of one ledger.
///
/// Cheap to construct; nothing on disk is touched until a method runs.
#[derive(Debug, Clone)]
pub struct LedgerStore {
    ledger: LedgerId,
    dir: PathBuf,
    forks_dir: PathBuf,
}

impl LedgerStore {
    pub(crate) fn new(ledger: LedgerId, dir: PathBuf, forks_dir: PathBuf) -> Self {
        Self {
            ledger,
            dir,
            forks_dir,
        }
    }

    /// The ledger this store holds.
    #[must_use]
    pub fn ledger_id(&self) -> LedgerId {
        self.ledger
    }

    /// `ledgers/<id>/`.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// `forks/<id>/`.
    #[must_use]
    pub fn forks_dir(&self) -> &Path {
        &self.forks_dir
    }

    /// Path of one event file.
    #[must_use]
    pub fn event_path(&self, seq: u64) -> PathBuf {
        self.dir.join(format!("{seq:0SEQ_DIGITS$}.{EVENT_EXT}"))
    }

    /// Path of the head cache.
    #[must_use]
    pub fn head_path(&self) -> PathBuf {
        self.dir.join(HEAD_FILE)
    }

    /// Path of the provenance record.
    #[must_use]
    pub fn meta_path(&self) -> PathBuf {
        self.dir.join(META_FILE)
    }

    /// True once the ledger directory exists.
    #[must_use]
    pub fn exists(&self) -> bool {
        self.dir.is_dir()
    }

    /// Every sequence with an event file, sorted.
    ///
    /// This is the raw listing: it may run past the head cache if a crash
    /// landed an event file whose `head.json` rename never happened.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] if the directory cannot be listed.
    pub fn sequences(&self) -> Result<Vec<u64>> {
        let mut seqs = read_seqs(&self.dir, EVENT_EXT)?;
        seqs.sort_unstable();
        Ok(seqs)
    }

    /// The head cache, or `None` if the ledger holds no events.
    ///
    /// Rebuilds and rewrites `head.json` when the file is missing.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Json`] if `head.json` is malformed and the
    /// errors of [`LedgerStore::rebuild_head`] when it has to rebuild.
    pub fn head(&self) -> Result<Option<Head>> {
        match self.cached_head()? {
            Some(head) => Ok(Some(head)),
            None => self.rebuild_head(),
        }
    }

    /// The head cache as stored, without rebuilding it.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Json`] if `head.json` is malformed.
    pub fn cached_head(&self) -> Result<Option<Head>> {
        let path = self.head_path();
        match fs::read(&path) {
            Ok(bytes) => Ok(Some(
                serde_json::from_slice(&bytes).map_err(json_at(&path))?,
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io_at(&path)(error)),
        }
    }

    /// Rebuilds `head.json` from the event files and writes it back.
    ///
    /// The chain runs from seq 0, so the rebuild takes the contiguous prefix
    /// starting at 0 and ignores anything past a gap.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::MalformedEvent`] if the last event file does
    /// not decode as a `SignedEvent`, and [`StorageError::Io`] on a failed
    /// read or write.
    pub fn rebuild_head(&self) -> Result<Option<Head>> {
        let Some(last) = contiguous_last(&self.sequences()?) else {
            return Ok(None);
        };
        let bytes = self.read_event(last)?;
        let head = Head {
            seq: last,
            event_id: self.event_id_of(last, &bytes)?,
            updated_ms: now_ms(),
        };
        self.write_head(&head)?;
        Ok(Some(head))
    }

    /// Appends a contiguous run of events and renames `head.json` last.
    ///
    /// Every event file is written and fsynced before the head cache moves,
    /// so a crash mid-append leaves a shorter but valid ledger: the events
    /// past the cached head are ignored by every read path until the cache is
    /// rebuilt.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::OutOfOrderAppend`] if the batch does not start
    /// at the ledger's next sequence or skips one,
    /// [`StorageError::EventIdMismatch`] if an event's bytes do not hash to
    /// the id the caller passed, and [`StorageError::Io`] on a failed write.
    pub fn append(&self, events: &[NewEvent<'_>]) -> Result<Option<Head>> {
        if events.is_empty() {
            return self.head();
        }
        let head = self.write_events(events)?;
        self.write_head(&head)?;
        Ok(Some(head))
    }

    /// Writes and fsyncs every event file, leaving `head.json` where it is.
    ///
    /// This is the first half of [`LedgerStore::append`]; a process that dies
    /// here has written valid event files that no read path serves yet.
    fn write_events(&self, events: &[NewEvent<'_>]) -> Result<Head> {
        create_dir(&self.dir)?;
        let expected = self.head()?.map_or(0, |head| head.seq + 1);
        for (offset, event) in events.iter().enumerate() {
            let want = expected + offset as u64;
            if event.seq != want {
                return Err(StorageError::OutOfOrderAppend {
                    ledger: self.ledger,
                    expected: want,
                    got: event.seq,
                });
            }
            let actual = self.event_id_of(event.seq, event.bytes)?;
            if actual != event.event_id {
                return Err(StorageError::EventIdMismatch {
                    seq: event.seq,
                    claimed: event.event_id,
                    actual,
                });
            }
        }
        for event in events {
            write_atomic(&self.event_path(event.seq), event.bytes, DATA_MODE)?;
        }
        let last = events[events.len() - 1];
        Ok(Head {
            seq: last.seq,
            event_id: last.event_id,
            updated_ms: now_ms(),
        })
    }

    /// Reads one event's stored bytes.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::MissingEvent`] if no file holds that sequence.
    pub fn read_event(&self, seq: u64) -> Result<Vec<u8>> {
        let path = self.event_path(seq);
        match fs::read(&path) {
            Ok(bytes) => Ok(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(StorageError::MissingEvent {
                    ledger: self.ledger,
                    seq,
                })
            }
            Err(error) => Err(io_at(&path)(error)),
        }
    }

    /// Reads events from `since` (inclusive) up to the head, at most `limit`.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`LedgerStore::head`] and
    /// [`LedgerStore::read_event`].
    pub fn read_from(&self, since: u64, limit: Option<usize>) -> Result<Vec<StoredEvent>> {
        let Some(head) = self.head()? else {
            return Ok(Vec::new());
        };
        let mut events = Vec::new();
        let mut seq = since;
        while seq <= head.seq {
            if limit.is_some_and(|limit| events.len() >= limit) {
                break;
            }
            events.push(StoredEvent {
                seq,
                bytes: self.read_event(seq)?,
            });
            seq += 1;
        }
        Ok(events)
    }

    /// Reads the whole ledger up to the head.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`LedgerStore::read_from`].
    pub fn read_all(&self) -> Result<Vec<StoredEvent>> {
        self.read_from(0, None)
    }

    /// The provenance record, or `None` if the ledger has none.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Json`] if `meta.json` is malformed.
    pub fn meta(&self) -> Result<Option<LedgerMeta>> {
        let path = self.meta_path();
        match fs::read(&path) {
            Ok(bytes) => Ok(Some(
                serde_json::from_slice(&bytes).map_err(json_at(&path))?,
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io_at(&path)(error)),
        }
    }

    /// Writes the provenance record.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] if the write fails.
    pub fn write_meta(&self, meta: &LedgerMeta) -> Result<()> {
        create_dir(&self.dir)?;
        let path = self.meta_path();
        let bytes = to_json(&path, meta)?;
        write_atomic(&path, &bytes, DATA_MODE)
    }

    /// Records the provenance of a ledger this node has not seen before,
    /// leaving an existing record alone.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`LedgerStore::meta`] and
    /// [`LedgerStore::write_meta`].
    pub fn note_first_seen(&self, source_endpoint: Option<EndpointId>) -> Result<LedgerMeta> {
        if let Some(meta) = self.meta()? {
            return Ok(meta);
        }
        let meta = LedgerMeta {
            source_endpoint,
            first_seen_ms: now_ms(),
        };
        self.write_meta(&meta)?;
        Ok(meta)
    }

    /// Writes `forks/<id>/<seq>-<conflicting_event_id>.fork`.
    ///
    /// `record` is the encoded `ForkRecord`, built by the caller; the file
    /// name carries the conflicting event's id, since the kept event already
    /// lives in the ledger directory.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] if the directory or the file cannot be
    /// written.
    pub fn record_fork(&self, seq: u64, conflicting: EventId, record: &[u8]) -> Result<PathBuf> {
        create_dir(&self.forks_dir)?;
        let path = self.fork_path(seq, conflicting);
        write_atomic(&path, record, DATA_MODE)?;
        Ok(path)
    }

    /// Path of one fork record.
    #[must_use]
    pub fn fork_path(&self, seq: u64, conflicting: EventId) -> PathBuf {
        self.forks_dir
            .join(format!("{seq:0SEQ_DIGITS$}-{conflicting}.{FORK_EXT}"))
    }

    /// Every fork record for this ledger, in sequence order.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] if the directory cannot be listed.
    pub fn forks(&self) -> Result<Vec<ForkFile>> {
        let entries = match fs::read_dir(&self.forks_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(io_at(&self.forks_dir)(error)),
        };
        let mut forks = Vec::new();
        for entry in entries {
            let entry = entry.map_err(io_at(&self.forks_dir))?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some(FORK_EXT) {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let Some((seq, conflicting)) = stem.split_once('-') else {
                continue;
            };
            let (Ok(seq), Ok(conflicting)) = (seq.parse::<u64>(), conflicting.parse::<EventId>())
            else {
                continue;
            };
            forks.push(ForkFile {
                seq,
                conflicting,
                path,
            });
        }
        forks.sort_by_key(|fork| (fork.seq, fork.conflicting));
        Ok(forks)
    }

    fn write_head(&self, head: &Head) -> Result<()> {
        let path = self.head_path();
        let bytes = to_json(&path, head)?;
        write_atomic(&path, &bytes, DATA_MODE)
    }

    fn event_id_of(&self, seq: u64, bytes: &[u8]) -> Result<EventId> {
        let event = SignedEvent::decode(bytes).map_err(|error| StorageError::MalformedEvent {
            path: self.event_path(seq),
            message: error.to_string(),
        })?;
        Ok(event_id(&event.body))
    }
}

fn to_json<T: Serialize>(path: &Path, value: &T) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(json_at(path))?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// The last sequence of the run `0, 1, 2, ...`, or `None` if seq 0 is absent.
fn contiguous_last(seqs: &[u64]) -> Option<u64> {
    let mut last = None;
    for (index, seq) in seqs.iter().enumerate() {
        if *seq != index as u64 {
            break;
        }
        last = Some(*seq);
    }
    last
}

/// Sequences parsed from the file names in `dir` with extension `ext`.
fn read_seqs(dir: &Path, ext: &str) -> Result<Vec<u64>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_at(dir)(error)),
    };
    let mut seqs = Vec::new();
    for entry in entries {
        let path = entry.map_err(io_at(dir))?.path();
        if path.extension().and_then(|found| found.to_str()) != Some(ext) {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if let Ok(seq) = stem.parse::<u64>() {
            seqs.push(seq);
        }
    }
    Ok(seqs)
}

#[cfg(test)]
mod tests {
    use mabel_core::{EventId, IdentityId, event_id};
    use mabel_proto::prost::Message;
    use mabel_proto::v0::SignedEvent;

    use super::{LedgerStore, NewEvent};

    /// A `SignedEvent` whose bytes are what storage must keep verbatim. The
    /// body is not a real `EventBody`; storage never looks inside it.
    pub(super) fn event(seed: u8) -> (EventId, Vec<u8>) {
        let body = vec![seed; 24];
        let signed = SignedEvent {
            body: body.clone(),
            sig: vec![seed; 64],
        };
        (event_id(&body), signed.encode_to_vec())
    }

    fn store(root: &std::path::Path) -> LedgerStore {
        let ledger = IdentityId::from_bytes([1u8; 32]);
        LedgerStore::new(
            ledger,
            root.join("ledgers").join(ledger.to_string()),
            root.join("forks").join(ledger.to_string()),
        )
    }

    #[test]
    fn appends_land_at_zero_padded_paths_in_chain_order() {
        let home = tempfile::tempdir().unwrap();
        let store = store(home.path());
        let events: Vec<_> = (0..3).map(event).collect();
        let batch: Vec<_> = events
            .iter()
            .enumerate()
            .map(|(seq, (id, bytes))| NewEvent {
                seq: seq as u64,
                event_id: *id,
                bytes,
            })
            .collect();
        let head = store.append(&batch).unwrap().expect("a head");

        assert_eq!(head.seq, 2);
        assert_eq!(head.event_id, events[2].0);
        assert!(store.event_path(0).ends_with("000000000000.ev"));
        assert_eq!(store.sequences().unwrap(), vec![0, 1, 2]);

        let mut names: Vec<_> = std::fs::read_dir(store.dir())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                "000000000000.ev",
                "000000000001.ev",
                "000000000002.ev",
                "head.json",
            ]
        );
    }

    #[test]
    fn stored_bytes_come_back_unmodified() {
        let home = tempfile::tempdir().unwrap();
        let store = store(home.path());
        let (id, bytes) = event(9);
        store
            .append(&[NewEvent {
                seq: 0,
                event_id: id,
                bytes: &bytes,
            }])
            .unwrap();
        assert_eq!(store.read_event(0).unwrap(), bytes);
        assert_eq!(std::fs::read(store.event_path(0)).unwrap(), bytes);
        assert_eq!(store.read_all().unwrap()[0].bytes, bytes);
    }

    #[test]
    fn an_append_that_skips_a_sequence_is_refused() {
        let home = tempfile::tempdir().unwrap();
        let store = store(home.path());
        let (id, bytes) = event(0);
        store
            .append(&[NewEvent {
                seq: 0,
                event_id: id,
                bytes: &bytes,
            }])
            .unwrap();

        let (next_id, next_bytes) = event(1);
        let error = store
            .append(&[NewEvent {
                seq: 2,
                event_id: next_id,
                bytes: &next_bytes,
            }])
            .expect_err("seq 2 skips seq 1");
        assert_eq!(error.exit_code(), 50);
        assert!(error.to_string().contains("expects seq 1"), "{error}");
    }

    #[test]
    fn an_event_id_that_does_not_match_the_bytes_is_refused() {
        let home = tempfile::tempdir().unwrap();
        let store = store(home.path());
        let (_, bytes) = event(4);
        let error = store
            .append(&[NewEvent {
                seq: 0,
                event_id: EventId::from_bytes([0u8; 32]),
                bytes: &bytes,
            }])
            .expect_err("the id does not hash the body");
        assert_eq!(error.exit_code(), 10);
        assert!(!store.event_path(0).exists(), "nothing was written");
    }

    #[test]
    fn a_crash_before_the_head_rename_leaves_a_shorter_valid_ledger() {
        let home = tempfile::tempdir().unwrap();
        let store = store(home.path());
        for seq in 0..2u64 {
            let (id, bytes) = event(seq as u8);
            store
                .append(&[NewEvent {
                    seq,
                    event_id: id,
                    bytes: &bytes,
                }])
                .unwrap();
        }

        // The crash: the seq-2 event file lands, the head.json rename does
        // not.
        let (orphan_id, orphan_bytes) = event(2);
        store
            .write_events(&[NewEvent {
                seq: 2,
                event_id: orphan_id,
                bytes: &orphan_bytes,
            }])
            .unwrap();
        assert_eq!(store.cached_head().unwrap().unwrap().seq, 1);

        let head = store.head().unwrap().expect("the cached head survives");
        assert_eq!(head.seq, 1);
        let read = store.read_all().unwrap();
        assert_eq!(read.len(), 2, "the orphan event is not served");
        assert_eq!(store.sequences().unwrap(), vec![0, 1, 2]);

        // The next append overwrites the orphan and moves the head.
        let (id, bytes) = event(7);
        let head = store
            .append(&[NewEvent {
                seq: 2,
                event_id: id,
                bytes: &bytes,
            }])
            .unwrap()
            .unwrap();
        assert_eq!(head.seq, 2);
        assert_eq!(head.event_id, id);
        assert_ne!(id, orphan_id);
        assert_eq!(store.read_event(2).unwrap(), bytes);
        assert_eq!(store.read_all().unwrap().len(), 3);
    }

    /// Names the home the crash child writes into.
    const CRASH_HOME_ENV: &str = "MABEL_TEST_CRASH_HOME";

    /// The child of `a_killed_append_leaves_a_shorter_valid_ledger`: it
    /// appends seq 0, writes the seq-1 event file and dies before the
    /// `head.json` rename.
    #[test]
    #[ignore = "run as a child process by a_killed_append_leaves_a_shorter_valid_ledger"]
    fn crash_child_dies_before_the_head_rename() {
        let Some(root) = std::env::var_os(CRASH_HOME_ENV) else {
            return;
        };
        let store = store(std::path::Path::new(&root));
        let (first, first_bytes) = event(0);
        store
            .append(&[NewEvent {
                seq: 0,
                event_id: first,
                bytes: &first_bytes,
            }])
            .unwrap();
        let (second, second_bytes) = event(1);
        store
            .write_events(&[NewEvent {
                seq: 1,
                event_id: second,
                bytes: &second_bytes,
            }])
            .unwrap();
        std::process::exit(9);
    }

    #[test]
    fn a_killed_append_leaves_a_shorter_valid_ledger() {
        let home = tempfile::tempdir().unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "--ignored",
                "ledger::tests::crash_child_dies_before_the_head_rename",
            ])
            .env(CRASH_HOME_ENV, home.path())
            .output()
            .expect("the child runs")
            .status;
        assert_eq!(
            status.code(),
            Some(9),
            "the child died before the head rename"
        );

        let store = store(home.path());
        let head = store.cached_head().unwrap().expect("head.json survives");
        assert_eq!(head.seq, 0, "the head still names the last committed event");
        assert_eq!(head.event_id, event(0).0);
        assert_eq!(store.sequences().unwrap(), vec![0, 1], "both files landed");
        assert_eq!(store.read_all().unwrap().len(), 1, "one valid event");

        std::fs::remove_file(store.head_path()).unwrap();
        let rebuilt = store.head().unwrap().expect("rebuilt from the files");
        assert_eq!(rebuilt.seq, 1, "the rebuild picks the event up");
        assert_eq!(rebuilt.event_id, event(1).0);
    }

    #[test]
    fn deleting_the_head_cache_rebuilds_it_from_the_event_files() {
        let home = tempfile::tempdir().unwrap();
        let store = store(home.path());
        let batch: Vec<_> = (0..4u64).map(|seq| (seq, event(seq as u8))).collect();
        let events: Vec<_> = batch
            .iter()
            .map(|(seq, (id, bytes))| NewEvent {
                seq: *seq,
                event_id: *id,
                bytes,
            })
            .collect();
        let written = store.append(&events).unwrap().unwrap();

        std::fs::remove_file(store.head_path()).unwrap();
        assert!(store.cached_head().unwrap().is_none());

        let rebuilt = store.head().unwrap().expect("rebuilt from the files");
        assert_eq!(rebuilt.seq, written.seq);
        assert_eq!(rebuilt.event_id, written.event_id);
        assert!(store.head_path().exists(), "the cache is written back");
        assert_eq!(store.cached_head().unwrap().unwrap(), rebuilt);
    }

    #[test]
    fn a_rebuild_stops_at_the_first_gap() {
        let home = tempfile::tempdir().unwrap();
        let store = store(home.path());
        let events: Vec<_> = (0..4u64).map(|seq| event(seq as u8)).collect();
        let batch: Vec<_> = events
            .iter()
            .enumerate()
            .map(|(seq, (id, bytes))| NewEvent {
                seq: seq as u64,
                event_id: *id,
                bytes,
            })
            .collect();
        store.append(&batch).unwrap();

        std::fs::remove_file(store.event_path(2)).unwrap();
        std::fs::remove_file(store.head_path()).unwrap();

        let rebuilt = store.head().unwrap().expect("the prefix survives");
        assert_eq!(rebuilt.seq, 1);
        assert_eq!(rebuilt.event_id, events[1].0);
    }

    #[test]
    fn an_empty_ledger_has_no_head_and_no_events() {
        let home = tempfile::tempdir().unwrap();
        let store = store(home.path());
        assert!(!store.exists());
        assert!(store.head().unwrap().is_none());
        assert!(store.read_all().unwrap().is_empty());
        assert!(store.sequences().unwrap().is_empty());
        assert!(store.forks().unwrap().is_empty());
    }

    #[test]
    fn read_from_is_inclusive_and_honours_a_limit() {
        let home = tempfile::tempdir().unwrap();
        let store = store(home.path());
        let events: Vec<_> = (0..5u64).map(|seq| event(seq as u8)).collect();
        let batch: Vec<_> = events
            .iter()
            .enumerate()
            .map(|(seq, (id, bytes))| NewEvent {
                seq: seq as u64,
                event_id: *id,
                bytes,
            })
            .collect();
        store.append(&batch).unwrap();

        let from_two = store.read_from(2, None).unwrap();
        assert_eq!(
            from_two.iter().map(|e| e.seq).collect::<Vec<_>>(),
            vec![2, 3, 4]
        );
        let capped = store.read_from(1, Some(2)).unwrap();
        assert_eq!(capped.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![1, 2]);
        assert!(store.read_from(9, None).unwrap().is_empty());
    }

    #[test]
    fn a_missing_event_read_names_the_sequence() {
        let home = tempfile::tempdir().unwrap();
        let store = store(home.path());
        let error = store.read_event(3).expect_err("nothing at seq 3");
        assert!(error.to_string().contains("seq 3"), "{error}");
    }

    #[test]
    fn fork_files_are_named_by_the_conflicting_event_id() {
        let home = tempfile::tempdir().unwrap();
        let store = store(home.path());
        let (kept, kept_bytes) = event(1);
        store
            .append(&[NewEvent {
                seq: 0,
                event_id: kept,
                bytes: &kept_bytes,
            }])
            .unwrap();

        let (conflicting, _) = event(2);
        let record = b"encoded ForkRecord".to_vec();
        let path = store.record_fork(0, conflicting, &record).unwrap();

        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(name, format!("000000000000-{conflicting}.fork"));
        assert!(!name.contains(&kept.to_string()), "the kept id is not used");
        assert!(path.starts_with(store.forks_dir()));
        assert_eq!(std::fs::read(&path).unwrap(), record);

        let forks = store.forks().unwrap();
        assert_eq!(forks.len(), 1);
        assert_eq!(forks[0].seq, 0);
        assert_eq!(forks[0].conflicting, conflicting);
    }

    #[test]
    fn provenance_is_recorded_once() {
        let home = tempfile::tempdir().unwrap();
        let store = store(home.path());
        let endpoint = iroh_base::SecretKey::from_bytes(&[5u8; 32]).public();

        assert!(store.meta().unwrap().is_none());
        let first = store.note_first_seen(Some(endpoint)).unwrap();
        assert_eq!(first.source_endpoint, Some(endpoint));
        assert!(first.first_seen_ms > 0);

        let again = store.note_first_seen(None).unwrap();
        assert_eq!(again, first, "an existing record is not overwritten");
        assert_eq!(store.meta().unwrap().unwrap(), first);
    }
}
