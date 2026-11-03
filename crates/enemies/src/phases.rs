//! Bosses whose next stage an event in their own arena starts, on something only
//! their own body does (docs/spec/enemies.md §3.2). The event goes where the boss
//! goes, naming the entity it now stands as, and no longer starts at home, where
//! no newcomer would set it off.
//!
//! Read from the scripts with `apoc-fs boss-phases`, 2026-11-03, and **not seen in
//! the game**:
//! - **Malenia** (15002811): her health held at 1 (`4[14]`, test 5 against 1) and
//!   not yet under effect 18480. The event gives her the second stage's effects.
//! - **Godrick** (10002811): effect 14785 on him, which only his body puts on. The
//!   event gives him effect 14750, eight `2004[26]` settings and animation 20011,
//!   after his arena's cutscene, and sets flag 10002802.
//!
//! What only makes sense in the home arena does nothing in the new one, rather than
//! being removed, so the event's counted skips still land where they did: cutscenes,
//! camera and music, `2004[41]`, the health bar, assets, effects at places, and the
//! stand-ins that are not in other maps. Tests of the player in a home region are
//! simply false elsewhere. The flag its defeat sets becomes the new spot's.
//!
//! Other bosses' stage events were read and stay home: they set a flag (music, in
//! the shape of the 60% health events every arena has), or they need their arena's
//! stand-ins and regions (Regal Ancestor Spirit's, Starscourge Radahn's and Rykard's),
//! and none of them is waited on by an event that calls the defeat. Such a boss
//! moves without that stage. See docs/progress.md, 2026-11-03.

use std::collections::BTreeMap;

use fs_bytes::{Result, format_err};
use fs_emevd::catalogue::{self, ArgKind};
use fs_emevd::{Emevd, Event, Instruction};
use gamedata::{MapId, Sources};

use crate::{Plan, Pool, Spot, malenia};

/// One boss's stage event.
pub struct Stage {
    /// The boss's entity at home.
    pub entity: u32,
    pub home: MapId,
    pub event: i64,
    /// Bodies of its arena the event acts on, which other maps do not have.
    pub stand_ins: &'static [u32],
}

/// Every stage event that goes with its boss.
pub const STAGES: [Stage; 2] = [
    Stage {
        entity: malenia::HER_ENTITY,
        home: MapId([15, 0, 0, 0]),
        event: 15_002_811,
        stand_ins: &[15_000_801],
    },
    Stage {
        entity: 10_000_800,
        home: MapId([10, 0, 0, 0]),
        event: 10_002_811,
        stand_ins: &[],
    },
];

/// `start_event`, in event 0 of every script.
const START: (u32, u32) = (2000, 0);

/// Whether a spot is where a boss with a stage event stands unmodded.
pub fn is_home(spot: &Spot) -> bool {
    STAGES.iter().any(|st| spot.entity == Some(st.entity))
}

fn names_word(i: &Instruction, value: u32) -> bool {
    i.args
        .chunks_exact(4)
        .any(|w| u32::from_le_bytes(w.try_into().unwrap()) == value)
}

fn is_arenas_own(i: &Instruction, stage: &Stage) -> bool {
    let bar = catalogue::named("boss_health_bar");
    matches!(i.bank, 2002 | 2005 | 2006 | 2008 | 2010 | 2011)
        || (i.bank, i.id) == (2004, 41)
        || (i.bank, i.id) == (bar.bank, bar.id)
        || stage.stand_ins.iter().any(|&s| names_word(i, s))
}

/// The stage event as it would run on `entity`, whose defeat sets `flag`.
pub fn moved_event(home: &Event, stage: &Stage, entity: u32, flag: Option<(u32, u32)>) -> Event {
    let mut event = home.clone();
    event.parameters.clear();
    for i in &mut event.instructions {
        if is_arenas_own(i, stage) {
            *i = malenia::nothing();
            continue;
        }
        // The defeat flag first, where the catalogue says a word is a flag.
        if let (Some((old, new)), Some(entry)) = (flag, catalogue::lookup(i.bank, i.id)) {
            for a in entry.args.iter().filter(|a| a.kind == ArgKind::Flag) {
                if let Some(w) = i.args.get_mut(a.word * 4..a.word * 4 + 4)
                    && u32::from_le_bytes((&*w).try_into().unwrap()) == old
                {
                    w.copy_from_slice(&new.to_le_bytes());
                }
            }
        }
        for w in i.args.chunks_exact_mut(4) {
            if u32::from_le_bytes((&*w).try_into().unwrap()) == stage.entity {
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

fn script_mut<'a>(
    s: &Sources,
    out: &'a mut BTreeMap<MapId, Emevd>,
    m: MapId,
) -> Result<&'a mut Emevd> {
    match out.entry(m) {
        std::collections::btree_map::Entry::Occupied(e) => Ok(e.into_mut()),
        std::collections::btree_map::Entry::Vacant(e) => {
            let stock = s
                .scripts
                .iter()
                .find(|(id, _)| *id == m)
                .map(|(_, e)| e.clone())
                .ok_or_else(|| format_err(format!("no script for {m}")))?;
            Ok(e.insert(stock))
        }
    }
}

/// The scripts to change for where this plan puts each boss with a stage event.
pub fn scripts(s: &Sources, pool: &Pool, plan: &Plan) -> Result<Vec<(MapId, Emevd)>> {
    let mut out: BTreeMap<MapId, Emevd> = BTreeMap::new();
    for stage in &STAGES {
        let Some(here) = plan
            .from
            .iter()
            .position(|&f| pool.spots[f].entity == Some(stage.entity))
        else {
            continue;
        };
        let spot = &pool.spots[here];
        if spot.entity == Some(stage.entity) {
            continue;
        }
        let (Some(boss), Some(entity)) = (&spot.boss, spot.entity) else {
            return Err(format_err(format!(
                "the boss of event {} landed on a spot with no script",
                stage.event
            )));
        };
        let home_flag = pool
            .spots
            .iter()
            .find(|p| p.entity == Some(stage.entity))
            .and_then(|p| p.boss.as_ref())
            .and_then(|b| b.defeat_flag);
        let flag = home_flag.zip(boss.defeat_flag);
        let stock = s
            .scripts
            .iter()
            .find(|(m, _)| *m == stage.home)
            .and_then(|(_, e)| e.events.iter().find(|e| e.id == stage.event))
            .cloned()
            .ok_or_else(|| format_err(format!("event {} is not in its arena", stage.event)))?;
        let moved = moved_event(&stock, stage, entity, flag);
        if boss.script == stage.home {
            // The same script: the event stays and is started as before.
            let home = script_mut(s, &mut out, stage.home)?;
            if let Some(e) = home.events.iter_mut().find(|e| e.id == stage.event) {
                *e = moved;
            }
            continue;
        }
        let home = script_mut(s, &mut out, stage.home)?;
        for e in home.events.iter_mut().filter(|e| e.id == 0) {
            for i in &mut e.instructions {
                if starts(i, stage.event) {
                    *i = malenia::nothing();
                }
            }
        }
        let there = script_mut(s, &mut out, boss.script)?;
        if there.events.iter().any(|e| e.id == stage.event) {
            return Err(format_err(format!(
                "event {}'s id is taken in the new script",
                stage.event
            )));
        }
        there.events.push(moved);
        there
            .events
            .iter_mut()
            .find(|e| e.id == 0)
            .ok_or_else(|| format_err("the new script has no event 0"))?
            .instructions
            .push(start_of(stage.event));
    }
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
        let her = &STAGES[0];
        let home = Event {
            id: her.event,
            rest: 0,
            instructions: vec![
                ins(2004, 8, &[her.entity, 18000]),
                ins(2008, 1, &[2121, 2121]),
                ins(2004, 8, &[her.stand_ins[0], 18400]),
                ins(2003, 66, &[0, 15_002_802, 1]),
            ],
            parameters: vec![],
        };
        let moved = moved_event(&home, her, 10_000_850, None);
        assert_eq!(moved.instructions.len(), 4);
        assert!(names_word(&moved.instructions[0], 10_000_850));
        assert_eq!(moved.instructions[1], malenia::nothing());
        assert_eq!(moved.instructions[2], malenia::nothing());
        assert!(names_word(&moved.instructions[3], 15_002_802));
    }

    #[test]
    fn the_defeat_flag_becomes_the_new_spots() {
        let godrick = &STAGES[1];
        let home = Event {
            id: godrick.event,
            rest: 0,
            instructions: vec![
                ins(1003, 2, &[256, 10_000_800]),
                ins(2005, 3, &[10_001_890, 0]),
                ins(2003, 18, &[10_000_800, 20011, 256, 0x3F80_0000]),
            ],
            parameters: vec![],
        };
        let moved = moved_event(&home, godrick, 30_000_800, Some((10_000_800, 30_002_800)));
        assert!(names_word(&moved.instructions[0], 30_002_800));
        assert_eq!(moved.instructions[1], malenia::nothing());
        assert!(names_word(&moved.instructions[2], 30_000_800));
    }
}
