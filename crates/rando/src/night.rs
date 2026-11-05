//! Night-only bosses at any hour, if the player asks (Nova, 2026-11-05). Untested.
//!
//! Night's Cavalry and their horses carry effect 11800 (NpcParam `effect_c`), which no
//! other row in the game has, and nothing else that was read ties them to the night
//! (`docs/progress.md`, "Night-only bosses"). So their rows lose it, the copies a seed
//! makes for moved bodies included, which is why this runs after the enemies move.
//! Deathbirds, Death Rite Birds and Bell Bearing Hunters are kept to the night some
//! other way, not yet found, and are left as they are.

use fs_bytes::{Result, format_err};
use fs_param::Regulation;
use fs_param::layout::for_table;

/// The effect that keeps Night's Cavalry to the night.
pub const NIGHT_EFFECT: i64 = 11800;

/// Clear [`NIGHT_EFFECT`] from every NpcParam row that carries it; how many rows.
pub fn apply(reg: &mut Regulation) -> Result<usize> {
    let f = for_table("NpcParam")
        .ok_or_else(|| format_err("no layout for NpcParam"))?
        .field("effect_c");
    let mut table = reg.table("NpcParam")?;
    let mut n = 0;
    for row in &mut table.rows {
        if f.get(&row.data, 0) == NIGHT_EFFECT {
            f.set(&mut row.data, 0, 0)?;
            n += 1;
        }
    }
    reg.set_table("NpcParam", &table)?;
    Ok(n)
}
