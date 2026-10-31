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
