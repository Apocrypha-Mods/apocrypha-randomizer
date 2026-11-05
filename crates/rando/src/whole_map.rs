//! The whole map from the start, if the player asks (Nova, 2026-11-05: the map only,
//! not the graces).
//!
//! Each piece of the world map is drawn once its flag is on (`WorldMapPieceParam`
//! `open_flag`), and the game turns that flag on when a map fragment is picked up. So
//! one event of ours, started from common event 0, turns every piece's flag on as the
//! game loads: the map is as if every fragment were held, landmarks and all, and graces
//! still show only once touched. The flags are read from the seed's own table, so a
//! mod's pieces are drawn too. The fragments themselves stay items like any other.

use std::collections::HashSet;

use fs_bytes::{Result, format_err};
use fs_emevd::{Emevd, Event};
use fs_param::Regulation;
use fs_param::layout::for_table;

use crate::capital::{GAME_COUNT, set_on, start};

/// Our event that turns every piece's flag on.
pub const EVENT: i64 = 738;
const PIECES: &str = "WorldMapPieceParam";

/// Every piece's flag, in the table's order, without repeats or zero.
pub fn piece_flags(reg: &Regulation) -> Result<Vec<u32>> {
    let flag = for_table(PIECES)
        .ok_or_else(|| format_err("no world map piece layout"))?
        .field("open_flag");
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for row in &reg.table(PIECES)?.rows {
        let f = flag.get(&row.data, 0) as u32;
        if f != 0 && seen.insert(f) {
            out.push(f);
        }
    }
    Ok(out)
}

/// The event turning `flags` on.
pub fn event(rest: u8, flags: &[u32]) -> Event {
    Event {
        id: EVENT,
        rest,
        instructions: flags.iter().map(|&f| set_on(f)).collect(),
        parameters: vec![],
    }
}

/// `common` with every piece of the map drawn from the start.
pub fn apply(common: &Emevd, reg: &Regulation) -> Result<Emevd> {
    let flags = piece_flags(reg)?;
    if flags.is_empty() {
        return Err(format_err(
            "the whole map: WorldMapPieceParam names no flag",
        ));
    }
    let mut out = common.clone();
    if out.events.iter().any(|e| e.id == EVENT) {
        return Err(format_err(format!(
            "the whole map: common.emevd already has an event {EVENT}"
        )));
    }
    // Ends and restarts as the game's own flag counter does.
    let rest = out
        .events
        .iter()
        .find(|e| e.id == GAME_COUNT)
        .map(|e| e.rest)
        .ok_or_else(|| format_err("the whole map: common.emevd has no event 730"))?;
    out.events
        .iter_mut()
        .find(|e| e.id == 0)
        .ok_or_else(|| format_err("the whole map: common.emevd has no event 0"))?
        .instructions
        .push(start(EVENT));
    out.events.push(event(rest, &flags));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs_emevd::catalogue::{Entry, lookup};

    #[test]
    fn the_event_sets_each_flag_on() {
        let e = event(0, &[62010, 62011]);
        let set = lookup(2003, 66).unwrap();
        assert_eq!(set.name, "set_flag");
        let read: Vec<(Option<u32>, Option<u32>)> = e
            .instructions
            .iter()
            .map(|i| {
                (
                    Entry::word(i, set.arg("flag").word),
                    Entry::word(i, set.arg("state").word),
                )
            })
            .collect();
        assert_eq!(read, [(Some(62010), Some(1)), (Some(62011), Some(1))]);
    }
}
