//! DLC Start (spec `dlc_start`, Nova, 2026-11-06): a new character is taken from the
//! Chapel of Anticipation to the Gravesite Plain, as Miquella's arm in Mohg's arena
//! takes one in the game; the area map begins there (`logic::Graph::set_dlc_start`).
//!
//! Common event [`EVENT`], started first in event 0 (which can end early on flag 2052),
//! runs once a save: three seconds after loading it turns [`STARTED`] on, then
//! - flag 105, the Roundtable Hold entered: a character who has never been there was
//!   taken there by the lab's warps (2026-11-06), so it is set as if they had been;
//! - flag 76800, the Gravesite Plain's grace lit (its BonfireWarpParam row 61464000);
//! - lot [`LOT`] and the row after it: Torrent's whistle (goods 130), which Melina gives
//!   in the base game, and the Spirit Calling Bell (8158), which Renna does;
//! - the arm's own respawn point and warp (m12_05 event 12052500): `2003[23]` on entity
//!   2046402020, then `2003[14]` to m61_46_40 at that entity, with grace 120500.
//!
//! None of it is seen in the game yet.

use std::collections::HashSet;

use fs_bytes::{Result, format_err};
use fs_emevd::{Emevd, Event};
use fs_param::layout::for_table;
use fs_param::{Regulation, Row};

use crate::capital::{ins, set_on, start};

/// Our event.
pub const EVENT: i64 = 7930;
/// On once the character has been taken to the DLC: block 0, in a run no word of any
/// script, talk script or flag field of the game holds.
pub const STARTED: u32 = 590;
/// The Roundtable Hold entered.
pub const ROUNDTABLE: u32 = 105;
/// The Gravesite Plain's grace lit.
pub const GRACE_LIT: u32 = 76_800;
/// Where the arm lands the player, in m61_46_40, and the grace it names.
pub const LANDING: u32 = 2_046_402_020;
pub const MAP: [u8; 4] = [61, 46, 40, 0];
pub const ARM_GRACE: u32 = 120_500;
/// Our lot, and the goods it and the row after it give.
pub const LOT: i32 = 19_999_000;
pub const GIVEN: [(i32, &str); 2] = [
    (130, "Spectral Steed Whistle"),
    (8158, "Spirit Calling Bell"),
];

const LOTS: &str = "ItemLotParam_map";
const GOODS: i64 = 1;
const ALWAYS: i64 = 1000;

/// Our lot rows: [`LOT`] and those after it, one goods each, laid out as a lot of the
/// game's that gives one goods alone.
pub fn lots(reg: &mut Regulation) -> Result<()> {
    let l = for_table(LOTS).ok_or_else(|| format_err("no item lot layout"))?;
    let (item, category, weight, count, flag) = (
        l.field("item_id"),
        l.field("item_category"),
        l.field("weight"),
        l.field("count"),
        l.field("get_item_flag"),
    );
    let mut table = reg.table(LOTS)?;
    let ids: Vec<i32> = (0..=GIVEN.len() as i32).map(|k| LOT + k).collect();
    // The row after the last must be free too, or the game would give it as well.
    if let Some(r) = table.rows.iter().find(|r| ids.contains(&r.id)) {
        return Err(format_err(format!(
            "DLC Start: {LOTS} already has a row {}",
            r.id
        )));
    }
    let template = table
        .rows
        .iter()
        .find(|r| {
            category.get(&r.data, 0) == GOODS
                && weight.get(&r.data, 0) == ALWAYS
                && (1..item.count).all(|s| item.get(&r.data, s) == 0)
        })
        .cloned()
        .ok_or_else(|| format_err("DLC Start: no lot gives one goods to copy"))?;
    for (k, &(goods, _)) in GIVEN.iter().enumerate() {
        let mut data = template.data.clone();
        item.set(&mut data, 0, i64::from(goods))?;
        count.set(&mut data, 0, 1)?;
        flag.set(&mut data, 0, 0)?;
        table.rows.push(Row {
            id: LOT + k as i32,
            data,
            name: template.name.as_ref().map(|_| Vec::new()),
        });
    }
    table.rows.sort_by_key(|r| r.id);
    reg.set_table(LOTS, &table)
}

/// The event that takes the character to the DLC.
pub fn event() -> Event {
    let packed = u32::from_le_bytes;
    Event {
        id: EVENT,
        rest: 0,
        instructions: vec![
            ins(1003, 2, &[packed([0, 1, 0, 0]), STARTED]),
            ins(1001, 0, &[3.0f32.to_bits()]),
            set_on(STARTED),
            set_on(ROUNDTABLE),
            set_on(GRACE_LIT),
            ins(2003, 4, &[LOT as u32]),
            ins(2003, 23, &[LANDING]),
            ins(2003, 14, &[packed(MAP), LANDING, ARM_GRACE]),
        ],
        parameters: vec![],
    }
}

/// `common` with the event, started first in event 0.
pub fn apply(common: &Emevd) -> Result<Emevd> {
    let mut out = common.clone();
    let taken: HashSet<i64> = out.events.iter().map(|e| e.id).collect();
    if taken.contains(&EVENT) {
        return Err(format_err(format!(
            "DLC Start: common.emevd already has an event {EVENT}"
        )));
    }
    let zero = out
        .events
        .iter_mut()
        .find(|e| e.id == 0)
        .ok_or_else(|| format_err("DLC Start: common.emevd has no event 0"))?;
    if !zero.parameters.is_empty() {
        return Err(format_err("DLC Start: common event 0 takes arguments"));
    }
    zero.instructions.insert(0, start(EVENT));
    out.events.push(event());
    Ok(out)
}
