//! Malenia's second phase (docs/spec/enemies.md §3.2). It is one body, and an
//! event in her arena's own script sets it off, so it has to go where she goes.
//! That event is copied into the script of the boss she replaces, naming that
//! boss's entity. Her flags come along unchanged: they are hers alone, and she
//! stands once in a seed.
//!
//! What only makes sense in her own arena does nothing there, rather than
//! being removed, so the event's counted skips still land where they did:
//! cutscenes, camera and music, warps, her health bar, and a stand-in who is not
//! in other maps. In her old arena the event no longer starts.

use std::collections::BTreeMap;

use fs_bytes::{Result, format_err};
use fs_emevd::{Emevd, Event, Instruction, catalogue};
use gamedata::{MapId, Sources};

use crate::{Plan, Pool, Spot};

const HER_ENTITY: u32 = 15_000_800;
const HER_STAND_IN: u32 = 15_000_801;
const PHASE_EVENT: i64 = 15_002_811;
const HOME: MapId = MapId([15, 0, 0, 0]);
/// `start_event`, in event 0 of every script.
const START: (u32, u32) = (2000, 0);

/// Whether a spot is the one she stands on in the unmodded game. Other rows of
/// her character exist, but only this one is the fight.
pub fn is_home(spot: &Spot) -> bool {
    spot.entity == Some(HER_ENTITY)
}

/// An instruction that does nothing: wait for no frames.
fn nothing() -> Instruction {
    let wait = catalogue::named("wait_frames");
    Instruction {
        bank: wait.bank,
        id: wait.id,
        args: 0i32.to_le_bytes().to_vec(),
        layer: None,
    }
}

fn names_word(i: &Instruction, value: u32) -> bool {
    i.args
        .chunks_exact(4)
        .any(|w| u32::from_le_bytes(w.try_into().unwrap()) == value)
}

fn is_arenas_own(i: &Instruction) -> bool {
    let bar = catalogue::named("boss_health_bar");
    matches!(i.bank, 2002 | 2008)
        || (i.bank, i.id) == (2004, 41)
        || (i.bank, i.id) == (bar.bank, bar.id)
        || names_word(i, HER_STAND_IN)
}

/// Her phase event as it would run on `entity`.
pub fn moved_event(home: &Event, entity: u32) -> Event {
    let mut event = home.clone();
    event.parameters.clear();
    for i in &mut event.instructions {
        if is_arenas_own(i) {
            *i = nothing();
            continue;
        }
        for w in i.args.chunks_exact_mut(4) {
            if u32::from_le_bytes((&*w).try_into().unwrap()) == HER_ENTITY {
                w.copy_from_slice(&entity.to_le_bytes());
            }
        }
    }
    event
}

fn start_of(event: i64) -> Instruction {
    let mut args = 0i32.to_le_bytes().to_vec();
    args.extend_from_slice(&(event as u32).to_le_bytes());
    args.extend_from_slice(&0i32.to_le_bytes());
    Instruction {
        bank: START.0,
        id: START.1,
        args,
        layer: None,
    }
}

fn starts(i: &Instruction, event: i64) -> bool {
    (i.bank, i.id) == START && i.args.get(4..8) == Some((event as u32).to_le_bytes().as_slice())
}

/// The scripts to change for where this plan puts her.
pub fn scripts(s: &Sources, pool: &Pool, plan: &Plan) -> Result<Vec<(MapId, Emevd)>> {
    let Some(here) = plan.from.iter().position(|&f| is_home(&pool.spots[f])) else {
        return Ok(vec![]);
    };
    let spot = &pool.spots[here];
    if spot.entity == Some(HER_ENTITY) {
        return Ok(vec![]);
    }
    let (Some(boss), Some(entity)) = (&spot.boss, spot.entity) else {
        return Err(format_err("Malenia landed on a spot with no script"));
    };
    let script = |m: MapId| {
        s.scripts
            .iter()
            .find(|(id, _)| *id == m)
            .map(|(_, e)| e.clone())
            .ok_or_else(|| format_err(format!("no script for {m}")))
    };
    let mut home = script(HOME)?;
    let event = home
        .events
        .iter()
        .find(|e| e.id == PHASE_EVENT)
        .cloned()
        .ok_or_else(|| format_err("her phase event is not in her arena's script"))?;
    for e in &mut home.events {
        if e.id == 0 {
            for i in &mut e.instructions {
                if starts(i, PHASE_EVENT) {
                    *i = nothing();
                }
            }
        }
    }
    let mut there = script(boss.script)?;
    if there.events.iter().any(|e| e.id == PHASE_EVENT) {
        return Err(format_err(
            "her phase event's id is taken in the new script",
        ));
    }
    there.events.push(moved_event(&event, entity));
    there
        .events
        .iter_mut()
        .find(|e| e.id == 0)
        .ok_or_else(|| format_err("the new script has no event 0"))?
        .instructions
        .push(start_of(PHASE_EVENT));
    let mut out: BTreeMap<MapId, Emevd> = BTreeMap::new();
    out.insert(HOME, home);
    out.insert(boss.script, there);
    Ok(out.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ins(bank: u32, id: u32, words: &[u32]) -> Instruction {
        Instruction {
            bank,
            id,
            args: words.iter().flat_map(|w| w.to_le_bytes()).collect(),
            layer: None,
        }
    }

    #[test]
    fn the_moved_event_names_the_new_boss_and_keeps_its_length() {
        let home = Event {
            id: PHASE_EVENT,
            rest: 0,
            instructions: vec![
                ins(2004, 8, &[HER_ENTITY, 18000]),
                ins(2008, 1, &[2121, 2121]),
                ins(2004, 8, &[HER_STAND_IN, 18400]),
                ins(2003, 66, &[0, 15_002_802, 1]),
            ],
            parameters: vec![],
        };
        let moved = moved_event(&home, 10_000_850);
        assert_eq!(moved.instructions.len(), 4);
        assert!(names_word(&moved.instructions[0], 10_000_850));
        assert_eq!(moved.instructions[1], nothing());
        assert_eq!(moved.instructions[2], nothing());
        assert!(names_word(&moved.instructions[3], 15_002_802));
    }
}
