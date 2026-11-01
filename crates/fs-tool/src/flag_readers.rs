//! What reads the pick-up flag of a key or quest item. A flag stays with its slot
//! when the item moves, so a door or a talk that tests it asks for the slot, not the
//! item, and the area map's requirements cannot see that.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use fs_bytes::Result;
use fs_emevd::catalogue;

pub fn run(game: PathBuf) -> Result<()> {
    let s = gamedata::Sources::open(&game)?;
    let model = gamedata::elden_ring::extract(&s)?;
    let graph = logic::elden_ring();
    let bound = logic::Bound::new(&graph, &model);
    let keys = bound.key_items();
    let quests: BTreeSet<&str> = model
        .quest_items
        .iter()
        .filter_map(|i| i.name.as_deref())
        .collect();
    // Each flag with the key and quest items its location gives.
    let mut flags: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
    for l in &model.locations {
        let Some(flag) = l.flag else { continue };
        for n in l.items.iter().filter_map(|i| i.name.as_deref()) {
            let tag = match (keys.contains(n), quests.contains(n)) {
                (true, _) => "key",
                (_, true) => "quest",
                _ => continue,
            };
            flags
                .entry(flag)
                .or_default()
                .insert(format!("{n} [{tag}] {}", l.source));
        }
    }
    let mut scripts: Vec<(String, &fs_emevd::Emevd)> =
        s.scripts.iter().map(|(m, e)| (m.to_string(), e)).collect();
    scripts.push(("common".to_string(), &s.common));
    let mut readers: BTreeMap<u32, BTreeSet<String>> = BTreeMap::new();
    for (map, script) in &scripts {
        for run in catalogue::runs(script, Some(&s.common_func)) {
            for i in &run.instructions {
                for w in i.args.chunks_exact(4) {
                    let v = u32::from_le_bytes(w.try_into().unwrap());
                    if flags.contains_key(&v) {
                        let words: Vec<i32> = i
                            .args
                            .chunks_exact(4)
                            .map(|c| i32::from_le_bytes(c.try_into().unwrap()))
                            .collect();
                        readers.entry(v).or_default().insert(format!(
                            "{map} event {} {}[{}] {words:?}",
                            run.event.id, i.bank, i.id
                        ));
                    }
                }
            }
        }
    }
    for (_, name, esd) in &s.talk {
        for &f in flags.keys() {
            let bytes = f.to_le_bytes();
            let n = esd.expressions.windows(4).filter(|w| *w == bytes).count();
            let set = fs_esd::calls::resolve(esd)
                .iter()
                .filter(|r| r.args.contains(&Some(f as i32)))
                .map(|r| format!("{}:{}", r.bank, r.id))
                .collect::<BTreeSet<_>>();
            if n > 0 || !set.is_empty() {
                readers
                    .entry(f)
                    .or_default()
                    .insert(format!("talk {name}: {n} expressions, commands {set:?}"));
            }
        }
    }
    let read = flags.keys().filter(|f| readers.contains_key(f)).count();
    println!(
        "{} key or quest flags, {read} read by something",
        flags.len()
    );
    for (f, items) in &flags {
        let Some(r) = readers.get(f) else { continue };
        println!("\n== {f}: {items:?}");
        for line in r {
            println!("  {line}");
        }
    }
    Ok(())
}
