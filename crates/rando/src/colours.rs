//! Enemy colours (Nova, 2026-11-06): each kind of ordinary enemy is drawn in a colour
//! of the seed's. Untested in play.
//!
//! The game draws a body in another colour through a chain of rows: an effect on the
//! character (SpEffectParam) names a `vfx` row (SpEffectVfxParam), which names a
//! `phantom` row (PhantomParam) holding the colours. The ghostly enemies run
//! 295000 → 57000 → 200, the spirit ashes 1400 → 11300 → 203. So [`HUES`] new phantom
//! rows are copied from the game's neutral one ([`NEUTRAL`], which draws a body as it
//! is) with the body's own colour, the third, made a hue; each gets a vfx row copied
//! from 57100 and an effect copied from 18672 (an effect of the game's that does nothing
//! but carry 57100). Every NpcParam row an ordinary enemy of the maps stands on, whose
//! last effect slot (`effect_last`) is empty, takes one of the effects. Rows are shared
//! by every enemy of a kind, so a kind is one colour; copies the enemy randomizer makes
//! carry it along.
//!
//! Left alone: bosses' rows, characters with a CharaInitParam (NPCs and invaders), and
//! rows whose last slot already holds an effect.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use fs_bytes::{Result, format_err};
use fs_param::layout::for_table;
use fs_param::{Regulation, Row};
use gamedata::model::Model;
use items::rng::Rng;

/// How many colours.
pub const HUES: usize = 12;
/// The game's rows copied.
pub const NEUTRAL: i32 = 999;
pub const VFX_TEMPLATE: i32 = 57_100;
pub const EFFECT_TEMPLATE: i32 = 18_672;
/// Our rows: each table's run is free in the game.
pub const PHANTOM_FIRST: i32 = 9_100;
pub const VFX_FIRST: i32 = 9_100_000;
pub const EFFECT_FIRST: i32 = 19_990_000;
/// The body's own colour, bytes 6 to 8 of `colour` (blue, green, red).
const BODY: usize = 6;

const SALT: u64 = 0x636f_6c6f_7572_7321;

/// The colours, blue, green, red: hues round the wheel, none darker than 96 in any
/// channel so no body turns black, none so pale it is lost.
pub fn hues() -> [[u8; 3]; HUES] {
    std::array::from_fn(|k| {
        let h = k as f64 / HUES as f64 * 6.0;
        let x = 1.0 - ((h % 2.0) - 1.0).abs();
        let (r, g, b) = match h as u32 {
            0 => (1.0, x, 0.0),
            1 => (x, 1.0, 0.0),
            2 => (0.0, 1.0, x),
            3 => (0.0, x, 1.0),
            4 => (x, 0.0, 1.0),
            _ => (1.0, 0.0, x),
        };
        let c = |v: f64| (96.0 + v * 159.0).round() as u8;
        [c(b), c(g), c(r)]
    })
}

fn copy_rows(
    reg: &mut Regulation,
    table: &str,
    template: i32,
    first: i32,
    edit: &dyn Fn(usize, &mut Vec<u8>) -> Result<()>,
) -> Result<()> {
    let mut t = reg.table(table)?;
    let ids = first..first + HUES as i32;
    if let Some(r) = t.rows.iter().find(|r| ids.contains(&r.id)) {
        return Err(format_err(format!("{table} already has a row {}", r.id)));
    }
    let source = t
        .rows
        .iter()
        .find(|r| r.id == template)
        .cloned()
        .ok_or_else(|| format_err(format!("{table} has no row {template}")))?;
    for k in 0..HUES {
        let mut data = source.data.clone();
        edit(k, &mut data)?;
        t.rows.push(Row {
            id: first + k as i32,
            data,
            name: source.name.as_ref().map(|_| Vec::new()),
        });
    }
    t.rows.sort_by_key(|r| r.id);
    reg.set_table(table, &t)
}

/// The NpcParam rows to colour: those ordinary enemies of the maps stand on.
pub fn rows_to_colour(model: &Model) -> BTreeSet<i32> {
    let bosses: HashSet<u32> = model.bosses.iter().filter_map(|b| b.entity).collect();
    let boss_rows: HashSet<i32> = model
        .enemies
        .iter()
        .filter(|e| e.entity.is_some_and(|id| bosses.contains(&id)))
        .map(|e| e.npc_param)
        .collect();
    model
        .enemies
        .iter()
        .filter(|e| e.chara_init.is_none() && e.npc_param > 0)
        .map(|e| e.npc_param)
        .filter(|r| !boss_rows.contains(r))
        .collect()
}

/// Write the colours' rows and give each row of `rows` with an empty last slot one of
/// them, drawn by the seed. Each row coloured, and its colour's number.
pub fn apply(
    reg: &mut Regulation,
    rows: &BTreeSet<i32>,
    seed: u64,
) -> Result<BTreeMap<i32, usize>> {
    let lay = |t: &str| for_table(t).ok_or_else(|| format_err(format!("no layout for {t}")));
    let colour = lay("PhantomParam")?.field("colour");
    let phantom = lay("SpEffectVfxParam")?.field("phantom");
    let vfx = lay("SpEffectParam")?.field("vfx");
    let last = lay("NpcParam")?.field("effect_last");
    let hues = hues();
    copy_rows(reg, "PhantomParam", NEUTRAL, PHANTOM_FIRST, &|k, d| {
        for (i, v) in hues[k].iter().enumerate() {
            colour.set(d, BODY + i, i64::from(*v))?;
        }
        Ok(())
    })?;
    copy_rows(reg, "SpEffectVfxParam", VFX_TEMPLATE, VFX_FIRST, &|k, d| {
        phantom.set(d, 0, i64::from(PHANTOM_FIRST + k as i32))
    })?;
    copy_rows(
        reg,
        "SpEffectParam",
        EFFECT_TEMPLATE,
        EFFECT_FIRST,
        &|k, d| vfx.set(d, 0, i64::from(VFX_FIRST + k as i32)),
    )?;

    let mut rng = Rng::new(seed ^ SALT);
    let mut table = reg.table("NpcParam")?;
    let mut out = BTreeMap::new();
    for r in &mut table.rows {
        if !rows.contains(&r.id) || last.get(&r.data, 0) != -1 {
            continue;
        }
        let k = rng.below(HUES);
        last.set(&mut r.data, 0, i64::from(EFFECT_FIRST + k as i32))?;
        out.insert(r.id, k);
    }
    reg.set_table("NpcParam", &table)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hues_are_bright_and_apart() {
        let h = hues();
        assert!(h.iter().flatten().all(|&c| c >= 96));
        let unique: BTreeSet<_> = h.iter().collect();
        assert_eq!(unique.len(), HUES);
        // The first is red: blue, green, red.
        assert_eq!(h[0], [96, 96, 255]);
    }
}
