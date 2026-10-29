//! One param table: fixed-size rows keyed by an id.
//!
//! Observed on every table of the game's file and two mods — format bytes
//! `0x2D = 0x85`, `0x2E = 0x07`, little-endian:
//!
//! ```text
//! 0x00 strings offset (u32)   0x0A row count (u16)   0x10 type-name offset (i64)
//! 0x2C 0 = little-endian      0x2D 0x85              0x2E 0x07 (UTF-16 names)
//! 0x30 data start (i64)       0x40 rows: id (i32), 0, data (i64), name (i64)
//! ```
//!
//! then the rows' data, contiguous, then the type name (ASCII), then row names
//! (UTF-16).
//!
//! Lifted from `apoc-fsparams` (`param.rs`, Nova's own code). Its reader is
//! unchanged; its writer is extended so the game's own tables come back byte
//! for byte, which all 194 in build 25080141 do:
//!
//! - **The strings offset at 0x00.** The game's tool stores a value that matches
//!   nothing else in 56 tables, some of them past the end of the file, so the
//!   game cannot read it. It is kept as read while the table's data is the
//!   same size, and otherwise set to the type name's offset, as the modding
//!   tools write it.
//! - **Rows with no name.** A name offset of 0 is no name at all, not an empty
//!   one. When no row has a name, the game writes no names and no closing
//!   zeros (`NetworkParam`, `SpeedtreeParam`).
//! - **Bytes after the end.** Five single-row tables (`EnemyCommonParam`,
//!   `GameSystemCommonParam`, `GraphicsCommonParam`, `MenuCommonParam`,
//!   `PlayerCommonParam`) carry 8 zero bytes after their type name. Why is not
//!   known; they are kept under the same rule as the strings offset.
//!
//! # Rows are not assumed unique or sorted
//!
//! The game's own file breaks both: `RandomAppearParam` repeats 26 ids and
//! `PhantomParam` is out of order. Rows are kept in file order, and a merge keys
//! them by id *and* occurrence.

use fs_bytes::{self as rd, Error, Result};

const HEADER_LEN: usize = 0x40;
const ROW_HEADER_LEN: usize = 24;
/// `0x2D`: the type name is stored by offset, and data offsets are 64-bit.
const OFFSET_TYPE_AND_LONG_OFFSETS: u8 = 0x80 | 0x04;
/// `0x2E`: row names are UTF-16.
const UNICODE_NAMES: u8 = 0x01;

/// One row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: i32,
    pub data: Vec<u8>,
    /// UTF-16 code units, without the terminator, or `None` for a name
    /// offset of 0. Usually `Some` and empty.
    pub name: Option<Vec<u16>>,
}

/// A parsed table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub(crate) header: [u8; HEADER_LEN],
    /// e.g. `ATK_PARAM_ST`.
    pub type_name: Vec<u8>,
    pub row_size: usize,
    pub rows: Vec<Row>,
    /// Bytes after the last thing the layout accounts for, kept while the
    /// table's data is the same size.
    trailer: Vec<u8>,
}

impl Param {
    pub fn read(b: &[u8]) -> Result<Self> {
        let header: [u8; HEADER_LEN] = rd::bytes(b, 0, HEADER_LEN, "param header")?
            .try_into()
            .unwrap();
        if header[0x2C] != 0 {
            return Err(Error::Format("big-endian params are not supported".into()));
        }
        if header[0x2D] & OFFSET_TYPE_AND_LONG_OFFSETS != OFFSET_TYPE_AND_LONG_OFFSETS {
            return Err(Error::Format(format!(
                "param format {:#x} is not the one this reads",
                header[0x2D]
            )));
        }
        if header[0x2E] & UNICODE_NAMES == 0 {
            return Err(Error::Format("param row names are not UTF-16".into()));
        }
        let count = rd::u16_le(b, 0x0A, "param header")? as usize;
        let type_at = rd::index(rd::i64_le(b, 0x10, "param header")?, "type offset")?;
        let data_start = rd::index(rd::i64_le(b, 0x30, "param header")?, "data offset")?;
        if data_start != HEADER_LEN + count * ROW_HEADER_LEN || type_at < data_start {
            return Err(Error::Format(
                "the param's layout is not the known one".into(),
            ));
        }

        let mut offsets = Vec::with_capacity(count);
        for i in 0..count {
            let r = HEADER_LEN + i * ROW_HEADER_LEN;
            let id = rd::i32_le(b, r, "row header")?;
            let data = rd::index(rd::i64_le(b, r + 8, "row header")?, "row offset")?;
            let name = rd::index(rd::i64_le(b, r + 16, "row header")?, "name offset")?;
            offsets.push((id, data, name));
        }

        // The stride between rows is the row size. A single row has none, so it
        // ends at whichever section the file places after it. That is the type
        // name in the game's own layout, but a modding tool may put the strings
        // block there first: Dark Souls III's Cinders has BudgetParam's one
        // 132-byte row followed by 16 bytes of strings and only then the type
        // name, so measuring to the type name read 148 bytes and refused a mod
        // whose row matched the game's exactly.
        //
        // The cost, stated rather than hidden: a malformed file can point the
        // strings or name offset *inside* its row, and this then reads a row
        // that is too short. What bounds it is the merger, which refuses any
        // table whose row size differs from the game's own file — and a real row
        // cannot differ from the game's, since the game's definition fixes it.
        // So a misleading offset gets through only by landing on exactly the
        // game's row size, and what is merged then is exactly as much of the row
        // as the game itself reads.
        //
        // That argument rests on the game's own file being the game's. The same
        // rule reads the base, so a base that was already replaced by a modded
        // copy with such an offset would mislead both sides alike. It is the
        // premise every merge already stands on — a modded base is taken for
        // vanilla whatever this rule does — not one this rule introduces.
        let row_size = match count {
            0 => 0,
            1 => {
                let strings_at = rd::u32_le(b, 0x00, "param header")? as usize;
                [strings_at, type_at, offsets[0].2]
                    .into_iter()
                    .filter(|&at| at > data_start)
                    .min()
                    .unwrap_or(type_at)
                    - data_start
            }
            _ => offsets[1]
                .1
                .checked_sub(offsets[0].1)
                .ok_or_else(|| Error::Format("the param's rows are not in data order".into()))?,
        };
        // Checked arithmetic: the offsets are 64-bit values from a downloaded
        // file, and a hostile stride would otherwise overflow in the sums.
        for (i, &(_, at, _)) in offsets.iter().enumerate() {
            let expected = i
                .checked_mul(row_size)
                .and_then(|n| n.checked_add(data_start));
            if Some(at) != expected {
                return Err(Error::Format("the param's rows are not contiguous".into()));
            }
        }
        let data_end = count
            .checked_mul(row_size)
            .and_then(|n| n.checked_add(data_start));
        if data_end.is_none_or(|end| end > type_at) {
            return Err(Error::Format(
                "the param's rows run into its type name".into(),
            ));
        }

        let type_name = rd::asciiz(b, type_at, 256, "param type name")?;
        let mut rows = Vec::with_capacity(count);
        // Many rows can point at one name, and each row gets its own copy. The
        // total is held to a few times the table's own size, so a table cannot
        // turn one long name into a copy per row.
        let mut name_budget = b.len().saturating_mul(4).saturating_add(64 * 1024) / 2;
        for (id, at, name_at) in offsets {
            let data = rd::bytes(b, at, row_size, "row data")?.to_vec();
            let name = if name_at == 0 {
                None
            } else {
                let name = rd::utf16z(b, name_at, name_budget, "row name")?;
                name_budget -= name.len();
                Some(name)
            };
            rows.push(Row { id, data, name });
        }
        let mut param = Param {
            header,
            type_name,
            row_size,
            rows,
            trailer: Vec::new(),
        };
        // Whatever follows the layout the writer reproduces, when the rest of
        // the file is exactly that layout.
        let canonical = param.write()?;
        if b.len() > canonical.len() && b.starts_with(&canonical) {
            param.trailer = b[canonical.len()..].to_vec();
        }
        Ok(param)
    }

    /// Write the table.
    ///
    /// Each distinct name — the empty one included — is written once, where a
    /// row first uses it, and every row using it points there; two zero bytes
    /// close the names. A row with no name points at 0, and a table where no
    /// row has a name has no names section at all.
    pub fn write(&self) -> Result<Vec<u8>> {
        let n = self.rows.len();
        let count = u16::try_from(n).map_err(|_| {
            Error::Format(format!(
                "{} would have {n} rows, more than a param can hold",
                String::from_utf8_lossy(&self.type_name)
            ))
        })?;
        if let Some(bad) = self.rows.iter().find(|r| r.data.len() != self.row_size) {
            return Err(Error::Format(format!(
                "row {} is {} bytes where its table's rows are {}",
                bad.id,
                bad.data.len(),
                self.row_size
            )));
        }
        let data_start = HEADER_LEN + n * ROW_HEADER_LEN;
        let type_at = data_start + n * self.row_size;
        let original_type_at = i64::from_le_bytes(self.header[0x10..0x18].try_into().unwrap());
        let same_size = original_type_at == type_at as i64;
        let strings_at = if same_size {
            u32::from_le_bytes(self.header[0x00..0x04].try_into().unwrap())
        } else {
            u32::try_from(type_at)
                .map_err(|_| Error::Format("the param is too large for its offsets".into()))?
        };

        let mut out = Vec::with_capacity(type_at + self.type_name.len() + 64);
        out.extend_from_slice(&self.header);
        out[0x00..0x04].copy_from_slice(&strings_at.to_le_bytes());
        out[0x0A..0x0C].copy_from_slice(&count.to_le_bytes());
        out[0x10..0x18].copy_from_slice(&(type_at as i64).to_le_bytes());
        out[0x30..0x38].copy_from_slice(&(data_start as i64).to_le_bytes());
        out.resize(data_start, 0);
        for row in &self.rows {
            out.extend_from_slice(&row.data);
        }
        out.extend_from_slice(&self.type_name);
        out.push(0);

        let mut name_at: std::collections::HashMap<&[u16], usize> = Default::default();
        for name in self.rows.iter().filter_map(|r| r.name.as_deref()) {
            if !name_at.contains_key(name) {
                name_at.insert(name, out.len());
                for unit in name.iter().chain([&0]) {
                    out.extend_from_slice(&unit.to_le_bytes());
                }
            }
        }
        if !name_at.is_empty() {
            out.extend_from_slice(&[0, 0]);
        }
        if same_size {
            out.extend_from_slice(&self.trailer);
        }

        for (i, row) in self.rows.iter().enumerate() {
            let r = HEADER_LEN + i * ROW_HEADER_LEN;
            out[r..r + 4].copy_from_slice(&row.id.to_le_bytes());
            out[r + 4..r + 8].copy_from_slice(&0i32.to_le_bytes());
            let data = (data_start + i * self.row_size) as i64;
            out[r + 8..r + 16].copy_from_slice(&data.to_le_bytes());
            let name = row.name.as_deref().map_or(0, |n| name_at[n] as i64);
            out[r + 16..r + 24].copy_from_slice(&name.to_le_bytes());
        }
        Ok(out)
    }

    /// A table built from scratch, for tests.
    #[cfg(test)]
    pub(crate) fn synthesize(type_name: &str, row_size: usize, rows: Vec<Row>) -> Self {
        let mut header = [0u8; HEADER_LEN];
        header[0x08..0x0A].copy_from_slice(&2u16.to_le_bytes());
        header[0x2D] = 0x85;
        header[0x2E] = 0x07;
        Param {
            header,
            type_name: type_name.as_bytes().to_vec(),
            row_size,
            rows,
            trailer: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i32, fill: u8, name: &str) -> Row {
        Row {
            id,
            data: vec![fill; 12],
            name: Some(name.encode_utf16().collect()),
        }
    }

    #[test]
    fn a_table_reads_back_what_was_written_names_and_all() {
        let p = Param::synthesize(
            "TEST_PARAM_ST",
            12,
            vec![
                row(1, 1, ""),
                row(5, 2, "Sword"),
                row(5, 3, ""),
                row(9, 4, "Sword"),
            ],
        );
        let bytes = p.write().unwrap();
        let back = Param::read(&bytes).unwrap();
        // The header is not compared: writing fills in the offsets a
        // synthesised one leaves at zero. What it describes is compared.
        assert_eq!(back.rows, p.rows);
        assert_eq!(back.type_name, p.type_name);
        assert_eq!(back.row_size, p.row_size);
        assert_eq!(
            back.write().unwrap(),
            bytes,
            "a second write changes nothing"
        );
    }

    #[test]
    fn a_single_row_table_knows_its_row_size() {
        let p = Param::synthesize("ONE_ST", 12, vec![row(0, 7, "")]);
        let back = Param::read(&p.write().unwrap()).unwrap();
        assert_eq!(back.row_size, 12);
        assert_eq!(back.rows[0].data, vec![7; 12]);
    }

    /// The layout Dark Souls III's modding tools write: the strings block sits
    /// between the only row and the type name. Measured to the type name, the
    /// row came out 16 bytes too long, and a merge refused Cinders over a
    /// BudgetParam row that matched the game's exactly.
    #[test]
    fn a_single_row_ends_where_the_strings_block_begins() {
        let p = Param::synthesize("ONE_ST", 12, vec![row(0, 7, "")]);
        let mut b = p.write().unwrap();

        let type_at = i64::from_le_bytes(b[0x10..0x18].try_into().unwrap()) as usize;
        let name_at = i64::from_le_bytes(b[0x50..0x58].try_into().unwrap()) as usize;
        b.splice(type_at..type_at, [0u8; 16]);
        b[0x00..0x04].copy_from_slice(&(type_at as u32).to_le_bytes());
        b[0x10..0x18].copy_from_slice(&((type_at + 16) as i64).to_le_bytes());
        if name_at >= type_at {
            b[0x50..0x58].copy_from_slice(&((name_at + 16) as i64).to_le_bytes());
        }

        let back = Param::read(&b).unwrap();
        assert_eq!(
            back.row_size, 12,
            "the strings block is not part of the row"
        );
        assert_eq!(back.rows[0].data, vec![7; 12]);
        assert_eq!(back.type_name, p.type_name);
    }

    /// The other side of that rule: an offset pointing *inside* the only row is
    /// taken as its end, so a malformed file reads a shorter row. Pinned here so
    /// the behaviour is deliberate; the merger's comparison against the game's
    /// own row size is what refuses such a file.
    #[test]
    fn a_strings_offset_inside_the_only_row_shortens_it_rather_than_widening_it() {
        let p = Param::synthesize("ONE_ST", 12, vec![row(0, 7, "")]);
        let mut b = p.write().unwrap();
        let data_at = i64::from_le_bytes(b[0x48..0x50].try_into().unwrap()) as usize;
        b[0x00..0x04].copy_from_slice(&((data_at + 8) as u32).to_le_bytes());

        let back = Param::read(&b).unwrap();
        assert_eq!(back.row_size, 8);
        assert_eq!(back.rows[0].data, vec![7; 8]);
    }

    /// As `NetworkParam` is stored: no row has a name, so there are no names
    /// and no closing zeros after the type name.
    #[test]
    fn a_table_with_no_names_ends_at_its_type_name() {
        let mut p = Param::synthesize("NETWORK_PARAM_ST", 12, vec![row(0, 7, ""), row(1, 8, "")]);
        for r in &mut p.rows {
            r.name = None;
        }
        let bytes = p.write().unwrap();
        assert!(bytes.ends_with(b"NETWORK_PARAM_ST\0"));
        let back = Param::read(&bytes).unwrap();
        assert_eq!(back.rows, p.rows);
        assert_eq!(back.write().unwrap(), bytes);
    }

    /// As 56 of the game's tables store it: a strings offset that matches
    /// nothing. Kept while the data is the same size, replaced once it is not.
    #[test]
    fn an_odd_strings_offset_is_kept_until_the_data_changes_size() {
        let p = Param::synthesize("T_ST", 12, vec![row(0, 0, ""), row(1, 1, "")]);
        let mut bytes = p.write().unwrap();
        bytes[0..4].copy_from_slice(&0x2908u32.to_le_bytes());
        let mut back = Param::read(&bytes).unwrap();
        assert_eq!(back.write().unwrap(), bytes, "kept");
        back.rows[0].data[0] = 9;
        assert_eq!(
            &back.write().unwrap()[0..4],
            &0x2908u32.to_le_bytes(),
            "same size, still kept"
        );
        back.rows.push(row(2, 2, ""));
        let grown = back.write().unwrap();
        let type_at = i64::from_le_bytes(grown[0x10..0x18].try_into().unwrap()) as u32;
        assert_eq!(
            &grown[0..4],
            &type_at.to_le_bytes(),
            "grown, so the modding tools' value"
        );
    }

    /// As `EnemyCommonParam` is stored: 8 zero bytes after the type name.
    #[test]
    fn trailing_bytes_are_kept_until_the_data_changes_size() {
        let mut p = Param::synthesize("ENEMY_COMMON_PARAM_ST", 12, vec![row(0, 7, "")]);
        p.rows[0].name = None;
        let mut bytes = p.write().unwrap();
        bytes.extend_from_slice(&[0; 8]);
        let mut back = Param::read(&bytes).unwrap();
        assert_eq!(back.write().unwrap(), bytes);
        back.rows.push(Row {
            id: 1,
            data: vec![0; 12],
            name: None,
        });
        assert!(back.write().unwrap().ends_with(b"ENEMY_COMMON_PARAM_ST\0"));
    }

    #[test]
    fn a_row_of_the_wrong_size_is_refused_rather_than_written() {
        let mut p = Param::synthesize("T_ST", 12, vec![row(0, 0, "")]);
        p.rows[0].data.pop();
        assert!(p.write().is_err());
    }

    /// Every row pointing at one long name would copy it once per row. The
    /// writer stores a shared name once, so this is exactly the table a hostile
    /// file would carry — and it has to be refused, not expanded.
    #[test]
    fn one_long_name_shared_by_every_row_is_refused_rather_than_copied_per_row() {
        let long: String = "x".repeat(1000);
        let rows = (0..200).map(|i| row(i, 0, &long)).collect();
        let bytes = Param::synthesize("T_ST", 12, rows).write().unwrap();
        let err = Param::read(&bytes).unwrap_err().to_string();
        assert!(err.contains("implausibly long"), "{err}");
    }

    /// Offsets are 64-bit values from a downloaded file. A stride chosen to
    /// overflow the offset arithmetic has to be an error, not a panic or a
    /// wrapped index.
    #[test]
    fn a_hostile_row_stride_is_an_error_and_not_an_overflow() {
        let mut bytes = Param::synthesize("T_ST", 12, vec![row(0, 0, ""), row(1, 1, "")])
            .write()
            .unwrap();
        let second_row_offset = HEADER_LEN + ROW_HEADER_LEN + 8;
        bytes[second_row_offset..second_row_offset + 8].copy_from_slice(&i64::MAX.to_le_bytes());
        assert!(Param::read(&bytes).is_err());
    }

    #[test]
    fn a_truncated_table_is_an_error_and_not_a_panic() {
        let bytes = Param::synthesize("T_ST", 12, vec![row(0, 0, ""), row(1, 1, "")])
            .write()
            .unwrap();
        for cut in [10, 0x40, 0x60, bytes.len() - 3] {
            assert!(Param::read(&bytes[..cut]).is_err(), "cut at {cut}");
        }
    }
}
