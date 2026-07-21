//! Persistent Storage engine featuring Write-Ahead Logging (WAL) and Snapshots.
//!
//! Provides durable crash recovery for CRDT states and operations.

use crate::core::ApplyDelta;
use serde::{Serialize, de::DeserializeOwned};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const WAL_MAGIC: &[u8; 8] = b"CRDTWAL1";
const RECORD_TYPE_SNAPSHOT: u8 = 1;
const RECORD_TYPE_DELTA: u8 = 2;

/// A WAL record containing either a full state snapshot or an operational delta.
#[derive(Debug, Clone, PartialEq)]
pub enum WalRecord<T, D> {
    Snapshot(T),
    Delta(D),
}

/// Persistent Write-Ahead Log (WAL) for state recovery and snapshotting.
#[derive(Debug)]
pub struct Wal<T, D> {
    path: PathBuf,
    file: File,
    _phantom: std::marker::PhantomData<(T, D)>,
}

impl<T, D> Wal<T, D>
where
    T: Serialize + DeserializeOwned + Clone,
    D: Serialize + DeserializeOwned + Clone,
{
    /// Open or create a WAL file at the given path.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        let metadata = file.metadata()?;
        if metadata.len() == 0 {
            file.write_all(WAL_MAGIC)?;
            file.flush()?;
        } else {
            let mut magic = [0u8; 8];
            file.read_exact(&mut magic)?;
            if &magic != WAL_MAGIC {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid WAL magic header",
                ));
            }
        }

        Ok(Wal {
            path,
            file,
            _phantom: std::marker::PhantomData,
        })
    }

    /// Append a full CRDT state snapshot to the log.
    pub fn append_snapshot(&mut self, state: &T) -> io::Result<()> {
        let payload = postcard::to_allocvec(state)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        self.write_record(RECORD_TYPE_SNAPSHOT, &payload)
    }

    /// Append an operational delta to the log.
    pub fn append_delta(&mut self, delta: &D) -> io::Result<()> {
        let payload = postcard::to_allocvec(delta)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        self.write_record(RECORD_TYPE_DELTA, &payload)
    }

    fn write_record(&mut self, record_type: u8, payload: &[u8]) -> io::Result<()> {
        self.file.seek(SeekFrom::End(0))?;
        let len = payload.len() as u64;
        self.file.write_all(&len.to_le_bytes())?;
        self.file.write_all(&[record_type])?;
        self.file.write_all(payload)?;
        self.file.flush()?;
        Ok(())
    }

    /// Replay the WAL and recover state by returning the latest snapshot (if any)
    /// and all deltas appended after that snapshot.
    pub fn recover(&mut self) -> io::Result<Option<(T, Vec<D>)>> {
        self.file.seek(SeekFrom::Start(8))?;

        let mut latest_snapshot: Option<T> = None;
        let mut deltas: Vec<D> = Vec::new();

        loop {
            let mut len_bytes = [0u8; 8];
            match self.file.read_exact(&mut len_bytes) {
                Ok(()) => {}
                Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }

            let len = u64::from_le_bytes(len_bytes) as usize;

            let mut type_byte = [0u8; 1];
            if let Err(e) = self.file.read_exact(&mut type_byte) {
                if e.kind() == io::ErrorKind::UnexpectedEof {
                    break;
                }
                return Err(e);
            }

            let mut payload = vec![0u8; len];
            if let Err(e) = self.file.read_exact(&mut payload) {
                if e.kind() == io::ErrorKind::UnexpectedEof {
                    // Truncated trailing write during crash, ignore partial record
                    break;
                }
                return Err(e);
            }

            match type_byte[0] {
                RECORD_TYPE_SNAPSHOT => {
                    let snapshot: T = postcard::from_bytes(&payload)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
                    latest_snapshot = Some(snapshot);
                    deltas.clear(); // Snapshot supersedes previous deltas
                }
                RECORD_TYPE_DELTA => {
                    let delta: D = postcard::from_bytes(&payload)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
                    deltas.push(delta);
                }
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Unknown WAL record type",
                    ));
                }
            }
        }

        if latest_snapshot.is_none() && deltas.is_empty() {
            Ok(None)
        } else {
            Ok(latest_snapshot.map(|s| (s, deltas.clone())).or_else(|| {
                // If there's no snapshot, return None for snapshot and deltas
                None
            }))
        }
    }

    /// Recover state into an initial CRDT state object by applying all deltas after snapshot.
    pub fn replay_into<C>(&mut self, initial: &mut C) -> io::Result<usize>
    where
        C: ApplyDelta<D> + Clone,
        T: Into<C>,
    {
        self.file.seek(SeekFrom::Start(8))?;
        let mut count = 0;

        let mut latest_snapshot: Option<T> = None;
        let mut deltas: Vec<D> = Vec::new();

        loop {
            let mut len_bytes = [0u8; 8];
            match self.file.read_exact(&mut len_bytes) {
                Ok(()) => {}
                Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e),
            }

            let len = u64::from_le_bytes(len_bytes) as usize;

            let mut type_byte = [0u8; 1];
            if let Err(e) = self.file.read_exact(&mut type_byte) {
                if e.kind() == io::ErrorKind::UnexpectedEof {
                    break;
                }
                return Err(e);
            }

            let mut payload = vec![0u8; len];
            if let Err(e) = self.file.read_exact(&mut payload) {
                if e.kind() == io::ErrorKind::UnexpectedEof {
                    break;
                }
                return Err(e);
            }

            match type_byte[0] {
                RECORD_TYPE_SNAPSHOT => {
                    let snapshot: T = postcard::from_bytes(&payload)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
                    latest_snapshot = Some(snapshot);
                    deltas.clear();
                }
                RECORD_TYPE_DELTA => {
                    let delta: D = postcard::from_bytes(&payload)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
                    deltas.push(delta);
                }
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Unknown WAL record type",
                    ));
                }
            }
        }

        if let Some(snap) = latest_snapshot {
            *initial = snap.into();
        }
        for d in deltas {
            initial.apply_delta(d);
            count += 1;
        }

        Ok(count)
    }

    /// Compact the WAL by replacing the log file with a single snapshot of the current state.
    pub fn compact(&mut self, state: &T) -> io::Result<()> {
        let tmp_path = self.path.with_extension("wal.tmp");
        {
            let mut tmp_wal = Wal::<T, D>::open(&tmp_path)?;
            tmp_wal.append_snapshot(state)?;
        }
        std::fs::rename(&tmp_path, &self.path)?;

        self.file = OpenOptions::new().read(true).write(true).open(&self.path)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sets::ORSet;

    #[test]
    fn test_wal_snapshot_and_deltas_recovery() {
        let dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = dir.join(format!("test_wal_{}.log", nanos));

        let mut set = ORSet::<String>::new(1);
        let _d1 = set.add("item1".to_string());
        let _d2 = set.add("item2".to_string());

        {
            let mut wal = Wal::<ORSet<String>, _>::open(&path).unwrap();
            wal.append_snapshot(&set).unwrap();
            let d3 = set.add("item3".to_string());
            wal.append_delta(&d3).unwrap();
        }

        // Re-open WAL and recover
        let mut wal = Wal::<ORSet<String>, _>::open(&path).unwrap();
        let mut recovered_set = ORSet::<String>::new(1);
        wal.replay_into(&mut recovered_set).unwrap();

        assert!(recovered_set.contains(&"item1".to_string()));
        assert!(recovered_set.contains(&"item2".to_string()));
        assert!(recovered_set.contains(&"item3".to_string()));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_wal_compaction() {
        let dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = dir.join(format!("test_wal_compact_{}.log", nanos));

        let mut set = ORSet::<String>::new(1);
        let d1 = set.add("alpha".to_string());

        {
            let mut wal = Wal::<ORSet<String>, _>::open(&path).unwrap();
            wal.append_delta(&d1).unwrap();
            wal.compact(&set).unwrap();
        }

        let mut wal = Wal::<ORSet<String>, _>::open(&path).unwrap();
        let mut recovered = ORSet::<String>::new(1);
        wal.replay_into(&mut recovered).unwrap();
        assert!(recovered.contains(&"alpha".to_string()));

        let _ = std::fs::remove_file(path);
    }
}
