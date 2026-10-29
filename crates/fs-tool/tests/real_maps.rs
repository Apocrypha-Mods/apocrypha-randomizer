//! The MSB field layout's claims, re-checked on the player's own maps
//! (`APOC_ER_STOCK`; skipped when unset). Maps are found by name
//! (`/map/mapstudio/mAA_BB_CC_DD.msb.dcx`): 1,248 of the game's 1,347.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::OnceLock;

use fs_dvdbnd::Dvd;
use fs_msb::Msb;
use fs_msb::layout::{entry_name, field};
use rayon::prelude::*;

struct Game {
    dir: PathBuf,
    maps: Vec<(String, Msb)>,
    reg: fs_param::Regulation,
}

fn game() -> Option<&'static Game> {
    static GAME: OnceLock<Option<Game>> = OnceLock::new();
    GAME.get_or_init(|| {
        let dir = PathBuf::from(std::env::var_os("APOC_ER_STOCK")?);
        let dvd = Dvd::open_elden_ring(&dir).unwrap();
        let mut names = Vec::new();
        for a in 10..100 {
            for b in 0..100 {
                for c in 0..100 {
                    for d in 0..4 {
                        let p = format!("/map/mapstudio/m{a:02}_{b:02}_{c:02}_{d:02}.msb.dcx");
                        if dvd.find(&p).is_some() {
                            names.push(p);
                        }
                    }
                }
            }
        }
        let maps = names
            .par_iter()
            .map(|p| {
                (
                    p.clone(),
                    Msb::read(&fs_dcx::decompress(&dvd.read_path(p).unwrap()).unwrap().1).unwrap(),
                )
            })
            .collect();
        Some(Game {
            reg: fs_param::Regulation::open(&dir).unwrap(),
            dir,
            maps,
        })
    })
    .as_ref()
}

macro_rules! game_or_skip {
    () => {
        match game() {
            Some(g) => g,
            None => {
                eprintln!("APOC_ER_STOCK is not set; skipping");
                return;
            }
        }
    };
}

fn pct(n: usize, of: usize) -> f64 {
    100.0 * n as f64 / of.max(1) as f64
}

fn rows(g: &Game, table: &str) -> HashSet<i64> {
    g.reg
        .table(table)
        .unwrap()
        .rows
        .iter()
        .map(|r| i64::from(r.id))
        .collect()
}

fn of_type<'a>(m: &'a Msb, table: &str, types: &[i32]) -> impl Iterator<Item = &'a Vec<u8>> {
    let ty = field(table, "type");
    m.table(table)
        .entries
        .iter()
        .filter(move |e| ty.get(e, 0).is_some_and(|t| types.contains(&(t as i32))))
}

#[test]
fn every_map_is_found_by_name_and_round_trips() {
    let g = game_or_skip!();
    assert_eq!(g.maps.len(), 1248);
    let dvd = Dvd::open_elden_ring(&g.dir).unwrap();
    // A sample: rewriting every map is `apoc-fs check`'s job.
    for (p, m) in g.maps.iter().step_by(50) {
        let plain = fs_dcx::decompress(&dvd.read_path(p).unwrap()).unwrap().1;
        assert!(m.write().unwrap() == plain, "{p} differs when rewritten");
    }
}

/// A part's name contains its model's name (part `c4720_9000` or
/// `m60_33_40_00-c6091_9000`, model `c4720` or `c6091`). 20 of 621,538 parts
/// do not: 16 assets named AEG099_720_… on model AEG099_710, and four player
/// starts (c0000) with no model.
#[test]
fn a_parts_model_is_the_one_its_name_contains() {
    let g = game_or_skip!();
    let f = field("PARTS_PARAM_ST", "model");
    let (mut ok, mut all) = (0, 0);
    for (_, m) in &g.maps {
        let models: Vec<String> = m
            .table("MODEL_PARAM_ST")
            .entries
            .iter()
            .map(|e| entry_name(e).unwrap())
            .collect();
        for e in &m.table("PARTS_PARAM_ST").entries {
            let (Some(i), Some(name)) = (f.get(e, 0), entry_name(e)) else {
                continue;
            };
            all += 1;
            if models
                .get(i as usize)
                .is_some_and(|model| name.contains(model.as_str()))
            {
                ok += 1;
            }
        }
    }
    eprintln!("part names containing their model's: {ok}/{all}");
    assert!(all - ok <= 20, "{} parts do not", all - ok);
}

#[test]
fn enemies_name_their_npc_and_ai() {
    let g = game_or_skip!();
    let (npc, think, init) = (
        rows(g, "NpcParam"),
        rows(g, "NpcThinkParam"),
        rows(g, "CharaInitParam"),
    );
    for (name, set, min) in [
        ("npc_param", &npc, 99.0),
        ("think", &think, 98.0),
        ("chara_init", &init, 99.0),
    ] {
        let f = field("PARTS_PARAM_ST", name);
        let vals: HashSet<i64> = g
            .maps
            .iter()
            .flat_map(|(_, m)| of_type(m, "PARTS_PARAM_ST", &[2]))
            .filter_map(|e| f.get(e, 0))
            .filter(|&v| v > 0)
            .collect();
        let hit = vals.iter().filter(|v| set.contains(v)).count();
        eprintln!(
            "enemy {name}: {hit}/{} distinct values are rows",
            vals.len()
        );
        assert!(pct(hit, vals.len()) >= min, "{name}");
    }
}

#[test]
fn treasure_sits_on_a_part_and_gives_an_item_lot() {
    let g = game_or_skip!();
    let lots = rows(g, "ItemLotParam_map");
    let (part, lot, ty) = (
        field("EVENT_PARAM_ST", "treasure_part"),
        field("EVENT_PARAM_ST", "item_lot"),
        field("PARTS_PARAM_ST", "type"),
    );
    let (mut on_part, mut all, mut lot_ok) = (0, 0, 0);
    let mut part_types = std::collections::BTreeMap::<i64, usize>::new();
    for (_, m) in &g.maps {
        let parts = &m.table("PARTS_PARAM_ST").entries;
        for e in of_type(m, "EVENT_PARAM_ST", &[4]) {
            all += 1;
            let p = part.get(e, 0).unwrap();
            if let Some(target) = usize::try_from(p).ok().and_then(|p| parts.get(p)) {
                on_part += 1;
                *part_types.entry(ty.get(target, 0).unwrap()).or_default() += 1;
            }
            lot_ok += usize::from(lots.contains(&lot.get(e, 0).unwrap()));
        }
    }
    eprintln!(
        "treasure: {all}; on a part {on_part}; part types {part_types:?}; lot is a row {lot_ok}"
    );
    assert!(pct(on_part, all) >= 98.0);
    assert!(pct(lot_ok, all) >= 95.0);
}

/// What an item lot's flag records: the lots placed as treasure nearly all
/// carry a flag, no two share one, and the scripts touch few of them.
#[test]
fn treasure_lots_carry_their_own_pick_up_flags() {
    let g = game_or_skip!();
    let lots = g.reg.table("ItemLotParam_map").unwrap();
    let flag = fs_param::layout::for_table("ItemLotParam_map")
        .unwrap()
        .field("get_item_flag");
    let flag_of: std::collections::HashMap<i64, i64> = lots
        .rows
        .iter()
        .map(|r| (i64::from(r.id), flag.get(&r.data, 0)))
        .collect();
    let lot = field("EVENT_PARAM_ST", "item_lot");
    let placed: HashSet<i64> = g
        .maps
        .iter()
        .flat_map(|(_, m)| of_type(m, "EVENT_PARAM_ST", &[4]))
        .filter_map(|e| lot.get(e, 0))
        .collect();
    let flags: Vec<i64> = placed
        .iter()
        .filter_map(|l| flag_of.get(l))
        .copied()
        .collect();
    let with_flag = flags.iter().filter(|&&f| f != 0).count();
    let distinct: HashSet<i64> = flags.iter().copied().filter(|&f| f != 0).collect();
    eprintln!(
        "treasure lots: {}; with a flag {with_flag}; distinct flags {}",
        placed.len(),
        distinct.len()
    );
    assert!(pct(with_flag, placed.len()) >= 95.0);
    assert!(
        pct(distinct.len(), with_flag) >= 95.0,
        "each treasure has its own flag"
    );
}

#[test]
fn boss_bars_name_enemies_placed_in_the_maps() {
    let g = game_or_skip!();
    let ids: HashSet<u32> = g
        .maps
        .iter()
        .flat_map(|(_, m)| of_type(m, "PARTS_PARAM_ST", &[2, 10]))
        .filter_map(|e| field("PARTS_PARAM_ST", "entity_id").get(e, 0))
        .map(|v| v as u32)
        .filter(|&v| v != 0)
        .collect();
    let dvd = Dvd::open_elden_ring(&g.dir).unwrap();
    let mut names = vec!["/event/common_func.emevd.dcx".to_string()];
    for (p, _) in &g.maps {
        names.push(
            p.replace("/map/mapstudio/", "/event/")
                .replace(".msb.dcx", ".emevd.dcx"),
        );
    }
    let scripts: Vec<fs_emevd::Emevd> = names
        .iter()
        .filter(|p| dvd.find(p).is_some())
        .map(|p| {
            fs_emevd::Emevd::read(&fs_dcx::decompress(&dvd.read_path(p).unwrap()).unwrap().1)
                .unwrap()
        })
        .collect();
    let refs: Vec<&fs_emevd::Emevd> = scripts.iter().collect();
    let bar = fs_emevd::catalogue::named("boss_health_bar");
    let bosses: HashSet<u32> = fs_emevd::catalogue::values(
        &refs,
        Some(&scripts[0]),
        bar.bank,
        bar.id,
        bar.arg("boss").word,
    )
    .into_iter()
    .filter(|&v| v != 0)
    .collect();
    let hit = bosses.iter().filter(|b| ids.contains(b)).count();
    eprintln!(
        "boss bar bosses that are enemy entity ids: {hit}/{}",
        bosses.len()
    );
    assert!(pct(hit, bosses.len()) >= 90.0);
}

/// An enemy part's talk id is the id of a talk script: how a merchant is
/// found from the script that opens their shop.
#[test]
fn characters_name_the_talk_scripts_they_run() {
    let g = game_or_skip!();
    let dvd = Dvd::open_elden_ring(&g.dir).unwrap();
    let mut scripts = HashSet::new();
    let mut binders = vec!["/script/talk/m00_00_00_00.talkesdbnd.dcx".to_string()];
    for (p, _) in &g.maps {
        binders.push(
            p.replace("/map/mapstudio/", "/script/talk/")
                .replace(".msb.dcx", ".talkesdbnd.dcx"),
        );
    }
    binders.push("/script/talk/m60_00_00_00.talkesdbnd.dcx".into());
    binders.push("/script/talk/m61_00_00_00.talkesdbnd.dcx".into());
    for p in binders.iter().filter(|p| dvd.find(p).is_some()) {
        let bnd = fs_bnd4::Bnd4::read(&fs_dcx::decompress(&dvd.read_path(p).unwrap()).unwrap().1)
            .unwrap();
        for f in &bnd.files {
            let name = f
                .short_name()
                .unwrap()
                .trim_start_matches('t')
                .trim_end_matches(".esd");
            scripts.insert(name.parse::<i64>().unwrap());
        }
    }
    let f = field("PARTS_PARAM_ST", "talk_id");
    let vals: HashSet<i64> = g
        .maps
        .iter()
        .flat_map(|(_, m)| of_type(m, "PARTS_PARAM_ST", &[2, 10]))
        .filter_map(|e| f.get(e, 0))
        .filter(|&v| v > 0)
        .collect();
    let hit = vals.iter().filter(|v| scripts.contains(v)).count();
    eprintln!(
        "talk ids: {hit}/{} distinct values are talk scripts, of {} scripts",
        vals.len(),
        scripts.len()
    );
    assert!(hit >= 300 && pct(hit, vals.len()) >= 90.0);
}
