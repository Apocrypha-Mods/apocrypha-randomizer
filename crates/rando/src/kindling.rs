//! Messmer's Kindling in shards, in the game's files (spec `kindling_shards`, Nova,
//! 2026-11-06); where the shards go is `items::kindling`.
//!
//! - **Goods.** Each shard is a copy of the Kindling's EquipParamGoods row (2008021), at
//!   `items::kindling::id`, named in each language as the Kindling with "(k/N)" after it,
//!   with the Kindling's help and caption.
//! - **The Sealing Tree.** Common event 916 is the tree: it asks whether the Kindling is
//!   held (`if_item`, three times), offers to burn it, and takes it (`remove_item`). Each
//!   ask becomes one of [`KINDLED`] in the same state, the taking does nothing, and the
//!   shards stay in the bag. One instruction for one, so the event's skips still land.
//! - **Counting.** An event of ours a shard turns one of [`HELD`] on once that shard is
//!   held, and [`COUNT_EVENT`] turns [`KINDLED`] on once `needed` of them are, as the
//!   capital's gate counts Great Runes (`capital`). The flags stay on, so a shard later
//!   lost still counts.

use std::collections::HashSet;

use fs_bytes::{Result, format_err};
use fs_emevd::{Emevd, Event, Instruction};
use fs_param::Regulation;

use crate::NewGoods;
use crate::capital::{ins, set_on, start};
use items::kindling::{KINDLING, MOST, id};

/// The tree's event.
pub const TREE_EVENT: i64 = 916;
/// Our flags, on once shard k (from 1) has been held: block 0, near the capital's (400
/// to 405), in a run of 21 that no word of any script, talk script or flag field of the
/// game holds.
pub const HELD: std::ops::RangeInclusive<u32> = 446..=465;
/// On once enough shards have been held: what the tree asks in place of the Kindling.
pub const KINDLED: u32 = 466;
/// Our events, one a shard, from 7900: no script of the game has an event 7900 to 7920.
pub const FIRST_SHARD_EVENT: i64 = 7900;
/// Our event that turns [`KINDLED`] on.
pub const COUNT_EVENT: i64 = 7920;

const MAIN: u8 = 0;
const ALL: u8 = 1;
const GOODS: u8 = 3;
const AT_LEAST: u32 = 4;

fn packed(b: [u8; 4]) -> u32 {
    u32::from_le_bytes(b)
}

fn words(i: &Instruction) -> Vec<u32> {
    i.args
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

fn held(k: u8) -> u32 {
    HELD.start() + u32::from(k) - 1
}

/// The shards' new goods rows: copies of the Kindling's.
pub fn goods(reg: &mut Regulation, count: u8) -> Result<usize> {
    if count <= 1 {
        return Ok(0);
    }
    let mut table = reg.table("EquipParamGoods")?;
    let kindling = table
        .rows
        .iter()
        .find(|r| r.id == KINDLING)
        .cloned()
        .ok_or_else(|| format_err("EquipParamGoods has no Messmer's Kindling"))?;
    for k in 1..=count {
        if table.rows.iter().any(|r| r.id == id(k)) {
            return Err(format_err(format!(
                "EquipParamGoods already has row {}",
                id(k)
            )));
        }
        let mut row = kindling.clone();
        row.id = id(k);
        table.rows.push(row);
    }
    table.rows.sort_by_key(|r| r.id);
    reg.set_table("EquipParamGoods", &table)?;
    Ok(usize::from(count))
}

/// The shards' text, from the Kindling's.
pub fn text(count: u8) -> Vec<NewGoods> {
    if count <= 1 {
        return vec![];
    }
    (1..=count)
        .map(|k| NewGoods {
            id: id(k),
            from: KINDLING,
            suffix: format!(" ({k}/{count})"),
        })
        .collect()
}

/// Turn `flag` on once shard `k` is held, in the player's own world.
fn shard_event(rest: u8, k: u8) -> Event {
    Event {
        id: FIRST_SHARD_EVENT + i64::from(k) - 1,
        rest,
        instructions: vec![
            ins(1003, 2, &[packed([0, 1, 0, 0]), held(k)]),
            ins(3, 26, &[packed([ALL, 0, 0, 0])]),
            ins(3, 4, &[packed([ALL, GOODS, 0, 0]), id(k) as u32, 1]),
            ins(0, 0, &[packed([MAIN, 1, ALL, 0])]),
            set_on(held(k)),
        ],
        parameters: vec![],
    }
}

/// Turn [`KINDLED`] on once `needed` of the first `count` of [`HELD`] are on.
fn count_event(rest: u8, count: u8, needed: u8) -> Event {
    Event {
        id: COUNT_EVENT,
        rest,
        instructions: vec![
            ins(1003, 2, &[packed([0, 1, 0, 0]), KINDLED]),
            ins(3, 26, &[packed([ALL, 0, 0, 0])]),
            ins(
                3,
                10,
                &[
                    packed([ALL, 0, 0, 0]),
                    held(1),
                    held(count),
                    AT_LEAST,
                    u32::from(needed),
                ],
            ),
            ins(0, 0, &[packed([MAIN, 1, ALL, 0])]),
            set_on(KINDLED),
        ],
        parameters: vec![],
    }
}

/// `common` with the tree asking for `needed` of `count` shards. Nothing changes at a
/// count of 1. Refuses a script (another mod's) whose tree event is not the game's
/// shape: no ask for the Kindling, or an argument of its own in what changes.
pub fn apply(common: &Emevd, count: u8, needed: u8) -> Result<Emevd> {
    if count <= 1 {
        return Ok(common.clone());
    }
    if count > MOST || needed == 0 || needed > count {
        return Err(format_err(format!(
            "Messmer's Kindling: {needed} of {count} shards is not 1 to {MOST}, at most all"
        )));
    }
    let mut out = common.clone();
    let taken: HashSet<i64> = out.events.iter().map(|e| e.id).collect();
    let ours: Vec<i64> = (0..i64::from(count))
        .map(|k| FIRST_SHARD_EVENT + k)
        .chain([COUNT_EVENT])
        .collect();
    if let Some(id) = ours.iter().find(|id| taken.contains(id)) {
        return Err(format_err(format!(
            "Messmer's Kindling: common.emevd already has an event {id}"
        )));
    }
    let tree = out
        .events
        .iter_mut()
        .find(|e| e.id == TREE_EVENT)
        .ok_or_else(|| format_err("Messmer's Kindling: common.emevd has no event 916"))?;
    let rest = tree.rest;
    let (mut asks, mut takes) = (0, 0);
    for k in 0..tree.instructions.len() {
        let i = &tree.instructions[k];
        let w = words(i);
        let kindling = w.get(1) == Some(&(KINDLING as u32));
        let changes = match (i.bank, i.id) {
            (3, 4) if kindling => {
                let [group, kind, ..] = w[0].to_le_bytes();
                if kind != GOODS {
                    continue;
                }
                // `if_item` asks held (1) or not (0); `if_flag`, on (1) or off (0).
                let state = u8::from(w[2] != 0);
                asks += 1;
                Some(ins(3, 0, &[packed([group, state, 0, 0]), KINDLED]))
            }
            (2003, 24) if kindling => {
                takes += 1;
                Some(enemies::malenia::nothing())
            }
            _ => None,
        };
        if let Some(new) = changes {
            if tree.parameters.iter().any(|p| p.instruction == k as i64) {
                return Err(format_err(
                    "Messmer's Kindling: the tree's ask takes an argument of its own",
                ));
            }
            tree.instructions[k] = new;
        }
    }
    if asks == 0 || takes == 0 {
        return Err(format_err(format!(
            "Messmer's Kindling: event 916 asks for it {asks} times and takes it {takes}, not the game's shape"
        )));
    }
    let zero = out
        .events
        .iter_mut()
        .find(|e| e.id == 0)
        .ok_or_else(|| format_err("Messmer's Kindling: common.emevd has no event 0"))?;
    if !zero.parameters.is_empty() {
        return Err(format_err(
            "Messmer's Kindling: common event 0 takes arguments",
        ));
    }
    // First, as event 0 can end early on flag 2052 (as `whole_map`'s start).
    for (k, &id) in ours.iter().enumerate() {
        zero.instructions.insert(k, start(id));
    }
    for k in 1..=count {
        out.events.push(shard_event(rest, k));
    }
    out.events.push(count_event(rest, count, needed));
    Ok(out)
}
