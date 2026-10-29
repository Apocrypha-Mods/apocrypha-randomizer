//! FMG: the game's text tables (item names, captions, place names), one per
//! file inside the `msgbnd` binders.
//!
//! Observed on Elden Ring 1.17.2's `item.msgbnd.dcx` (little-endian):
//!
//! ```text
//! 0x00 u8 0  0x01 u8 0 (little-endian)  0x02 u8 2 (version)  0x03 u8 0
//! 0x04 i32 file size  0x08 u8 1 (UTF-16)  0x09 0 0 0
//! 0x0C i32 group count  0x10 i32 string count  0x14 i32 0xFF
//! 0x18 i64 offset of the string offsets  0x20 i64 0
//! 0x28 groups: i32 first string index, i32 first id, i32 last id, i32 0
//!      string offsets: i64 each, 0 for an id with no text
//!      strings: UTF-16LE with a terminator
//!      zeros to a multiple of 4 (a few of the game's files carry 4 or more
//!      extra; they are kept as read)
//! ```
//!
//! A group covers a run of consecutive ids; string `first index + (id - first
//! id)` belongs to each. The writer rebuilds groups, offsets and strings from
//! the entries, and `apoc-fs check` holds every FMG in the game to coming back
//! byte for byte. Three of the game's FMGs (`GR_NGWord` in zhoCN and
//! `NetworkMessage_dlc01` in two languages) hold leftover bytes between strings
//! that no id points at; those are not rebuilt but returned as read while
//! their entries are unchanged.

use std::collections::BTreeMap;

use fs_bytes::{self as rd, Error, Result, format_err};

const HEADER_LEN: usize = 0x28;
const GROUP_LEN: usize = 16;
/// The longest string accepted, in UTF-16 units. Captions run to a few
/// thousand.
const MAX_UNITS: usize = 1 << 16;

/// Every entry of a table, by id.
type Entries = BTreeMap<i32, Option<String>>;

/// A text table: each id's text, or `None` where the id is listed without any.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Fmg {
    pub entries: Entries,
    /// Zero bytes after the usual padding, as read. A few dozen of the game's
    /// FMGs have them; why is not known.
    extra_padding: usize,
    /// The file as read, with the entries it held, when it carries bytes
    /// between strings that no id points at (3 of the game's 3,927 FMGs).
    /// Returned as is while the entries are unchanged.
    original: Option<Box<(Entries, Vec<u8>)>>,
}

impl Fmg {
    pub fn read(b: &[u8]) -> Result<Self> {
        let unsupported = |what: &str| Error::Unsupported(format!("this FMG {what}"));
        if rd::bytes(b, 0, 4, "FMG header")? != [0, 0, 2, 0] {
            return Err(unsupported("is not a little-endian version 2 table"));
        }
        if rd::i32_le(b, 4, "FMG header")? as i64 != b.len() as i64 {
            return Err(format_err("the FMG's size field does not match its length"));
        }
        if rd::bytes(b, 8, 4, "FMG header")? != [1, 0, 0, 0]
            || rd::i32_le(b, 0x14, "FMG header")? != 0xFF
        {
            return Err(unsupported("has an unknown header"));
        }
        let groups = rd::index(rd::i32_le(b, 0x0C, "FMG header")?.into(), "group count")?;
        let strings = rd::index(rd::i32_le(b, 0x10, "FMG header")?.into(), "string count")?;
        let offsets_at = rd::index(rd::i64_le(b, 0x18, "FMG header")?, "string offsets offset")?;
        if offsets_at != HEADER_LEN + groups.saturating_mul(GROUP_LEN)
            || strings
                .checked_mul(8)
                .is_none_or(|n| offsets_at + n > b.len())
        {
            return Err(format_err("the FMG's tables do not fit it"));
        }
        let mut entries = BTreeMap::new();
        let mut budget = b.len();
        for g in 0..groups {
            let at = HEADER_LEN + g * GROUP_LEN;
            let first = rd::index(rd::i32_le(b, at, "FMG group")?.into(), "group start")?;
            let lo = rd::i32_le(b, at + 4, "FMG group")?;
            let hi = rd::i32_le(b, at + 8, "FMG group")?;
            let span = i64::from(hi) - i64::from(lo);
            if span < 0 || first as i64 + span >= strings as i64 {
                return Err(format_err("an FMG group runs past its strings"));
            }
            for k in 0..=span as usize {
                let id = lo + k as i32;
                let off = rd::index(
                    rd::i64_le(b, offsets_at + (first + k) * 8, "string offset")?,
                    "string offset",
                )?;
                let text = if off == 0 {
                    None
                } else {
                    let units = rd::utf16z(b, off, MAX_UNITS.min(budget), "FMG string")?;
                    budget = budget.saturating_sub(units.len());
                    Some(
                        String::from_utf16(&units)
                            .map_err(|_| format_err("an FMG string is not valid UTF-16"))?,
                    )
                };
                if entries.insert(id, text).is_some() {
                    return Err(format_err(format!("FMG id {id} is listed twice")));
                }
            }
        }
        let mut fmg = Fmg {
            entries,
            extra_padding: 0,
            original: None,
        };
        let canonical = fmg.write()?;
        // `write` sets the size field, so compare past it.
        if b.len() > canonical.len()
            && b[8..canonical.len()] == canonical[8..]
            && b[canonical.len()..].iter().all(|&x| x == 0)
        {
            fmg.extra_padding = b.len() - canonical.len();
        } else if canonical != b {
            fmg.original = Some(Box::new((fmg.entries.clone(), b.to_vec())));
        }
        Ok(fmg)
    }

    pub fn get(&self, id: i32) -> Option<&str> {
        self.entries.get(&id)?.as_deref()
    }

    pub fn write(&self) -> Result<Vec<u8>> {
        if let Some(original) = &self.original
            && original.0 == self.entries
        {
            return Ok(original.1.clone());
        }
        // Groups: runs of consecutive ids.
        let mut groups: Vec<(usize, i32, i32)> = Vec::new();
        for (i, &id) in self.entries.keys().enumerate() {
            match groups.last_mut() {
                Some(g) if g.2.checked_add(1) == Some(id) => g.2 = id,
                _ => groups.push((i, id, id)),
            }
        }
        let n = self.entries.len();
        let offsets_at = HEADER_LEN + groups.len() * GROUP_LEN;
        let mut out = vec![0u8; offsets_at + n * 8];
        out[2] = 2;
        out[8] = 1;
        let to_i32 = |v: usize, what: &str| {
            i32::try_from(v).map_err(|_| format_err(format!("the FMG's {what} is too large")))
        };
        out[0x0C..0x10].copy_from_slice(&to_i32(groups.len(), "group count")?.to_le_bytes());
        out[0x10..0x14].copy_from_slice(&to_i32(n, "string count")?.to_le_bytes());
        out[0x14..0x18].copy_from_slice(&0xFFi32.to_le_bytes());
        out[0x18..0x20].copy_from_slice(&(offsets_at as i64).to_le_bytes());
        for (g, &(first, lo, hi)) in groups.iter().enumerate() {
            let at = HEADER_LEN + g * GROUP_LEN;
            out[at..at + 4].copy_from_slice(&to_i32(first, "string index")?.to_le_bytes());
            out[at + 4..at + 8].copy_from_slice(&lo.to_le_bytes());
            out[at + 8..at + 12].copy_from_slice(&hi.to_le_bytes());
        }
        for (i, text) in self.entries.values().enumerate() {
            let off = match text {
                None => 0,
                Some(t) => {
                    let at = out.len();
                    for u in t.encode_utf16().chain([0]) {
                        out.extend_from_slice(&u.to_le_bytes());
                    }
                    at
                }
            };
            let slot = offsets_at + i * 8;
            out[slot..slot + 8].copy_from_slice(&(off as i64).to_le_bytes());
        }
        // Zeros to a multiple of 4, then any extra zeros the file was read with.
        out.resize(fs_bytes::align_up(out.len(), 4), 0);
        out.resize(out.len() + self.extra_padding, 0);
        let len = to_i32(out.len(), "size")?;
        out[4..8].copy_from_slice(&len.to_le_bytes());
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Fmg {
        let mut f = Fmg::default();
        for (id, t) in [
            (10, Some("Dagger")),
            (11, None),
            (12, Some("Club")),
            (100, Some("")),
            (200, Some("Rune")),
        ] {
            f.entries.insert(id, t.map(str::to_string));
        }
        f
    }

    #[test]
    fn a_table_reads_back_what_was_written_and_a_second_write_is_identical() {
        let bytes = sample().write().unwrap();
        let back = Fmg::read(&bytes).unwrap();
        assert_eq!(back, sample());
        assert_eq!(back.write().unwrap(), bytes);
        assert_eq!(back.get(12), Some("Club"));
        assert_eq!(back.get(11), None);
        assert_eq!(
            u32::from_le_bytes(bytes[0x0C..0x10].try_into().unwrap()),
            3,
            "three runs of ids"
        );
    }

    #[test]
    fn extra_zeros_at_the_end_are_kept() {
        let mut bytes = sample().write().unwrap();
        bytes.extend_from_slice(&[0; 4]);
        let len = bytes.len() as i32;
        bytes[4..8].copy_from_slice(&len.to_le_bytes());
        assert_eq!(Fmg::read(&bytes).unwrap().write().unwrap(), bytes);
    }

    #[test]
    fn unreferenced_bytes_are_returned_as_read_until_an_entry_changes() {
        let bytes = sample().write().unwrap();
        // Move the last string 4 bytes on, leaving leftovers before it.
        let mut odd = bytes.clone();
        let offsets_at = u64::from_le_bytes(odd[0x18..0x20].try_into().unwrap()) as usize;
        let last = offsets_at + 4 * 8;
        let at = u64::from_le_bytes(odd[last..last + 8].try_into().unwrap()) as usize;
        odd.splice(at..at, [0xAB, 0xCD, 0xEF, 0x01]);
        odd[last..last + 8].copy_from_slice(&((at + 4) as u64).to_le_bytes());
        odd.truncate(fs_bytes::align_up(odd.len() - 4, 4) + 4);
        let len = odd.len() as i32;
        odd[4..8].copy_from_slice(&len.to_le_bytes());
        let mut f = Fmg::read(&odd).unwrap();
        assert_eq!(f.get(200), Some("Rune"));
        assert_eq!(f.write().unwrap(), odd, "unchanged, so as read");
        f.entries.insert(10, Some("Knife".into()));
        assert_eq!(
            Fmg::read(&f.write().unwrap()).unwrap().get(10),
            Some("Knife"),
            "changed, so rebuilt"
        );
    }

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
            let _ = Fmg::read(&b);
        }
    }
}
