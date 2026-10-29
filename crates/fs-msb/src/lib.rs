//! MSB: a map's placements. Six tables, in order: models, events, points
//! (regions), routes, layers and parts (map pieces, enemies, assets, …).
//!
//! Observed on Elden Ring 1.17.2's `map/mapstudio/*.msb.dcx` (little-endian,
//! 64-bit offsets):
//!
//! ```text
//! 0x00 "MSB "  0x04 i32 1  0x08 i32 0x10 (header size)  0x0C 0 0 1 0xFF
//! 0x10 tables, each: i32 version (74), i32 entry count + 1,
//!      i64 offset of the table's name, i64 per entry, i64 next table (0
//!      after the last); then the name (UTF-16), padded to 8; then entries.
//! ```
//!
//! Every offset inside an entry is relative to the entry's own start (a
//! model's name at +0x28, a part's at +0xC0), so an entry is a self-contained
//! blob. This crate keeps entries as blobs, each running to where the next one
//! starts (its padding included), and rebuilds the tables around them; typed
//! fields are read from the blobs at offsets worked out from the data.
//! `apoc-fs check` holds every MSB in the game to coming back byte for byte.

use fs_bytes::{self as rd, Error, Result, align_up, format_err};

const HEADER: &[u8; 0x10] = b"MSB \x01\0\0\0\x10\0\0\0\0\0\x01\xff";
/// The tables, in the order every map stores them.
pub const TABLES: [&str; 6] = [
    "MODEL_PARAM_ST",
    "EVENT_PARAM_ST",
    "POINT_PARAM_ST",
    "ROUTE_PARAM_ST",
    "LAYER_PARAM_ST",
    "PARTS_PARAM_ST",
];
/// The longest table name accepted, in UTF-16 units.
const MAX_NAME: usize = 64;

/// One table: its version, its name, and its entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub version: i32,
    pub name: String,
    pub entries: Vec<Vec<u8>>,
}

/// A parsed map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Msb {
    pub tables: Vec<Table>,
}

/// Whether `b` starts like an MSB.
pub fn is_msb(b: &[u8]) -> bool {
    b.starts_with(b"MSB ")
}

impl Msb {
    pub fn read(b: &[u8]) -> Result<Self> {
        if rd::bytes(b, 0, 0x10, "MSB header")? != HEADER {
            return Err(Error::Unsupported(
                "this MSB header is not the little-endian 64-bit one".into(),
            ));
        }
        let mut tables = Vec::new();
        let mut at = 0x10usize;
        loop {
            let version = rd::i32_le(b, at, "MSB table")?;
            let count = rd::index(
                rd::i32_le(b, at + 4, "MSB table")?.into(),
                "MSB table count",
            )?;
            if count == 0 || count.checked_mul(8).is_none_or(|n| at + 0x10 + n > b.len()) {
                return Err(format_err(
                    "an MSB table claims more entries than the file holds",
                ));
            }
            let name_at = rd::index(rd::i64_le(b, at + 8, "MSB table")?, "MSB table name")?;
            let units = rd::utf16z(b, name_at, MAX_NAME, "MSB table name")?;
            let name = String::from_utf16(&units)
                .map_err(|_| format_err("an MSB table name is not UTF-16"))?;
            let offsets: Vec<usize> = (0..count)
                .map(|i| {
                    rd::i64_le(b, at + 0x10 + i * 8, "MSB table offset")
                        .and_then(|o| rd::index(o, "MSB entry offset"))
                })
                .collect::<Result<_>>()?;
            let next = offsets[count - 1];
            let end = if next == 0 { b.len() } else { next };
            let mut entries = Vec::with_capacity(count - 1);
            for i in 0..count - 1 {
                let start = offsets[i];
                let stop = if i + 2 < count { offsets[i + 1] } else { end };
                if start >= stop || stop > b.len() {
                    return Err(format_err(format!(
                        "{name}: entry {i} has an impossible extent"
                    )));
                }
                entries.push(b[start..stop].to_vec());
            }
            if tables.len() >= TABLES.len() {
                return Err(format_err("the MSB has more tables than the known six"));
            }
            tables.push(Table {
                version,
                name,
                entries,
            });
            if next == 0 {
                break;
            }
            if next <= at {
                return Err(format_err("the MSB's tables do not run forward"));
            }
            at = next;
        }
        let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
        if names != TABLES {
            return Err(Error::Unsupported(format!(
                "this MSB's tables are {names:?}"
            )));
        }
        Ok(Msb { tables })
    }

    pub fn write(&self) -> Result<Vec<u8>> {
        let mut out = HEADER.to_vec();
        for (t, table) in self.tables.iter().enumerate() {
            let at = out.len();
            let count = table.entries.len() + 1;
            out.extend_from_slice(&table.version.to_le_bytes());
            out.extend_from_slice(
                &i32::try_from(count)
                    .map_err(|_| format_err("too many MSB entries"))?
                    .to_le_bytes(),
            );
            out.resize(at + 0x10 + count * 8, 0);
            let name_at = out.len();
            out[at + 8..at + 0x10].copy_from_slice(&(name_at as i64).to_le_bytes());
            for unit in table.name.encode_utf16().chain([0]) {
                out.extend_from_slice(&unit.to_le_bytes());
            }
            out.resize(align_up(out.len(), 8), 0);
            for (i, e) in table.entries.iter().enumerate() {
                let slot = at + 0x10 + i * 8;
                let here = out.len() as i64;
                out[slot..slot + 8].copy_from_slice(&here.to_le_bytes());
                out.extend_from_slice(e);
            }
            let next = if t + 1 < self.tables.len() {
                out.len() as i64
            } else {
                0
            };
            let slot = at + 0x10 + (count - 1) * 8;
            out[slot..slot + 8].copy_from_slice(&next.to_le_bytes());
        }
        Ok(out)
    }

    pub fn table(&self, name: &str) -> &Table {
        self.tables
            .iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("no MSB table {name}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Msb {
        Msb {
            tables: TABLES
                .iter()
                .enumerate()
                .map(|(i, n)| Table {
                    version: 74,
                    name: n.to_string(),
                    entries: (0..i % 3)
                        .map(|k| vec![(i * 16 + k) as u8; 24 + 8 * k])
                        .collect(),
                })
                .collect(),
        }
    }

    #[test]
    fn a_map_reads_back_what_was_written_and_a_second_write_is_identical() {
        let bytes = sample().write().unwrap();
        let back = Msb::read(&bytes).unwrap();
        assert_eq!(back, sample());
        assert_eq!(back.write().unwrap(), bytes);
        assert_eq!(back.table("LAYER_PARAM_ST").entries.len(), 1);
    }

    #[test]
    fn tables_out_of_the_known_order_are_refused() {
        let mut m = sample();
        m.tables.swap(0, 1);
        assert!(Msb::read(&m.write().unwrap()).is_err());
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
            let _ = Msb::read(&b);
        }
    }
}
