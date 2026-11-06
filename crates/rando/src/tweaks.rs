//! Changes to the game's tables a player may turn on, apart from where items go
//! (Nova, 2026-11-06): every crafting recipe known, one Smithing Stone per regular
//! upgrade, and no stat requirements on weapons and spells.

use fs_bytes::{Result, format_err};
use fs_param::Regulation;
use fs_param::layout::{Layout, for_table};

/// The regular Smithing Stones, [1] to [8]. Ancient Dragon Smithing Stones and the
/// somber stones are already spent one at a time.
pub const SMITHING_STONES: std::ops::RangeInclusive<i64> = 10100..=10107;

/// The goods category in a material set's slot.
const GOODS: i64 = 4;

fn layout(table: &str) -> Result<&'static Layout> {
    for_table(table).ok_or_else(|| format_err(format!("no layout for {table}")))
}

/// Every crafting recipe on sale from the start: the flag a cookbook turns on is
/// cleared, as the game's starting recipes have none. The empty row, with no
/// materials, is left. How many rows changed.
pub fn all_recipes(reg: &mut Regulation) -> Result<usize> {
    let l = layout("ShopLineupParam_Recipe")?;
    let (flag, mtrl) = (l.field("on_sale_flag"), l.field("mtrl_set_id"));
    let mut table = reg.table("ShopLineupParam_Recipe")?;
    let mut n = 0;
    for row in &mut table.rows {
        if mtrl.get(&row.data, 0) >= 0 && flag.get(&row.data, 0) != 0 {
            flag.set(&mut row.data, 0, 0)?;
            n += 1;
        }
    }
    reg.set_table("ShopLineupParam_Recipe", &table)?;
    Ok(n)
}

/// One Smithing Stone for each regular upgrade: every material set's slot that
/// spends regular stones spends one. How many slots changed.
pub fn cheap_upgrades(reg: &mut Regulation) -> Result<usize> {
    let l = layout("EquipMtrlSetParam")?;
    let (item, count, category) = (l.field("item_id"), l.field("count"), l.field("category"));
    let mut table = reg.table("EquipMtrlSetParam")?;
    let mut n = 0;
    for row in &mut table.rows {
        for k in 0..6 {
            if SMITHING_STONES.contains(&item.get(&row.data, k))
                && category.get(&row.data, k) == GOODS
                && count.get(&row.data, k) > 1
            {
                count.set(&mut row.data, k, 1)?;
                n += 1;
            }
        }
    }
    reg.set_table("EquipMtrlSetParam", &table)?;
    Ok(n)
}

/// No weapon or spell asks for strength, dexterity, intelligence, faith or arcane.
/// How many rows changed.
pub fn no_requirements(reg: &mut Regulation) -> Result<usize> {
    let mut n = 0;
    for (table, fields) in [
        ("EquipParamWeapon", ["requirement", "requirement_arcane"]),
        ("Magic", ["requirement", "requirement_arcane"]),
    ] {
        let l = layout(table)?;
        let mut t = reg.table(table)?;
        for row in &mut t.rows {
            let mut changed = false;
            for name in fields {
                let f = l.field(name);
                for k in 0..f.count {
                    if f.get(&row.data, k) != 0 {
                        f.set(&mut row.data, k, 0)?;
                        changed = true;
                    }
                }
            }
            n += usize::from(changed);
        }
        reg.set_table(table, &t)?;
    }
    Ok(n)
}
