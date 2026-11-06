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

/// Malenia's attacks (AtkParam_Npc 2120000 to 2129999), whose rows' first digits are her
/// model's, c2120.
const MALENIA_ATTACKS: std::ops::Range<i32> = 2_120_000..2_130_000;

/// Malenia no longer heals when she hits (spec `nerf_malenia_heal`). Her attacks set off
/// effects that take back a share of her health, 18410 to 18419 on 2.7.2: each heal
/// that only her attacks name, and the rest of its block of ten, gives nothing. Their
/// other work, if any, stays. Which rows changed.
pub fn no_malenia_heal(reg: &mut Regulation) -> Result<Vec<i32>> {
    let (atk, sp) = (layout("AtkParam_Npc")?, layout("SpEffectParam")?);
    let on_hit = atk.field("sp_effect");
    let (rate, point) = (sp.field("hp_change_rate"), sp.field("hp_change_point"));
    let mut hers = std::collections::BTreeSet::new();
    let mut others = std::collections::BTreeSet::new();
    for table in ["AtkParam_Npc", "AtkParam_Pc"] {
        for row in reg.table(table)?.rows {
            for k in 0..on_hit.count {
                let e = on_hit.get(&row.data, k);
                if e > 0 {
                    if table == "AtkParam_Npc" && MALENIA_ATTACKS.contains(&row.id) {
                        hers.insert(e);
                    } else {
                        others.insert(e);
                    }
                }
            }
        }
    }
    let mut effects = reg.table("SpEffectParam")?;
    let heals = |data: &[u8]| rate.get_f32(data, 0) < 0.0 || point.get(data, 0) < 0;
    let blocks: std::collections::BTreeSet<i64> = effects
        .rows
        .iter()
        .filter(|r| {
            let id = i64::from(r.id);
            hers.contains(&id) && !others.contains(&id) && heals(&r.data)
        })
        .map(|r| i64::from(r.id) / 10)
        .collect();
    let mut changed = Vec::new();
    for row in &mut effects.rows {
        let id = i64::from(row.id);
        if blocks.contains(&(id / 10)) && !others.contains(&id) && heals(&row.data) {
            rate.set(&mut row.data, 0, 0)?;
            point.set(&mut row.data, 0, 0)?;
            changed.push(row.id);
        }
    }
    reg.set_table("SpEffectParam", &effects)?;
    Ok(changed)
}

/// The Valiant Gargoyles' bullets (Bullet 4770000 to 4779999), from their model, c4770.
const GARGOYLE_BULLETS: std::ops::Range<i32> = 4_770_000..4_780_000;
/// Poison, as a hit sets it (SpEffectParam 20003: 0.07% and 7 health a second).
const POISON: i64 = 20003;

/// The poison clouds the Valiant Gargoyles' breath leaves behind no longer hurt (spec
/// `nerf_gargoyle_poison`): of their bullets, those another makes as it ends that last
/// five seconds or more and poison what they touch hit with nothing and set nothing. The
/// breath itself is as it was. Which rows changed.
pub fn no_gargoyle_poison(reg: &mut Regulation) -> Result<Vec<i32>> {
    let l = layout("Bullet")?;
    let (atk, life, child, effect) = (
        l.field("atk_id"),
        l.field("life"),
        l.field("child"),
        l.field("sp_effect"),
    );
    let mut table = reg.table("Bullet")?;
    let made: std::collections::BTreeSet<i64> = table
        .rows
        .iter()
        .filter(|r| GARGOYLE_BULLETS.contains(&r.id))
        .map(|r| child.get(&r.data, 0))
        .collect();
    let mut changed = Vec::new();
    for row in &mut table.rows {
        if GARGOYLE_BULLETS.contains(&row.id)
            && made.contains(&i64::from(row.id))
            && effect.get(&row.data, 0) == POISON
            && life.get_f32(&row.data, 0) >= 5.0
        {
            atk.set(&mut row.data, 0, 0)?;
            effect.set(&mut row.data, 0, 0)?;
            changed.push(row.id);
        }
    }
    reg.set_table("Bullet", &table)?;
    Ok(changed)
}
