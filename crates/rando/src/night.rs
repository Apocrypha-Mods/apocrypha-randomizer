//! Night-only bosses at any hour, if the player asks (Nova, 2026-11-05). Untested.
//!
//! Night's Cavalry and their horses carry effect 11800 (NpcParam `effect_c`), which no
//! other row in the game has (`docs/progress.md`, "Night-only bosses"). So their rows
//! lose it, the copies a seed makes for moved bodies included, which is why this runs
//! after the enemies move. Each of the nine maps they ride on also tells them the hour
//! in its script, effect 11825 by night and 11826 by day (two rows alike byte for
//! byte, so a mark for something else to read): they get 11825 always.
//!
//! Bell Bearing Hunters come by their shack's script: the hunter's event (common
//! 90005760) waits on a flag that the shack's own event turns on only between 20:00
//! and 05:30 (condition 1[5]), so that event's hours become the whole day.
//!
//! Deathbirds and Death Rite Birds: no script that names them tests the hour, and the
//! one thing that sets them apart is their own effect, 17650 and 17651 (NpcParam
//! +0x1E8), which match the blank effect 90000 but for a behaviour number the engine
//! keeps for them alone (SpEffectParam `behaviour`, 388 and 389). Taken to be what
//! keeps them to the night (Nova, 2026-11-06: by day too, with the option), it is
//! cleared. It may instead be their revival; not yet seen in the game.

use fs_bytes::{Result, format_err};
use fs_emevd::{Emevd, Instruction};
use fs_param::Regulation;
use fs_param::layout::for_table;

/// The condition that holds between two times of day: a group byte, then the start's
/// hour, minute and second, then the end's.
pub const HOURS: (u32, u32) = (1, 5);
/// What the Cavalry's map scripts give them by night and by day.
pub const NIGHT_MARK: u32 = 11825;
pub const DAY_MARK: u32 = 11826;
/// The Bell Bearing Hunter's event, started by each shack with the flag it waits on
/// last.
pub const HUNTER: u32 = 90005760;
const START: (u32, u32) = (2000, 6);
const SP_EFFECT: (u32, u32) = (2004, 8);
const SET_FLAG: [(u32, u32); 2] = [(2003, 66), (2003, 69)];

fn words(i: &Instruction) -> Vec<u32> {
    i.args
        .chunks_exact(4)
        .map(|w| u32::from_le_bytes(w.try_into().unwrap()))
        .collect()
}

/// Edit one map's script so its night-only bosses keep no hours; whether anything
/// changed. Safe to run on a script already edited.
pub fn edit_script(script: &mut Emevd) -> bool {
    let mut changed = false;
    // The flags the shacks' hunters wait on.
    let waits: Vec<u32> = script
        .events
        .iter()
        .flat_map(|e| &e.instructions)
        .filter(|i| (i.bank, i.id) == START)
        .map(words)
        .filter(|w| w.len() >= 6 && w[1] == HUNTER)
        .map(|w| w[5])
        .collect();
    for e in &mut script.events {
        let sets_wait = e.instructions.iter().any(|i| {
            SET_FLAG.contains(&(i.bank, i.id)) && {
                let w = words(i);
                w.len() >= 3 && waits.contains(&w[1]) && w[2] == 1
            }
        });
        for i in &mut e.instructions {
            if sets_wait && (i.bank, i.id) == HOURS && i.args.len() >= 7 {
                // From 00:00:00 to 23:59:59.
                let whole = [0, 0, 0, 23, 59, 59];
                if i.args[1..7] != whole {
                    i.args[1..7].copy_from_slice(&whole);
                    changed = true;
                }
            }
            if (i.bank, i.id) == SP_EFFECT && i.args.len() >= 8 && words(i)[1] == DAY_MARK {
                i.args[4..8].copy_from_slice(&NIGHT_MARK.to_le_bytes());
                changed = true;
            }
        }
    }
    changed
}

/// The effect that keeps Night's Cavalry to the night.
pub const NIGHT_EFFECT: i64 = 11800;

/// The Deathbird's and the Death Rite Bird's own effects, and their behaviour numbers.
pub const BIRDS: [(i32, i64); 2] = [(17650, 388), (17651, 389)];

/// Clear the birds' behaviour numbers from their effects; how many.
pub fn birds(reg: &mut Regulation) -> Result<usize> {
    let f = for_table("SpEffectParam")
        .ok_or_else(|| format_err("no layout for SpEffectParam"))?
        .field("behaviour");
    let mut table = reg.table("SpEffectParam")?;
    let mut n = 0;
    for row in &mut table.rows {
        if let Some(&(_, b)) = BIRDS.iter().find(|(id, _)| *id == row.id) {
            if f.get(&row.data, 0) != b {
                return Err(format_err(format!(
                    "effect {} has behaviour {}, not {b}",
                    row.id,
                    f.get(&row.data, 0)
                )));
            }
            f.set(&mut row.data, 0, 0)?;
            n += 1;
        }
    }
    reg.set_table("SpEffectParam", &table)?;
    Ok(n)
}

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
