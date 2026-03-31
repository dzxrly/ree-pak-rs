use std::{collections::HashMap, path::Path, str};

use nohash::BuildNoHashHasher;
use rayon::{iter::ParallelIterator, str::ParallelString};

use crate::{
    error::{PakError, Result},
    utf16_hash::{Utf16HashExt, Utf16LeString},
};

/// A lookup table from entry hash (`u64`) to UTF-16LE file path.
#[derive(Debug, Clone, Default)]
pub struct FileNameTable {
    file_names: HashMap<u64, Utf16LeString, BuildNoHashHasher<u64>>,
}

impl FileNameTable {
    /// Iterate over all `(hash, name)` pairs.
    pub fn file_names(&self) -> impl Iterator<Item = (&u64, &Utf16LeString)> {
        self.file_names.iter()
    }

    /// Load a file list from disk.
    ///
    /// The list file must be plain UTF-8 text.
    pub fn from_list_file<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let content = std::fs::read(path.as_ref())?;
        Self::from_bytes(&content)
    }

    /// Parse a file list from UTF-8 text bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let file_names = Self::parse_raw_file_names(bytes)?;
        let capacity = estimate_entry_capacity(file_names);
        let entries = {
            file_names
                .par_lines()
                .filter(|line| !line.starts_with('#'))
                .map(entry_from_str)
                .collect_vec_list()
        };

        Ok(Self::from_entries(capacity, entries.into_iter().flatten()))
    }

    /// Build a table from a list of UTF-8 path strings.
    ///
    /// Path separators are normalized (`\\` → `/`) before hashing.
    pub fn from_list<S>(file_names: impl IntoIterator<Item = S>) -> Result<Self>
    where
        S: AsRef<str>,
    {
        let file_names = file_names.into_iter();
        let (lower_bound, _) = file_names.size_hint();
        let mut table = Self::with_capacity(lower_bound);

        for line in file_names {
            table.push_str(line.as_ref());
        }

        Ok(table)
    }

    /// Insert one file name into the table.
    pub fn push_str(&mut self, file_name: &str) {
        push_into_map(&mut self.file_names, file_name);
    }

    /// Get the file name string by its mixed hash.
    pub fn get_file_name(&self, hash: u64) -> Option<&Utf16LeString> {
        self.file_names.get(&hash)
    }

    fn with_capacity(capacity: usize) -> Self {
        Self {
            file_names: HashMap::with_capacity_and_hasher(capacity, BuildNoHashHasher::default()),
        }
    }

    fn from_entries(capacity: usize, entries: impl IntoIterator<Item = (u64, Utf16LeString)>) -> Self {
        let mut table = Self::with_capacity(capacity);

        for (hash, file_name) in entries {
            table.file_names.insert(hash, file_name);
        }

        table
    }

    fn parse_raw_file_names(bytes: &[u8]) -> Result<&str> {
        str::from_utf8(bytes).map_err(|e| PakError::InvalidFileList(Box::new(e)))
    }
}

fn push_into_map(file_names: &mut HashMap<u64, Utf16LeString, BuildNoHashHasher<u64>>, file_name: &str) {
    let (hash, file_name) = entry_from_str(file_name);
    file_names.insert(hash, file_name);
}

fn entry_from_str(file_name: &str) -> (u64, Utf16LeString) {
    let file_name = encode_normalized_path(file_name);
    let hash = file_name.hash_mixed();
    (hash, file_name)
}

fn encode_normalized_path(file_name: &str) -> Utf16LeString {
    // Fast path for file lists that are effectively ASCII: normalize separators while
    // widening each UTF-8 byte into one UTF-16 unit. Non-ASCII bytes are preserved as-is,
    // which keeps this path infallible even though the decoded result may be lossy.
    let mut utf16_units = Vec::with_capacity(file_name.len());

    for &byte in file_name.as_bytes() {
        let byte = if byte == b'\\' { b'/' } else { byte };
        utf16_units.push(byte as u16);
    }

    Utf16LeString::from_utf16_units(utf16_units)
}

fn estimate_entry_capacity(file_names: &str) -> usize {
    file_names
        .as_bytes()
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        .saturating_add((!file_names.is_empty()) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_bytes_parses_utf8_list() {
        let table = FileNameTable::from_bytes(b"foo/bar.txt\n# comment\nfoo\\baz.bin\n").unwrap();

        assert_eq!(table.file_names().count(), 2);
        assert!(
            table
                .get_file_name(Utf16LeString::new_from_str("foo/bar.txt").hash_mixed())
                .is_some()
        );
        assert!(
            table
                .get_file_name(Utf16LeString::new_from_str("foo/baz.bin").hash_mixed())
                .is_some()
        );
    }

    #[test]
    fn from_bytes_rejects_non_utf8_bytes() {
        let err = FileNameTable::from_bytes(&[0x28, 0xB5, 0x2F, 0xFD]).unwrap_err();

        assert!(matches!(err, PakError::InvalidFileList(_)));
    }

    #[test]
    fn from_list_accepts_str_slices_without_allocating_lines() {
        let table = FileNameTable::from_list(["foo/bar.txt", "foo\\baz.bin"]).unwrap();

        assert_eq!(table.file_names().count(), 2);
        assert!(
            table
                .get_file_name(Utf16LeString::new_from_str("foo/bar.txt").hash_mixed())
                .is_some()
        );
        assert!(
            table
                .get_file_name(Utf16LeString::new_from_str("foo/baz.bin").hash_mixed())
                .is_some()
        );
    }

    #[test]
    fn from_bytes_accepts_non_ascii_without_panicking() {
        let table = FileNameTable::from_bytes("测试/目录\\文件.bin\n".as_bytes()).unwrap();

        assert_eq!(table.file_names().count(), 1);
        assert!(table.file_names().next().unwrap().1.to_string().is_ok());
    }
}
