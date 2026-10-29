//! For each small-valued column of a table, every value with the names of a
//! few rows that have it: how a field's meaning is read off once rows have
//! names. Rows are named from the name table given (`GoodsName` for
//! EquipParamGoods, and so on).

use std::collections::BTreeMap;
use std::path::PathBuf;

use fs_bytes::{Result, format_err};
use fs_dvdbnd::Dvd;
use fs_param::Regulation;

pub fn run(
    game: PathBuf,
    table: &str,
    names: &str,
    from: usize,
    to: Option<usize>,
    max_distinct: usize,
    samples: usize,
) -> Result<()> {
    let reg = Regulation::open(&game)?;
    let dvd = Dvd::open_elden_ring(&game)?;
    let all = gamedata::sources::text(&dvd)?;
    let names = all
        .get(names)
        .ok_or_else(|| format_err(format!("no name table {names}")))?;
    let p = reg.table(table)?;
    let to = to.unwrap_or(p.row_size);
    let label = |id: i32| names.get(&id).cloned().unwrap_or_else(|| format!("#{id}"));
    for off in from..to {
        for (width, tag) in [(1usize, "u8"), (2, "u16"), (4, "i32")] {
            if !off.is_multiple_of(width) || off + width > p.row_size {
                continue;
            }
            let mut by: BTreeMap<i64, Vec<i32>> = BTreeMap::new();
            for r in &p.rows {
                let b = &r.data[off..off + width];
                let v = match width {
                    1 => i64::from(b[0]),
                    2 => i64::from(u16::from_le_bytes(b.try_into().unwrap())),
                    _ => i64::from(i32::from_le_bytes(b.try_into().unwrap())),
                };
                by.entry(v).or_default().push(r.id);
            }
            if by.len() < 2 || by.len() > max_distinct {
                continue;
            }
            // A wider read of a narrower field repeats it; show the narrowest.
            if width > 1 && by.values().any(|ids| ids.len() == p.rows.len()) {
                continue;
            }
            println!("{off:#05x} {tag}:");
            for (v, ids) in &by {
                let named: Vec<String> = ids
                    .iter()
                    .filter(|id| names.contains_key(id))
                    .take(samples)
                    .map(|&id| label(id))
                    .collect();
                println!("    {v:>10} ×{:<5} {}", ids.len(), named.join(" | "));
            }
        }
    }
    Ok(())
}
