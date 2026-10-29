//! The field layouts' "confirmed" claims, re-checked on the player's own
//! `regulation.bin` (`APOC_ER_STOCK`; skipped when unset).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::OnceLock;

use fs_param::{Regulation, layout};

fn regulation() -> Option<&'static Regulation> {
    static REG: OnceLock<Option<Regulation>> = OnceLock::new();
    REG.get_or_init(|| {
        let dir = PathBuf::from(std::env::var_os("APOC_ER_STOCK")?);
        Some(Regulation::open(&dir).expect("regulation.bin opens with the exe's key"))
    })
    .as_ref()
}

macro_rules! regulation_or_skip {
    () => {
        match regulation() {
            Some(r) => r,
            None => {
                eprintln!("APOC_ER_STOCK is not set; skipping");
                return;
            }
        }
    };
}

fn ids(reg: &Regulation, table: &str) -> HashSet<i64> {
    reg.table(table)
        .unwrap()
        .rows
        .iter()
        .map(|r| i64::from(r.id))
        .collect()
}

#[test]
fn every_table_rewrites_byte_for_byte() {
    let reg = regulation_or_skip!();
    for f in &reg.bnd.files {
        let p = fs_param::Param::read(&f.data).unwrap();
        assert!(
            p.write().unwrap() == f.data,
            "{:?} differs when rewritten",
            f.name
        );
    }
}

#[test]
fn every_layout_matches_its_tables() {
    let reg = regulation_or_skip!();
    for l in layout::elden_ring() {
        for t in &l.tables {
            l.check(&reg.table(t).unwrap()).unwrap();
        }
    }
}

/// A share as a percentage, for the messages.
fn pct(n: usize, of: usize) -> f64 {
    100.0 * n as f64 / of.max(1) as f64
}

#[test]
fn item_lot_ids_belong_to_the_table_their_category_names() {
    let reg = regulation_or_skip!();
    let tables: HashMap<i64, HashSet<i64>> = [
        (1, "EquipParamGoods"),
        (2, "EquipParamWeapon"),
        (3, "EquipParamProtector"),
        (4, "EquipParamAccessory"),
        (5, "EquipParamGem"),
        (6, "EquipParamCustomWeapon"),
    ]
    .into_iter()
    .map(|(c, t)| (c, ids(reg, t)))
    .collect();
    let weapons = &tables[&2];
    for name in ["ItemLotParam_map", "ItemLotParam_enemy"] {
        let p = reg.table(name).unwrap();
        let l = layout::for_table(name).unwrap();
        let (id, cat) = (l.field("item_id"), l.field("item_category"));
        let mut seen: HashMap<i64, (usize, usize)> = HashMap::new();
        let mut upgraded = 0;
        for r in &p.rows {
            for i in 0..8 {
                let (v, c) = (id.get(&r.data, i), cat.get(&r.data, i));
                if c == 0 {
                    continue;
                }
                let e = seen.entry(c).or_default();
                e.1 += 1;
                if tables[&c].contains(&v) {
                    e.0 += 1;
                } else if c == 2 && weapons.contains(&(v - v % 100)) {
                    // A base weapon plus an upgrade level.
                    e.0 += 1;
                    upgraded += 1;
                }
            }
        }
        eprintln!("{name}: {seen:?}, {upgraded} weapons with an upgrade level");
        for (c, (hit, all)) in &seen {
            assert!(tables.contains_key(c), "{name}: unknown category {c}");
            assert!(
                pct(*hit, *all) >= 90.0,
                "{name}: category {c} matches {hit}/{all}"
            );
        }
    }
}

#[test]
fn item_lot_slots_after_the_first_are_empty_together() {
    let reg = regulation_or_skip!();
    for name in ["ItemLotParam_map", "ItemLotParam_enemy"] {
        let p = reg.table(name).unwrap();
        let l = layout::for_table(name).unwrap();
        let (id, cat, count) = (
            l.field("item_id"),
            l.field("item_category"),
            l.field("count"),
        );
        let mut mismatched = 0;
        let mut slots = 0;
        for r in &p.rows {
            for i in 1..8 {
                slots += 1;
                let empty = [
                    id.get(&r.data, i) == 0,
                    cat.get(&r.data, i) == 0,
                    count.get(&r.data, i) == 0,
                ];
                if empty.iter().any(|&e| e != empty[0]) {
                    mismatched += 1;
                }
            }
        }
        eprintln!("{name}: {mismatched} of {slots} slots disagree on being empty");
        assert!(
            pct(mismatched, slots) < 0.5,
            "{name}: {mismatched} of {slots}"
        );
    }
}

#[test]
fn item_lot_weights_add_up_to_a_thousand() {
    let reg = regulation_or_skip!();
    for name in ["ItemLotParam_map", "ItemLotParam_enemy"] {
        let p = reg.table(name).unwrap();
        let w = layout::for_table(name).unwrap().field("weight");
        let mut sums: HashMap<i64, usize> = HashMap::new();
        for r in &p.rows {
            *sums
                .entry((0..8).map(|i| w.get(&r.data, i)).sum())
                .or_default() += 1;
        }
        let mut top: Vec<_> = sums.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1));
        eprintln!("{name}: weight sums {:?}", &top[..top.len().min(6)]);
        assert!(pct(sums.get(&1000).copied().unwrap_or(0), p.rows.len()) >= 90.0);
    }
}

#[test]
fn shop_ids_belong_to_the_table_their_type_names() {
    let reg = regulation_or_skip!();
    let tables: HashMap<i64, HashSet<i64>> = [
        (0, "EquipParamWeapon"),
        (1, "EquipParamProtector"),
        (2, "EquipParamAccessory"),
        (3, "EquipParamGoods"),
        (4, "EquipParamGem"),
    ]
    .into_iter()
    .map(|(c, t)| (c, ids(reg, t)))
    .collect();
    let mtrl = ids(reg, "EquipMtrlSetParam");
    for name in ["ShopLineupParam", "ShopLineupParam_Recipe"] {
        let p = reg.table(name).unwrap();
        let l = layout::for_table(name).unwrap();
        let (id, ty, m) = (
            l.field("equip_id"),
            l.field("equip_type"),
            l.field("mtrl_set_id"),
        );
        let mut seen: HashMap<i64, (usize, usize)> = HashMap::new();
        let (mut mhit, mut mall) = (0, 0);
        for r in &p.rows {
            let (v, t) = (id.get(&r.data, 0), ty.get(&r.data, 0));
            let e = seen.entry(t).or_default();
            e.1 += 1;
            if tables
                .get(&t)
                .is_some_and(|s| s.contains(&v) || (t == 0 && s.contains(&(v - v % 100))))
            {
                e.0 += 1;
            }
            let mv = m.get(&r.data, 0);
            if mv != -1 {
                mall += 1;
                mhit += usize::from(mtrl.contains(&mv));
            }
        }
        eprintln!("{name}: {seen:?}; material sets {mhit}/{mall}");
        for (t, (hit, all)) in &seen {
            if *all > 3 {
                assert!(
                    pct(*hit, *all) >= 90.0,
                    "{name}: type {t} matches {hit}/{all}"
                );
            }
        }
        assert!(pct(mhit, mall) >= 90.0);
    }
}

#[test]
fn material_ids_belong_to_the_table_their_category_names() {
    let reg = regulation_or_skip!();
    let tables: HashMap<i64, HashSet<i64>> = [(1, "EquipParamProtector"), (4, "EquipParamGoods")]
        .into_iter()
        .map(|(c, t)| (c, ids(reg, t)))
        .collect();
    let l = layout::for_table("EquipMtrlSetParam").unwrap();
    let (id, cat) = (l.field("item_id"), l.field("category"));
    let mut seen: HashMap<i64, (usize, usize)> = HashMap::new();
    for r in &reg.table("EquipMtrlSetParam").unwrap().rows {
        for i in 0..id.count {
            let v = id.get(&r.data, i);
            if v == -1 {
                continue;
            }
            let c = cat.get(&r.data, i);
            let e = seen.entry(c).or_default();
            e.1 += 1;
            e.0 += usize::from(tables.get(&c).is_some_and(|s| s.contains(&v)));
        }
    }
    eprintln!("{seen:?}");
    assert_eq!(
        seen.keys().copied().collect::<HashSet<_>>(),
        HashSet::from([1, 4])
    );
    for (c, (hit, all)) in &seen {
        assert!(pct(*hit, *all) >= 90.0, "category {c} matches {hit}/{all}");
    }
}

#[test]
fn npc_item_lots_point_at_item_lot_rows() {
    let reg = regulation_or_skip!();
    let l = layout::for_table("NpcParam").unwrap();
    let p = reg.table("NpcParam").unwrap();
    for (field, table, min) in [
        ("item_lot_enemy_id", "ItemLotParam_enemy", 95.0),
        ("item_lot_map_id", "ItemLotParam_map", 100.0),
    ] {
        let f = l.field(field);
        let rows = ids(reg, table);
        let distinct: HashSet<i64> = p
            .rows
            .iter()
            .map(|r| f.get(&r.data, 0))
            .filter(|&v| v != -1)
            .collect();
        let hit = distinct.iter().filter(|v| rows.contains(v)).count();
        eprintln!(
            "NpcParam.{field}: {hit}/{} distinct values are {table} rows",
            distinct.len()
        );
        assert!(pct(hit, distinct.len()) >= min);
    }
}

/// An asset model's pick-up lot is set exactly when its prompt is, and both
/// point at their tables.
#[test]
fn asset_pick_ups_have_a_lot_and_a_prompt() {
    let reg = regulation_or_skip!();
    let l = layout::for_table("AssetEnvironmentGeometryParam").unwrap();
    let p = reg.table("AssetEnvironmentGeometryParam").unwrap();
    let (lot, button, broken) = (
        l.field("pick_item_lot"),
        l.field("pick_action_button"),
        l.field("break_item_lot"),
    );
    let both = p
        .rows
        .iter()
        .filter(|r| (lot.get(&r.data, 0) == -1) == (button.get(&r.data, 0) == -1))
        .count();
    assert_eq!(both, p.rows.len(), "a lot exactly when a prompt");
    for (f, table, min) in [
        (lot, "ItemLotParam_map", 98.0),
        (button, "ActionButtonParam", 95.0),
        (broken, "ItemLotParam_map", 85.0),
    ] {
        let rows = ids(reg, table);
        let distinct: HashSet<i64> = p
            .rows
            .iter()
            .map(|r| f.get(&r.data, 0))
            .filter(|&v| v != -1)
            .collect();
        let hit = distinct.iter().filter(|v| rows.contains(v)).count();
        eprintln!(
            "{}: {hit}/{} distinct values are {table} rows",
            f.name,
            distinct.len()
        );
        assert!(pct(hit, distinct.len()) >= min, "{}", f.name);
    }
}

/// The game's English text tables (`GoodsName`, `PlaceName`, …), each merged
/// across its DLC parts.
fn text() -> Option<&'static HashMap<String, HashMap<i32, String>>> {
    static TEXT: OnceLock<Option<HashMap<String, HashMap<i32, String>>>> = OnceLock::new();
    TEXT.get_or_init(|| {
        let dir = PathBuf::from(std::env::var_os("APOC_ER_STOCK")?);
        let dvd = fs_dvdbnd::Dvd::open_elden_ring(dir).unwrap();
        let stored = dvd.read_path("/msg/engus/item_dlc02.msgbnd.dcx").unwrap();
        let bnd = fs_bnd4::Bnd4::read(&fs_dcx::decompress(&stored).unwrap().1).unwrap();
        let mut out: HashMap<String, HashMap<i32, String>> = HashMap::new();
        for f in &bnd.files {
            let Some(stem) = f.short_name().and_then(|n| n.strip_suffix(".fmg")) else {
                continue;
            };
            let base = stem
                .trim_end_matches("_dlc01")
                .trim_end_matches("_dlc02")
                .to_string();
            for (id, t) in fs_fmg::Fmg::read(&f.data).unwrap().entries {
                if let Some(t) = t.filter(|t| !t.is_empty()) {
                    out.entry(base.clone()).or_default().insert(id, t);
                }
            }
        }
        Some(out)
    })
    .as_ref()
}

/// The first id a name table gives `name`.
fn id_named(table: &str, name: &str) -> i64 {
    let t = &text().unwrap()[table];
    let mut ids: Vec<i32> = t
        .iter()
        .filter(|(_, n)| n.as_str() == name)
        .map(|(&id, _)| id)
        .collect();
    ids.sort();
    i64::from(
        *ids.first()
            .unwrap_or_else(|| panic!("no {table} named {name}")),
    )
}

#[test]
fn starting_classes_start_with_real_equipment() {
    let reg = regulation_or_skip!();
    let l = layout::for_table("CharaInitParam").unwrap();
    let p = reg.table("CharaInitParam").unwrap();
    let slots = [
        ("weapon", "EquipParamWeapon"),
        ("protector", "EquipParamProtector"),
        ("ammo", "EquipParamWeapon"),
        ("accessory", "EquipParamAccessory"),
        ("magic", "Magic"),
        ("goods_a", "EquipParamGoods"),
        ("goods", "EquipParamGoods"),
        ("goods_b", "EquipParamGoods"),
    ];
    let classes: Vec<_> = p
        .rows
        .iter()
        .filter(|r| (3000..=3009).contains(&r.id))
        .collect();
    assert_eq!(classes.len(), 10);
    for (field, table) in slots {
        let f = l.field(field);
        let rows = ids(reg, table);
        for r in &classes {
            for i in 0..f.count {
                let v = f.get(&r.data, i);
                if v > 0 {
                    assert!(
                        rows.contains(&v) || rows.contains(&(v - v % 10_000)),
                        "class {} {field}[{i}] = {v}",
                        r.id
                    );
                }
            }
        }
    }
    // Each armor slot holds ids whose hundreds digit is the slot: always in
    // the class rows, and in nearly every other row.
    let f = l.field("protector");
    let armor = ids(reg, "EquipParamProtector");
    let (mut odd, mut all) = (Vec::new(), 0);
    for r in &p.rows {
        for i in 0..4 {
            let v = f.get(&r.data, i);
            if armor.contains(&v) {
                all += 1;
                if v % 1000 / 100 != i as i64 {
                    assert!(
                        !(3000..=3009).contains(&r.id),
                        "class {} protector[{i}] = {v}",
                        r.id
                    );
                    odd.push((r.id, i, v));
                }
            }
        }
    }
    eprintln!(
        "protector slots off their hundreds digit: {}/{all}: {:?}",
        odd.len(),
        &odd[..odd.len().min(8)]
    );
    assert!(pct(odd.len(), all) < 1.0);
}

#[test]
fn goods_types_group_items_as_their_names_show() {
    let reg = regulation_or_skip!();
    if text().is_none() {
        return;
    }
    let f = layout::for_table("EquipParamGoods")
        .unwrap()
        .field("goods_type");
    let p = reg.table("EquipParamGoods").unwrap();
    let type_of = |name: &str| {
        let id = id_named("GoodsName", name);
        let row = p.rows.iter().find(|r| i64::from(r.id) == id).unwrap();
        f.get(&row.data, 0)
    };
    for (name, want) in [
        ("Tarnished's Furled Finger", 0),
        ("Fire Pot", 0),
        ("Sliver of Meat", 1),
        ("Golden Seed", 2),
        ("Smithing Stone [1]", 2),
        ("Godrick's Great Rune", 3),
        ("Deathroot", 3),
        ("Glintstone Pebble", 3),
        ("Catch Flame", 4),
        ("Red Letter", 6),
        ("About Sites of Grace", 7),
    ] {
        assert_eq!(type_of(name), want, "{name}");
    }
}

#[test]
fn weapon_reinforce_types_start_upgrade_paths() {
    let reg = regulation_or_skip!();
    let f = layout::for_table("EquipParamWeapon")
        .unwrap()
        .field("reinforce_type");
    let p = reg.table("EquipParamWeapon").unwrap();
    let paths = ids(reg, "ReinforceParamWeapon");
    let distinct: HashSet<i64> = p.rows.iter().map(|r| f.get(&r.data, 0)).collect();
    let hit = distinct.iter().filter(|v| paths.contains(v)).count();
    eprintln!(
        "reinforce_type: {hit}/{} values are ReinforceParamWeapon rows",
        distinct.len()
    );
    assert!(pct(hit, distinct.len()) >= 90.0);
    let last_level = |base: i64| {
        (0..100)
            .filter(|n| paths.contains(&(base + n)))
            .max()
            .unwrap()
    };
    assert_eq!(last_level(0), 25, "regular weapons go to +25");
    assert_eq!(last_level(2200), 10, "somber weapons go to +10");
    if text().is_some() {
        let moonveil = id_named("WeaponName", "Moonveil");
        let row = p.rows.iter().find(|r| i64::from(r.id) == moonveil).unwrap();
        assert_eq!(f.get(&row.data, 0), 2200);
    }
}

#[test]
fn graces_and_npcs_are_named_from_the_game_text() {
    let reg = regulation_or_skip!();
    if text().is_none() {
        return;
    }
    let t = text().unwrap();
    for (table, field, names, min) in [
        ("BonfireWarpParam", "text_id", "PlaceName", 99.0),
        ("NpcParam", "name_id", "NpcName", 95.0),
    ] {
        let f = layout::for_table(table).unwrap().field(field);
        let distinct: HashSet<i64> = reg
            .table(table)
            .unwrap()
            .rows
            .iter()
            .map(|r| f.get(&r.data, 0))
            .filter(|&v| v > 0)
            .collect();
        let hit = distinct
            .iter()
            .filter(|&&v| t[names].contains_key(&(v as i32)))
            .count();
        eprintln!(
            "{table}.{field}: {hit}/{} distinct values are {names} ids",
            distinct.len()
        );
        assert!(pct(hit, distinct.len()) >= min);
    }
    let sub = layout::for_table("BonfireWarpParam")
        .unwrap()
        .field("sub_category_id");
    let cats = ids(reg, "BonfireWarpSubCategoryParam");
    for r in &reg.table("BonfireWarpParam").unwrap().rows {
        let v = sub.get(&r.data, 0);
        assert!(
            v == 0 || cats.contains(&v),
            "grace {} sub category {v}",
            r.id
        );
    }
}

/// Boss defeat flags are event flags: the scripts test and set them.
#[test]
fn boss_defeat_flags_are_used_by_the_event_scripts() {
    let reg = regulation_or_skip!();
    let dir = PathBuf::from(std::env::var_os("APOC_ER_STOCK").unwrap());
    let dvd = fs_dvdbnd::Dvd::open_elden_ring(dir).unwrap();
    let f = layout::for_table("GameAreaParam")
        .unwrap()
        .field("defeat_flag");
    let flags: HashSet<u32> = reg
        .table("GameAreaParam")
        .unwrap()
        .rows
        .iter()
        .map(|r| f.get(&r.data, 0) as u32)
        .filter(|&v| v != 0)
        .collect();
    // The flag words, from the instruction catalogue (all of them word 1).
    let flag_words: Vec<(u32, u32)> = fs_emevd::catalogue::elden_ring()
        .iter()
        .filter(|e| {
            e.args
                .iter()
                .any(|a| a.kind == fs_emevd::catalogue::ArgKind::Flag && a.word == 1)
        })
        .map(|e| (e.bank, e.id))
        .collect();
    let (mut anywhere, mut as_flag) = (HashSet::new(), HashSet::new());
    let mut names = vec![
        "/event/common.emevd.dcx".to_string(),
        "/event/common_func.emevd.dcx".to_string(),
    ];
    for a in 10..62 {
        for b in 0..100 {
            for c in 0..100 {
                for d in 0..4 {
                    names.push(format!("/event/m{a:02}_{b:02}_{c:02}_{d:02}.emevd.dcx"));
                }
            }
        }
    }
    for p in names.iter().filter(|p| dvd.find(p).is_some()) {
        let plain = fs_dcx::decompress(&dvd.read_path(p).unwrap()).unwrap().1;
        for ins in fs_emevd::Emevd::read(&plain)
            .unwrap()
            .events
            .iter()
            .flat_map(|e| &e.instructions)
        {
            for (w, word) in ins.args.chunks_exact(4).enumerate() {
                let v = u32::from_le_bytes(word.try_into().unwrap());
                if flags.contains(&v) {
                    anywhere.insert(v);
                    if w == 1 && flag_words.contains(&(ins.bank, ins.id)) {
                        as_flag.insert(v);
                    }
                }
            }
        }
    }
    eprintln!(
        "defeat flags: {}/{} in the scripts, {} as a flag word",
        anywhere.len(),
        flags.len(),
        as_flag.len()
    );
    assert!(pct(anywhere.len(), flags.len()) >= 95.0);
    assert!(pct(as_flag.len(), flags.len()) >= 70.0);
}
