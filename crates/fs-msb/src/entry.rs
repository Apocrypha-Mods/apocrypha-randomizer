//! One entry taken apart into its sections, so that a section can change
//! size (a new name, say) and the entry be laid out again.
//!
//! Found from the game's maps (`fs-tool/tests/real_maps.rs` re-checks it on
//! all 1,347): an entry starts with a fixed header, the same size for every
//! entry of a table, holding the offsets (i64, from the entry's start) of its
//! sections at fixed places, or 0 for a section the entry lacks. The sections
//! follow the header in the order of those places. Text sections (the name,
//! and the model file path of models and parts) are UTF-16 ending in a zero
//! unit; the section after one starts at the next multiple of [`text_align`],
//! and the entry ends padded to 8.

use fs_bytes::{Result, format_err};

/// Where a table's entries hold the offsets of their sections, in the order
/// the sections are stored.
pub fn section_slots(table: &str) -> &'static [usize] {
    match table {
        "MODEL_PARAM_ST" => &[0x00, 0x10],
        "EVENT_PARAM_ST" => &[0x00, 0x18, 0x20, 0x28],
        "POINT_PARAM_ST" => &[0x00, 0x30, 0x38, 0x48, 0x50, 0x58, 0x60],
        "ROUTE_PARAM_ST" => &[0x00],
        "PARTS_PARAM_ST" => &[
            0x00, 0x18, 0x50, 0x58, 0x60, 0x68, 0x70, 0x78, 0x80, 0x88, 0x90, 0x98, 0xA0, 0xA8,
            0xB0, 0xB8,
        ],
        _ => &[0x00],
    }
}

/// The header's size: where the name starts.
pub fn header_len(table: &str) -> usize {
    match table {
        "MODEL_PARAM_ST" => 0x28,
        "EVENT_PARAM_ST" => 0x30,
        "POINT_PARAM_ST" => 0x68,
        "ROUTE_PARAM_ST" => 0x80,
        "PARTS_PARAM_ST" => 0xC0,
        _ => 0x10,
    }
}

/// Which sections are text.
pub fn is_text(table: &str, slot: usize) -> bool {
    slot == 0
        || (slot == 0x10 && table == "MODEL_PARAM_ST")
        || (slot == 0x18 && table == "PARTS_PARAM_ST")
}

/// What the section after a text section is aligned to: a text section
/// straight after another starts where it ends (2).
pub fn text_align(table: &str, next_is_text: bool) -> usize {
    if next_is_text {
        return 2;
    }
    match table {
        "PARTS_PARAM_ST" | "MODEL_PARAM_ST" | "EVENT_PARAM_ST" => 8,
        _ => 4,
    }
}

/// An entry in parts: its header, and each present section's bytes. A text
/// section holds its text and the zero unit ending it; any other section
/// holds everything up to the next section, padding included, so the last
/// one carries the entry's end padding and may gain zeros when moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub table: &'static str,
    pub header: Vec<u8>,
    /// (the slot holding its offset, its bytes), in stored order.
    pub sections: Vec<(usize, Vec<u8>)>,
}

fn slot_value(b: &[u8], slot: usize) -> Result<i64> {
    Ok(i64::from_le_bytes(
        b.get(slot..slot + 8)
            .ok_or_else(|| format_err("an MSB entry is shorter than its header"))?
            .try_into()
            .unwrap(),
    ))
}

fn text_len(b: &[u8], at: usize) -> Result<usize> {
    let mut i = at;
    while i + 1 < b.len() {
        if b[i] == 0 && b[i + 1] == 0 {
            return Ok(i + 2 - at);
        }
        i += 2;
    }
    Err(format_err("an MSB entry's text runs off its end"))
}

impl Entry {
    /// Take an entry of `table` apart.
    pub fn read(table: &str, b: &[u8]) -> Result<Self> {
        let table = crate::TABLES
            .iter()
            .find(|t| **t == table)
            .copied()
            .ok_or_else(|| format_err(format!("no MSB table {table}")))?;
        let head = header_len(table);
        if b.len() < head {
            return Err(format_err("an MSB entry is shorter than its header"));
        }
        let mut at: Vec<(usize, usize)> = Vec::new();
        for &slot in section_slots(table) {
            let v = slot_value(b, slot)?;
            if v == 0 {
                continue;
            }
            let v = usize::try_from(v)
                .ok()
                .filter(|&v| v >= head && v <= b.len())
                .ok_or_else(|| {
                    format_err(format!("{table}: a section offset is outside its entry"))
                })?;
            if at.last().is_some_and(|&(_, last)| v < last) {
                return Err(format_err(format!("{table}: sections are out of order")));
            }
            at.push((slot, v));
        }
        if at.first().is_none_or(|&(slot, v)| slot != 0 || v != head) {
            return Err(format_err(format!(
                "{table}: the name does not follow the header"
            )));
        }
        let mut sections = Vec::with_capacity(at.len());
        for (i, &(slot, start)) in at.iter().enumerate() {
            let stop = at.get(i + 1).map_or(b.len(), |&(_, s)| s);
            let len = if is_text(table, slot) {
                let n = text_len(b, start)?;
                if start + n > stop {
                    return Err(format_err(format!(
                        "{table}: text runs into the next section"
                    )));
                }
                n
            } else {
                stop - start
            };
            sections.push((slot, b[start..start + len].to_vec()));
        }
        Ok(Entry {
            table,
            header: b[..head].to_vec(),
            sections,
        })
    }

    /// Lay the entry out again, every offset set to where its section lands.
    pub fn write(&self) -> Vec<u8> {
        let mut out = self.header.clone();
        for &slot in section_slots(self.table) {
            out[slot..slot + 8].fill(0);
        }
        for (i, (slot, bytes)) in self.sections.iter().enumerate() {
            let here = out.len() as i64;
            out[*slot..*slot + 8].copy_from_slice(&here.to_le_bytes());
            out.extend_from_slice(bytes);
            if is_text(self.table, *slot) {
                let align = match self.sections.get(i + 1) {
                    Some((next, _)) => text_align(self.table, is_text(self.table, *next)),
                    None => 8,
                };
                out.resize(out.len().next_multiple_of(align), 0);
            }
        }
        out.resize(out.len().next_multiple_of(8), 0);
        out
    }

    /// The section a slot points at, if present.
    pub fn section(&self, slot: usize) -> Option<&[u8]> {
        self.sections
            .iter()
            .find(|(s, _)| *s == slot)
            .map(|(_, b)| &b[..])
    }

    /// A text section as a string.
    pub fn text(&self, slot: usize) -> Option<String> {
        let b = self.section(slot)?;
        let units: Vec<u16> = b
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&u| u != 0)
            .collect();
        String::from_utf16(&units).ok()
    }

    pub fn name(&self) -> Option<String> {
        self.text(0)
    }

    /// Replace a text section, at whatever length the new text needs.
    pub fn set_text(&mut self, slot: usize, text: &str) -> Result<()> {
        if !is_text(self.table, slot) {
            return Err(format_err(format!("{} +{slot:#x} is not text", self.table)));
        }
        let mut bytes: Vec<u8> = text
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect();
        if text.contains('\0') {
            return Err(format_err("MSB text cannot hold a zero"));
        }
        let s = self
            .sections
            .iter_mut()
            .find(|(s, _)| *s == slot)
            .ok_or_else(|| format_err(format!("the entry has no section at +{slot:#x}")))?;
        std::mem::swap(&mut s.1, &mut bytes);
        Ok(())
    }

    pub fn set_name(&mut self, name: &str) -> Result<()> {
        self.set_text(0, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Vec<u8> {
        s.encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    /// A point as the game lays one out: name padded to 4, two sections.
    fn point() -> Vec<u8> {
        let mut b = vec![0u8; 0x68];
        b[0x08] = 5;
        b.extend(text("Env_Point000"));
        b.resize(b.len().next_multiple_of(4), 0);
        let a = b.len();
        b.extend([1, 0, 0xE4, 2]);
        let c = b.len();
        b.extend([0xAA; 12]);
        b.resize(b.len().next_multiple_of(8), 0);
        b[0..8].copy_from_slice(&0x68i64.to_le_bytes());
        b[0x30..0x38].copy_from_slice(&(a as i64).to_le_bytes());
        b[0x50..0x58].copy_from_slice(&(c as i64).to_le_bytes());
        b
    }

    #[test]
    fn an_entry_taken_apart_lays_out_the_same() {
        let b = point();
        let e = Entry::read("POINT_PARAM_ST", &b).unwrap();
        assert_eq!(e.name().as_deref(), Some("Env_Point000"));
        assert_eq!(e.sections.len(), 3);
        assert_eq!(e.write(), b);
    }

    #[test]
    fn a_longer_name_moves_the_sections_after_it() {
        let b = point();
        let mut e = Entry::read("POINT_PARAM_ST", &b).unwrap();
        e.set_name("a much longer name than before").unwrap();
        let out = e.write();
        let back = Entry::read("POINT_PARAM_ST", &out).unwrap();
        assert_eq!(
            back.name().as_deref(),
            Some("a much longer name than before")
        );
        assert_eq!(back.section(0x30), e.section(0x30));
        // The last section carries the entry's end padding, which may grow.
        let last = back.section(0x50).unwrap();
        let was = e.section(0x50).unwrap();
        assert!(last.starts_with(was) && last[was.len()..].iter().all(|&b| b == 0));
        assert_eq!(out.len() % 8, 0);
        let at = i64::from_le_bytes(out[0x30..0x38].try_into().unwrap()) as usize;
        assert_eq!(at % 4, 0, "the section after text is aligned");
        assert_eq!(back.write(), out);
    }

    #[test]
    fn a_part_keeps_its_file_path_straight_after_its_name() {
        let mut b = vec![0u8; 0xC0];
        b.extend(text("c1234_9000"));
        let sib = b.len();
        b.extend(text(""));
        b.resize(b.len().next_multiple_of(8), 0);
        let s = b.len();
        b.extend([7; 0x40]);
        b[0..8].copy_from_slice(&0xC0i64.to_le_bytes());
        b[0x18..0x20].copy_from_slice(&(sib as i64).to_le_bytes());
        b[0x60..0x68].copy_from_slice(&(s as i64).to_le_bytes());
        let mut e = Entry::read("PARTS_PARAM_ST", &b).unwrap();
        assert_eq!(e.write(), b);
        e.set_name("c1234_9001x").unwrap();
        let out = e.write();
        let sib2 = i64::from_le_bytes(out[0x18..0x20].try_into().unwrap()) as usize;
        assert_eq!(sib2, 0xC0 + 24, "no gap between name and path");
        assert_eq!(
            Entry::read("PARTS_PARAM_ST", &out).unwrap().section(0x60),
            Some(&[7; 0x40][..])
        );
    }

    #[test]
    fn broken_entries_are_errors() {
        let b = point();
        assert!(Entry::read("POINT_PARAM_ST", &b[..0x40]).is_err());
        let mut bad = b.clone();
        bad[0x30..0x38].copy_from_slice(&0x1000i64.to_le_bytes());
        assert!(Entry::read("POINT_PARAM_ST", &bad).is_err());
        let mut bad = b.clone();
        bad[0x30..0x38].copy_from_slice(&0x70i64.to_le_bytes());
        bad[0x50..0x58].copy_from_slice(&0x6Ai64.to_le_bytes());
        assert!(Entry::read("POINT_PARAM_ST", &bad).is_err());
        assert!(Entry::read("NO_SUCH", &b).is_err());
    }

    #[test]
    fn mutated_entries_never_panic() {
        let base = point();
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..20000 {
            let mut b = base.clone();
            for _ in 0..1 + next() % 6 {
                let i = (next() as usize) % b.len();
                b[i] = next() as u8;
            }
            if next() % 4 == 0 {
                b.truncate((next() as usize) % b.len());
            }
            if let Ok(e) = Entry::read("POINT_PARAM_ST", &b) {
                let _ = e.write();
            }
        }
    }
}
