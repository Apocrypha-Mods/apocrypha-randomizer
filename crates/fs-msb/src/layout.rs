//! Fields of MSB entries, worked out from the game's own maps with
//! `apoc-fs msb-columns` and stored as our own TOML (`layouts/elden_ring.toml`).
//!
//! A field sits at an offset from the entry's start, or inside a section: the
//! entry holds the section's offset (an i64, relative to the entry) at
//! `section`, and the field is at `offset` from there. Each field says how
//! sure it is and why; `fs-tool/tests/real_maps.rs` re-checks the claims.

use std::sync::OnceLock;

use fs_bytes::{Result, format_err};
use serde::Deserialize;

const ELDEN_RING: &str = include_str!("../layouts/elden_ring.toml");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    I32,
    U32,
    F32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Confirmed,
    Probable,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub table: String,
    /// The entry types the field belongs to; empty for every type.
    #[serde(default)]
    pub types: Vec<i32>,
    pub name: String,
    #[serde(default)]
    pub section: Option<usize>,
    pub offset: usize,
    pub kind: Kind,
    #[serde(default = "one")]
    pub count: usize,
    pub confidence: Confidence,
    pub evidence: String,
    /// The table this value is an index into (-1 for none), so that adding
    /// or removing an entry there can move it ([`crate::Msb::insert`]).
    #[serde(default)]
    pub refers: Option<String>,
}

fn one() -> usize {
    1
}

impl Field {
    /// Where value `index` sits in `entry`, if the entry has it.
    fn at(&self, entry: &[u8], index: usize) -> Option<usize> {
        assert!(
            index < self.count,
            "{} has {} values",
            self.name,
            self.count
        );
        let base = match self.section {
            None => 0,
            Some(s) => {
                usize::try_from(i64::from_le_bytes(entry.get(s..s + 8)?.try_into().unwrap()))
                    .ok()
                    .filter(|&b| b > 0)?
            }
        };
        let at = base + self.offset + index * 4;
        (at + 4 <= entry.len()).then_some(at)
    }

    /// Whether an entry of this type has the field.
    pub fn applies(&self, entry: &[u8]) -> bool {
        self.types.is_empty()
            || field_opt(&self.table, "type")
                .and_then(|t| t.get(entry, 0))
                .is_some_and(|t| self.types.contains(&(t as i32)))
    }

    /// The value (an f32 as its bits), or `None` if the entry is too short.
    pub fn get(&self, entry: &[u8], index: usize) -> Option<i64> {
        let at = self.at(entry, index)?;
        let w: [u8; 4] = entry[at..at + 4].try_into().unwrap();
        Some(match self.kind {
            Kind::I32 => i64::from(i32::from_le_bytes(w)),
            Kind::U32 | Kind::F32 => i64::from(u32::from_le_bytes(w)),
        })
    }

    pub fn get_f32(&self, entry: &[u8], index: usize) -> Option<f32> {
        self.get(entry, index).map(|v| f32::from_bits(v as u32))
    }

    pub fn set(&self, entry: &mut [u8], index: usize, value: i64) -> Result<()> {
        let at = self
            .at(entry, index)
            .ok_or_else(|| format_err(format!("the entry has no {}", self.name)))?;
        let bytes = match self.kind {
            Kind::I32 => i32::try_from(value)
                .map_err(|_| format_err(format!("{value} does not fit {}", self.name)))?
                .to_le_bytes(),
            Kind::U32 | Kind::F32 => u32::try_from(value)
                .map_err(|_| format_err(format!("{value} does not fit {}", self.name)))?
                .to_le_bytes(),
        };
        entry[at..at + 4].copy_from_slice(&bytes);
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(rename = "field")]
    fields: Vec<Field>,
}

pub fn elden_ring() -> &'static [Field] {
    static FIELDS: OnceLock<Vec<Field>> = OnceLock::new();
    FIELDS.get_or_init(|| {
        toml::from_str::<File>(ELDEN_RING)
            .expect("layouts/elden_ring.toml parses")
            .fields
    })
}

/// The field `name` of `table`.
pub fn field(table: &str, name: &str) -> &'static Field {
    field_opt(table, name).unwrap_or_else(|| panic!("no MSB field {table}.{name}"))
}

/// The field `name` of `table`, if the layout has it.
pub fn field_opt(table: &str, name: &str) -> Option<&'static Field> {
    elden_ring()
        .iter()
        .find(|f| f.table == table && f.name == name)
}

/// An entry's name: the UTF-16 string its first word points at.
pub fn entry_name(entry: &[u8]) -> Option<String> {
    let at = usize::try_from(i64::from_le_bytes(entry.get(0..8)?.try_into().unwrap())).ok()?;
    let units = fs_bytes::utf16z(entry, at, 256, "MSB entry name").ok()?;
    String::from_utf16(&units).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_field_is_well_formed() {
        for f in elden_ring() {
            assert!(crate::TABLES.contains(&f.table.as_str()), "{}", f.table);
            assert!(!f.evidence.trim().is_empty(), "{}.{}", f.table, f.name);
            if let Some(r) = &f.refers {
                assert!(
                    crate::TABLES.contains(&r.as_str()),
                    "{}.{} refers to {r}",
                    f.table,
                    f.name
                );
                assert_eq!(
                    f.kind,
                    Kind::I32,
                    "{}.{}: an index is signed",
                    f.table,
                    f.name
                );
            }
        }
        // Every table has the type and type index adding an entry needs.
        for t in [
            "MODEL_PARAM_ST",
            "EVENT_PARAM_ST",
            "POINT_PARAM_ST",
            "ROUTE_PARAM_ST",
            "PARTS_PARAM_ST",
        ] {
            assert!(
                field_opt(t, "type").is_some() && field_opt(t, "type_index").is_some(),
                "{t}"
            );
        }
    }

    #[test]
    fn a_field_in_a_section_is_found_through_the_section_offset() {
        let f = field("EVENT_PARAM_ST", "item_lot");
        let mut entry = vec![0u8; 0x80];
        entry[0x20..0x28].copy_from_slice(&0x40i64.to_le_bytes());
        f.set(&mut entry, 0, 10_000_450).unwrap();
        assert_eq!(&entry[0x50..0x54], &10_000_450i32.to_le_bytes());
        assert_eq!(f.get(&entry, 0), Some(10_000_450));
        assert_eq!(
            f.get(&entry[..0x52], 0),
            None,
            "an entry too short has no value"
        );
    }

    #[test]
    fn an_entry_name_is_read_from_its_own_offset() {
        let mut entry = vec![0u8; 0x30];
        entry[0..8].copy_from_slice(&0x10i64.to_le_bytes());
        for (i, u) in "c4720_9000".encode_utf16().enumerate() {
            entry[0x10 + i * 2..0x12 + i * 2].copy_from_slice(&u.to_le_bytes());
        }
        assert_eq!(entry_name(&entry).as_deref(), Some("c4720_9000"));
    }
}
