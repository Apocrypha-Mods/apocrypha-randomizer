//! The MSB field layout's claims, re-checked on the player's own maps
//! (`APOC_ER_STOCK`; skipped when unset). Maps are found by name
//! (`/map/mapstudio/mAA_BB_CC_DD.msb.dcx`): all 1,347 of the game's.

use std::collections::{HashMap, HashSet};
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
        let names: Vec<String> = dvd
            .map_ids("/map/mapstudio/", ".msb.dcx")
            .iter()
            .map(|m| format!("/map/mapstudio/{m}.msb.dcx"))
            .collect();
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
    assert_eq!(g.maps.len(), 1347);
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

/// Every entry of every map, taken apart into its sections and laid out
/// again, comes back byte for byte: the section places, text alignment and
/// end padding in `fs_msb::entry` are the game's.
#[test]
fn every_entry_lays_out_again_byte_for_byte() {
    let g = game_or_skip!();
    let mut bad: std::collections::BTreeMap<&str, (usize, Vec<String>)> = Default::default();
    let mut all = 0;
    for (p, m) in &g.maps {
        for t in &m.tables {
            for (i, e) in t.entries.iter().enumerate() {
                all += 1;
                let ok = fs_msb::entry::Entry::read(&t.name, e).map(|x| x.write());
                if ok.as_ref().is_ok_and(|w| w == e) {
                    continue;
                }
                let b = bad
                    .entry(fs_msb::TABLES.iter().find(|n| **n == t.name).unwrap())
                    .or_default();
                b.0 += 1;
                if b.1.len() < 5 {
                    let why = match &ok {
                        Err(err) => err.to_string(),
                        Ok(w) => format!(
                            "len {} vs {}, first difference at {:?}",
                            w.len(),
                            e.len(),
                            w.iter().zip(e).position(|(a, b)| a != b)
                        ),
                    };
                    b.1.push(format!("{p} #{i} {:?}: {why}", entry_name(e)));
                }
            }
        }
    }
    for (t, (n, eg)) in &bad {
        eprintln!("{t}: {n} differ");
        for e in eg {
            eprintln!("  {e}");
        }
    }
    eprintln!("entries: {all}");
    assert!(bad.is_empty());
}

/// What adding an entry must keep true: each table grouped by type, each
/// entry's place among its type, and each model's count of the parts using it.
#[test]
fn tables_are_grouped_by_type_and_models_count_their_parts() {
    let g = game_or_skip!();
    for t in [
        "MODEL_PARAM_ST",
        "EVENT_PARAM_ST",
        "POINT_PARAM_ST",
        "ROUTE_PARAM_ST",
        "PARTS_PARAM_ST",
    ] {
        let (ty, index) = (field(t, "type"), field(t, "type_index"));
        let (mut grouped, mut maps, mut placed, mut all) = (0, 0, 0, 0);
        for (_, m) in &g.maps {
            let es = &m.table(t).entries;
            if es.is_empty() {
                continue;
            }
            maps += 1;
            let types: Vec<i64> = es.iter().map(|e| ty.get(e, 0).unwrap()).collect();
            let mut done = HashSet::new();
            let ok = types
                .windows(2)
                .all(|w| w[0] == w[1] || (done.insert(w[0]) && !done.contains(&w[1])));
            grouped += usize::from(ok);
            let mut seen = HashMap::<i64, i64>::new();
            for (e, t) in es.iter().zip(&types) {
                let k = seen.entry(*t).or_default();
                all += 1;
                placed += usize::from(index.get(e, 0) == Some(*k));
                *k += 1;
            }
        }
        eprintln!("{t}: grouped by type in {grouped}/{maps} maps; type index right {placed}/{all}");
        assert_eq!(grouped, maps, "{t}");
        assert!(all - placed <= 10, "{t}");
    }
    let (model, uses) = (
        field("PARTS_PARAM_ST", "model"),
        field("MODEL_PARAM_ST", "instances"),
    );
    let (mut ok, mut all) = (0, 0);
    for (_, m) in &g.maps {
        let mut n = HashMap::<i64, i64>::new();
        for e in &m.table("PARTS_PARAM_ST").entries {
            *n.entry(model.get(e, 0).unwrap()).or_default() += 1;
        }
        for (i, e) in m.table("MODEL_PARAM_ST").entries.iter().enumerate() {
            all += 1;
            let want = n.get(&(i as i64)).copied().unwrap_or(0);
            ok += usize::from(uses.get(e, 0) == Some(want));
        }
    }
    eprintln!("models counting their parts: {ok}/{all}");
    assert_eq!(ok, all);
}

/// Every argument word of every map's event script, as numbers.
fn script_words(g: &Game) -> HashSet<i64> {
    let dvd = Dvd::open_elden_ring(&g.dir).unwrap();
    let mut out = HashSet::new();
    for m in dvd.map_ids("/event/", ".emevd.dcx") {
        let p = format!("/event/{m}.emevd.dcx");
        let e = fs_emevd::Emevd::read(&fs_dcx::decompress(&dvd.read_path(&p).unwrap()).unwrap().1)
            .unwrap();
        for ev in &e.events {
            for i in &ev.instructions {
                for w in i.args.chunks_exact(4) {
                    out.insert(i64::from(u32::from_le_bytes(w.try_into().unwrap())));
                }
            }
        }
    }
    out
}

/// Points: the shape says how big its section is, ids are each map's own and
/// routes join two of them, and entity ids are what the scripts name.
#[test]
fn points_have_shapes_ids_and_entity_ids_and_routes_join_them() {
    let g = game_or_skip!();
    let (shape, id, entity_id) = (
        field("POINT_PARAM_ST", "shape"),
        field("POINT_PARAM_ST", "id"),
        field("POINT_PARAM_ST", "entity_id"),
    );
    let mut sizes = std::collections::BTreeMap::<(i64, usize), usize>::new();
    let (mut unique, mut maps, mut joined, mut ends) = (0, 0, 0, 0);
    let mut entity = HashSet::new();
    for (_, m) in &g.maps {
        let points = &m.table("POINT_PARAM_ST").entries;
        for e in points {
            let s = fs_msb::entry::Entry::read("POINT_PARAM_ST", e).unwrap();
            let n = s.section(0x48).map_or(0, <[u8]>::len);
            *sizes.entry((shape.get(e, 0).unwrap(), n)).or_default() += 1;
            if let Some(v) = entity_id.get(e, 0)
                && v != 0
            {
                entity.insert(v);
            }
        }
        if points.is_empty() {
            continue;
        }
        maps += 1;
        let ids: HashSet<i64> = points.iter().map(|e| id.get(e, 0).unwrap()).collect();
        unique += usize::from(ids.len() == points.len());
        for r in &m.table("ROUTE_PARAM_ST").entries {
            for f in ["from_point", "to_point"] {
                ends += 1;
                let v = field("ROUTE_PARAM_ST", f).get(r, 0).unwrap();
                joined += usize::from(ids.contains(&v));
            }
        }
    }
    eprintln!("point (shape, shape bytes): {sizes:?}");
    let want = [(0, 0), (1, 4), (2, 4), (3, 8), (4, 8), (5, 12), (6, 64)];
    assert!(
        sizes.keys().all(|k| want.contains(k)),
        "each shape has one size"
    );
    eprintln!(
        "point ids unique in {unique}/{maps} maps; route ends that are point ids {joined}/{ends}"
    );
    assert_eq!(unique, maps);
    assert_eq!(joined, ends);
    let words = script_words(g);
    let named = entity.iter().filter(|v| words.contains(v)).count();
    eprintln!(
        "point entity ids named by scripts: {named}/{}",
        entity.len()
    );
    assert!(pct(named, entity.len()) >= 60.0);
}

/// Entity groups: ids many parts share, few of them a part's own id.
#[test]
fn parts_share_entity_groups() {
    let g = game_or_skip!();
    let (groups, own) = (
        field("PARTS_PARAM_ST", "entity_groups"),
        field("PARTS_PARAM_ST", "entity_id"),
    );
    let (mut uses, mut ids, mut owns) = (0, HashSet::new(), HashSet::new());
    for (_, m) in &g.maps {
        for e in &m.table("PARTS_PARAM_ST").entries {
            owns.insert(own.get(e, 0).unwrap_or(0));
            for i in 0..8 {
                if let Some(v) = groups.get(e, i)
                    && v != 0
                {
                    uses += 1;
                    ids.insert(v);
                }
            }
        }
    }
    let words = script_words(g);
    let both = ids.iter().filter(|v| owns.contains(v)).count();
    let named = ids.iter().filter(|v| words.contains(v)).count();
    eprintln!(
        "entity groups: {} ids over {uses} uses; also a part's own id {both}; in scripts {named}",
        ids.len()
    );
    assert!(uses > 5 * ids.len(), "groups are shared");
    assert!(both * 100 < ids.len(), "and are not parts' own ids");
}

/// Adding an enemy after the last of its kind, in every map that has one,
/// moves every index the layout knows so that each still names the entry it
/// named; taking it out again gives the map back byte for byte.
#[test]
fn an_enemy_added_and_taken_out_again_leaves_every_map_as_it_was() {
    let g = game_or_skip!();
    let refs: Vec<&fs_msb::layout::Field> = fs_msb::layout::elden_ring()
        .iter()
        .filter(|f| f.refers.is_some())
        .collect();
    // The name of what each known index points at, leaving out one part.
    let targets = |m: &Msb, skip: Option<usize>| -> Vec<Option<String>> {
        let mut out = Vec::new();
        for f in &refs {
            let to = &m.table(f.refers.as_deref().unwrap()).entries;
            for (k, e) in m.table(&f.table).entries.iter().enumerate() {
                if !f.applies(e) || (f.table == "PARTS_PARAM_ST" && Some(k) == skip) {
                    continue;
                }
                for i in 0..f.count {
                    let v = f.get(e, i).and_then(|v| usize::try_from(v).ok());
                    out.push(v.and_then(|v| to.get(v)).and_then(|t| entry_name(t)));
                }
            }
        }
        out
    };
    let ty = field("PARTS_PARAM_ST", "type");
    let tried: usize = g
        .maps
        .par_iter()
        .map(|(p, m)| {
            let parts = &m.table("PARTS_PARAM_ST").entries;
            let Some(enemy) = parts.iter().find(|e| ty.get(e, 0) == Some(2)) else {
                return 0;
            };
            let before = m.write().unwrap();
            let named = targets(m, None);
            let mut m = m.clone();
            let mut e = fs_msb::entry::Entry::read("PARTS_PARAM_ST", enemy).unwrap();
            e.set_name("c9999_9999_apocrypha_test").unwrap();
            let at = m.place_for("PARTS_PARAM_ST", 2);
            m.insert("PARTS_PARAM_ST", at, e.write()).unwrap();
            assert!(
                targets(&m, Some(at)) == named,
                "{p}: an index lost its target"
            );
            let back = Msb::read(&m.write().unwrap()).unwrap();
            assert!(back == m, "{p}: the enlarged map reads back");
            m.remove("PARTS_PARAM_ST", at).unwrap();
            assert!(
                m.write().unwrap() == before,
                "{p} differs after the round trip"
            );
            1
        })
        .sum();
    eprintln!("maps with an enemy added and taken out: {tried}");
    assert!(tried > 500);
}
