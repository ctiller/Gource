//! Binary on-disk persistence for repository history with FNV-1a checksum validation.

use crate::theseus::{ChurnDecayModel, CohortMode};
use crate::{ChangeOp, CommitInput, FileChangeInput, History, HistoryBuilder};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Magic header bytes identifying the binary cache format and version 1.
pub const CACHE_MAGIC: [u8; 8] = *b"GRCHIST\x01";
const CACHE_MAGIC_PREFIX: &[u8; 7] = b"GRCHIST";
const CACHE_VERSION: u8 = 1;

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

/// 64-bit FNV-1a hash algorithm for fast, deterministic cache checksums.
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Errors occurring during binary cache serialization or deserialization.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CacheError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("Unexpected EOF while reading cache")]
    UnexpectedEof,
    #[error("Invalid magic header: expected {expected:?}, found {found:?}")]
    InvalidMagic { expected: [u8; 8], found: [u8; 8] },
    #[error("Unsupported cache version: {0}")]
    UnsupportedVersion(u8),
    #[error("Cache repository key mismatch: expected '{expected}', found '{found}'")]
    KeyMismatch { expected: String, found: String },
    #[error("Checksum mismatch: expected {expected:#x}, calculated {calculated:#x}")]
    ChecksumMismatch { expected: u64, calculated: u64 },
    #[error("Corrupted cache payload: {0}")]
    Corrupted(String),
}

struct ByteWriter {
    buf: Vec<u8>,
}

impl ByteWriter {
    fn new() -> Self {
        Self {
            buf: Vec::with_capacity(4096),
        }
    }

    fn write_u8(&mut self, val: u8) {
        self.buf.push(val);
    }

    fn write_u16(&mut self, val: u16) {
        self.buf.extend_from_slice(&val.to_le_bytes());
    }

    fn write_u32(&mut self, val: u32) {
        self.buf.extend_from_slice(&val.to_le_bytes());
    }

    fn write_u64(&mut self, val: u64) {
        self.buf.extend_from_slice(&val.to_le_bytes());
    }

    fn write_i64(&mut self, val: i64) {
        self.buf.extend_from_slice(&val.to_le_bytes());
    }

    fn write_str(&mut self, s: &str) {
        let bytes = s.as_bytes();
        self.write_u32(bytes.len() as u32);
        self.buf.extend_from_slice(bytes);
    }
}

struct ByteReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> ByteReader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    fn read_bytes(&mut self, count: usize) -> Result<&'a [u8], CacheError> {
        if self.remaining() < count {
            return Err(CacheError::UnexpectedEof);
        }
        let slice = &self.buf[self.pos..self.pos + count];
        self.pos += count;
        Ok(slice)
    }

    fn read_u8(&mut self) -> Result<u8, CacheError> {
        let slice = self.read_bytes(1)?;
        Ok(slice[0])
    }

    fn read_u16(&mut self) -> Result<u16, CacheError> {
        let slice = self.read_bytes(2)?;
        Ok(u16::from_le_bytes([slice[0], slice[1]]))
    }

    fn read_u32(&mut self) -> Result<u32, CacheError> {
        let slice = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
    }

    fn read_u64(&mut self) -> Result<u64, CacheError> {
        let slice = self.read_bytes(8)?;
        Ok(u64::from_le_bytes([
            slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
        ]))
    }

    fn read_i64(&mut self) -> Result<i64, CacheError> {
        let slice = self.read_bytes(8)?;
        Ok(i64::from_le_bytes([
            slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
        ]))
    }

    fn read_str(&mut self) -> Result<&'a str, CacheError> {
        let len = self.read_u32()? as usize;
        let bytes = self.read_bytes(len)?;
        std::str::from_utf8(bytes)
            .map_err(|e| CacheError::Corrupted(format!("invalid UTF-8 string: {e}")))
    }
}

impl History {
    /// Serializes history to a versioned binary byte buffer with an FNV-1a checksum.
    pub fn save_to_bytes(&self, cache_key: &str) -> Vec<u8> {
        let mut w = ByteWriter::new();
        // 1. Magic
        w.buf.extend_from_slice(&CACHE_MAGIC);
        // 2. Cache key
        w.write_str(cache_key);
        // 3. Configurations
        let mode_u8 = match self.cohort_mode {
            CohortMode::Year => 0u8,
            CohortMode::Quarter => 1u8,
            CohortMode::Author => 2u8,
        };
        w.write_u8(mode_u8);
        let model_u8 = match self.decay_model {
            ChurnDecayModel::LifoYoungestFirst => 0u8,
            ChurnDecayModel::Proportional => 1u8,
        };
        w.write_u8(model_u8);

        // 4. Intern tables
        w.write_u32(self.paths.len() as u32);
        for (_, entry) in self.paths.iter() {
            w.write_str(&entry.path);
        }

        w.write_u32(self.users.len() as u32);
        for (_, name) in self.users.iter() {
            w.write_str(name);
        }

        w.write_u32(self.cohorts.len() as u32);
        for (_, label) in self.cohorts.iter() {
            w.write_str(label);
        }

        // 5. Changes
        w.write_u32(self.changes.len() as u32);
        for ch in &self.changes {
            w.write_u32(ch.path.0);
            let op_u8 = match ch.op {
                ChangeOp::Add => 0u8,
                ChangeOp::Modify => 1u8,
                ChangeOp::Delete => 2u8,
            };
            w.write_u8(op_u8);
            w.write_u32(ch.lines_added);
            w.write_u32(ch.lines_removed);
            if let Some(bytes) = ch.byte_size {
                w.write_u8(1);
                w.write_u64(bytes);
            } else {
                w.write_u8(0);
            }
            w.write_u8(if ch.is_binary { 1 } else { 0 });
        }

        // 6. Commits
        w.write_u32(self.commits.len() as u32);
        for c in &self.commits {
            w.write_i64(c.timestamp);
            w.write_u32(c.user.0);
            w.write_u16(c.cohort.0);
            w.write_u32(c.change_start);
            w.write_u32(c.change_len);
        }

        // 7. Checksum over all preceding bytes
        let checksum = fnv1a_64(&w.buf);
        w.write_u64(checksum);

        w.buf
    }

    /// Deserializes a history instance from a binary buffer, checking magic, version, key, and checksum.
    pub fn load_from_bytes(bytes: &[u8], expected_key: &str) -> Result<Self, CacheError> {
        if bytes.len() < 16 {
            return Err(CacheError::UnexpectedEof);
        }

        // Verify trailing 64-bit checksum
        let payload_len = bytes.len() - 8;
        let expected_checksum = u64::from_le_bytes([
            bytes[payload_len],
            bytes[payload_len + 1],
            bytes[payload_len + 2],
            bytes[payload_len + 3],
            bytes[payload_len + 4],
            bytes[payload_len + 5],
            bytes[payload_len + 6],
            bytes[payload_len + 7],
        ]);
        let calculated_checksum = fnv1a_64(&bytes[..payload_len]);
        if expected_checksum != calculated_checksum {
            return Err(CacheError::ChecksumMismatch {
                expected: expected_checksum,
                calculated: calculated_checksum,
            });
        }

        let mut r = ByteReader::new(&bytes[..payload_len]);

        // 1. Magic check
        let magic = r.read_bytes(8)?;
        if &magic[..7] != CACHE_MAGIC_PREFIX {
            let mut found = [0u8; 8];
            found.copy_from_slice(magic);
            return Err(CacheError::InvalidMagic {
                expected: CACHE_MAGIC,
                found,
            });
        }
        let version = magic[7];
        if version != CACHE_VERSION {
            return Err(CacheError::UnsupportedVersion(version));
        }

        // 2. Cache key check
        let key = r.read_str()?;
        if key != expected_key {
            return Err(CacheError::KeyMismatch {
                expected: expected_key.to_string(),
                found: key.to_string(),
            });
        }

        // 3. Configurations
        let mode_u8 = r.read_u8()?;
        let cohort_mode = match mode_u8 {
            0 => CohortMode::Year,
            1 => CohortMode::Quarter,
            2 => CohortMode::Author,
            other => {
                return Err(CacheError::Corrupted(format!(
                    "invalid cohort mode: {other}"
                )));
            }
        };
        let model_u8 = r.read_u8()?;
        let decay_model = match model_u8 {
            0 => ChurnDecayModel::LifoYoungestFirst,
            1 => ChurnDecayModel::Proportional,
            other => {
                return Err(CacheError::Corrupted(format!(
                    "invalid decay model: {other}"
                )));
            }
        };

        // 4. Intern tables
        let mut builder = HistoryBuilder::new(cohort_mode, decay_model);

        let path_count = r.read_u32()? as usize;
        let mut path_strings = Vec::with_capacity(path_count);
        for _ in 0..path_count {
            path_strings.push(r.read_str()?.to_string());
        }

        let user_count = r.read_u32()? as usize;
        let mut user_strings = Vec::with_capacity(user_count);
        for _ in 0..user_count {
            user_strings.push(r.read_str()?.to_string());
        }

        let cohort_count = r.read_u32()? as usize;
        for _ in 0..cohort_count {
            let _ = r.read_str()?;
        }

        // 5. Changes
        let change_count = r.read_u32()? as usize;
        let mut changes = Vec::with_capacity(change_count);
        for _ in 0..change_count {
            let path_idx = r.read_u32()?;
            let op_u8 = r.read_u8()?;
            let op = match op_u8 {
                0 => ChangeOp::Add,
                1 => ChangeOp::Modify,
                2 => ChangeOp::Delete,
                other => return Err(CacheError::Corrupted(format!("invalid change op: {other}"))),
            };
            let lines_added = r.read_u32()?;
            let lines_removed = r.read_u32()?;
            let has_bytes = r.read_u8()?;
            let byte_size = if has_bytes == 1 {
                Some(r.read_u64()?)
            } else {
                None
            };
            let is_binary = r.read_u8()? != 0;

            let path_str = path_strings
                .get(path_idx as usize)
                .ok_or_else(|| CacheError::Corrupted("invalid path index".to_string()))?;

            changes.push(FileChangeInput {
                path: path_str.clone(),
                op,
                lines_added,
                lines_removed,
                byte_size,
                is_binary,
            });
        }

        // 6. Commits
        let commit_count = r.read_u32()? as usize;
        for _ in 0..commit_count {
            let timestamp = r.read_i64()?;
            let user_idx = r.read_u32()?;
            let _cohort_idx = r.read_u16()?;
            let change_start = r.read_u32()? as usize;
            let change_len = r.read_u32()? as usize;

            let username = user_strings
                .get(user_idx as usize)
                .ok_or_else(|| CacheError::Corrupted("invalid user index".to_string()))?;

            let commit_files = if change_len > 0 {
                if change_start + change_len > changes.len() {
                    return Err(CacheError::Corrupted(
                        "change range out of bounds".to_string(),
                    ));
                }
                changes[change_start..change_start + change_len].to_vec()
            } else {
                Vec::new()
            };

            builder.add_commit(CommitInput {
                timestamp,
                username: username.clone(),
                files: commit_files,
            });
        }

        Ok(builder.finish())
    }

    pub fn save_to_path(&self, path: &Path, cache_key: &str) -> Result<(), CacheError> {
        let bytes = self.save_to_bytes(cache_key);
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let temp_filename = format!(
            ".tmp_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let temp_path = parent.join(temp_filename);
        let mut file = File::create(&temp_path)
            .map_err(|e| CacheError::Io(format!("failed to create temp file: {e}")))?;
        if let Err(e) = file.write_all(&bytes) {
            let _ = std::fs::remove_file(&temp_path);
            return Err(CacheError::Io(format!("failed to write cache: {e}")));
        }
        if let Err(e) = file.sync_all() {
            let _ = std::fs::remove_file(&temp_path);
            return Err(CacheError::Io(format!("failed to sync cache: {e}")));
        }
        drop(file);
        std::fs::rename(&temp_path, path).map_err(|e| {
            let _ = std::fs::remove_file(&temp_path);
            CacheError::Io(format!("failed to persist cache file: {e}"))
        })?;
        Ok(())
    }

    /// Loads history from a file path with integrity and key validation.
    pub fn load_from_path(path: &Path, expected_key: &str) -> Result<Self, CacheError> {
        let mut file = File::open(path)
            .map_err(|e| CacheError::Io(format!("failed to open cache file: {e}")))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|e| CacheError::Io(format!("failed to read cache file: {e}")))?;
        Self::load_from_bytes(&bytes, expected_key)
    }
}
