//! How the param fields named as flags show up in the event scripts.
//!
//! For each such field it reports what share of its distinct values occur as
//! a 4-byte word in some instruction's arguments, and the instructions (bank,
//! id, word position) they occur in most. A field whose values the scripts
//! test and set in a handful of instructions is an event flag; those
//! instructions are then the ones that read and write flags.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use fs_bytes::Result;
use fs_dvdbnd::Dvd;
use fs_emevd::Emevd;
use fs_param::{Regulation, layout};
use rayon::prelude::*;

/// Where a word sits: instruction bank, id, and word index in the arguments.
type Place = (u32, u32, usize);

/// The argument words that hold a flag, from the instruction catalogue.
fn flag_words() -> Vec<Place> {
    fs_emevd::catalogue::elden_ring()
        .iter()
        .flat_map(|e| {
            e.args
                .iter()
                .filter(|a| a.kind == fs_emevd::catalogue::ArgKind::Flag)
                .map(move |a| (e.bank, e.id, a.word))
        })
        .collect()
}

pub fn run(game: PathBuf) -> Result<()> {
    let reg = Regulation::open(&game)?;
    let dvd = Dvd::open_elden_ring(&game)?;

    // Every flag-named field's distinct values, skipping 0 and -1.
    let mut fields: Vec<(String, HashSet<u32>)> = Vec::new();
    for l in layout::elden_ring() {
        for f in l.fields.iter().filter(|f| f.name.contains("flag")) {
            let mut vals = HashSet::new();
            for t in &l.tables {
                for r in &reg.table(t)?.rows {
                    for i in 0..f.count {
                        let v = f.get(&r.data, i) as u32;
                        if v != 0 && v != u32::MAX {
                            vals.insert(v);
                        }
                    }
                }
            }
            fields.push((format!("{}.{}", l.tables.join("/"), f.name), vals));
        }
    }
    let wanted: HashSet<u32> = fields.iter().flat_map(|(_, v)| v.iter().copied()).collect();

    // Every place each wanted value occurs in any script.
    let scripts: Vec<_> = dvd.entries().map(|(r, _)| r).collect();
    let seen: Vec<HashMap<u32, Vec<Place>>> = scripts
        .par_iter()
        .filter_map(|&r| {
            let stored = dvd.read(r).ok()?;
            let plain = fs_dcx::decompress(&stored).ok()?.1;
            let e = fs_emevd::is_emevd(&plain)
                .then(|| Emevd::read(&plain).ok())
                .flatten()?;
            let mut m: HashMap<u32, Vec<Place>> = HashMap::new();
            for ins in e.events.iter().flat_map(|e| &e.instructions) {
                for (w, word) in ins.args.chunks_exact(4).enumerate() {
                    let v = u32::from_le_bytes(word.try_into().unwrap());
                    if wanted.contains(&v) {
                        m.entry(v).or_default().push((ins.bank, ins.id, w));
                    }
                }
            }
            Some(m)
        })
        .collect();
    let mut places: HashMap<u32, Vec<Place>> = HashMap::new();
    for m in seen {
        for (v, p) in m {
            places.entry(v).or_default().extend(p);
        }
    }

    let flag_words = flag_words();
    for (name, vals) in &fields {
        let found = vals.iter().filter(|v| places.contains_key(v)).count();
        let as_flag = vals
            .iter()
            .filter(|v| {
                places
                    .get(v)
                    .is_some_and(|p| p.iter().any(|p| flag_words.contains(p)))
            })
            .count();
        let mut by_place: BTreeMap<Place, usize> = BTreeMap::new();
        for v in vals {
            for p in places.get(v).into_iter().flatten().collect::<HashSet<_>>() {
                *by_place.entry(*p).or_default() += 1;
            }
        }
        let mut top: Vec<_> = by_place.into_iter().collect();
        top.sort_by_key(|p| std::cmp::Reverse(p.1));
        let top: Vec<String> = top
            .iter()
            .take(6)
            .map(|((b, i, w), n)| format!("{b}[{i}] word {w}: {n}"))
            .collect();
        println!(
            "{name}: {found}/{} distinct values occur in the scripts ({:.0}%), {as_flag} in a flag word\n    {}",
            vals.len(),
            100.0 * found as f64 / vals.len().max(1) as f64,
            top.join("; ")
        );
    }
    Ok(())
}
