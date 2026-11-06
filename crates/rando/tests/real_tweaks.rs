//! The table options on the player's own game (`APOC_ER_STOCK`; skipped when
//! unset): every crafting recipe known, one stone per upgrade, no stat
//! requirements. Each changes its own rows and nothing else.

use std::path::PathBuf;

use fs_param::Regulation;
use fs_param::layout::for_table;
use gamedata::Sources;
use rando::tweaks;

fn regulation() -> Option<Regulation> {
    let dir = PathBuf::from(std::env::var_os("APOC_ER_STOCK")?);
    Some(Sources::open(&dir).unwrap().reg)
}

fn changed_tables(a: &Regulation, b: &Regulation) -> Vec<String> {
    a.table_names()
        .filter(|t| a.table(t).unwrap().rows != b.table(t).unwrap().rows)
        .map(str::to_string)
        .collect()
}

#[test]
fn every_recipe_is_on_sale_from_the_start() {
    let Some(reg) = regulation() else { return };
    let l = for_table("ShopLineupParam_Recipe").unwrap();
    let flag = l.field("on_sale_flag");
    let mut out = reg.clone();
    let n = tweaks::all_recipes(&mut out).unwrap();
    // 188 rows: the empty one, and six the game gives from the start.
    assert_eq!(n, 181);
    let rows = out.table("ShopLineupParam_Recipe").unwrap().rows;
    assert!(rows.iter().all(|r| flag.get(&r.data, 0) == 0));
    assert_eq!(changed_tables(&reg, &out), ["ShopLineupParam_Recipe"]);
}

#[test]
fn a_regular_upgrade_costs_one_stone() {
    let Some(reg) = regulation() else { return };
    let l = for_table("EquipMtrlSetParam").unwrap();
    let (item, count) = (l.field("item_id"), l.field("count"));
    let mut out = reg.clone();
    // 309 sets spend regular stones, two, four or six of them.
    assert_eq!(tweaks::cheap_upgrades(&mut out).unwrap(), 309);
    for r in out.table("EquipMtrlSetParam").unwrap().rows {
        for k in 0..6 {
            if tweaks::SMITHING_STONES.contains(&item.get(&r.data, k)) {
                assert_eq!(count.get(&r.data, k), 1, "set {}", r.id);
            }
        }
    }
    assert_eq!(changed_tables(&reg, &out), ["EquipMtrlSetParam"]);
}

#[test]
fn nothing_asks_for_stats() {
    let Some(reg) = regulation() else { return };
    let mut out = reg.clone();
    assert!(tweaks::no_requirements(&mut out).unwrap() > 1000);
    for table in ["EquipParamWeapon", "Magic"] {
        let l = for_table(table).unwrap();
        for r in out.table(table).unwrap().rows {
            for name in ["requirement", "requirement_arcane"] {
                let f = l.field(name);
                assert!(
                    (0..f.count).all(|k| f.get(&r.data, k) == 0),
                    "{table} {}",
                    r.id
                );
            }
        }
    }
    let mut changed = changed_tables(&reg, &out);
    changed.sort();
    assert_eq!(changed, ["EquipParamWeapon", "Magic"]);
}
