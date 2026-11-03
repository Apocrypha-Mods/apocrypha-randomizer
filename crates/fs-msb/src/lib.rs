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

pub mod entry;
pub mod layout;

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

    fn table_mut(&mut self, name: &str) -> Result<&mut Table> {
        self.tables
            .iter_mut()
            .find(|t| t.name == name)
            .ok_or_else(|| format_err(format!("no MSB table {name}")))
    }

    /// Where an entry of `ty` goes so that its table stays grouped by type:
    /// after the last entry of that type, or at the end if there is none.
    pub fn place_for(&self, table: &str, ty: i32) -> usize {
        let f = layout::field(table, "type");
        let es = &self.table(table).entries;
        es.iter()
            .rposition(|e| f.get(e, 0) == Some(i64::from(ty)))
            .map_or(es.len(), |i| i + 1)
    }

    /// Put `entry` into `table` at `at`, keeping what the map's other entries
    /// say true: every index into that table the layout knows of
    /// ([`layout::Field::refers`]) that pointed at `at` or later moves up one,
    /// the new entry and those after it of its type get their place among
    /// their type (`type_index`), and a part's model counts one more use.
    ///
    /// The layout knows the indices the game's maps show most clearly, not
    /// necessarily all, so an entry is best added after the last of its type
    /// in a table with few references into it ([`Msb::place_for`]).
    pub fn insert(&mut self, table: &str, at: usize, mut entry: Vec<u8>) -> Result<()> {
        let len = self.table(table).entries.len();
        if at > len {
            return Err(format_err(format!("{table} has {len} entries, not {at}")));
        }
        entry::Entry::read(table, &entry)?;
        let ty = layout::field(table, "type");
        let index = layout::field(table, "type_index");
        let t = ty.get(&entry, 0);
        let before = self.table(table).entries[..at]
            .iter()
            .filter(|e| ty.get(e, 0) == t)
            .count();
        index.set(&mut entry, 0, before as i64)?;
        let entries = &mut self.table_mut(table)?.entries;
        for e in entries[at..].iter_mut().filter(|e| ty.get(e, 0) == t) {
            let v = index.get(e, 0).unwrap_or(0);
            index.set(e, 0, v + 1)?;
        }
        let model = (table == "PARTS_PARAM_ST")
            .then(|| layout::field(table, "model").get(&entry, 0))
            .flatten();
        entries.insert(at, entry);
        // The new entry's own indices count as the map's did before it.
        self.shift(table, at, 1)?;
        if let Some(m) = model {
            self.count_use(m, 1)?;
        }
        Ok(())
    }

    /// Take entry `at` out of `table`, the inverse of [`Msb::insert`]. Refused
    /// while a known index still points at it.
    pub fn remove(&mut self, table: &str, at: usize) -> Result<Vec<u8>> {
        let len = self.table(table).entries.len();
        if at >= len {
            return Err(format_err(format!("{table} has {len} entries, not {at}")));
        }
        let ty = layout::field(table, "type");
        let index = layout::field(table, "type_index");
        let entries = &mut self.table_mut(table)?.entries;
        let entry = entries.remove(at);
        if let Err(e) = self.shift(table, at, -1) {
            // Nothing has moved yet: the check comes before any change.
            self.table_mut(table)?.entries.insert(at, entry);
            return Err(e);
        }
        let entries = &mut self.table_mut(table)?.entries;
        let t = ty.get(&entry, 0);
        for e in entries[at..].iter_mut().filter(|e| ty.get(e, 0) == t) {
            let v = index.get(e, 0).unwrap_or(0);
            index.set(e, 0, v - 1)?;
        }
        if table == "PARTS_PARAM_ST"
            && let Some(m) = layout::field(table, "model").get(&entry, 0)
        {
            self.count_use(m, -1)?;
        }
        Ok(entry)
    }

    fn count_use(&mut self, model: i64, by: i64) -> Result<()> {
        let f = layout::field("MODEL_PARAM_ST", "instances");
        let models = &mut self.table_mut("MODEL_PARAM_ST")?.entries;
        if let Some(m) = usize::try_from(model).ok().and_then(|m| models.get_mut(m)) {
            let n = f.get(m, 0).unwrap_or(0);
            f.set(m, 0, n + by)?;
        }
        Ok(())
    }

    /// Move every known index into `table` at or past `at` by `by` (+1 to
    /// make room, -1 to close a gap, which fails, changing nothing, if
    /// something points at `at`).
    fn shift(&mut self, table: &str, at: usize, by: i64) -> Result<()> {
        let at = at as i64;
        let fields: Vec<&layout::Field> = layout::elden_ring()
            .iter()
            .filter(|f| f.refers.as_deref() == Some(table))
            .collect();
        if by < 0 {
            for t in &self.tables {
                for f in fields.iter().filter(|f| f.table == t.name) {
                    for e in t.entries.iter().filter(|e| f.applies(e)) {
                        if (0..f.count).any(|i| f.get(e, i) == Some(at)) {
                            return Err(format_err(format!(
                                "{}.{} still points at {table} entry {at}",
                                f.table, f.name
                            )));
                        }
                    }
                }
            }
        }
        for t in &mut self.tables {
            for f in fields.iter().filter(|f| f.table == t.name) {
                for e in t.entries.iter_mut().filter(|e| f.applies(e)) {
                    for i in 0..f.count {
                        let Some(v) = f.get(e, i) else { continue };
                        if v >= at {
                            f.set(e, i, v + by)?;
                        }
                    }
                }
            }
        }
        Ok(())
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

    fn text(s: &str) -> Vec<u8> {
        s.encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    fn put(b: &mut [u8], at: usize, v: i64, wide: bool) {
        if wide {
            b[at..at + 8].copy_from_slice(&v.to_le_bytes());
        } else {
            b[at..at + 4].copy_from_slice(&(v as i32).to_le_bytes());
        }
    }

    /// A part laid out as the game's are: header, name, model path, an
    /// entity section and a type section.
    fn part(name: &str, ty: i32, index: i32, model: i32, collision: i32) -> Vec<u8> {
        let mut b = vec![0u8; 0xC0];
        put(&mut b, 0x0C, ty.into(), false);
        put(&mut b, 0x10, index.into(), false);
        put(&mut b, 0x14, model.into(), false);
        put(&mut b, 0, 0xC0, true);
        b.extend(text(name));
        let here = b.len() as i64;
        put(&mut b, 0x18, here, true);
        b.extend(text(""));
        b.resize(b.len().next_multiple_of(8), 0);
        let here = b.len() as i64;
        put(&mut b, 0x60, here, true);
        b.extend([0u8; 0x40]);
        let here = b.len() as i64;
        put(&mut b, 0x68, here, true);
        let s = b.len();
        b.extend([0u8; 0x60]);
        put(&mut b, s + 0x1C, collision.into(), false);
        b
    }

    fn model(name: &str, ty: i32, instances: i32) -> Vec<u8> {
        let mut b = vec![0u8; 0x28];
        put(&mut b, 0x08, ty.into(), false);
        put(&mut b, 0x18, instances.into(), false);
        put(&mut b, 0, 0x28, true);
        b.extend(text(name));
        let here = b.len() as i64;
        put(&mut b, 0x10, here, true);
        b.extend(text(""));
        b.resize(b.len().next_multiple_of(8), 0);
        b
    }

    /// A treasure (event type 4) on part `on`.
    fn treasure(on: i32) -> Vec<u8> {
        let mut b = vec![0u8; 0x30];
        put(&mut b, 0x0C, 4, false);
        put(&mut b, 0, 0x30, true);
        b.extend(text("treasure"));
        b.resize(b.len().next_multiple_of(8), 0);
        let here = b.len() as i64;
        put(&mut b, 0x18, here, true);
        b.extend((-1i32).to_le_bytes());
        b.extend((-1i32).to_le_bytes());
        b.extend([0u8; 0x18]);
        let here = b.len() as i64;
        put(&mut b, 0x20, here, true);
        let s = b.len();
        b.extend([0u8; 0x40]);
        put(&mut b, s + 0x08, on.into(), false);
        b
    }

    fn small_map() -> Msb {
        let mut m = sample();
        for t in &mut m.tables {
            t.entries.clear();
        }
        m.tables[0].entries = vec![
            model("c1000", 2, 2),
            model("h0001", 5, 1),
            model("AEG1", 13, 1),
        ];
        m.tables[1].entries = vec![treasure(3)];
        m.tables[5].entries = vec![
            part("c1000_9000", 2, 0, 0, 2),
            part("c1000_9001", 2, 1, 0, 2),
            part("h0001", 5, 0, 1, -1),
            part("AEG1_000", 13, 0, 2, -1),
        ];
        m
    }

    #[test]
    fn an_added_part_moves_the_indices_that_point_past_it() {
        use layout::field;
        let mut m = small_map();
        let before = m.write().unwrap();
        let at = m.place_for("PARTS_PARAM_ST", 2);
        assert_eq!(at, 2, "after the last enemy");
        let mut e =
            entry::Entry::read("PARTS_PARAM_ST", &m.table("PARTS_PARAM_ST").entries[0]).unwrap();
        e.set_name("c1000_9002").unwrap();
        m.insert("PARTS_PARAM_ST", at, e.write()).unwrap();
        let parts = &m.table("PARTS_PARAM_ST").entries;
        let names: Vec<_> = parts
            .iter()
            .map(|p| layout::entry_name(p).unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "c1000_9000",
                "c1000_9001",
                "c1000_9002",
                "h0001",
                "AEG1_000"
            ]
        );
        let coll = field("PARTS_PARAM_ST", "collision_part");
        assert!(
            parts[..3].iter().all(|p| coll.get(p, 0) == Some(3)),
            "the collision moved to 3"
        );
        assert_eq!(
            field("PARTS_PARAM_ST", "type_index").get(&parts[2], 0),
            Some(2)
        );
        let tr = &m.table("EVENT_PARAM_ST").entries[0];
        assert_eq!(field("EVENT_PARAM_ST", "treasure_part").get(tr, 0), Some(4));
        assert_eq!(
            field("EVENT_PARAM_ST", "part").get(tr, 0),
            Some(-1),
            "-1 stays"
        );
        let models = &m.table("MODEL_PARAM_ST").entries;
        assert_eq!(
            field("MODEL_PARAM_ST", "instances").get(&models[0], 0),
            Some(3)
        );
        // Taking it out again gives the map back, byte for byte.
        m.remove("PARTS_PARAM_ST", at).unwrap();
        assert_eq!(m.write().unwrap(), before);
    }

    /// Damaged entries handed to insert, at any place, give errors or a map
    /// that still writes; never a panic. A sound one comes back out whole.
    #[test]
    fn inserting_damaged_entries_never_panics() {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let base = small_map();
        for _ in 0..5000 {
            let mut m = base.clone();
            let t = ["PARTS_PARAM_ST", "MODEL_PARAM_ST", "EVENT_PARAM_ST"][(next() % 3) as usize];
            let src = &m.table(t).entries;
            let mut e = src[(next() as usize) % src.len()].clone();
            for _ in 0..next() % 4 {
                let i = (next() as usize) % e.len();
                e[i] = next() as u8;
            }
            let at = (next() as usize) % (src.len() + 2);
            if m.insert(t, at, e.clone()).is_ok() {
                assert!(m.write().is_ok());
                if let Ok(back) = m.remove(t, at) {
                    assert_eq!(back.len(), e.len());
                }
            }
        }
    }

    #[test]
    fn a_part_still_pointed_at_is_not_removed() {
        let mut m = small_map();
        let before = m.write().unwrap();
        assert!(
            m.remove("PARTS_PARAM_ST", 2).is_err(),
            "the enemies stand on it"
        );
        assert!(
            m.remove("PARTS_PARAM_ST", 3).is_err(),
            "the treasure sits on it"
        );
        assert!(m.remove("MODEL_PARAM_ST", 0).is_err(), "parts use it");
        assert_eq!(m.write().unwrap(), before, "a refusal changes nothing");
        assert!(m.remove("PARTS_PARAM_ST", 9).is_err());
        assert!(
            m.insert("PARTS_PARAM_ST", 9, part("x", 2, 0, 0, -1))
                .is_err()
        );
        assert!(
            m.insert("PARTS_PARAM_ST", 0, vec![1, 2, 3]).is_err(),
            "not an entry"
        );
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
