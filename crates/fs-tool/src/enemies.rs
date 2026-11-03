//! What the enemy randomizer has to work with: how the placed enemies divide,
//! and each boss as the scripts name it.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;

use fs_bytes::Result;
use fs_emevd::catalogue;
use fs_msb::layout::{entry_name, field};
use gamedata::Sources;

const PARTS: &str = "PARTS_PARAM_ST";

pub fn run(game: PathBuf) -> Result<()> {
    let s = Sources::open(&game)?;
    let get = |e: &[u8], n: &str| field(PARTS, n).get(e, 0).unwrap_or(-1);
    let npcs: HashMap<i32, Vec<u8>> = s
        .reg
        .table("NpcParam")?
        .rows
        .into_iter()
        .map(|r| (r.id, r.data))
        .collect();

    // Every word any script gives an entity-shaped argument, by value.
    let mut bars: BTreeMap<u32, BTreeSet<(String, i32)>> = BTreeMap::new();
    let bar = catalogue::named("boss_health_bar");
    for (m, script) in &s.scripts {
        for u in catalogue::uses(script, Some(&s.common_func), bar.bank, bar.id) {
            if let (Some(b), Some(t)) = (u.word(bar.arg("boss").word), u.word(bar.arg("name").word))
            {
                bars.entry(b).or_default().insert((m.to_string(), t as i32));
            }
        }
    }

    let mut counts: BTreeMap<(i64, bool, bool, bool), usize> = BTreeMap::new();
    let mut by_entity: HashMap<u32, Vec<(String, Vec<u8>)>> = HashMap::new();
    for (m, msb) in &s.maps {
        for e in &msb.table(PARTS).entries {
            let t = get(e, "type");
            if t != 2 && t != 10 {
                continue;
            }
            let ent = get(e, "entity_id");
            *counts
                .entry((t, ent > 0, get(e, "talk_id") > 0, get(e, "chara_init") > 0))
                .or_default() += 1;
            if ent > 0 {
                by_entity
                    .entry(ent as u32)
                    .or_default()
                    .push((m.to_string(), e.clone()));
            }
        }
    }
    println!("type, has entity, has talk, has chara_init: parts");
    for (k, n) in &counts {
        println!("  {k:?}: {n}");
    }

    println!("\nboss bars: entity, map, part, npc, think, talk, name; NpcParam row start");
    for (b, names) in &bars {
        let name = names
            .iter()
            .next()
            .and_then(|(_, t)| s.text("NpcName", *t))
            .unwrap_or("?");
        let placed = by_entity.get(b);
        match placed {
            None => println!("{b} {name}: no enemy part ({} bars)", names.len()),
            Some(parts) => {
                for (m, e) in parts {
                    let n = get(e, "npc_param") as i32;
                    let row = npcs.get(&n);
                    let hp = row.map(|r| i32::from_le_bytes(r[0x24..0x28].try_into().unwrap()));
                    println!(
                        "{b} {name}: {m} {} type {} npc {n} think {} talk {} chara {} +0x24 {hp:?}",
                        entry_name(e).unwrap_or_default(),
                        get(e, "type"),
                        get(e, "think"),
                        get(e, "talk_id"),
                        get(e, "chara_init"),
                    );
                }
            }
        }
    }
    Ok(())
}

/// How NpcParam rows of one character differ.
pub fn diff(game: PathBuf, rows: Vec<i32>) -> Result<()> {
    let s = Sources::open(&game)?;
    let t = s.reg.table("NpcParam")?;
    let picked: Vec<_> = rows
        .iter()
        .filter_map(|id| t.rows.iter().find(|r| r.id == *id))
        .collect();
    println!(
        "rows found: {} of {}; size {}",
        picked.len(),
        rows.len(),
        picked[0].data.len()
    );
    let len = picked[0].data.len();
    for at in (0..len).step_by(4) {
        let w: Vec<i32> = picked
            .iter()
            .map(|r| i32::from_le_bytes(r.data[at..at + 4].try_into().unwrap()))
            .collect();
        if w.iter().any(|v| *v != w[0]) {
            println!("{at:#05x} {w:?}");
        }
    }
    let sp = s.reg.table("SpEffectParam")?;
    println!(
        "SpEffectParam {} rows, size {}",
        sp.rows.len(),
        sp.rows[0].data.len()
    );
    Ok(())
}

/// How SpEffectParam rows differ: as floats where that reads as one.
pub fn sp_diff(game: PathBuf, rows: Vec<i32>) -> Result<()> {
    let s = Sources::open(&game)?;
    let t = s.reg.table("SpEffectParam")?;
    let picked: Vec<_> = rows
        .iter()
        .filter_map(|id| t.rows.iter().find(|r| r.id == *id))
        .collect();
    println!("rows found: {} of {}", picked.len(), rows.len());
    for at in (0..picked[0].data.len()).step_by(4) {
        let w: Vec<f32> = picked
            .iter()
            .map(|r| f32::from_le_bytes(r.data[at..at + 4].try_into().unwrap()))
            .collect();
        if w.iter().any(|v| v.to_bits() != w[0].to_bits()) {
            println!("{at:#05x} {w:?}");
        }
    }
    // Which NpcParam column holds these, over the whole table.
    let n = s.reg.table("NpcParam")?;
    for at in [0x4cusize, 0x58, 0x6c] {
        let mut c: BTreeMap<i32, usize> = BTreeMap::new();
        for r in &n.rows {
            *c.entry(i32::from_le_bytes(r.data[at..at + 4].try_into().unwrap()))
                .or_default() += 1;
        }
        let mut top: Vec<_> = c.iter().collect();
        top.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
        println!(
            "{at:#05x} distinct {} {:?}",
            c.len(),
            &top[..top.len().min(14)]
        );
    }
    Ok(())
}

/// Every event run in a map's script that names an entity, instruction by
/// instruction, with its call's arguments filled in.
pub fn events(game: PathBuf, map: String, entities: Vec<u32>) -> Result<()> {
    let s = Sources::open(&game)?;
    let Some((_, script)) = s.scripts.iter().find(|(m, _)| m.to_string() == map) else {
        return Err(fs_bytes::format_err("no such script"));
    };
    for run in catalogue::runs(script, Some(&s.common_func)) {
        let hit = run.instructions.iter().any(|i| {
            (0..i.args.len() / 4)
                .filter_map(|w| catalogue::Entry::word(i, w))
                .any(|v| entities.contains(&v))
        });
        if !hit {
            continue;
        }
        println!("event {}", run.event.id);
        for i in &run.instructions {
            let words: Vec<i64> = (0..i.args.len() / 4)
                .filter_map(|w| catalogue::Entry::word(i, w))
                .map(|v| i64::from(v as i32))
                .collect();
            let name = catalogue::lookup(i.bank, i.id).map_or("", |e| e.name.as_str());
            println!("  {}[{}] {name} {words:?}", i.bank, i.id);
        }
    }
    Ok(())
}

/// Every placed enemy, tab-separated: where it stands, what the map gives it,
/// its name, and its NpcParam row in hex.
pub fn dump(game: PathBuf, out: PathBuf) -> Result<()> {
    let s = Sources::open(&game)?;
    let get = |e: &[u8], n: &str| field(PARTS, n).get(e, 0).unwrap_or(-1);
    let npcs: HashMap<i32, Vec<u8>> = s
        .reg
        .table("NpcParam")?
        .rows
        .into_iter()
        .map(|r| (r.id, r.data))
        .collect();
    let mut text = String::new();
    for (m, msb) in &s.maps {
        for e in &msb.table(PARTS).entries {
            let t = get(e, "type");
            if t != 2 && t != 10 {
                continue;
            }
            let n = get(e, "npc_param") as i32;
            let Some(row) = npcs.get(&n) else { continue };
            let name_id = i32::from_le_bytes(row[0x0C..0x10].try_into().unwrap());
            let name = Some(name_id)
                .filter(|&i| i > 0)
                .and_then(|i| s.text("NpcName", i))
                .unwrap_or("");
            let hex: String = row.iter().map(|b| format!("{b:02x}")).collect();
            text += &format!(
                "{m}\t{}\t{t}\t{}\t{n}\t{}\t{}\t{}\t{name}\t{hex}\n",
                entry_name(e).unwrap_or_default(),
                get(e, "entity_id"),
                get(e, "think"),
                get(e, "talk_id"),
                get(e, "chara_init"),
            );
        }
    }
    std::fs::write(out, text)?;
    Ok(())
}

/// Map entries that hold a number, with where it sits and the plausible floats
/// in the entry, for finding what lies where.
pub fn find(game: PathBuf, map: String, values: Vec<u32>) -> Result<()> {
    let s = Sources::open(&game)?;
    let maps: Vec<_> = s
        .maps
        .iter()
        .filter(|(m, _)| map == "all" || m.to_string() == map)
        .collect();
    if maps.is_empty() {
        return Err(fs_bytes::format_err("no such map"));
    }
    for (m, msb) in maps {
        for t in &msb.tables {
            for e in &t.entries {
                let words: Vec<u32> = e
                    .chunks_exact(4)
                    .map(|w| u32::from_le_bytes(w.try_into().unwrap()))
                    .collect();
                for v in &values {
                    if let Some(at) = words.iter().position(|w| w == v) {
                        let floats: Vec<String> = words
                            .iter()
                            .enumerate()
                            .take(64)
                            .filter_map(|(i, w)| {
                                let f = f32::from_bits(*w);
                                (f.is_finite() && f.abs() > 0.01 && f.abs() < 5000.0)
                                    .then(|| format!("{:x}:{f:.1}", i * 4))
                            })
                            .collect();
                        println!(
                            "{v} {m} {} {} at {:x}: {}",
                            t.name,
                            entry_name(e).unwrap_or_default(),
                            at * 4,
                            floats.join(" ")
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

/// Every part and point of a map nearer than `radius` to a part with this entity.
pub fn near(game: PathBuf, map: String, entity: u32, radius: f32) -> Result<()> {
    let s = Sources::open(&game)?;
    let Some((_, msb)) = s.maps.iter().find(|(m, _)| m.to_string() == map) else {
        return Err(fs_bytes::format_err("no such map"));
    };
    let pos = |e: &[u8], at: usize| -> Option<[f32; 3]> {
        let f = |o: usize| Some(f32::from_le_bytes(e.get(o..o + 4)?.try_into().ok()?));
        Some([f(at)?, f(at + 4)?, f(at + 8)?])
    };
    let parts = msb.table(PARTS);
    let here = parts
        .entries
        .iter()
        .find(|e| get_entity(e) == entity)
        .and_then(|e| pos(e, 0x20))
        .ok_or_else(|| fs_bytes::format_err("no such entity"))?;
    let mut rows: Vec<(f32, String)> = Vec::new();
    for (table, at) in [(PARTS, 0x20), ("POINT_PARAM_ST", 0x14)] {
        for e in &msb.table(table).entries {
            let Some(p) = pos(e, at) else { continue };
            let d = (0..3).map(|i| (p[i] - here[i]).powi(2)).sum::<f32>().sqrt();
            rows.push((
                d,
                format!("{table} {} {p:?}", entry_name(e).unwrap_or_default()),
            ));
        }
    }
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    println!("{} entries; nearest beyond {radius}:", rows.len());
    for (d, r) in rows.iter().filter(|(d, _)| *d >= radius).take(12) {
        println!("  {d:.0} m {r}");
    }
    let within = rows.iter().filter(|(d, _)| *d < radius);
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for (_, r) in within {
        let name = r.split_whitespace().nth(1).unwrap_or("");
        let k = name.split(['_', '-']).next().unwrap_or("").to_string();
        *kinds.entry(k).or_default() += 1;
    }
    println!("within {radius} m: {kinds:?}");
    Ok(())
}

fn get_entity(e: &[u8]) -> u32 {
    field(PARTS, "entity_id").get(e, 0).unwrap_or(-1) as u32
}

/// What each movable boss's script tells it to do, tab-separated, with the
/// arguments filled in: what a newcomer on that spot would be told.
pub fn boss_commands(game: PathBuf, out: PathBuf) -> Result<()> {
    let s = Sources::open(&game)?;
    let model = gamedata::elden_ring::extract(&s)?;
    let pool = ::enemies::pool(&s, &model)?;
    let mut text = String::new();
    for spot in pool.bosses() {
        let (Some(boss), Some(entity)) = (&spot.boss, spot.entity) else {
            continue;
        };
        let Some((_, script)) = s.scripts.iter().find(|(m, _)| *m == boss.script) else {
            continue;
        };
        for run in catalogue::runs(script, Some(&s.common_func)) {
            let named = run.instructions.iter().any(|i| {
                (0..i.args.len() / 4).any(|w| catalogue::Entry::word(i, w) == Some(entity))
            });
            for (n, i) in run.instructions.iter().enumerate() {
                let words: Vec<i64> = (0..i.args.len() / 4)
                    .filter_map(|w| catalogue::Entry::word(i, w))
                    .map(|v| i64::from(v as i32))
                    .collect();
                // Cutscenes too, in any event that deals with this boss.
                if words.contains(&i64::from(entity)) || (named && i.bank == 2002) {
                    text += &format!(
                        "{}\t{entity}\t{}\t{}\t{n}\t{}\t{}\t{words:?}\n",
                        boss.script, spot.body.name, run.event.id, i.bank, i.id
                    );
                }
            }
        }
    }
    std::fs::write(out, text)?;
    Ok(())
}

/// Every event that changes a boss's stage: one that waits on the boss's own
/// state (an effect it has, 4[5] or 4[34]; its health as a share, 4[2], or as a
/// value above nothing, 4[14]) and does not call the defeat. Prints the wait and
/// everything the event does, with what the pool says of the boss.
pub fn boss_phases(game: PathBuf) -> Result<()> {
    let s = Sources::open(&game)?;
    let model = gamedata::elden_ring::extract(&s)?;
    let pool = ::enemies::pool(&s, &model)?;
    let moving: BTreeSet<u32> = pool.bosses().filter_map(|b| b.entity).collect();
    let bar = catalogue::named("boss_health_bar");
    let defeat = catalogue::named("boss_defeated");
    let mut bars: BTreeMap<u32, String> = BTreeMap::new();
    for (_, script) in &s.scripts {
        for u in catalogue::uses(script, Some(&s.common_func), bar.bank, bar.id) {
            if let (Some(b), Some(t)) = (u.word(bar.arg("boss").word), u.word(bar.arg("name").word))
            {
                let name = s.text("NpcName", t as i32).unwrap_or("?").to_string();
                bars.entry(b).or_insert(name);
            }
        }
    }
    let word = |i: &fs_emevd::Instruction, w: usize| catalogue::Entry::word(i, w);
    for (m, script) in &s.scripts {
        for run in catalogue::runs(script, Some(&s.common_func)) {
            if run
                .instructions
                .iter()
                .any(|i| (i.bank, i.id) == (defeat.bank, defeat.id))
            {
                continue;
            }
            let waits: Vec<(u32, String)> = run
                .instructions
                .iter()
                .filter_map(|i| {
                    let e = word(i, 1)?;
                    bars.contains_key(&e).then_some(())?;
                    let what = match (i.bank, i.id) {
                        (4, 5) => format!("has effect {}", word(i, 2)? as i32),
                        (4, 34) => format!("has effect type {}", word(i, 2)? as i32),
                        (4, 2) => format!(
                            "health share, test {} against {}",
                            word(i, 2)?,
                            f32::from_bits(word(i, 3)?)
                        ),
                        (4, 14) if word(i, 3)? > 0 => {
                            format!("health, test {} against {}", word(i, 2)?, word(i, 3)?)
                        }
                        _ => return None,
                    };
                    Some((e, what))
                })
                .collect();
            for (e, what) in waits {
                println!(
                    "{m} event {} ({}) {} [{e}] {}: {what}",
                    run.event.id,
                    run.caller.map_or("top".into(), |c| format!("from {c}")),
                    bars[&e],
                    if moving.contains(&e) {
                        "moves"
                    } else {
                        "stays"
                    },
                );
                for i in &run.instructions {
                    let words: Vec<i64> = (0..i.args.len() / 4)
                        .filter_map(|w| word(i, w))
                        .map(|v| i64::from(v as i32))
                        .collect();
                    let name = catalogue::lookup(i.bank, i.id).map_or("", |c| c.name.as_str());
                    println!("    {}[{}] {name} {words:?}", i.bank, i.id);
                }
            }
        }
    }
    Ok(())
}

/// Every use of one instruction in every script: map, event, arguments.
pub fn instruction_uses(game: PathBuf, bank: u32, id: u32) -> Result<()> {
    let s = Sources::open(&game)?;
    for (m, script) in &s.scripts {
        for u in catalogue::uses(script, Some(&s.common_func), bank, id) {
            let words: Vec<i64> = (0..u.args.len() / 4)
                .filter_map(|w| u.word(w))
                .map(|v| i64::from(v as i32))
                .collect();
            println!("{m} {} {words:?}", u.event);
        }
    }
    Ok(())
}

/// A map's creature models as bytes, and how many parts use each.
pub fn models(game: PathBuf, map: String) -> Result<()> {
    let s = Sources::open(&game)?;
    let Some((_, msb)) = s.maps.iter().find(|(m, _)| m.to_string() == map) else {
        return Err(fs_bytes::format_err("no such map"));
    };
    let parts = &msb.table(PARTS).entries;
    for (i, e) in msb.table("MODEL_PARAM_ST").entries.iter().enumerate() {
        let name = entry_name(e).unwrap_or_default();
        if !name.starts_with('c') {
            continue;
        }
        let users = parts
            .iter()
            .filter(|p| field(PARTS, "model").get(p, 0) == Some(i as i64))
            .count();
        let hex: Vec<String> = e
            .chunks(8)
            .map(|c| c.iter().rev().map(|b| format!("{b:02x}")).collect())
            .collect();
        println!("{i} {name} users {users} len {}", e.len());
        println!("  {}", hex.join(" "));
        println!("  {}", String::from_utf8_lossy(e).replace('\0', ""));
    }
    Ok(())
}
