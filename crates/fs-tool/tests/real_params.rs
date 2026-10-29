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
