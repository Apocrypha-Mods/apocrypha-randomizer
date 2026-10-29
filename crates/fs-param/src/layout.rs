//! Field layouts for the param tables the randomizer touches, worked out from
//! the game's own data and stored as our own TOML (`layouts/elden_ring.toml`).
//!
//! Paramdex has no licence, so its definitions cannot be used (PLAN.md §1).
//! Every field here was found with `apoc-fs columns`: which other table's row
//! ids a column's values fall in, which columns are empty exactly when another
//! is, which add up to a round number. Each field says how sure that is, and
//! why, and `fs-tool/tests/real_files.rs` re-checks the claims on the real
//! file. A field nobody has a use for yet is simply not listed.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use fs_bytes::{Result, format_err};
use serde::Deserialize;

use crate::param::Param;

const ELDEN_RING: &str = include_str!("../layouts/elden_ring.toml");

/// How a field's value is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    F32,
}

impl Kind {
    pub fn size(self) -> usize {
        match self {
            Kind::U8 | Kind::I8 => 1,
            Kind::U16 | Kind::I16 => 2,
            Kind::U32 | Kind::I32 | Kind::F32 => 4,
        }
    }
}

/// How sure a field is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// The data alone settles it, and a real-file test checks it.
    Confirmed,
    /// Suggestive, but another format (EMEVD, MSB) or play is needed to settle.
    Probable,
}

/// One field, possibly repeated: `count` values `stride` bytes apart.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub name: String,
    pub offset: usize,
    pub kind: Kind,
    #[serde(default = "one")]
    pub count: usize,
    #[serde(default)]
    pub stride: usize,
    pub confidence: Confidence,
    pub evidence: String,
    /// Meanings of particular values, where they are known.
    #[serde(default)]
    pub values: BTreeMap<String, String>,
}

fn one() -> usize {
    1
}

impl Field {
    fn at(&self, index: usize) -> usize {
        assert!(
            index < self.count,
            "{} has {} values, not {}",
            self.name,
            self.count,
            index + 1
        );
        self.offset + index * self.stride.max(self.kind.size())
    }

    /// The value at `index`, widened to i64 (an f32 is returned as its bits).
    pub fn get(&self, row: &[u8], index: usize) -> i64 {
        let at = self.at(index);
        let b = &row[at..at + self.kind.size()];
        match self.kind {
            Kind::U8 => i64::from(b[0]),
            Kind::I8 => i64::from(b[0] as i8),
            Kind::U16 => i64::from(u16::from_le_bytes(b.try_into().unwrap())),
            Kind::I16 => i64::from(i16::from_le_bytes(b.try_into().unwrap())),
            Kind::U32 | Kind::F32 => i64::from(u32::from_le_bytes(b.try_into().unwrap())),
            Kind::I32 => i64::from(i32::from_le_bytes(b.try_into().unwrap())),
        }
    }

    pub fn get_f32(&self, row: &[u8], index: usize) -> f32 {
        f32::from_bits(self.get(row, index) as u32)
    }

    /// Set the value at `index`, refusing one the field cannot hold.
    pub fn set(&self, row: &mut [u8], index: usize, value: i64) -> Result<()> {
        let at = self.at(index);
        let out_of_range = || {
            format_err(format!(
                "{value} does not fit {} ({:?})",
                self.name, self.kind
            ))
        };
        match self.kind {
            Kind::U8 => row[at] = u8::try_from(value).map_err(|_| out_of_range())?,
            Kind::I8 => row[at] = i8::try_from(value).map_err(|_| out_of_range())? as u8,
            Kind::U16 => row[at..at + 2].copy_from_slice(
                &u16::try_from(value)
                    .map_err(|_| out_of_range())?
                    .to_le_bytes(),
            ),
            Kind::I16 => row[at..at + 2].copy_from_slice(
                &i16::try_from(value)
                    .map_err(|_| out_of_range())?
                    .to_le_bytes(),
            ),
            Kind::U32 | Kind::F32 => row[at..at + 4].copy_from_slice(
                &u32::try_from(value)
                    .map_err(|_| out_of_range())?
                    .to_le_bytes(),
            ),
            Kind::I32 => row[at..at + 4].copy_from_slice(
                &i32::try_from(value)
                    .map_err(|_| out_of_range())?
                    .to_le_bytes(),
            ),
        }
        Ok(())
    }
}

/// The fields of one row type.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    /// The type name the table's header carries, e.g. `ITEMLOT_PARAM_ST`.
    #[serde(rename = "type")]
    pub type_name: String,
    pub row_size: usize,
    /// The tables of this type, e.g. `ItemLotParam_map`.
    pub tables: Vec<String>,
    #[serde(rename = "field")]
    pub fields: Vec<Field>,
}

impl Layout {
    pub fn field(&self, name: &str) -> &Field {
        self.fields
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{} has no field {name}", self.type_name))
    }

    /// Whether `param` is a table this layout describes.
    pub fn check(&self, param: &Param) -> Result<()> {
        let type_name = String::from_utf8_lossy(&param.type_name);
        if type_name != self.type_name || param.row_size != self.row_size {
            return Err(format_err(format!(
                "the layout is for {} with {}-byte rows, not {type_name} with {}",
                self.type_name, self.row_size, param.row_size
            )));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(rename = "layout")]
    layouts: Vec<Layout>,
}

/// Every Elden Ring layout.
pub fn elden_ring() -> &'static [Layout] {
    static LAYOUTS: OnceLock<Vec<Layout>> = OnceLock::new();
    LAYOUTS.get_or_init(|| {
        let file: File = toml::from_str(ELDEN_RING).expect("layouts/elden_ring.toml parses");
        file.layouts
    })
}

/// The layout for a table, by table name.
pub fn for_table(table: &str) -> Option<&'static Layout> {
    elden_ring()
        .iter()
        .find(|l| l.tables.iter().any(|t| t == table))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_layout_is_well_formed() {
        for l in elden_ring() {
            assert!(!l.tables.is_empty(), "{}", l.type_name);
            let mut used = vec![None::<&str>; l.row_size];
            for f in &l.fields {
                assert!(f.count >= 1, "{}.{}", l.type_name, f.name);
                assert!(
                    f.count == 1 || f.stride >= f.kind.size(),
                    "{}.{} needs a stride",
                    l.type_name,
                    f.name
                );
                for i in 0..f.count {
                    let (start, end) = (f.at(i), f.at(i) + f.kind.size());
                    assert!(
                        end <= l.row_size,
                        "{}.{} runs past the row",
                        l.type_name,
                        f.name
                    );
                    for (b, slot) in used.iter_mut().enumerate().take(end).skip(start) {
                        if let Some(other) = slot {
                            panic!("{}.{} overlaps {other} at {b:#x}", l.type_name, f.name);
                        }
                        *slot = Some(&f.name);
                    }
                }
                assert!(
                    !f.evidence.trim().is_empty(),
                    "{}.{} gives no evidence",
                    l.type_name,
                    f.name
                );
            }
            let mut names: Vec<&str> = l.fields.iter().map(|f| f.name.as_str()).collect();
            names.sort();
            names.dedup();
            assert_eq!(
                names.len(),
                l.fields.len(),
                "{} repeats a field name",
                l.type_name
            );
        }
    }

    #[test]
    fn values_are_read_and_written_at_their_offsets() {
        let l = for_table("ItemLotParam_map").unwrap();
        let mut row = vec![0u8; l.row_size];
        let id = l.field("item_id");
        id.set(&mut row, 3, 2_020_001).unwrap();
        assert_eq!(id.get(&row, 3), 2_020_001);
        assert_eq!(&row[0x0C..0x10], &2_020_001i32.to_le_bytes());
        let count = l.field("count");
        assert!(count.set(&mut row, 0, 256).is_err(), "a u8 holds no 256");
        count.set(&mut row, 7, 99).unwrap();
        assert_eq!(row[count.offset + 7], 99);
    }
}
