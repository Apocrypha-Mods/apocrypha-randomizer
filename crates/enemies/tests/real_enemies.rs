//! Enemy placement on the player's own game (`APOC_ER_STOCK`; skipped when
//! unset). Nothing here runs the game: it reads back what was written.

use std::collections::HashSet;
use std::path::PathBuf;

use enemies::{Options, Size, apply, malenia, pool, shuffle};
use fs_msb::Msb;
use fs_msb::layout::{entry_name, field};
use fs_param::layout::for_table;
use gamedata::{Sources, elden_ring};

const PARTS: &str = "PARTS_PARAM_ST";

fn sources() -> Option<Sources> {
    let dir = PathBuf::from(std::env::var_os("APOC_ER_STOCK")?);
    Some(Sources::open(&dir).expect("the game opens"))
}

fn all() -> Options {
    Options {
        bosses: 100,
        ordinary: 100,
        ..Default::default()
    }
}

#[test]
fn the_game_gives_the_numbers_players_know() {
    let Some(s) = sources() else { return };
    let npc = for_table("NpcParam").unwrap();
    let rows = s.reg.table("NpcParam").unwrap().rows;
    let effects = s.reg.table("SpEffectParam").unwrap().rows;
    for (row, known) in [(21_300_014, 4174.0), (21_300_534, 10_399.0)] {
        let r = rows.iter().find(|r| r.id == row).unwrap();
        let tier = npc.field("scale_a").get(&r.data, 0) as i32;
        let e = effects.iter().find(|e| e.id == tier).unwrap();
        let rate = f32::from_le_bytes(e.data[0x10..0x14].try_into().unwrap());
        let hp = npc.field("hp").get(&r.data, 0) as f32 * rate;
        assert!((hp - known).abs() < 2.0, "row {row}: {hp}");
    }
}

#[test]
fn the_pool_holds_what_the_spec_says() {
    let Some(s) = sources() else { return };
    let model = elden_ring::extract(&s).unwrap();
    let p = pool(&s, &model).unwrap();
    let names: HashSet<&str> = p.bosses().map(|b| b.body.name.as_str()).collect();
    for want in [
        "Margit, the Fell Omen",
        "Malenia, Blade of Miquella",
        "Elden Beast",
    ] {
        assert!(names.contains(want), "{want} moves");
    }
    for s in &p.spots {
        assert!(!s.body.name.contains("Kal"), "no merchant: {}", s.body.name);
    }
    assert!(!names.contains("Sir Gideon Ofnir, the All-Knowing"));
    // Patches the friend stays; the row that fights at Murkwater is hostile and moves.
    assert!(p.spots.iter().all(|s| s.entity != Some(31_000_850)));
    for s in p.spots.iter().filter(|s| s.boss.is_none()) {
        assert!(!s.body.name.contains("Millicent") || s.body.chara > 0);
    }
    for rises in [11_050_801, 14_000_840, 14_000_845] {
        assert!(p.spots.iter().all(|s| s.entity != Some(rises)), "{rises}");
    }
    assert!(names.contains("Stonedigger Troll"));
    assert!(names.contains("Magma Wyrm Makar"));
    let options = Options {
        models: 20,
        ..all()
    };
    let plan = shuffle(&p, 1, &options);
    let mut added: std::collections::BTreeMap<usize, HashSet<&str>> = Default::default();
    for (i, spot) in p.spots.iter().enumerate() {
        let body = &p.spots[plan.from[i]];
        if body.boss.is_none() && !p.listed[spot.map_index].contains(&body.body.model) {
            added
                .entry(spot.map_index)
                .or_default()
                .insert(&body.body.model);
        }
    }
    let most = added.values().map(HashSet::len).max().unwrap_or(0);
    println!("ordinary models added: at most {most} to a map");
    // Only swaps hold the limit, so a map can stay a little over.
    assert!(most <= 30, "the default limit is far off on seed 1: {most}");
    let ordinary = p.spots.iter().filter(|s| s.boss.is_none()).count();
    assert!(ordinary > 10_000, "only {ordinary} ordinary enemies");
    println!("bosses {} fixed {}", p.bosses().count(), p.fixed.len());
    for f in &p.fixed {
        println!("  fixed {}: {}", f.name, f.reason);
    }
    let n = |z| {
        p.spots
            .iter()
            .filter(|s| s.boss.is_none() && s.size == z)
            .count()
    };
    println!(
        "ordinary small {} medium {} large {}",
        n(Size::Small),
        n(Size::Medium),
        n(Size::Large)
    );
    for z in [Size::Small, Size::Medium, Size::Large] {
        let mut by: std::collections::BTreeMap<&str, usize> = Default::default();
        for s in p.spots.iter().filter(|s| s.size == z && s.boss.is_none()) {
            *by.entry(s.body.model.as_str()).or_default() += 1;
        }
        let mut top: Vec<_> = by.into_iter().collect();
        top.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        top.truncate(25);
        println!("{z:?}: {top:?}");
    }
    let mut by_name: std::collections::BTreeMap<&str, usize> = Default::default();
    for b in p.bosses() {
        *by_name.entry(b.body.name.as_str()).or_default() += 1;
    }
    println!("distinct boss names {}", by_name.len());
    println!(
        "repeats {:?}",
        by_name.iter().filter(|(_, n)| **n > 1).collect::<Vec<_>>()
    );
    let humans = p
        .spots
        .iter()
        .filter(|s| s.boss.is_none() && s.body.chara > 0)
        .count();
    let named: std::collections::BTreeSet<_> = p
        .spots
        .iter()
        .filter(|s| s.boss.is_none() && !s.body.name.is_empty())
        .map(|s| s.body.name.as_str())
        .collect();
    println!("ordinary humans {humans}; named {named:?}");
}

#[test]
fn a_written_seed_reads_back_with_each_part_standing_as_planned() {
    let Some(s) = sources() else { return };
    let model = elden_ring::extract(&s).unwrap();
    let p = pool(&s, &model).unwrap();
    let options = all();
    let plan = shuffle(&p, 1, &options);
    let mut reg = s.reg.clone();
    let applied = apply(&mut reg, &s, &p, &plan, &options).unwrap();

    let again = apply(&mut s.reg.clone(), &s, &p, &plan, &options).unwrap();
    assert_eq!(applied.maps.len(), again.maps.len());
    for (a, b) in applied.maps.iter().zip(&again.maps) {
        assert_eq!(
            a.1.write().unwrap(),
            b.1.write().unwrap(),
            "same seed, same map"
        );
    }

    let npc = for_table("NpcParam").unwrap();
    let rows = reg.table("NpcParam").unwrap().rows;
    assert!(
        rows.windows(2).all(|w| w[0].id < w[1].id),
        "rows stay in order"
    );
    println!("{} maps, {} new rows", applied.maps.len(), applied.new_rows);

    for (i, spot) in p.spots.iter().enumerate() {
        let body = &p.spots[plan.from[i]].body;
        let Some((_, msb)) = applied.maps.iter().find(|(m, _)| *m == spot.map) else {
            assert_eq!(plan.from[i], i);
            continue;
        };
        let back = Msb::read(&msb.write().unwrap()).unwrap();
        let e = &back.table(PARTS).entries[spot.part];
        let model_at = field(PARTS, "model").get(e, 0).unwrap() as usize;
        let name = entry_name(&back.table("MODEL_PARAM_ST").entries[model_at]).unwrap();
        assert_eq!(name, body.model);
        assert_eq!(field(PARTS, "think").get(e, 0).unwrap() as i32, body.think);
        if plan.from[i] == i {
            continue;
        }
        let id = field(PARTS, "npc_param").get(e, 0).unwrap() as i32;
        let row = rows
            .iter()
            .find(|r| r.id == id)
            .expect("the new row exists");
        let old = rows.iter().find(|r| r.id == spot.body.npc).unwrap();
        for f in [
            "item_lot_enemy_id",
            "item_lot_map_id",
            "runes",
            "hp",
            "scale_a",
        ] {
            let f = npc.field(f);
            assert_eq!(f.get(&row.data, 0), f.get(&old.data, 0), "{}", f.name);
        }
        if i > 200 {
            break;
        }
    }
    assert!(!applied.bar_names.is_empty());
    if plan
        .from
        .iter()
        .enumerate()
        .any(|(i, &f)| i != f && malenia::is_home(&p.spots[f]))
    {
        let others = applied
            .scripts
            .iter()
            .filter(|(m, _)| m.to_string() != "m13_00_00_00")
            .count();
        // Companions' bosses add scripts of their own.
        assert!(
            others >= 2,
            "her arena and her new one, beside Placidusax's"
        );
    }
}

/// Cutscenes stay as they are (docs/spec/enemies.md §3.1): a seed changes none,
/// but for Malenia's second-phase event, which goes with her without its own.
/// What the area map counts on stays, and the rule has something to catch.
#[test]
fn what_the_area_map_counts_on_is_not_in_the_pool() {
    let Some(s) = sources() else { return };
    let model = elden_ring::extract(&s).unwrap();
    let p = pool(&s, &model).unwrap();
    for kind in ["c4470", "c4450"] {
        assert!(
            !p.spots
                .iter()
                .any(|s| s.boss.is_none() && s.body.model == kind),
            "{kind} can move"
        );
    }
    // Where public guides list Abductor Virgins, and the Mausoleums' open world.
    let held = |kind: &str, map: &str| {
        s.maps.iter().any(|(m, msb)| {
            m.to_string() == map
                && msb
                    .table("MODEL_PARAM_ST")
                    .entries
                    .iter()
                    .any(|e| entry_name(e).as_deref() == Some(kind))
        })
    };
    for map in ["m11_00_00_00", "m14_00_00_00", "m16_00_00_00"] {
        assert!(held("c4470", map), "{map}");
    }
    assert!(held("c4450", "m60_10_08_02"));
}

#[test]
fn a_boss_placed_twice_has_neither_copy_moved() {
    let Some(s) = sources() else { return };
    let model = elden_ring::extract(&s).unwrap();
    let p = pool(&s, &model).unwrap();
    let twice: HashSet<&str> = p
        .fixed
        .iter()
        .filter(|f| f.reason == "placed twice")
        .map(|f| f.name.as_str())
        .collect();
    println!("placed twice: {twice:?}");
    for b in p.spots.iter().filter(|s| s.boss.is_some()) {
        assert!(!twice.contains(b.body.name.as_str()), "{}", b.body.name);
    }
}

#[test]
fn no_cutscene_is_changed() {
    let Some(s) = sources() else { return };
    let model = elden_ring::extract(&s).unwrap();
    let p = pool(&s, &model).unwrap();
    let options = all();
    let plan = shuffle(&p, 1, &options);
    let mut reg = s.reg.clone();
    let written = apply(&mut reg, &s, &p, &plan, &options).unwrap();
    assert!(!written.scripts.is_empty(), "the seed moves Malenia");
    let cutscenes = |e: &fs_emevd::Emevd| -> Vec<(i64, Vec<u8>)> {
        e.events
            .iter()
            .filter(|ev| ev.id != 15_002_811)
            .flat_map(|ev| ev.instructions.iter().map(move |i| (ev.id, i)))
            .filter(|(_, i)| i.bank == 2002)
            .map(|(id, i)| (id, i.args.clone()))
            .collect()
    };
    for (m, script) in &written.scripts {
        let (_, stock) = s.scripts.iter().find(|(id, _)| id == m).unwrap();
        assert_eq!(cutscenes(script), cutscenes(stock), "{m}");
    }
}

/// A newcomer on Placidusax's spot stands awake, and the fight starts on arrival.
#[test]
fn placidusaxs_arena_has_no_sleep() {
    let Some(s) = sources() else { return };
    let model = elden_ring::extract(&s).unwrap();
    let p = pool(&s, &model).unwrap();
    let options = all();
    let plan = shuffle(&p, 1, &options);
    let mut reg = s.reg.clone();
    let written = apply(&mut reg, &s, &p, &plan, &options).unwrap();
    let (_, script) = written
        .scripts
        .iter()
        .find(|(m, _)| m.to_string() == "m13_00_00_00")
        .expect("his arena's script changes");
    let event = script.events.iter().find(|e| e.id == 13_002_835).unwrap();
    let spot = 13_000_830u32.to_le_bytes();
    let animations = event
        .instructions
        .iter()
        .filter(|i| (i.bank, i.id) == (2003, 18) && i.args[..4] == spot)
        .count();
    assert_eq!(animations, 0, "no pose and no wake");
    assert!(
        event
            .instructions
            .iter()
            .any(|i| (i.bank, i.id) == (3, 3) && i.args[8..12] == spot),
        "the fight starts by distance"
    );
}

/// Mount events tie a rider's part to a mount's, on models RideParam pairs, and
/// neither part moves: changing either's body crashed the game around Stormveil.
#[test]
fn riders_and_mounts_stay_where_their_event_ties_them() {
    let Some(s) = sources() else { return };
    let model = elden_ring::extract(&s).unwrap();
    let p = pool(&s, &model).unwrap();
    let rides: HashSet<(i32, i32)> = s
        .reg
        .table("RideParam")
        .unwrap()
        .rows
        .iter()
        .map(|r| {
            let w = |o: usize| i32::from_le_bytes(r.data[o..o + 4].try_into().unwrap());
            (w(0), w(4))
        })
        .collect();
    let (mut pairs, mut in_ride_param) = (0, 0);
    let mut tied_stormveil = 0;
    let spots: HashSet<(usize, usize)> = p.spots.iter().map(|s| (s.map_index, s.part)).collect();
    for (i, (m, msb)) in s.maps.iter().enumerate() {
        let parts = &msb.table(PARTS).entries;
        let models = &msb.table("MODEL_PARAM_ST").entries;
        let number = |part: usize| -> Option<i32> {
            let at = field(PARTS, "model").get(parts.get(part)?, 0)?;
            entry_name(models.get(usize::try_from(at).ok()?)?)?[1..]
                .parse()
                .ok()
        };
        for e in &msb.table("EVENT_PARAM_ST").entries {
            let kind = field("EVENT_PARAM_ST", "type").get(e, 0);
            if kind == Some(5) {
                let list = field("EVENT_PARAM_ST", "generator_part");
                for part in (0..32).filter_map(|n| list.get(e, n)) {
                    if let Ok(part) = usize::try_from(part) {
                        assert!(!spots.contains(&(i, part)), "{m} generator part {part}");
                    }
                }
            }
            if kind != Some(21) {
                continue;
            }
            let end = |n: &str| {
                field("EVENT_PARAM_ST", n)
                    .get(e, 0)
                    .and_then(|v| usize::try_from(v).ok())
            };
            let (rider, mount) = (end("rider_part"), end("mount_part"));
            for part in [rider, mount].into_iter().flatten() {
                assert!(
                    !spots.contains(&(i, part)),
                    "{m} part {part} is in the pool"
                );
            }
            if let (Some(r), Some(t)) = (rider, mount) {
                pairs += 1;
                if m.to_string() == "m10_00_00_00" {
                    tied_stormveil += 1;
                }
                if number(r)
                    .zip(number(t))
                    .is_some_and(|pair| rides.contains(&pair))
                {
                    in_ride_param += 1;
                }
            }
        }
    }
    assert_eq!(tied_stormveil, 16);
    assert!(pairs > 100, "{pairs}");
    println!(
        "{pairs} pairs, {in_ride_param} in RideParam, {} spots",
        p.spots.len()
    );
    assert!(
        in_ride_param * 100 >= pairs * 95,
        "{in_ride_param} of {pairs}"
    );
}
