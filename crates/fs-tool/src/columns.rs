//! Column statistics for one param table, the raw material for working out
//! its field layout from the data alone (PLAN.md §6: Paramdex has no licence).
//!
//! For each offset it reports, read as i32, u16, u8 and f32: how many distinct
//! values there are, how many rows are zero, the range, the commonest values,
//! and which other tables' row ids the non-zero values fall in.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use fs_bytes::Result;
use fs_param::{Param, Regulation};

#[derive(Clone, Copy)]
enum Width {
    W32,
    W16,
    W8,
}

/// Keep only rows whose i32 (or, written `OFFb`, byte) at `off` is `value`.
#[derive(Clone)]
pub struct Filter {
    pub off: usize,
    pub byte: bool,
    pub value: i32,
}

impl Filter {
    fn matches(&self, row: &[u8]) -> bool {
        if self.byte {
            i32::from(row[self.off]) == self.value
        } else {
            i32::from_le_bytes(row[self.off..self.off + 4].try_into().unwrap()) == self.value
        }
    }
}

impl std::str::FromStr for Filter {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        let (off, value) = s.split_once('=').ok_or("expected OFFSET=VALUE")?;
        let off = off.trim_start_matches("0x");
        let (off, byte) = match off.strip_suffix('b') {
            Some(o) => (o, true),
            None => (off, false),
        };
        Ok(Filter {
            off: usize::from_str_radix(off, 16).map_err(|e| e.to_string())?,
            byte,
            value: value
                .parse()
                .map_err(|e: std::num::ParseIntError| e.to_string())?,
        })
    }
}

pub fn run(
    game: PathBuf,
    table: &str,
    widths: &str,
    from: usize,
    to: Option<usize>,
    filter: &[Filter],
) -> Result<()> {
    let reg = Regulation::open(&game)?;
    let mut p = reg.table(table)?;
    p.rows.retain(|r| filter.iter().all(|f| f.matches(&r.data)));
    if p.rows.is_empty() {
        println!("no rows match");
        return Ok(());
    }
    let ids: Vec<(String, HashSet<i32>)> = reg
        .table_names()
        .map(str::to_string)
        .collect::<Vec<_>>()
        .into_iter()
        .filter_map(|n| {
            reg.table(&n)
                .ok()
                .map(|t| (n, t.rows.iter().map(|r| r.id).collect()))
        })
        .collect();
    println!(
        "{table}: {} rows of {} bytes ({})",
        p.rows.len(),
        p.row_size,
        String::from_utf8_lossy(&p.type_name)
    );
    let to = to.unwrap_or(p.row_size);
    let mut off = from;
    while off < to {
        for (w, ch) in [(Width::W32, '4'), (Width::W16, '2'), (Width::W8, '1')] {
            let size = match w {
                Width::W32 => 4,
                Width::W16 => 2,
                Width::W8 => 1,
            };
            if widths.contains(ch) && off.is_multiple_of(size) && off + size <= p.row_size {
                describe(&p, off, w, &ids);
            }
        }
        off += 1;
    }
    Ok(())
}

fn value(row: &[u8], off: usize, w: Width) -> i64 {
    match w {
        Width::W32 => i64::from(i32::from_le_bytes(row[off..off + 4].try_into().unwrap())),
        Width::W16 => i64::from(u16::from_le_bytes(row[off..off + 2].try_into().unwrap())),
        Width::W8 => i64::from(row[off]),
    }
}

fn describe(p: &Param, off: usize, w: Width, ids: &[(String, HashSet<i32>)]) {
    let vals: Vec<i64> = p.rows.iter().map(|r| value(&r.data, off, w)).collect();
    let mut counts: HashMap<i64, usize> = HashMap::new();
    for &v in &vals {
        *counts.entry(v).or_default() += 1;
    }
    let zeros = counts.get(&0).copied().unwrap_or(0);
    let (min, max) = (vals.iter().min().unwrap(), vals.iter().max().unwrap());
    let mut top: Vec<(i64, usize)> = counts.iter().map(|(&v, &c)| (v, c)).collect();
    top.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let top: Vec<String> = top
        .iter()
        .take(6)
        .map(|(v, c)| format!("{v}×{c}"))
        .collect();
    let tag = match w {
        Width::W32 => "i32",
        Width::W16 => "u16",
        Width::W8 => "u8 ",
    };
    let mut extra = String::new();
    if let Width::W32 = w {
        // Floats: share of non-zero values that read as ordinary floats of
        // modest size, which random integers almost never do.
        let floats = vals
            .iter()
            .filter(|&&v| v != 0)
            .filter(|&&v| {
                let f = f32::from_bits(v as u32);
                f.is_normal() && (1e-4..1e7).contains(&f.abs())
            })
            .count();
        let nonzero = vals.len() - zeros;
        if nonzero > 0 && floats * 10 >= nonzero * 9 {
            let fs: Vec<f32> = vals.iter().map(|&v| f32::from_bits(v as u32)).collect();
            let lo = fs.iter().cloned().fold(f32::INFINITY, f32::min);
            let hi = fs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            extra.push_str(&format!(" FLOAT {lo}..{hi}"));
        }
        // References: tables whose row ids hold most of the non-zero,
        // non-minus-one values.
        let refs: Vec<i32> = vals
            .iter()
            .filter(|&&v| v != 0 && v != -1)
            .map(|&v| v as i32)
            .collect();
        let distinct: HashSet<i32> = refs.iter().copied().collect();
        if distinct.len() >= 3 {
            let mut hits: BTreeMap<usize, Vec<&str>> = BTreeMap::new();
            for (name, set) in ids {
                let n = distinct.iter().filter(|v| set.contains(v)).count();
                if n * 100 >= distinct.len() * 80 {
                    hits.entry(n * 100 / distinct.len()).or_default().push(name);
                }
            }
            if let Some((pct, names)) = hits.iter().next_back() {
                let shown: Vec<&str> = names.iter().take(5).copied().collect();
                extra.push_str(&format!(
                    " REFS {pct}% {}{}",
                    shown.join(","),
                    if names.len() > 5 { ",…" } else { "" }
                ));
            }
        }
    }
    println!(
        "  {off:#05x} {tag} distinct {:5} zero {:5} range {min}..{max}  top [{}]{extra}",
        counts.len(),
        zeros,
        top.join(" ")
    );
}
