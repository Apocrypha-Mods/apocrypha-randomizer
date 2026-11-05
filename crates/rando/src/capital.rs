//! Leyndell's gate asks what the area map asks: two of six Great Runes held (Nova,
//! 2026-11-03).
//!
//! The gate (m60_45_52 event 1045522500) waits on flag 182 and on flag 105. In the game,
//! common event 730, started from common event 0 with (182, 2), turns 182 on when two of
//! flags 170 to 179 are on: flags of the bosses' reward rows and of common event 6905,
//! which stay with the reward, not with a moved rune. So every seed rewrites that one
//! start: event 0 starts [`COUNT_EVENT`] there instead, and six events of ours, one a
//! rune, each turn a flag of [`HELD`] on once the player holds that rune, unrestored or
//! restored. [`COUNT_EVENT`] turns 182 on when as many of those are on as the seed's
//! option asks (`leyndell_runes`, 0 to 4, the game's 2 by default); at 0 it turns 182 on
//! as the game loads. The gate still asks for flag 105, the Roundtable Hold entered.
//!
//! Each flag stays on once set, so a rune restored at its Divine Tower (event 90005110
//! takes the unrestored one and gives a lot) still counts, and so does one whose tower
//! now gives something else. Each event ends at once if its flag is on, as 730 does, so on
//! a reload nothing is counted twice. Morgott's rune stands behind the gate, so it does not
//! count; Rennala's Great Rune of the Unborn does, as in the game, by Nova's choice. Event 730's other
//! starts (flags 180, 181 and 183 to 187), flags 170 to 179 and all that reads them stay
//! as the game has them.
//!
//! Written in every seed, whatever moves: the map asks for runes whether or not they
//! moved, and with runes where the game puts them the game asks the same. With other mods, this builds on their `common.emevd`.

use std::collections::HashSet;

use fs_bytes::{Result, format_err};
use fs_emevd::{Emevd, Event, Instruction};

/// The flag the gate waits on.
pub const GATE: u32 = 182;
/// The event that turns [`GATE`] on in the game, and the arguments event 0 starts it with.
pub const GAME_COUNT: i64 = 730;
/// Our flags, one a rune, on once that rune has been held. Block 0, beside the game's own
/// rune counters; no script, talk script or table of the game names them.
pub const HELD: [u32; 6] = [400, 401, 402, 403, 404, 405];
/// Our event that turns [`GATE`] on.
pub const COUNT_EVENT: i64 = 731;
/// Our events, one a rune, in the order of [`RUNES`].
pub const RUNE_EVENTS: [i64; 6] = [732, 733, 734, 735, 736, 737];
/// The most Great Runes the gate can be set to ask for.
pub const MOST: u8 = 4;

/// A Great Rune the gate counts: its name, as the area map names both forms, and its
/// goods ids. The unrestored one is a boss's reward; the restored one is given at the
/// Divine Tower, whose event 90005110 is called with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rune {
    pub name: &'static str,
    pub unrestored: u32,
    pub restored: u32,
}

pub const RUNES: [Rune; 6] = [
    Rune {
        name: "Godrick's Great Rune",
        unrestored: 8148,
        restored: 191,
    },
    Rune {
        name: "Radahn's Great Rune",
        unrestored: 8149,
        restored: 192,
    },
    Rune {
        name: "Rykard's Great Rune",
        unrestored: 8151,
        restored: 194,
    },
    Rune {
        name: "Mohg's Great Rune",
        unrestored: 8152,
        restored: 195,
    },
    Rune {
        name: "Malenia's Great Rune",
        unrestored: 8153,
        restored: 196,
    },
    // Rennala's, which the game's gate counted too (her reward's flag 177), by Nova's
    // choice (2026-11-03). It has one form: no tower restores it.
    Rune {
        name: "Great Rune of the Unborn",
        unrestored: 10080,
        restored: 10080,
    },
];

/// Condition groups, as bytes: the event's own, one where all must hold, one where any.
const MAIN: u8 = 0;
const ALL: u8 = 1;
const ANY: u8 = 0xFF;
/// `if_item`'s category for goods.
const GOODS: u8 = 3;
/// `if_flag_count`'s comparison: at least.
const AT_LEAST: u32 = 4;

pub(crate) fn ins(bank: u32, id: u32, words: &[u32]) -> Instruction {
    Instruction {
        bank,
        id,
        args: words.iter().flat_map(|w| w.to_le_bytes()).collect(),
        layer: None,
    }
}

fn packed(b: [u8; 4]) -> u32 {
    u32::from_le_bytes(b)
}

/// End the event if this flag is already on (`end_if_flag`).
fn end_if_on(flag: u32) -> Instruction {
    ins(1003, 2, &[packed([0, 1, 0, 0]), flag])
}

/// The player's own world, into a group (`if_my_world`).
fn my_world(group: u8) -> Instruction {
    ins(3, 26, &[packed([group, 0, 0, 0])])
}

/// Put whether `source` holds into `group`; into the main group, wait for it.
fn combine(group: u8, source: u8) -> Instruction {
    ins(0, 0, &[packed([group, 1, source, 0])])
}

pub(crate) fn set_on(flag: u32) -> Instruction {
    ins(2003, 66, &[0, flag, 1])
}

/// Start an event of no arguments, as the game's own starts do (`start_event`).
pub(crate) fn start(id: i64) -> Instruction {
    ins(2000, 0, &[0, id as u32, 0])
}

/// Turn [`GATE`] on once `needed` of [`HELD`] are on, in the player's own world: event
/// 730's shape, counting our flags. At 0 the count always holds, as 730's own start for
/// flag 180 (at least 0 of 170 to 179) does.
pub fn count_event(rest: u8, needed: u8) -> Event {
    Event {
        id: COUNT_EVENT,
        rest,
        instructions: vec![
            end_if_on(GATE),
            my_world(ALL),
            ins(
                3,
                10,
                &[
                    packed([ALL, 0, 0, 0]),
                    HELD[0],
                    HELD[HELD.len() - 1],
                    AT_LEAST,
                    u32::from(needed),
                ],
            ),
            combine(MAIN, ALL),
            set_on(GATE),
        ],
        parameters: vec![],
    }
}

/// Turn `flag` on once the player holds either form of `rune`, in their own world.
pub fn rune_event(id: i64, rest: u8, rune: Rune, flag: u32) -> Event {
    let holds = |item| ins(3, 4, &[packed([ANY, GOODS, 0, 0]), item, 1]);
    Event {
        id,
        rest,
        instructions: vec![
            end_if_on(flag),
            my_world(ALL),
            holds(rune.unrestored),
            holds(rune.restored),
            combine(ALL, ANY),
            combine(MAIN, ALL),
            set_on(flag),
        ],
        parameters: vec![],
    }
}

fn words(i: &Instruction) -> impl Iterator<Item = u32> + '_ {
    i.args
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
}

/// Whether an instruction is event 0's start of 730 for [`GATE`].
pub fn starts_game_count(i: &Instruction) -> bool {
    let w: Vec<u32> = words(i).collect();
    (i.bank, i.id) == (2000, 0) && w.get(1..3) == Some(&[GAME_COUNT as u32, GATE][..])
}

/// `common` with the gate counting `needed` runes held. Refuses a script (another mod's) that does
/// not start 730 for 182 exactly once from event 0, or already holds our events' ids.
pub fn apply(common: &Emevd, needed: u8) -> Result<Emevd> {
    if needed > MOST {
        return Err(format_err(format!(
            "the capital's gate can ask for 0 to 4 Great Runes, not {needed}"
        )));
    }
    let mut out = common.clone();
    let taken: HashSet<i64> = out.events.iter().map(|e| e.id).collect();
    if let Some(id) = RUNE_EVENTS
        .iter()
        .chain([&COUNT_EVENT])
        .find(|id| taken.contains(id))
    {
        return Err(format_err(format!(
            "the capital's gate: common.emevd already has an event {id}"
        )));
    }
    let rest = out
        .events
        .iter()
        .find(|e| e.id == GAME_COUNT)
        .map(|e| e.rest)
        .ok_or_else(|| format_err("the capital's gate: common.emevd has no event 730"))?;
    let zero = out
        .events
        .iter_mut()
        .find(|e| e.id == 0)
        .ok_or_else(|| format_err("the capital's gate: common.emevd has no event 0"))?;
    let at: Vec<usize> = zero
        .instructions
        .iter()
        .enumerate()
        .filter(|(_, i)| starts_game_count(i))
        .map(|(k, _)| k)
        .collect();
    let [at] = at[..] else {
        return Err(format_err(format!(
            "the capital's gate: common event 0 starts event 730 for flag 182 {} times, not once",
            at.len()
        )));
    };
    if zero.parameters.iter().any(|p| p.instruction == at as i64) {
        return Err(format_err(
            "the capital's gate: event 730's start takes an argument of event 0",
        ));
    }
    zero.instructions[at] = start(COUNT_EVENT);
    zero.instructions
        .extend(RUNE_EVENTS.iter().map(|&id| start(id)));
    out.events.push(count_event(rest, needed));
    for ((&id, rune), flag) in RUNE_EVENTS.iter().zip(RUNES).zip(HELD) {
        out.events.push(rune_event(id, rest, rune, flag));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs_emevd::catalogue::{Entry, lookup};

    /// A common script with event 0 starting 730 for 181 and 182, as the game's does.
    fn common() -> Emevd {
        let game_730 = Event {
            id: GAME_COUNT,
            rest: 0,
            instructions: vec![end_if_on(0), set_on(0)],
            parameters: vec![],
        };
        let zero = Event {
            id: 0,
            rest: 0,
            instructions: vec![
                ins(2000, 0, &[1, 730, 181, 1]),
                ins(2000, 0, &[2, 730, 182, 2]),
                ins(2000, 0, &[0, 6905, 0]),
            ],
            parameters: vec![],
        };
        Emevd {
            events: vec![zero, game_730],
            ..Default::default()
        }
    }

    /// An instruction as its catalogue name and its words.
    fn read(i: &Instruction) -> (String, Vec<u32>) {
        let name = lookup(i.bank, i.id).map_or("?".to_string(), |e| e.name.clone());
        (name, words(i).collect())
    }

    #[test]
    fn event_zero_starts_ours_in_place_of_730_for_182_and_nothing_else_changes() {
        let before = common();
        let after = apply(&before, 2).unwrap();
        let zero = &after.events[0];
        assert_eq!(zero.instructions[0], before.events[0].instructions[0]);
        assert_eq!(zero.instructions[2], before.events[0].instructions[2]);
        assert_eq!(
            read(&zero.instructions[1]),
            ("start_event".into(), vec![0, 731, 0])
        );
        let started: Vec<u32> = zero.instructions[3..]
            .iter()
            .map(|i| words(i).nth(1).unwrap())
            .collect();
        assert_eq!(started, [732, 733, 734, 735, 736, 737]);
        assert_eq!(after.events[1], before.events[1]);
        assert_eq!(after.events.len(), 2 + 7);
        // The game's 730 is no longer started for 182.
        assert!(!zero.instructions.iter().any(starts_game_count));
    }

    #[test]
    fn the_count_reads_as_n_of_our_flags_in_the_players_world() {
        for n in 0..=4 {
            let e = &apply(&common(), n).unwrap().events[2];
            assert_eq!(e.id, COUNT_EVENT);
            assert_eq!(read(&e.instructions[2]).1[1..], [400, 405, 4, u32::from(n)]);
        }
        assert!(apply(&common(), 5).is_err());
        let e = count_event(0, 2);
        let read: Vec<_> = e.instructions.iter().map(read).collect();
        let names: Vec<&str> = read.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "end_if_flag",
                "if_my_world",
                "if_flag_count",
                "combine_group",
                "set_flag"
            ]
        );
        assert_eq!(read[0].1[1], 182);
        assert_eq!(read[2].1[1..], [400, 405, 4, 2]);
        assert_eq!(read[4].1, [0, 182, 1]);
        let combine = lookup(0, 0).unwrap();
        assert_eq!(combine.byte(&e.instructions[3], "group"), Some(0));
        assert_eq!(combine.byte(&e.instructions[3], "source"), Some(1));
        let end = lookup(1003, 2).unwrap();
        assert_eq!(end.byte(&e.instructions[0], "state"), Some(1));
        assert_eq!(end.byte(&e.instructions[0], "end"), Some(0));
    }

    #[test]
    fn each_rune_event_waits_on_either_form_held() {
        for ((&id, rune), flag) in RUNE_EVENTS.iter().zip(RUNES).zip(HELD) {
            let e = rune_event(id, 0, rune, flag);
            let item = lookup(3, 4).unwrap();
            let held: Vec<u32> = e
                .instructions
                .iter()
                .filter(|i| (i.bank, i.id) == (3, 4))
                .inspect(|i| {
                    assert_eq!(item.byte(i, "group"), Some(ANY));
                    assert_eq!(item.byte(i, "category"), Some(3));
                    assert_eq!(Entry::word(i, 2), Some(1), "held");
                })
                .map(|i| Entry::word(i, 1).unwrap())
                .collect();
            assert_eq!(held, [rune.unrestored, rune.restored]);
            let last = read(e.instructions.last().unwrap());
            assert_eq!(last, ("set_flag".into(), vec![0, flag, 1]));
            assert_eq!(read(&e.instructions[0]).1, [packed([0, 1, 0, 0]), flag]);
            let combine = lookup(0, 0).unwrap();
            let groups: Vec<_> = e
                .instructions
                .iter()
                .filter(|i| (i.bank, i.id) == (0, 0))
                .map(|i| (combine.byte(i, "group"), combine.byte(i, "source")))
                .collect();
            assert_eq!(groups, [(Some(ALL), Some(ANY)), (Some(MAIN), Some(ALL))]);
        }
    }

    #[test]
    fn a_script_that_does_not_start_730_for_182_once_is_refused() {
        let mut none = common();
        none.events[0].instructions.remove(1);
        assert!(apply(&none, 2).is_err());
        let mut twice = common();
        let again = twice.events[0].instructions[1].clone();
        twice.events[0].instructions.push(again);
        assert!(apply(&twice, 2).is_err());
        let mut taken = common();
        taken.events.push(Event {
            id: 733,
            rest: 0,
            instructions: vec![],
            parameters: vec![],
        });
        assert!(apply(&taken, 2).is_err());
    }
}
