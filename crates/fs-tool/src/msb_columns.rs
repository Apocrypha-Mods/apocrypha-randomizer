//! Column statistics for one MSB table across every map: for each offset
//! within an entry, the spread of values and which known sets they fall in.
//! The raw material for MSB field layouts, as `columns` is for params.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use fs_bytes::Result;
use fs_dvdbnd::Dvd;
use fs_msb::Msb;
use fs_param::Regulation;
use rayon::prelude::*;

use crate::instructions::{Known, describe, known_sets};

/// Every map, found by trying each name the pattern
/// `/map/mapstudio/mAA_BB_CC_DD.msb.dcx` allows: all 1,347.
pub fn load_maps(dvd: &Dvd) -> Vec<(String, Msb)> {
    let names: Vec<String> = dvd
        .map_ids("/map/mapstudio/", ".msb.dcx")
        .iter()
        .map(|m| format!("/map/mapstudio/{m}.msb.dcx"))
        .collect();
    names
        .par_iter()
        .filter_map(|p| {
            let plain = fs_dcx::decompress(&dvd.read_path(p).ok()?).ok()?.1;
            Some((p.clone(), Msb::read(&plain).ok()?))
        })
        .collect()
}

pub struct Query {
    pub table: String,
    /// Keep only entries whose i32 at this offset has this value.
    pub type_at: Option<(usize, i32)>,
    pub from: usize,
    pub to: Option<usize>,
    /// Measure offsets from the section whose (i64) offset the entry holds
    /// here, rather than from the entry's start.
    pub base: Option<usize>,
}

pub fn run(game: PathBuf, cache: PathBuf, q: Query) -> Result<()> {
    let reg = Regulation::open(&game)?;
    let dvd = Dvd::open_elden_ring(&game)?;
    let maps = load_maps(&dvd);
    let scripts = crate::scripts::load(&game, &cache)?;
    let text = gamedata::sources::text(&dvd)?;
    let mut sets = known_sets(&reg, &scripts, &text)?;
    // Entity ids the instruction catalogue's entity arguments take.
    let refs: Vec<&fs_emevd::Emevd> = scripts.iter().map(|(_, e)| e).collect();
    let mut entities = HashSet::new();
    for e in fs_emevd::catalogue::elden_ring() {
        for a in e
            .args
            .iter()
            .filter(|a| a.kind == fs_emevd::catalogue::ArgKind::Entity)
        {
            entities.extend(fs_emevd::catalogue::values(
                &refs, None, e.bank, e.id, a.word,
            ));
        }
    }
    sets.push(Known {
        name: "script entity".into(),
        values: entities,
    });

    let entries: Vec<&[u8]> = maps
        .iter()
        .flat_map(|(_, m)| &m.table(&q.table).entries)
        .filter(|e| {
            q.type_at.is_none_or(|(at, v)| {
                e.get(at..at + 4)
                    .is_some_and(|b| i32::from_le_bytes(b.try_into().unwrap()) == v)
            })
        })
        .filter_map(|e| match q.base {
            None => Some(&e[..]),
            Some(at) => {
                let base = i64::from_le_bytes(e.get(at..at + 8)?.try_into().unwrap());
                e.get(usize::try_from(base).ok().filter(|&b| b > 0)?..)
            }
        })
        .collect();
    let max_len = entries.iter().map(|e| e.len()).max().unwrap_or(0);
    println!(
        "{} maps; {} {} entries, up to {max_len} bytes",
        maps.len(),
        entries.len(),
        q.table
    );
    let to = q.to.unwrap_or(max_len);
    for off in (q.from..to).step_by(4) {
        let mut vals: HashMap<u32, usize> = HashMap::new();
        for e in &entries {
            if let Some(b) = e.get(off..off + 4) {
                *vals
                    .entry(u32::from_le_bytes(b.try_into().unwrap()))
                    .or_default() += 1;
            }
        }
        if !vals.is_empty() {
            println!("  +{off:#05x} {}", describe(&vals, &sets));
        }
    }
    Ok(())
}
