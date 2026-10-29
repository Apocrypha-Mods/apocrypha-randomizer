//! Opening archives and reading the files in them.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use aes::Aes128;
use aes::cipher::{BlockDecrypt, KeyInit, generic_array::GenericArray};
use fs_bytes::{Error, Result, format_err};

use crate::bhd::{Bhd, Entry};
use crate::key::ArchiveKey;

/// The largest file read out of an archive. The game's largest is a few
/// hundred megabytes; an entry claiming more than this is a broken header.
pub const MAX_ENTRY: u64 = 2 * 1024 * 1024 * 1024;

/// One `.bhd`/`.bdt` pair.
#[derive(Debug)]
pub struct Archive {
    /// The file stem, e.g. `Data0`.
    pub name: String,
    pub bhd: Bhd,
    bdt: File,
    bdt_len: u64,
}

impl Archive {
    /// Open `<dir>/<name>.bhd` and `.bdt` with whichever of `keys` opens the
    /// header.
    pub fn open(dir: &Path, name: &str, keys: &[ArchiveKey]) -> Result<Self> {
        let bhd_path = dir.join(format!("{name}.bhd"));
        let encrypted = std::fs::read(&bhd_path).map_err(|e| Error::Io(with_path(e, &bhd_path)))?;
        let key = keys
            .iter()
            .find(|k| k.opens(&encrypted))
            .ok_or_else(|| Error::Crypto(format!("none of the exe's keys opens {name}.bhd")))?;
        let bhd = Bhd::read(&key.decrypt(&encrypted)?)?;
        let bdt_path = dir.join(format!("{name}.bdt"));
        let bdt = File::open(&bdt_path).map_err(|e| Error::Io(with_path(e, &bdt_path)))?;
        let bdt_len = bdt.metadata()?.len();
        Self::from_parts(name, bhd, bdt, bdt_len)
    }

    /// An archive from a parsed header and an open data file.
    pub fn from_parts(name: &str, bhd: Bhd, bdt: File, bdt_len: u64) -> Result<Self> {
        for e in &bhd.entries {
            let end = e.offset.checked_add(u64::from(e.padded_size));
            if end.is_none_or(|end| end > bdt_len) {
                return Err(format_err(format!(
                    "{name}: entry {:#018x} lies past the end of the .bdt",
                    e.hash
                )));
            }
            if e.size > e.padded_size {
                return Err(format_err(format!(
                    "{name}: entry {:#018x} is larger than its padded size",
                    e.hash
                )));
            }
        }
        Ok(Archive {
            name: name.to_string(),
            bhd,
            bdt,
            bdt_len,
        })
    }

    /// The size of the `.bdt`.
    pub fn bdt_len(&self) -> u64 {
        self.bdt_len
    }

    /// The file's bytes, decrypted, cut to its size when the header gives one.
    pub fn read(&self, entry: &Entry) -> Result<Vec<u8>> {
        if u64::from(entry.padded_size) > MAX_ENTRY {
            return Err(format_err(format!(
                "entry {:#018x} is implausibly large",
                entry.hash
            )));
        }
        let mut data = vec![0u8; entry.padded_size as usize];
        read_exact_at(&self.bdt, &mut data, entry.offset)?;
        if let Some(aes) = &entry.aes {
            let cipher = Aes128::new(GenericArray::from_slice(&aes.key));
            for r in &aes.ranges {
                let (start, end) = (r.start as usize, r.end as usize);
                if end > data.len() || !(end - start).is_multiple_of(16) {
                    return Err(Error::Crypto(format!(
                        "entry {:#018x}: encrypted range {start}..{end} does not fit its {} bytes",
                        entry.hash,
                        data.len()
                    )));
                }
                let blocks: &mut [GenericArray<u8, _>] = bytemuck_blocks(&mut data[start..end]);
                cipher.decrypt_blocks(blocks);
            }
        }
        data.truncate(entry.effective_size() as usize);
        Ok(data)
    }
}

/// A slice whose length is a multiple of 16, as AES blocks.
fn bytemuck_blocks(b: &mut [u8]) -> &mut [GenericArray<u8, aes::cipher::consts::U16>] {
    debug_assert!(b.len().is_multiple_of(16));
    let n = b.len() / 16;
    // SAFETY: GenericArray<u8, U16> is a transparent [u8; 16], with alignment 1,
    // and the slice holds exactly `n` of them.
    unsafe { std::slice::from_raw_parts_mut(b.as_mut_ptr().cast(), n) }
}

fn with_path(e: std::io::Error, path: &Path) -> std::io::Error {
    std::io::Error::new(e.kind(), format!("{}: {e}", path.display()))
}

#[cfg(unix)]
fn read_exact_at(f: &File, buf: &mut [u8], offset: u64) -> std::io::Result<()> {
    std::os::unix::fs::FileExt::read_exact_at(f, buf, offset)
}

#[cfg(windows)]
fn read_exact_at(f: &File, mut buf: &mut [u8], mut offset: u64) -> std::io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        match f.seek_read(buf, offset) {
            Ok(0) => return Err(std::io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => {
                buf = &mut buf[n..];
                offset += n as u64;
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Where an entry is: which archive, and which of its entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntryRef {
    pub archive: usize,
    pub entry: usize,
}

/// Every archive of a game, opened together, with an index by path hash.
#[derive(Debug)]
pub struct Dvd {
    pub archives: Vec<Archive>,
    /// Hashes filed in more than one archive, with every place they appear.
    /// Empty on Elden Ring 1.17.2.
    pub duplicates: HashMap<u64, Vec<EntryRef>>,
    index: HashMap<u64, EntryRef>,
}

impl Dvd {
    /// Open the named archives in `dir`, with the keys read from `exe`.
    pub fn open(dir: &Path, exe: &Path, names: &[&str]) -> Result<Self> {
        let exe_bytes = std::fs::read(exe).map_err(|e| Error::Io(with_path(e, exe)))?;
        let keys = ArchiveKey::all_in_exe(&exe_bytes)?;
        let archives = names
            .iter()
            .map(|n| Archive::open(dir, n, &keys))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self::from_archives(archives))
    }

    /// Elden Ring's five archives and its exe, from one game folder.
    pub fn open_elden_ring(game_dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = game_dir.into();
        Self::open(
            &dir,
            &dir.join("eldenring.exe"),
            &crate::ELDEN_RING_ARCHIVES,
        )
    }

    pub fn from_archives(archives: Vec<Archive>) -> Self {
        let mut index = HashMap::new();
        let mut duplicates: HashMap<u64, Vec<EntryRef>> = HashMap::new();
        for (a, archive) in archives.iter().enumerate() {
            for (e, entry) in archive.bhd.entries.iter().enumerate() {
                let here = EntryRef {
                    archive: a,
                    entry: e,
                };
                if let Some(first) = index.insert(entry.hash, here) {
                    // The first archive keeps the hash: which one the game
                    // prefers is not known, so the duplicate is reported.
                    index.insert(entry.hash, first);
                    duplicates
                        .entry(entry.hash)
                        .or_insert_with(|| vec![first])
                        .push(here);
                }
            }
        }
        Dvd {
            archives,
            duplicates,
            index,
        }
    }

    /// Every entry of every archive.
    pub fn entries(&self) -> impl Iterator<Item = (EntryRef, &Entry)> {
        self.archives.iter().enumerate().flat_map(|(a, archive)| {
            archive
                .bhd
                .entries
                .iter()
                .enumerate()
                .map(move |(e, entry)| {
                    (
                        EntryRef {
                            archive: a,
                            entry: e,
                        },
                        entry,
                    )
                })
        })
    }

    pub fn len(&self) -> usize {
        self.archives.iter().map(|a| a.bhd.entries.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn entry(&self, r: EntryRef) -> &Entry {
        &self.archives[r.archive].bhd.entries[r.entry]
    }

    pub fn find_hash(&self, hash: u64) -> Option<EntryRef> {
        self.index.get(&hash).copied()
    }

    pub fn find(&self, path: &str) -> Option<EntryRef> {
        self.find_hash(crate::hash_path(path))
    }

    pub fn read(&self, r: EntryRef) -> Result<Vec<u8>> {
        self.archives[r.archive].read(self.entry(r))
    }

    /// A file by path.
    pub fn read_path(&self, path: &str) -> Result<Vec<u8>> {
        let r = self
            .find(path)
            .ok_or_else(|| format_err(format!("{path} is not in any archive")))?;
        self.read(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bhd::tests::{Spec, build};
    use crate::key::tests::TestKey;
    use aes::cipher::BlockEncrypt;
    use std::io::Write;

    /// A two-file archive on disk: one plain file and one with its first block
    /// encrypted, as the game lays them out.
    fn fixture(dir: &Path, key: &TestKey) -> (Vec<u8>, Vec<u8>) {
        let plain_a = b"plain file, no encryption at all".to_vec();
        let mut secret = b"DCX\0 the first sixteen are secret; the rest is not".to_vec();
        secret.resize(64, 0);
        let aes_key = [7u8; 16];
        let mut enc = secret.clone();
        let c = Aes128::new(GenericArray::from_slice(&aes_key));
        c.encrypt_blocks(bytemuck_blocks(&mut enc[..32]));

        let mut bdt = plain_a.clone();
        bdt.resize(64, 0);
        bdt.extend_from_slice(&enc);
        std::fs::File::create(dir.join("Data9.bdt"))
            .unwrap()
            .write_all(&bdt)
            .unwrap();

        let header = build(
            2,
            &[
                Spec {
                    hash: crate::hash_path("/a.txt"),
                    padded_size: 64,
                    size: plain_a.len() as u32,
                    offset: 0,
                    aes: None,
                },
                Spec {
                    hash: crate::hash_path("/b.dcx"),
                    padded_size: 64,
                    size: 0,
                    offset: 64,
                    aes: Some((aes_key, vec![(0, 32), (-1, -1)])),
                },
            ],
        );
        std::fs::write(dir.join("Data9.bhd"), key.encrypt(&header)).unwrap();
        (plain_a, secret)
    }

    #[test]
    fn files_are_found_by_path_and_decrypted() {
        let tmp = tempfile::tempdir().unwrap();
        let key = TestKey::new();
        let (a, b) = fixture(tmp.path(), &key);
        let archive =
            Archive::open(tmp.path(), "Data9", std::slice::from_ref(&key.public)).unwrap();
        let dvd = Dvd::from_archives(vec![archive]);
        assert_eq!(dvd.len(), 2);
        assert!(dvd.duplicates.is_empty());
        assert_eq!(dvd.read_path("/a.txt").unwrap(), a);
        assert_eq!(
            dvd.read_path("B.DCX").unwrap(),
            b,
            "padded size, since the size is 0"
        );
        assert!(dvd.read_path("/c").is_err());
    }

    #[test]
    fn an_archive_no_key_opens_is_refused_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let key = TestKey::new();
        fixture(tmp.path(), &key);
        let err = Archive::open(tmp.path(), "Data9", &[])
            .unwrap_err()
            .to_string();
        assert!(err.contains("Data9.bhd"), "{err}");
    }

    #[test]
    fn an_entry_past_the_end_of_the_data_file_is_refused_on_open() {
        let tmp = tempfile::tempdir().unwrap();
        let key = TestKey::new();
        fixture(tmp.path(), &key);
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(tmp.path().join("Data9.bdt"))
            .unwrap();
        f.set_len(100).unwrap();
        assert!(Archive::open(tmp.path(), "Data9", std::slice::from_ref(&key.public)).is_err());
    }
}
