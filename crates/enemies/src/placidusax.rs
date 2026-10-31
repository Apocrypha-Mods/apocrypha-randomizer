//! Placidusax's arena (docs/spec/enemies.md §3.3). His spot sleeps until the
//! player comes close: a looping pose, its AI off, then a wake. A newcomer may
//! not have those animations, so it stands awake instead, and the fight starts as
//! the player arrives. The event keeps its length, so counted skips still land.

use fs_bytes::{Result, format_err};
use fs_emevd::{Emevd, Event, Instruction};
use gamedata::{MapId, Sources};

use crate::malenia::nothing;
use crate::{Plan, Pool};

const SPOT: u32 = 13_000_830;
const START_EVENT: i64 = 13_002_835;
const SCRIPT: MapId = MapId([13, 0, 0, 0]);
/// The player.
const PLAYER: u32 = 10_000;
/// The region that starts the fight in the unmodded game: it reaches 72 m from him.
const START_REGION: u32 = 13_002_831;
/// How near the player must be. The cutscene lands the player 116 m from the spot,
/// or a guest 119 m, and nothing outside the arena lies within this.
const ARRIVED: f32 = 125.0;
const POSE: u32 = 30_000;
const WAKE: u32 = 20_000;

fn word(i: &Instruction, n: usize) -> Option<u32> {
    i.args
        .get(n * 4..n * 4 + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
}

fn within(group: u32) -> Instruction {
    let mut args = Vec::new();
    args.extend(group.to_le_bytes());
    args.extend(PLAYER.to_le_bytes());
    args.extend(SPOT.to_le_bytes());
    args.extend(ARRIVED.to_le_bytes());
    args.extend(1u32.to_le_bytes());
    Instruction {
        bank: 3,
        id: 3,
        args,
        layer: None,
    }
}

/// The start event with no sleep in it, or an error if it is not as expected.
fn awake(event: &mut Event) -> Result<()> {
    let (mut pose, mut ai_off, mut wake, mut region) = (false, false, false, false);
    for i in &mut event.instructions {
        let is = |bank, id, n, v| (i.bank, i.id) == (bank, id) && word(i, n) == Some(v);
        if is(2003, 18, 1, POSE) && word(i, 0) == Some(SPOT) {
            pose = true;
            *i = nothing();
        } else if is(2003, 18, 1, WAKE) && word(i, 0) == Some(SPOT) {
            wake = true;
            *i = nothing();
        } else if is(2004, 1, 0, SPOT) && word(i, 1) == Some(0) {
            ai_off = true;
            *i = nothing();
        } else if is(3, 2, 2, START_REGION) {
            region = true;
            let group = word(i, 0).unwrap_or(0);
            *i = within(group);
        }
    }
    if pose && ai_off && wake && region {
        Ok(())
    } else {
        Err(format_err("Placidusax's start event is not as expected"))
    }
}

/// Wake his arena if the plan puts a newcomer there, in `scripts` as it stands.
pub fn apply(
    s: &Sources,
    pool: &Pool,
    plan: &Plan,
    scripts: &mut Vec<(MapId, Emevd)>,
) -> Result<()> {
    let Some(spot) = pool.spots.iter().position(|p| p.entity == Some(SPOT)) else {
        return Ok(());
    };
    if plan.from[spot] == spot {
        return Ok(());
    }
    if !scripts.iter().any(|(m, _)| *m == SCRIPT) {
        let stock = s
            .scripts
            .iter()
            .find(|(m, _)| *m == SCRIPT)
            .ok_or_else(|| format_err("no script for his arena"))?;
        scripts.push(stock.clone());
    }
    let (_, script) = scripts
        .iter_mut()
        .find(|(m, _)| *m == SCRIPT)
        .ok_or_else(|| format_err("no script for his arena"))?;
    let event = script
        .events
        .iter_mut()
        .find(|e| e.id == START_EVENT)
        .ok_or_else(|| format_err("his start event is not in the script"))?;
    awake(event)
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
    fn the_sleep_goes_and_the_length_stays() {
        let mut event = Event {
            id: START_EVENT,
            rest: 0,
            instructions: vec![
                ins(2004, 1, &[SPOT, 0]),
                ins(2003, 18, &[SPOT, POSE, 1, 0x3F80_0000]),
                ins(3, 2, &[1, PLAYER, START_REGION, 1]),
                ins(2003, 18, &[SPOT, WAKE, 0, 0x3F80_0000]),
                ins(2004, 1, &[SPOT, 1]),
            ],
            parameters: vec![],
        };
        awake(&mut event).unwrap();
        assert_eq!(event.instructions.len(), 5);
        assert_eq!(event.instructions[0], nothing());
        assert_eq!(event.instructions[1], nothing());
        assert_eq!(event.instructions[2], within(1));
        assert_eq!(event.instructions[3], nothing());
        assert_eq!(event.instructions[4], ins(2004, 1, &[SPOT, 1]));
    }

    #[test]
    fn an_event_not_as_expected_stops_the_seed() {
        let mut event = Event {
            id: START_EVENT,
            rest: 0,
            instructions: vec![],
            parameters: vec![],
        };
        assert!(awake(&mut event).is_err());
    }
}
