//! BND4 binders: a header, one entry per file, the names, a hash table of the
//! names, and the files' data.
//!
//! Observed on Elden Ring 1.17.2's archives and on `regulation.bin`
//! (little-endian throughout):
//!
//! ```text
//! 0x00 "BND4"  0x04 u8  0x05 u8  0x06 0 0 0  0x09 u8 0 (little-endian)
//! 0x0A u8 1    0x0B 0   0x0C i32 file count  0x10 i64 0x40 (header size)
//! 0x18 8-byte version string, e.g. "07D7R6"  0x20 i64 entry size
//! 0x28 i64 end of the index (names and hash table included)
//! 0x30 u8 1 (UTF-16 names) or 0 (single-byte)  0x31 u8 format
//! 0x32 u8 4 (hash table present)
//! 0x33 0  0x34 i32 0  0x38 i64 hash table offset
//! ```
//!
//! The format byte lists which fields an entry has, with its bits in reverse
//! order: Elden Ring's `0x74` reversed is `0x2E`, which is ids (0x02), names
//! (0x04 and 0x08) and a second size field (0x20). Every entry is
//!
//! ```text
//! +0x00 u8 flags  +0x01 0 0 0  +0x04 i32 -1  +0x08 i64 size
//! [+ i64 size again]  + u32 data offset (u64 with 0x10)  [+ i32 id]  [+ u32 name offset]
//! ```
//!
//! Names follow the entries in entry order, UTF-16LE with a terminator. Two
//! binders in Data0 (AutoInvadePoint lists, format `0x70`) store them one byte
//! per character instead; theirs are all ASCII, and only ASCII is accepted,
//! since other single-byte text would be Shift-JIS. The hash table follows the
//! names, aligned to 8:
//!
//! ```text
//! i64 offset of the hash records  i32 bucket count  u8 0x10 u8 8 u8 8 u8 0
//! bucket: i32 record count, i32 first record
//! record: u32 hash, i32 entry index
//! ```
//!
//! A name's hash is `h = h * 37 + unit` over 32 bits, of `/` followed by the
//! full stored name, lowercased, with forward slashes. Records are sorted by
//! bucket (`hash % bucket count`) and then by hash. Data starts after the hash
//! table, each file aligned to 16, except that an empty file is not aligned:
//! its offset is wherever the previous file ended.
//!
//! The writer rebuilds names, hash table and offsets from the files rather
//! than keeping the original index. That an unchanged binder comes back byte
//! for byte is the test that this description is complete: it is run over
//! every binder in the game (`apoc-fs check`).

use fs_bytes::{self as rd, Error, Result, align_up, format_err};

const HEADER_LEN: usize = 0x40;
/// The longest file name accepted. The game's run to a few hundred.
const MAX_NAME_UNITS: usize = 2048;
/// Where the hash table starts, relative to the end of the names.
const HASH_TABLE_ALIGN: usize = 8;
const HASH_TABLE_HEADER: [u8; 4] = [0x10, 8, 8, 0];

/// Fields an entry carries, as the format byte lists them once reversed.
pub mod format {
    pub const BIG_ENDIAN: u8 = 0x01;
    pub const IDS: u8 = 0x02;
    pub const NAMES1: u8 = 0x04;
    pub const NAMES2: u8 = 0x08;
    pub const LONG_OFFSETS: u8 = 0x10;
    pub const COMPRESSION: u8 = 0x20;
}

/// The entry size a format implies.
fn entry_len(fmt: u8) -> usize {
    let mut n = 0x10;
    if fmt & format::COMPRESSION != 0 {
        n += 8;
    }
    n += if fmt & format::LONG_OFFSETS != 0 {
        8
    } else {
        4
    };
    if fmt & format::IDS != 0 {
        n += 4;
    }
    if fmt & (format::NAMES1 | format::NAMES2) != 0 {
        n += 4;
    }
    n
}

/// One file in a binder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    /// The entry's flag byte; 0x40 on every file seen so far.
    pub flags: u8,
    /// The id, when the format has ids.
    pub id: Option<i32>,
    /// The full stored name, e.g. `N:\GR\data\INTERROOT_win64\chr\c2010\c2010.flver`.
    pub name: Option<String>,
    pub data: Vec<u8>,
}

impl File {
    /// The name without its directory, e.g. `c2010.flver`.
    pub fn short_name(&self) -> Option<&str> {
        let name = self.name.as_deref()?;
        Some(name.rsplit(['\\', '/']).next().unwrap_or(name))
    }
}

/// The header fields that are not derived from the files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub unk04: u8,
    pub unk05: u8,
    /// The byte at 0x0A; 1 on every binder seen so far.
    pub unk0a: u8,
    pub version: [u8; 8],
    /// The format byte as stored, bits not reversed.
    pub raw_format: u8,
    /// Whether a hash table follows the names (the byte at 0x32 is 4).
    pub hash_table: bool,
    /// Whether names are UTF-16 (the byte at 0x30 is 1) rather than one byte
    /// per character.
    pub unicode: bool,
}

impl Header {
    /// The fields an entry has.
    pub fn format(&self) -> u8 {
        self.raw_format.reverse_bits()
    }

    /// The version string, e.g. `07D7R6`.
    pub fn version_str(&self) -> String {
        String::from_utf8_lossy(&self.version)
            .trim_end_matches('\0')
            .to_string()
    }
}

/// A parsed binder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bnd4 {
    pub header: Header,
    pub files: Vec<File>,
}

/// Whether `b` starts like a BND4.
pub fn is_bnd4(b: &[u8]) -> bool {
    b.starts_with(b"BND4")
}

impl Bnd4 {
    pub fn read(b: &[u8]) -> Result<Self> {
        if rd::bytes(b, 0, 4, "binder header")? != b"BND4" {
            return Err(format_err("not a BND4 binder"));
        }
        let unsupported = |what: &str| Error::Unsupported(format!("this binder {what}"));
        if rd::bytes(b, 6, 3, "binder header")? != [0, 0, 0]
            || rd::u8_at(b, 0x0B, "binder header")? != 0
            || rd::bytes(b, 0x33, 5, "binder header")? != [0; 5]
        {
            return Err(unsupported("has values where zeros are expected"));
        }
        if rd::u8_at(b, 0x09, "binder header")? != 0 {
            return Err(unsupported("is big-endian"));
        }
        let header = Header {
            unk04: rd::u8_at(b, 0x04, "binder header")?,
            unk05: rd::u8_at(b, 0x05, "binder header")?,
            unk0a: rd::u8_at(b, 0x0A, "binder header")?,
            version: rd::bytes(b, 0x18, 8, "binder header")?.try_into().unwrap(),
            raw_format: rd::u8_at(b, 0x31, "binder header")?,
            hash_table: match rd::u8_at(b, 0x32, "binder header")? {
                0 => false,
                4 => true,
                other => return Err(unsupported(&format!("has extension byte {other}"))),
            },
            unicode: match rd::u8_at(b, 0x30, "binder header")? {
                0 => false,
                1 => true,
                other => return Err(unsupported(&format!("has name encoding {other}"))),
            },
        };
        let fmt = header.format();
        if fmt & format::BIG_ENDIAN != 0 {
            return Err(unsupported("has big-endian entries"));
        }
        if rd::i64_le(b, 0x10, "binder header")? != HEADER_LEN as i64 {
            return Err(unsupported("has an unknown header size"));
        }
        let entry_size = rd::index(rd::i64_le(b, 0x20, "binder header")?, "entry size")?;
        if entry_size != entry_len(fmt) {
            return Err(unsupported(&format!(
                "has {entry_size}-byte entries, not the {} its format {:#04x} implies",
                entry_len(fmt),
                header.raw_format
            )));
        }
        let count = rd::index(rd::i32_le(b, 0x0C, "binder header")?.into(), "file count")?;
        // Checked before anything is allocated: a tiny file can claim two
        // billion entries.
        if count
            .checked_mul(entry_size)
            .is_none_or(|n| HEADER_LEN.saturating_add(n) > b.len())
        {
            return Err(format_err("the binder claims more files than it holds"));
        }

        let mut files = Vec::with_capacity(count);
        // Payloads are copied out, so they are held to data order without
        // overlap; otherwise every entry could name one large payload.
        let mut data_end = 0usize;
        // Names likewise: many entries could share one long name.
        let mut name_budget = b.len() / 2 + 1024;
        for i in 0..count {
            let mut at = HEADER_LEN + i * entry_size;
            let flags = rd::u8_at(b, at, "binder entry")?;
            if rd::bytes(b, at + 1, 3, "binder entry")? != [0, 0, 0]
                || rd::i32_le(b, at + 4, "binder entry")? != -1
            {
                return Err(unsupported(
                    "has an entry with unknown values in its padding",
                ));
            }
            let size = rd::index(rd::i64_le(b, at + 8, "binder entry")?, "file size")?;
            at += 0x10;
            if fmt & format::COMPRESSION != 0 {
                if rd::i64_le(b, at, "binder entry")? != size as i64 {
                    return Err(unsupported("has a compressed file inside it"));
                }
                at += 8;
            }
            let offset = if fmt & format::LONG_OFFSETS != 0 {
                let o = rd::index(rd::i64_le(b, at, "binder entry")?, "file offset")?;
                at += 8;
                o
            } else {
                let o = rd::u32_le(b, at, "binder entry")? as usize;
                at += 4;
                o
            };
            let id = if fmt & format::IDS != 0 {
                let id = rd::i32_le(b, at, "binder entry")?;
                at += 4;
                Some(id)
            } else {
                None
            };
            let name = if fmt & (format::NAMES1 | format::NAMES2) != 0 {
                let name_at = rd::u32_le(b, at, "binder entry")? as usize;
                let max = MAX_NAME_UNITS.min(name_budget);
                let name = if header.unicode {
                    let units = rd::utf16z(b, name_at, max, "file name")?;
                    String::from_utf16(&units)
                        .map_err(|_| format_err("a file name is not valid UTF-16"))?
                } else {
                    let bytes = rd::asciiz(b, name_at, max, "file name")?;
                    if !bytes.is_ascii() {
                        return Err(unsupported("has a single-byte name that is not ASCII"));
                    }
                    String::from_utf8(bytes).unwrap()
                };
                name_budget -= name.len();
                Some(name)
            } else {
                None
            };
            if offset < data_end {
                return Err(format_err("the binder's files overlap or are out of order"));
            }
            let data = rd::bytes(b, offset, size, "file data")?.to_vec();
            data_end = offset + size;
            files.push(File {
                flags,
                id,
                name,
                data,
            });
        }
        Ok(Bnd4 { header, files })
    }

    /// Write the binder.
    pub fn write(&self) -> Result<Vec<u8>> {
        let fmt = self.header.format();
        let has_names = fmt & (format::NAMES1 | format::NAMES2) != 0;
        for f in &self.files {
            if has_names != f.name.is_some() || (fmt & format::IDS != 0) != f.id.is_some() {
                return Err(format_err(
                    "a file's id or name does not match the binder's format",
                ));
            }
        }
        let count = self.files.len();
        let entry_size = entry_len(fmt);
        let mut out = vec![0u8; HEADER_LEN + count * entry_size];
        out[0..4].copy_from_slice(b"BND4");
        out[0x04] = self.header.unk04;
        out[0x05] = self.header.unk05;
        out[0x0A] = self.header.unk0a;
        out[0x0C..0x10].copy_from_slice(&to_i32(count, "file count")?.to_le_bytes());
        out[0x10..0x18].copy_from_slice(&(HEADER_LEN as i64).to_le_bytes());
        out[0x18..0x20].copy_from_slice(&self.header.version);
        out[0x20..0x28].copy_from_slice(&(entry_size as i64).to_le_bytes());
        out[0x30] = u8::from(self.header.unicode);
        out[0x31] = self.header.raw_format;
        out[0x32] = if self.header.hash_table { 4 } else { 0 };

        // Names, in entry order.
        let mut name_at = Vec::with_capacity(count);
        for f in &self.files {
            name_at.push(out.len());
            match &f.name {
                Some(name) if self.header.unicode => {
                    for unit in name.encode_utf16().chain([0]) {
                        out.extend_from_slice(&unit.to_le_bytes());
                    }
                }
                Some(name) if name.is_ascii() => {
                    out.extend_from_slice(name.as_bytes());
                    out.push(0);
                }
                Some(_) => return Err(format_err("a single-byte name must be ASCII")),
                None => {}
            }
        }

        if self.header.hash_table {
            if !has_names {
                return Err(format_err(
                    "a binder without names cannot have a hash table",
                ));
            }
            out.resize(align_up(out.len(), HASH_TABLE_ALIGN), 0);
            let table_at = out.len();
            out[0x38..0x40].copy_from_slice(&(table_at as i64).to_le_bytes());
            let names = self
                .files
                .iter()
                .map(|f| f.name.as_deref().unwrap_or_default());
            write_hash_table(&mut out, names)?;
        }
        let index_end = out.len();
        out[0x28..0x30].copy_from_slice(&(index_end as i64).to_le_bytes());

        for (i, f) in self.files.iter().enumerate() {
            // An empty file is not aligned: its offset is wherever the last
            // file ended, and a binder ending in one is not padded.
            if !f.data.is_empty() {
                out.resize(align_up(out.len(), 16), 0);
            }
            let offset = out.len();
            let mut at = HEADER_LEN + i * entry_size;
            out[at] = f.flags;
            out[at + 4..at + 8].copy_from_slice(&(-1i32).to_le_bytes());
            out[at + 8..at + 16].copy_from_slice(&(f.data.len() as i64).to_le_bytes());
            at += 0x10;
            if fmt & format::COMPRESSION != 0 {
                out[at..at + 8].copy_from_slice(&(f.data.len() as i64).to_le_bytes());
                at += 8;
            }
            if fmt & format::LONG_OFFSETS != 0 {
                out[at..at + 8].copy_from_slice(&(offset as u64).to_le_bytes());
                at += 8;
            } else {
                let o = u32::try_from(offset)
                    .map_err(|_| format_err("the binder is too large for its offsets"))?;
                out[at..at + 4].copy_from_slice(&o.to_le_bytes());
                at += 4;
            }
            if let Some(id) = f.id {
                out[at..at + 4].copy_from_slice(&id.to_le_bytes());
                at += 4;
            }
            if has_names {
                let n = u32::try_from(name_at[i])
                    .map_err(|_| format_err("the binder's names lie too far in"))?;
                out[at..at + 4].copy_from_slice(&n.to_le_bytes());
            }
            out.extend_from_slice(&f.data);
        }
        Ok(out)
    }
}

fn to_i32(n: usize, what: &str) -> Result<i32> {
    i32::try_from(n).map_err(|_| format_err(format!("the {what} does not fit the binder")))
}

/// The hash a binder files a name under.
pub fn hash_name(name: &str) -> u32 {
    let prefix = (!name.starts_with(['/', '\\'])).then_some(u16::from(b'/'));
    prefix
        .into_iter()
        .chain(name.encode_utf16())
        .map(|u| match char::from_u32(u32::from(u)) {
            Some('\\') => u16::from(b'/'),
            Some(c) if c.is_ascii_uppercase() => u16::from(c.to_ascii_lowercase() as u8),
            _ => u,
        })
        .fold(0u32, |h, u| h.wrapping_mul(37).wrapping_add(u32::from(u)))
}

/// The bucket count for `files` names: the smallest prime at least
/// `files / 7`.
pub fn bucket_count(files: usize) -> usize {
    (files / 7..).find(|&n| is_prime(n)).unwrap()
}

fn is_prime(n: usize) -> bool {
    n >= 2
        && (2..)
            .take_while(|d| d * d <= n)
            .all(|d| !n.is_multiple_of(d))
}

fn write_hash_table<'a>(out: &mut Vec<u8>, names: impl Iterator<Item = &'a str>) -> Result<()> {
    let mut records: Vec<(u32, usize)> =
        names.enumerate().map(|(i, n)| (hash_name(n), i)).collect();
    let buckets = bucket_count(records.len());
    records.sort_by_key(|&(h, i)| (h as usize % buckets, h, i));
    let table_at = out.len();
    let records_at = table_at + 0x10 + buckets * 8;
    out.extend_from_slice(&(records_at as i64).to_le_bytes());
    out.extend_from_slice(&to_i32(buckets, "bucket count")?.to_le_bytes());
    out.extend_from_slice(&HASH_TABLE_HEADER);
    let mut first = 0;
    for bucket in 0..buckets {
        let n = records[first..]
            .iter()
            .take_while(|&&(h, _)| h as usize % buckets == bucket)
            .count();
        out.extend_from_slice(&to_i32(n, "bucket size")?.to_le_bytes());
        out.extend_from_slice(&to_i32(first, "bucket start")?.to_le_bytes());
        first += n;
    }
    for (h, i) in records {
        out.extend_from_slice(&h.to_le_bytes());
        out.extend_from_slice(&to_i32(i, "file index")?.to_le_bytes());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> Header {
        Header {
            unk04: 0,
            unk05: 0,
            unk0a: 1,
            version: *b"07D7R6\0\0",
            raw_format: 0x74,
            hash_table: true,
            unicode: true,
        }
    }

    fn sample() -> Bnd4 {
        let names = [
            "c2010.flver",
            "c2010.hkx",
            "c2010.hkxpwv",
            "c2010_c.hkx",
            "c2010_c.clm2",
        ];
        Bnd4 {
            header: header(),
            files: names
                .iter()
                .enumerate()
                .map(|(i, n)| File {
                    flags: 0x40,
                    id: Some(200 + i as i32),
                    name: Some(format!(r"N:\GR\data\INTERROOT_win64\chr\c2010\{n}")),
                    data: vec![i as u8; 10 + i * 7],
                })
                .collect(),
        }
    }

    #[test]
    fn a_binder_reads_back_what_was_written_and_a_second_write_is_identical() {
        let bytes = sample().write().unwrap();
        let back = Bnd4::read(&bytes).unwrap();
        assert_eq!(back, sample());
        assert_eq!(back.write().unwrap(), bytes);
        assert_eq!(back.files[1].short_name(), Some("c2010.hkx"));
        assert_eq!(back.header.version_str(), "07D7R6");
    }

    /// The five hashes and their order as the game's `c2010.chrbnd.dcx` stores
    /// them: two buckets, each sorted by hash.
    #[test]
    fn the_hash_table_matches_the_one_the_game_ships_for_c2010() {
        let bytes = sample().write().unwrap();
        let table = u64::from_le_bytes(bytes[0x38..0x40].try_into().unwrap()) as usize;
        assert_eq!(&bytes[table + 8..table + 16], &[2, 0, 0, 0, 0x10, 8, 8, 0]);
        let records_at = u64::from_le_bytes(bytes[table..table + 8].try_into().unwrap()) as usize;
        let records: Vec<(u32, i32)> = (0..5)
            .map(|i| {
                let r = records_at + i * 8;
                (
                    u32::from_le_bytes(bytes[r..r + 4].try_into().unwrap()),
                    i32::from_le_bytes(bytes[r + 4..r + 8].try_into().unwrap()),
                )
            })
            .collect();
        assert_eq!(
            records,
            vec![
                (3262248816, 2),
                (3966678040, 4),
                (571535121, 3),
                (1477430431, 0),
                (3979181635, 1)
            ]
        );
    }

    #[test]
    fn data_is_aligned_to_sixteen_and_the_index_end_is_recorded() {
        let bytes = sample().write().unwrap();
        let back = Bnd4::read(&bytes).unwrap();
        let entry = 0x24;
        for i in 0..back.files.len() {
            let off = u32::from_le_bytes(bytes[0x40 + i * entry + 0x18..][..4].try_into().unwrap());
            assert_eq!(off % 16, 0);
        }
        let index_end = u64::from_le_bytes(bytes[0x28..0x30].try_into().unwrap()) as usize;
        assert!(
            index_end <= u32::from_le_bytes(bytes[0x40 + 0x18..][..4].try_into().unwrap()) as usize
        );
    }

    /// As the game's binders lay them out: an empty file sits where the last
    /// one ended, unaligned, and the files after it are aligned again.
    #[test]
    fn an_empty_file_is_not_aligned() {
        let mut b = sample();
        b.files[2].data.clear();
        let bytes = b.write().unwrap();
        let offset = |i: usize| {
            u32::from_le_bytes(bytes[0x40 + i * 0x24 + 0x18..][..4].try_into().unwrap()) as usize
        };
        assert_eq!(offset(2), offset(1) + b.files[1].data.len());
        assert_eq!(offset(3) % 16, 0);
        assert_eq!(Bnd4::read(&bytes).unwrap(), b);

        b.files[4].data.clear();
        let bytes = b.write().unwrap();
        let end = u32::from_le_bytes(bytes[0x40 + 3 * 0x24 + 0x18..][..4].try_into().unwrap())
            as usize
            + b.files[3].data.len();
        assert_eq!(bytes.len(), end, "a trailing empty file adds no padding");
    }

    #[test]
    fn bucket_counts_are_the_next_prime_from_a_seventh() {
        assert_eq!(bucket_count(0), 2);
        assert_eq!(bucket_count(5), 2);
        assert_eq!(bucket_count(26), 3);
        assert_eq!(bucket_count(7 * 24), 29);
    }

    #[test]
    fn the_name_hash_ignores_case_and_slash_direction() {
        assert_eq!(hash_name(r"N:\GR\A.txt"), hash_name("/n:/gr/a.TXT"));
        assert_ne!(hash_name("a"), hash_name("b"));
    }

    #[test]
    fn a_binder_without_names_or_hash_table_round_trips() {
        let mut b = sample();
        b.header.raw_format = (format::IDS | format::COMPRESSION).reverse_bits();
        b.header.hash_table = false;
        for f in &mut b.files {
            f.name = None;
        }
        let bytes = b.write().unwrap();
        assert_eq!(Bnd4::read(&bytes).unwrap(), b);
    }

    /// As the AutoInvadePoint lists in Data0 store them: format 0x70, names
    /// one byte per character.
    #[test]
    fn a_binder_with_single_byte_names_round_trips() {
        let mut b = sample();
        b.header.unicode = false;
        b.header.raw_format = 0x70;
        let bytes = b.write().unwrap();
        assert_eq!(Bnd4::read(&bytes).unwrap(), b);
        assert_eq!(bytes[0x30], 0);

        b.files[0].name = Some("ｍ".into());
        assert!(
            b.write().is_err(),
            "only ASCII is written one byte per character"
        );
    }

    #[test]
    fn a_file_that_does_not_match_the_format_is_refused_on_write() {
        let mut b = sample();
        b.files[0].id = None;
        assert!(b.write().is_err());
    }

    #[test]
    fn an_entry_size_that_disagrees_with_the_format_is_refused() {
        let mut bytes = sample().write().unwrap();
        bytes[0x20] = 0x20;
        assert!(Bnd4::read(&bytes).is_err());
    }

    #[test]
    fn overlapping_payloads_and_impossible_counts_are_refused() {
        let mut bytes = sample().write().unwrap();
        let first = bytes[0x40 + 0x18..0x40 + 0x1C].to_vec();
        bytes[0x40 + 0x24 + 0x18..0x40 + 0x24 + 0x1C].copy_from_slice(&first);
        assert!(Bnd4::read(&bytes).is_err());

        let mut bytes = sample().write().unwrap();
        bytes[0x0C..0x10].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(Bnd4::read(&bytes).is_err());
    }

    #[test]
    fn every_truncation_is_an_error_and_never_a_panic() {
        let bytes = sample().write().unwrap();
        for len in 0..bytes.len() {
            let _ = Bnd4::read(&bytes[..len]);
        }
        let last = bytes.len() - 1;
        assert!(Bnd4::read(&bytes[..last]).is_err());
    }
    /// Random damage to a valid file: every outcome must be a value or an
    /// error, never a panic. A small xorshift keeps it repeatable without a
    /// dependency.
    #[test]
    fn mutated_files_never_panic() {
        let base = sample().write().unwrap();
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
            let _ = Bnd4::read(&b);
        }
    }
}
