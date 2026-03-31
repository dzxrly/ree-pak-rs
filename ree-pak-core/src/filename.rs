use std::{collections::HashMap, path::Path};

use nohash::BuildNoHashHasher;
use parking_lot::Mutex;

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
        let iter = file_names.lines().filter_map(|line| {
            if line.starts_with('#') {
                None
            } else {
                Some(line.to_string())
            }
        });

        Self::from_list(iter)
    }

    /// Build a table from a list of UTF-8 path strings.
    ///
    /// Path separators are normalized (`\\` → `/`) before hashing.
    pub fn from_list(file_names: impl IntoIterator<Item = String>) -> Result<Self> {
        let this = Mutex::new(Self::default());
        file_names.into_iter().for_each(|line| {
            let file_name = Utf16LeString::new_from_str(&line.replace('\\', "/"));
            let hash = file_name.hash_mixed();
            this.lock().file_names.insert(hash, file_name);
        });

        Ok(this.into_inner())
    }

    /// Insert one file name into the table.
    pub fn push_str(&mut self, file_name: &str) {
        let file_name = Utf16LeString::new_from_str(&file_name.replace('\\', "/"));
        let hash = file_name.hash_mixed();
        self.file_names.insert(hash, file_name);
    }

    /// Get the file name string by its mixed hash.
    pub fn get_file_name(&self, hash: u64) -> Option<&Utf16LeString> {
        self.file_names.get(&hash)
    }

    fn parse_raw_file_names(bytes: &[u8]) -> Result<String> {
        String::from_utf8(bytes.to_vec()).map_err(|e| PakError::InvalidFileList(Box::new(e)))
    }
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
}
