//! Which scripts and tables hold the id of each key item: a door that asks for the
//! item itself follows it wherever it moves.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use fs_bytes::Result;
use fs_emevd::catalogue;
use gamedata::model::ItemKind;

pub fn run(game: PathBuf) -> Result<()> {
    let s = gamedata::Sources::open(&game)?;
    let model = gamedata::elden_ring::extract(&s)?;
    let graph = logic::elden_ring();
    let bound = logic::Bound::new(&graph, &model);
    let keys = bound.key_items();
    let mut ids: BTreeMap<i32, &str> = BTreeMap::new();
    for i in model.locations.iter().flat_map(|l| &l.items) {
        if i.kind == ItemKind::Goods
            && let Some(n) = i.name.as_deref()
            && keys.contains(n)
        {
            ids.insert(i.id, n);
        }
    }
    let mut scripts: Vec<(String, &fs_emevd::Emevd)> =
        s.scripts.iter().map(|(m, e)| (m.to_string(), e)).collect();
    scripts.push(("common".to_string(), &s.common));
    let mut found: BTreeMap<i32, BTreeSet<String>> = BTreeMap::new();
    for (map, script) in &scripts {
        for run in catalogue::runs(script, Some(&s.common_func)) {
            for i in &run.instructions {
                if (i.bank, i.id) == (2000, 0) || (i.bank, i.id) == (2000, 6) {
                    continue;
                }
                let words: Vec<i32> = i
                    .args
                    .chunks_exact(4)
                    .map(|c| i32::from_le_bytes(c.try_into().unwrap()))
                    .collect();
                // Only instructions testing goods: 3[4] and its kin in other banks.
                if i.id != 4 && i.bank != 3 {
                    continue;
                }
                for w in &words {
                    if ids.contains_key(w) {
                        found.entry(*w).or_default().insert(format!(
                            "{map} event {} {}[{}] {words:?}",
                            run.event.id, i.bank, i.id
                        ));
                    }
                }
            }
        }
    }
    for table in ["ActionButtonParam", "ObjActParam"] {
        let Ok(t) = s.reg.table(table) else { continue };
        for r in &t.rows {
            for w in r.data.chunks_exact(4) {
                let v = i32::from_le_bytes(w.try_into().unwrap());
                if ids.contains_key(&v) {
                    found
                        .entry(v)
                        .or_default()
                        .insert(format!("param {table} row {}", r.id));
                }
            }
        }
    }
    for (_, name, esd) in &s.talk {
        for &id in ids.keys() {
            let b = id.to_le_bytes();
            if esd.expressions.windows(4).any(|w| w == b) {
                found.entry(id).or_default().insert(format!("talk {name}"));
            }
        }
    }
    for (id, n) in &ids {
        println!("\n== {id} {n}");
        for l in found.get(id).into_iter().flatten() {
            println!("  {l}");
        }
    }
    Ok(())
}
