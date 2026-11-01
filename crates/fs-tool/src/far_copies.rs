//! Open-world enemies on coarse tiles, whether a fine tile holds the same one, and
//! how many human-built and ride-tied bodies the pool has.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use fs_bytes::Result;
use fs_msb::layout::{entry_name, field};
use gamedata::Sources;

const PARTS: &str = "PARTS_PARAM_ST";
const MODELS: &str = "MODEL_PARAM_ST";

/// A model, and where it stands to the nearest 2 m.
type Key = (String, i32, i32);

pub fn run(game: PathBuf) -> Result<()> {
    let s = Sources::open(&game)?;
    let model = gamedata::elden_ring::extract(&s)?;
    let pool = enemies::pool(&s, &model)?;
    // Every enemy part of the open world, at its place on the world's ground.
    let mut near: HashMap<Key, Vec<(u8, u32, i64)>> = HashMap::new();
    let mut coarse = Vec::new();
    for (i, (m, msb)) in s.maps.iter().enumerate() {
        if !m.is_open_world() {
            continue;
        }
        let names = &msb.table(MODELS).entries;
        for (p, e) in msb.table(PARTS).entries.iter().enumerate() {
            if field(PARTS, "type").get(e, 0) != Some(2) {
                continue;
            }
            let at = |n| field(PARTS, "position").get_f32(e, n).unwrap_or(0.0);
            let Some(w) = m.world([at(0), at(1), at(2)]) else {
                continue;
            };
            let name = usize::try_from(field(PARTS, "model").get(e, 0).unwrap_or(-1))
                .ok()
                .and_then(|i| names.get(i))
                .and_then(|e| entry_name(e))
                .unwrap_or_default();
            let en = field(PARTS, "entity_id").get(e, 0).unwrap_or(0) as u32;
            let npc = field(PARTS, "npc_param").get(e, 0).unwrap_or(0);
            let level = m.0[3];
            let key = (
                name.clone(),
                (w[0] / 2.0).round() as i32,
                (w[1] / 2.0).round() as i32,
            );
            near.entry(key.clone()).or_default().push((level, en, npc));
            if level > 0 {
                coarse.push((i, p, *m, key, en, npc));
            }
        }
    }
    let mut level_counts: BTreeMap<u8, usize> = BTreeMap::new();
    let (mut paired, mut same_entity, mut same_npc) = (0, 0, 0);
    for (_, _, m, key, en, npc) in &coarse {
        *level_counts.entry(m.0[3]).or_default() += 1;
        let fine: Vec<_> = near[key].iter().filter(|c| c.0 < m.0[3]).collect();
        if !fine.is_empty() {
            paired += 1;
            same_entity += usize::from(fine.iter().any(|c| c.1 == *en && *en != 0));
            same_npc += usize::from(fine.iter().any(|c| c.2 == *npc));
        }
    }
    println!(
        "coarse enemy parts by level {level_counts:?}; {paired} have a finer part of the same model within 2 m; {same_entity} of those share an entity, {same_npc} an NpcParam row"
    );
    let moved_coarse = pool
        .spots
        .iter()
        .filter(|p| p.map.is_open_world() && p.map.0[3] > 0)
        .count();
    println!(
        "of the pool's {} spots, {moved_coarse} are on coarse tiles",
        pool.spots.len()
    );
    for (_, _, m, key, en, npc) in coarse.iter().take(12) {
        println!(
            "  {m} {} at {},{} entity {en} row {npc}: {:?}",
            key.0,
            key.1 * 2,
            key.2 * 2,
            near[key]
        );
    }
    // A wider net: the same entity anywhere, any part type, and the same model within 30 m.
    let mut by_entity: HashMap<u32, Vec<String>> = HashMap::new();
    let mut coarse_types: BTreeMap<i64, usize> = BTreeMap::new();
    let mut everyone: Vec<(u8, String, f32, f32)> = Vec::new();
    for (m, msb) in s.maps.iter() {
        if !m.is_open_world() {
            continue;
        }
        let names = &msb.table(MODELS).entries;
        for e in &msb.table(PARTS).entries {
            let ty = field(PARTS, "type").get(e, 0).unwrap_or(-1);
            if ty != 2 && ty != 10 {
                continue;
            }
            if m.0[3] > 0 {
                *coarse_types.entry(ty).or_default() += 1;
            }
            let en = field(PARTS, "entity_id").get(e, 0).unwrap_or(0) as u32;
            if en != 0 {
                by_entity.entry(en).or_default().push(m.to_string());
            }
            let at = |n| field(PARTS, "position").get_f32(e, n).unwrap_or(0.0);
            let Some(w) = m.world([at(0), at(1), at(2)]) else {
                continue;
            };
            let name = usize::try_from(field(PARTS, "model").get(e, 0).unwrap_or(-1))
                .ok()
                .and_then(|i| names.get(i))
                .and_then(|e| entry_name(e))
                .unwrap_or_default();
            everyone.push((m.0[3], name, w[0], w[1]));
        }
    }
    let shared: Vec<_> = by_entity.iter().filter(|(_, v)| v.len() > 1).collect();
    println!(
        "coarse parts by type {coarse_types:?}; entities on two or more open-world parts: {}",
        shared.len()
    );
    for (en, v) in shared.iter().take(8) {
        println!("  {en}: {v:?}");
    }
    let wide = everyone
        .iter()
        .filter(|c| c.0 > 0)
        .filter(|c| {
            everyone.iter().any(|f| {
                f.0 < c.0 && f.1 == c.1 && (f.2 - c.2).abs() < 30.0 && (f.3 - c.3).abs() < 30.0
            })
        })
        .count();
    println!("coarse parts with a finer part of the same model within 30 m: {wide}");
    // What a seed does: human-built bodies per map at every ordinary enemy moved.
    let options = enemies::Options {
        ordinary: 100,
        ..Default::default()
    };
    let mut worst: BTreeMap<usize, usize> = BTreeMap::new();
    for seed in 0..200 {
        let plan = enemies::shuffle(&pool, seed, &options);
        let mut per: HashMap<usize, usize> = HashMap::new();
        for (i, sp) in pool.spots.iter().enumerate() {
            let b = &pool.spots[plan.from[i]];
            if b.boss.is_none() && b.body.model == "c0000" {
                *per.entry(sp.map_index).or_default() += 1;
            }
        }
        *worst
            .entry(per.values().copied().max().unwrap_or(0))
            .or_default() += 1;
    }
    println!("most human-built bodies in one map, over 200 seeds (count: seeds): {worst:?}");
    let humans: Vec<_> = pool
        .spots
        .iter()
        .filter(|p| p.boss.is_none() && p.body.model == "c0000")
        .collect();
    let named = humans.iter().filter(|p| !p.body.name.is_empty()).count();
    let with_entity = humans.iter().filter(|p| p.entity.is_some()).count();
    println!(
        "human-built ordinary spots {} ({named} named, {with_entity} with an entity)",
        humans.len()
    );
    let mut per: BTreeMap<String, usize> = BTreeMap::new();
    for h in &humans {
        *per.entry(h.map.to_string()).or_default() += 1;
    }
    let mut most: Vec<_> = per.into_iter().collect();
    most.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    most.truncate(10);
    println!("most in a map: {most:?}");
    Ok(())
}

/// Hostile humans in the pool and what scripts do with their spots, and the ordinary
/// spots that share an event with a boss: helpers a fight may rely on.
pub fn helpers(game: PathBuf) -> Result<()> {
    let s = Sources::open(&game)?;
    let model = gamedata::elden_ring::extract(&s)?;
    let pool = enemies::pool(&s, &model)?;
    let npc = fs_param::layout::for_table("NpcParam").unwrap();
    let rows: HashMap<i32, fs_param::Row> = s
        .reg
        .table("NpcParam")?
        .rows
        .into_iter()
        .map(|r| (r.id, r))
        .collect();
    let word = |i: &fs_emevd::Instruction, w| fs_emevd::catalogue::Entry::word(i, w);
    let by_entity: HashMap<u32, &enemies::Spot> = pool
        .spots
        .iter()
        .filter_map(|p| p.entity.map(|e| (e, p)))
        .collect();
    let mut runs_of = HashMap::new();
    for (m, script) in &s.scripts {
        runs_of.insert(*m, fs_emevd::catalogue::runs(script, Some(&s.common_func)));
    }
    let mut classes: BTreeMap<(bool, String), usize> = BTreeMap::new();
    for p in pool.spots.iter().filter(|p| p.boss.is_none()) {
        *classes
            .entry((p.dlc, format!("{:?} {:?}", p.class, p.size)))
            .or_default() += 1;
    }
    println!(
        "ordinary spots {}: {classes:?}",
        classes.values().sum::<usize>()
    );
    println!("--- hostile humans");
    for p in pool
        .spots
        .iter()
        .filter(|p| p.boss.is_none() && p.body.model == "c0000")
    {
        let team = rows
            .get(&p.body.npc)
            .map_or(-1, |r| npc.field("team").get(&r.data, 0));
        let mut evs: Vec<String> = Vec::new();
        if let (Some(en), Some(runs)) = (p.entity, runs_of.get(&p.map)) {
            for r in runs {
                let hit = r
                    .instructions
                    .iter()
                    .any(|i| (0..i.args.len() / 4).any(|w| word(i, w) == Some(en)));
                if hit {
                    evs.push(r.event.id.to_string());
                }
            }
        }
        println!(
            "{} {:?} team {team} row {} {:?} events {evs:?}",
            p.map, p.entity, p.body.npc, p.body.name
        );
    }
    println!("--- ordinary spots named in a boss's events");
    for b in pool.bosses() {
        let (Some(bs), Some(en)) = (&b.boss, b.entity) else {
            continue;
        };
        let Some(runs) = runs_of.get(&bs.script) else {
            continue;
        };
        let mut found: BTreeMap<u32, Vec<String>> = BTreeMap::new();
        for r in runs {
            let hit = r
                .instructions
                .iter()
                .any(|i| (0..i.args.len() / 4).any(|w| word(i, w) == Some(en)));
            if !hit {
                continue;
            }
            for i in &r.instructions {
                for w in 0..i.args.len() / 4 {
                    if let Some(v) = word(i, w) {
                        if v != en && by_entity.get(&v).is_some_and(|o| o.boss.is_none()) {
                            found
                                .entry(v)
                                .or_default()
                                .push(format!("{}:{}[{}]", r.event.id, i.bank, i.id));
                        }
                    }
                }
            }
        }
        for (v, how) in found {
            let o = by_entity[&v];
            let mut how = how;
            how.dedup();
            how.truncate(6);
            println!(
                "{} ({en}) <- {v} {} {:?} {how:?}",
                b.body.name, o.body.model, o.body.name
            );
        }
    }
    Ok(())
}
