//! What in the maps marks harmful ground near enemy spots (`enemies::hazards`):
//! `apoc-fs hazards` lists the spots on it, `apoc-fs hazard-regions` the regions
//! named for swamps, lava and enemies' fall walls, and the collisions' first byte.

use std::collections::BTreeMap;
use std::path::PathBuf;

use fs_bytes::Result;
use fs_msb::layout::entry_name;
use gamedata::Sources;

fn i32_at(e: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_le_bytes(e.get(at..at + 4)?.try_into().ok()?))
}

fn f32_at(e: &[u8], at: usize) -> f32 {
    e.get(at..at + 4)
        .map_or(f32::NAN, |w| f32::from_le_bytes(w.try_into().unwrap()))
}

/// Which spots stand on harmful ground, and a check of how boxes turn.
pub fn spots(game: PathBuf) -> Result<()> {
    use enemies::hazards::{self, Hazard};
    let s = Sources::open(&game)?;
    let model = gamedata::elden_ring::extract(&s)?;
    let pool = enemies::pool(&s, &model)?;
    // Turn check: enemy parts held by the maps' play regions (type 43) and sound
    // boxes, with the turn as read and reversed. The right way should hold more.
    let (mut fwd, mut back) = (0usize, 0usize);
    for (_, msb) in &s.maps {
        let boxes: Vec<_> = msb
            .table("POINT_PARAM_ST")
            .entries
            .iter()
            .filter(|e| matches!(i32_at(e, 0x08), Some(43) | Some(4) | Some(30)))
            .filter_map(|e| hazards::volume(e))
            .filter(|v| matches!(v.shape, hazards::Shape::Box { .. }) && v.turn.abs() > 10.0)
            .collect();
        for e in &msb.table("PARTS_PARAM_ST").entries {
            if i32_at(e, 0x0C) != Some(2) {
                continue;
            }
            let Some(p) = hazards::position(e) else {
                continue;
            };
            fwd += boxes.iter().filter(|v| v.holds(p)).count();
            back += boxes
                .iter()
                .filter(|v| {
                    hazards::Volume {
                        turn: -v.turn,
                        ..**v
                    }
                    .holds(p)
                })
                .count();
        }
    }
    println!("enemy parts in turned boxes: turn as read {fwd}, reversed {back}");
    let mut names: BTreeMap<(Hazard, String), usize> = BTreeMap::new();
    for (m, msb) in &s.maps {
        for (h, _, n) in hazards::grounds(msb) {
            let stem: String = n.chars().filter(|c| !c.is_ascii_digit()).collect();
            *names
                .entry((h, format!("{}:{stem}", &m.to_string()[..3])))
                .or_default() += 1;
        }
    }
    println!("{} harmful grounds", names.values().sum::<usize>());
    for ((h, n), c) in &names {
        println!("  {h:?} x{c} {n}");
    }
    let grounds = hazards::Grounds::read(&s.maps);
    let mut by: BTreeMap<(bool, Option<Hazard>), usize> = BTreeMap::new();
    let mut maps: BTreeMap<(Hazard, String), usize> = BTreeMap::new();
    for sp in &pool.spots {
        let msb = &s.maps[sp.map_index].1;
        let e = &msb.table("PARTS_PARAM_ST").entries[sp.part];
        let Some(p) = hazards::position(e) else {
            continue;
        };
        let h = grounds.at(sp.map_index, sp.map, p);
        *by.entry((sp.boss.is_some(), h)).or_default() += 1;
        if let Some(h) = h {
            *maps.entry((h, sp.map.to_string())).or_default() += 1;
        }
        if sp.boss.is_some() {
            if let Some(h) = h {
                println!("  boss {} in {} on {h:?}", sp.body.name, sp.map);
            }
            // Harmful ground near but not holding it, to see what was missed.
            for (h, v, n) in hazards::grounds(msb) {
                let d = ((p[0] - v.at[0]).powi(2) + (p[2] - v.at[2]).powi(2)).sqrt();
                if d < 40.0 && !v.holds(p) {
                    println!(
                        "    near {} ({}): {h:?} {n} {d:.0} m, dy {:.0}, {:?}",
                        sp.body.name,
                        sp.map,
                        v.at[1] - p[1],
                        v.shape
                    );
                }
            }
        }
    }
    println!("spots by (boss, ground): {by:?}");
    for ((h, m), n) in &maps {
        println!("  {h:?} {m}: {n}");
    }
    Ok(())
}

/// Hazard-named regions: their types, shapes and sizes; and collisions' first byte.
pub fn named(game: PathBuf) -> Result<()> {
    let s = Sources::open(&game)?;
    let re = [
        "毒沼",
        "赤毒",
        "毒の滝",
        "毒滝",
        "_毒",
        "溶岩",
        "マグマ",
        "腐れ",
        "朱",
        "落下防止",
    ];
    let mut rows: BTreeMap<(i32, i32, String), (usize, f32, f32)> = BTreeMap::new();
    for (m, msb) in &s.maps {
        for e in &msb.table("POINT_PARAM_ST").entries {
            let name = entry_name(e).unwrap_or_default();
            if !re.iter().any(|w| name.contains(w)) {
                continue;
            }
            let ty = i32_at(e, 0x08).unwrap_or(-9);
            let shape = i32_at(e, 0x10).unwrap_or(-9);
            let sec = i64::from_le_bytes(e[0x48..0x50].try_into().unwrap()) as usize;
            let a = if sec > 0 { f32_at(e, sec) } else { 0.0 };
            let b = if sec > 0 { f32_at(e, sec + 4) } else { 0.0 };
            let stem: String = name.chars().filter(|c| !c.is_ascii_digit()).collect();
            let k = (ty, shape, format!("{}:{stem}", &m.to_string()[..3]));
            let v = rows.entry(k).or_insert((0, 0.0, 0.0));
            v.0 += 1;
            v.1 = v.1.max(a);
            v.2 = v.2.max(b);
        }
    }
    for ((ty, shape, n), (c, a, b)) in &rows {
        println!("type {ty} shape {shape} x{c} max {a:.0} {b:.0}  {n}");
    }
    let mut hit: BTreeMap<u8, (usize, BTreeMap<String, usize>)> = BTreeMap::new();
    for (m, msb) in &s.maps {
        for e in &msb.table("PARTS_PARAM_ST").entries {
            if i32_at(e, 0x0C) != Some(5) {
                continue;
            }
            let sec = i64::from_le_bytes(e[0x68..0x70].try_into().unwrap()) as usize;
            let f = e.get(sec).copied().unwrap_or(255);
            let v = hit.entry(f).or_default();
            v.0 += 1;
            *v.1.entry(m.to_string()[..3].to_string()).or_default() += 1;
        }
    }
    for (f, (n, areas)) in &hit {
        println!("collision first byte {f}: {n} parts, by area {areas:?}");
    }
    Ok(())
}
