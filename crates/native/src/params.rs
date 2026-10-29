//! The game's param tables as it holds them in memory: the same bytes as the
//! table in `regulation.bin` (`fs-param`'s `param.rs` has the layout), so a
//! table is found by its header and type name, and a row by the row headers'
//! offsets, which count from the table's start.

const HEADER_LEN: usize = 0x40;
const ROW_HEADER_LEN: usize = 24;

/// One table found in memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// The table's address.
    pub at: usize,
    /// Each row's id and the address of its data, in file order.
    pub rows: Vec<(i32, usize)>,
    pub row_size: usize,
}

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}

fn i64_at(b: &[u8], o: usize) -> Option<i64> {
    Some(i64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?))
}

/// Whether `b` (read from address `at`) starts like a table header: the
/// format bytes at 0x2C, and data starting straight after the row headers.
/// Returns the row count and the type name's offset.
pub fn header(b: &[u8]) -> Option<(usize, usize)> {
    if b.get(0x2C..0x2F)? != [0x00, 0x85, 0x07] {
        return None;
    }
    let count = u16_at(b, 0x0A)? as usize;
    let type_at = usize::try_from(i64_at(b, 0x10)?).ok()?;
    let data_start = usize::try_from(i64_at(b, 0x30)?).ok()?;
    (count > 0 && data_start == HEADER_LEN + count * ROW_HEADER_LEN && type_at > data_start)
        .then_some((count, type_at))
}

/// Read a whole table from `b`, the bytes from its address `at` on, if its
/// type name is `type_name`.
pub fn table(b: &[u8], at: usize, type_name: &str) -> Option<Table> {
    let (count, type_at) = header(b)?;
    let name = b.get(type_at..type_at + type_name.len() + 1)?;
    if &name[..type_name.len()] != type_name.as_bytes() || name[type_name.len()] != 0 {
        return None;
    }
    let mut rows = Vec::with_capacity(count);
    for i in 0..count {
        let r = HEADER_LEN + i * ROW_HEADER_LEN;
        let id = i32::from_le_bytes(b.get(r..r + 4)?.try_into().ok()?);
        let data = usize::try_from(i64_at(b, r + 8)?).ok()?;
        if data < HEADER_LEN + count * ROW_HEADER_LEN || data >= type_at {
            return None;
        }
        rows.push((id, at + data));
    }
    let row_size = match rows.as_slice() {
        [(_, a), (_, b), ..] => b.checked_sub(*a)?,
        [(_, a)] => at + type_at - a,
        [] => 0,
    };
    Some(Table { at, rows, row_size })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A table in the game's layout: header, row headers, rows, type name.
    pub fn make(type_name: &str, row_size: usize, rows: &[(i32, Vec<u8>)]) -> Vec<u8> {
        let n = rows.len();
        let data_start = HEADER_LEN + n * ROW_HEADER_LEN;
        let type_at = data_start + n * row_size;
        let mut b = vec![0u8; HEADER_LEN];
        b[0..4].copy_from_slice(&(type_at as u32).to_le_bytes());
        b[0x0A..0x0C].copy_from_slice(&(n as u16).to_le_bytes());
        b[0x10..0x18].copy_from_slice(&(type_at as i64).to_le_bytes());
        b[0x2C..0x2F].copy_from_slice(&[0x00, 0x85, 0x07]);
        b[0x30..0x38].copy_from_slice(&(data_start as i64).to_le_bytes());
        for (i, (id, _)) in rows.iter().enumerate() {
            b.extend_from_slice(&id.to_le_bytes());
            b.extend_from_slice(&[0; 4]);
            b.extend_from_slice(&((data_start + i * row_size) as i64).to_le_bytes());
            b.extend_from_slice(&(type_at as i64).to_le_bytes());
        }
        for (_, data) in rows {
            let mut d = data.clone();
            d.resize(row_size, 0);
            b.extend_from_slice(&d);
        }
        b.extend_from_slice(type_name.as_bytes());
        b.push(0);
        b
    }

    #[test]
    fn a_table_is_found_among_other_bytes() {
        let t = make("ITEMLOT_PARAM_ST", 8, &[(10, vec![1]), (20, vec![2])]);
        let mut mem = vec![0xAAu8; 0x30];
        mem.extend_from_slice(&t);
        mem.extend_from_slice(&[0x55; 0x20]);
        let base = 0x7000_0000;
        let found: Vec<Table> = (0..mem.len())
            .filter(|&o| header(&mem[o..]).is_some())
            .filter_map(|o| table(&mem[o..], base + o, "ITEMLOT_PARAM_ST"))
            .collect();
        assert_eq!(found.len(), 1);
        let t = &found[0];
        assert_eq!(t.at, base + 0x30);
        assert_eq!(t.row_size, 8);
        assert_eq!(t.rows[1].0, 20);
        let at = t.rows[1].1 - base;
        assert_eq!(mem[at], 2);
        assert!(table(&mem[0x30..], base + 0x30, "SHOP_LINEUP_PARAM").is_none());
    }
}
