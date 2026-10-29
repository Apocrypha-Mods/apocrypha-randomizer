//! The decrypted header: a `BHD5` table of buckets of file entries.
//!
//! Observed on all five Elden Ring archives (little-endian throughout):
//!
//! ```text
//! 0x00 "BHD5"  0x04 i8 -1 (little-endian)  0x05 u8 1, or 0 for DLC  0x06 u16 0
//! 0x08 i32 1   0x0C i32 table size  0x10 i32 bucket count  0x14 i32 bucket offset
//! 0x18 i32 salt length  0x1C salt (ASCII, e.g. "GR_o…")
//!
//! bucket: i32 entry count, i32 entry offset
//! entry (0x28):
//!   +0x00 u64 path hash   +0x08 u32 padded size   +0x0C u32 size (0 for DCX files)
//!   +0x10 i64 offset in the .bdt   +0x18 i64 hash-record offset (0 if none)
//!   +0x20 i64 AES-record offset (0 if none)
//! AES record: 16-byte key, i32 range count, then (i64 start, i64 end) per range
//! hash record: 32-byte digest, i32 range count, then ranges as above
//! ```
//!
//! The last entry of every archive ends exactly at the end of its `.bdt`, which
//! is the check that the entry layout is right.

use fs_bytes::{self as rd, Result, format_err};

const ENTRY_LEN: usize = 0x28;
/// The most ranges one record may list. The game's list at most five.
const MAX_RANGES: usize = 64;

/// A byte range of a file, relative to its start.
pub type Range = std::ops::Range<u64>;

/// The AES-128 key and the ranges it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AesRecord {
    pub key: [u8; 16],
    /// The ranges that are encrypted, with the game's `(-1, -1)` and empty
    /// placeholders left out.
    pub ranges: Vec<Range>,
}

/// A digest of some of a file's bytes, kept but not yet checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashRecord {
    pub digest: [u8; 32],
    pub ranges: Vec<Range>,
}

/// One file in an archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub hash: u64,
    /// Bytes the file occupies in the `.bdt`, including padding.
    pub padded_size: u32,
    /// The file's own size, or 0 when the header leaves it out (DCX files,
    /// whose size is in their own header).
    pub size: u32,
    pub offset: u64,
    pub aes: Option<AesRecord>,
    pub digest: Option<HashRecord>,
}

impl Entry {
    /// How many bytes of the padded block are the file.
    pub fn effective_size(&self) -> u32 {
        if self.size != 0 {
            self.size
        } else {
            self.padded_size
        }
    }
}

/// A parsed header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bhd {
    /// The byte at 0x05: 1 for `Data0`–`Data3`, 0 for `DLC`. Meaning unknown.
    pub flag: u8,
    pub salt: String,
    /// Entries in table order, each with the bucket it was filed in.
    pub entries: Vec<Entry>,
    pub bucket_count: usize,
}

impl Bhd {
    pub fn read(b: &[u8]) -> Result<Self> {
        if rd::bytes(b, 0, 4, "archive header")? != b"BHD5" {
            return Err(format_err("not a BHD5 archive header"));
        }
        if rd::u8_at(b, 4, "archive header")? != 0xff {
            return Err(format_err("big-endian archive headers are not supported"));
        }
        let flag = rd::u8_at(b, 5, "archive header")?;
        let table_size = rd::index(rd::i32_le(b, 0x0C, "archive header")?.into(), "table size")?;
        if table_size > b.len() {
            return Err(format_err(
                "the archive header claims to be larger than it is",
            ));
        }
        let b = &b[..table_size];
        let bucket_count = rd::index(
            rd::i32_le(b, 0x10, "archive header")?.into(),
            "bucket count",
        )?;
        let buckets_at = rd::index(
            rd::i32_le(b, 0x14, "archive header")?.into(),
            "bucket offset",
        )?;
        let salt_len = rd::index(rd::i32_le(b, 0x18, "archive header")?.into(), "salt length")?;
        let salt = rd::bytes(b, 0x1C, salt_len, "salt")?;
        let salt =
            String::from_utf8(salt.to_vec()).map_err(|_| format_err("the salt is not text"))?;
        // Checked before anything is allocated: a bucket is 8 bytes.
        if bucket_count
            .checked_mul(8)
            .is_none_or(|n| buckets_at.saturating_add(n) > b.len())
        {
            return Err(format_err(
                "the archive header claims more buckets than it holds",
            ));
        }

        let mut entries = Vec::new();
        for bucket in 0..bucket_count {
            let at = buckets_at + bucket * 8;
            let count = rd::index(rd::i32_le(b, at, "bucket")?.into(), "bucket size")?;
            let first = rd::index(rd::i32_le(b, at + 4, "bucket")?.into(), "bucket offset")?;
            if count
                .checked_mul(ENTRY_LEN)
                .is_none_or(|n| first.saturating_add(n) > b.len())
            {
                return Err(format_err(
                    "a bucket claims more entries than the header holds",
                ));
            }
            for i in 0..count {
                let e = first + i * ENTRY_LEN;
                let hash = rd::u64_le(b, e, "entry")?;
                if hash % bucket_count as u64 != bucket as u64 {
                    return Err(format_err(format!(
                        "entry {hash:#018x} is filed in bucket {bucket}, not the one its hash names"
                    )));
                }
                let offset = rd::i64_le(b, e + 0x10, "entry")?;
                let offset = u64::try_from(offset)
                    .map_err(|_| format_err("an entry's offset is negative"))?;
                let digest_at = rd::index(rd::i64_le(b, e + 0x18, "entry")?, "hash record offset")?;
                let aes_at = rd::index(rd::i64_le(b, e + 0x20, "entry")?, "AES record offset")?;
                let aes = match aes_at {
                    0 => None,
                    at => {
                        let key = rd::bytes(b, at, 16, "AES record")?.try_into().unwrap();
                        let ranges = ranges(b, at + 16)?;
                        Some(AesRecord { key, ranges })
                    }
                };
                let digest = match digest_at {
                    0 => None,
                    at => {
                        let digest = rd::bytes(b, at, 32, "hash record")?.try_into().unwrap();
                        let ranges = ranges(b, at + 32)?;
                        Some(HashRecord { digest, ranges })
                    }
                };
                entries.push(Entry {
                    hash,
                    padded_size: rd::u32_le(b, e + 0x08, "entry")?,
                    size: rd::u32_le(b, e + 0x0C, "entry")?,
                    offset,
                    aes,
                    digest,
                });
            }
        }
        Ok(Bhd {
            flag,
            salt,
            entries,
            bucket_count,
        })
    }
}

/// A range count and its ranges, without the placeholders.
fn ranges(b: &[u8], at: usize) -> Result<Vec<Range>> {
    let count = rd::index(rd::i32_le(b, at, "range list")?.into(), "range count")?;
    if count > MAX_RANGES {
        return Err(format_err(format!("a record lists {count} ranges")));
    }
    let mut out = Vec::new();
    for i in 0..count {
        let r = at + 4 + i * 16;
        let start = rd::i64_le(b, r, "range")?;
        let end = rd::i64_le(b, r + 8, "range")?;
        match (start, end) {
            (-1, -1) => {}
            (s, e) if s == e => {}
            (s, e) if 0 <= s && s < e => out.push(s as u64..e as u64),
            (s, e) => return Err(format_err(format!("a record lists the range {s}..{e}"))),
        }
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// An AES key and the raw `(start, end)` ranges a record lists.
    pub(crate) type RawAes = ([u8; 16], Vec<(i64, i64)>);

    /// One file for [`build`]: hash, padded size, size, offset, AES key and
    /// ranges.
    pub(crate) struct Spec {
        pub hash: u64,
        pub padded_size: u32,
        pub size: u32,
        pub offset: u64,
        pub aes: Option<RawAes>,
    }

    /// A plaintext header laid out the way the game's are.
    pub(crate) fn build(bucket_count: usize, files: &[Spec]) -> Vec<u8> {
        let salt = b"GR_test";
        let buckets_at = 0x1C + salt.len();
        let entries_at = buckets_at + bucket_count * 8;
        let mut by_bucket: Vec<Vec<&Spec>> = vec![Vec::new(); bucket_count];
        for f in files {
            by_bucket[(f.hash % bucket_count as u64) as usize].push(f);
        }
        let records_at = entries_at + files.len() * ENTRY_LEN;
        let mut b = vec![0u8; records_at];
        b[0..4].copy_from_slice(b"BHD5");
        b[4] = 0xff;
        b[5] = 1;
        b[8..12].copy_from_slice(&1i32.to_le_bytes());
        b[0x10..0x14].copy_from_slice(&(bucket_count as i32).to_le_bytes());
        b[0x14..0x18].copy_from_slice(&(buckets_at as i32).to_le_bytes());
        b[0x18..0x1C].copy_from_slice(&(salt.len() as i32).to_le_bytes());
        b[0x1C..0x1C + salt.len()].copy_from_slice(salt);
        let mut e = entries_at;
        for (i, bucket) in by_bucket.iter().enumerate() {
            let at = buckets_at + i * 8;
            b[at..at + 4].copy_from_slice(&(bucket.len() as i32).to_le_bytes());
            b[at + 4..at + 8].copy_from_slice(&(e as i32).to_le_bytes());
            for f in bucket {
                b[e..e + 8].copy_from_slice(&f.hash.to_le_bytes());
                b[e + 8..e + 12].copy_from_slice(&f.padded_size.to_le_bytes());
                b[e + 12..e + 16].copy_from_slice(&f.size.to_le_bytes());
                b[e + 16..e + 24].copy_from_slice(&(f.offset as i64).to_le_bytes());
                if let Some((key, ranges)) = &f.aes {
                    let at = b.len();
                    b[e + 32..e + 40].copy_from_slice(&(at as i64).to_le_bytes());
                    b.extend_from_slice(key);
                    b.extend_from_slice(&(ranges.len() as i32).to_le_bytes());
                    for (s, end) in ranges {
                        b.extend_from_slice(&s.to_le_bytes());
                        b.extend_from_slice(&end.to_le_bytes());
                    }
                }
                e += ENTRY_LEN;
            }
        }
        let len = b.len() as i32;
        b[0x0C..0x10].copy_from_slice(&len.to_le_bytes());
        b
    }

    fn spec(hash: u64) -> Spec {
        Spec {
            hash,
            padded_size: 32,
            size: 20,
            offset: hash * 32,
            aes: None,
        }
    }

    #[test]
    fn entries_are_read_from_every_bucket_with_their_records() {
        let mut files = vec![spec(1), spec(2), spec(7)];
        files[1].aes = Some(([9; 16], vec![(0, 16), (-1, -1), (16, 16)]));
        let bhd = Bhd::read(&build(3, &files)).unwrap();
        assert_eq!(bhd.salt, "GR_test");
        assert_eq!(bhd.bucket_count, 3);
        let mut hashes: Vec<u64> = bhd.entries.iter().map(|e| e.hash).collect();
        hashes.sort();
        assert_eq!(hashes, vec![1, 2, 7]);
        let two = bhd.entries.iter().find(|e| e.hash == 2).unwrap();
        assert_eq!(
            two.aes,
            Some(AesRecord {
                key: [9; 16],
                ranges: std::iter::once(0..16).collect()
            })
        );
        assert_eq!(two.effective_size(), 20);
    }

    #[test]
    fn an_entry_in_the_wrong_bucket_is_refused() {
        let mut b = build(3, &[spec(1)]);
        // Move the entry's hash so it no longer belongs in bucket 1.
        let entries_at = 0x1C + 7 + 3 * 8;
        b[entries_at..entries_at + 8].copy_from_slice(&2u64.to_le_bytes());
        assert!(Bhd::read(&b).is_err());
    }

    #[test]
    fn impossible_counts_are_refused_before_allocating() {
        let mut b = build(3, &[spec(1)]);
        b[0x10..0x14].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(Bhd::read(&b).is_err());

        let mut b = build(3, &[spec(1)]);
        let bucket1 = 0x1C + 7 + 8;
        b[bucket1..bucket1 + 4].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(Bhd::read(&b).is_err());
    }

    #[test]
    fn a_backwards_range_is_refused() {
        let mut files = vec![spec(1)];
        files[0].aes = Some(([0; 16], vec![(32, 16)]));
        assert!(Bhd::read(&build(2, &files)).is_err());
    }

    #[test]
    fn every_truncation_is_an_error_and_never_a_panic() {
        let mut files = vec![spec(1), spec(4)];
        files[0].aes = Some(([1; 16], vec![(0, 16)]));
        let b = build(2, &files);
        for len in 0..b.len() {
            let mut cut = b[..len].to_vec();
            if cut.len() >= 0x10 {
                cut[0x0C..0x10].copy_from_slice(&(len as i32).to_le_bytes());
            }
            assert!(Bhd::read(&cut).is_err(), "cut at {len}");
        }
    }
    /// Random damage to a valid file: every outcome must be a value or an
    /// error, never a panic. A small xorshift keeps it repeatable without a
    /// dependency.
    #[test]
    fn mutated_files_never_panic() {
        let base = {
            let mut files = vec![spec(1), spec(4), spec(9)];
            files[0].aes = Some(([1; 16], vec![(0, 16), (-1, -1)]));
            build(3, &files)
        };
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..20000 {
            let mut b = base.clone();
            for _ in 0..1 + next() % 8 {
                let i = (next() as usize) % b.len();
                b[i] = next() as u8;
            }
            if next() % 4 == 0 {
                b.truncate((next() as usize) % b.len());
            }
            let _ = Bhd::read(&b);
        }
    }
}
