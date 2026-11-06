//! DLC Start (spec `dlc_start`, Nova, 2026-11-06): a new character is taken from the
//! Chapel of Anticipation to the Gravesite Plain, as Miquella's arm in Mohg's arena
//! takes one in the game; the area map begins there (`logic::Graph::set_dlc_start`).
//!
//! Common event [`EVENT`], started first in event 0 (which can end early on flag 2052),
//! runs once a save: three seconds after loading it turns [`STARTED`] on, then
//! - flag 105, the Roundtable Hold entered: a character who has never been there was
//!   taken there by the lab's warps (2026-11-06), so it is set as if they had been;
//! - flag 76800, the Gravesite Plain's grace lit (its BonfireWarpParam row 61464000);
//! - flag [`ACCORD`], which Melina's talk (`t000003000`) turns on as she gives the
//!   whistle, and which the Site of Grace's menu (`t000001000`) asks before it offers
//!   "Level Up";
//! - lot [`LOT`] and the row after it: Torrent's whistle (goods 130), which Melina gives
//!   in the base game, and the Spirit Calling Bell (8158), which Renna does;
//! - the extras the player chose ([`Extras`]), each a lot of its own;
//! - the arm's own respawn point and warp (m12_05 event 12052500): `2003[23]` on entity
//!   2046402020, then `2003[14]` to m61_46_40 at that entity, with grace 120500.
//!
//! The base-game shop ([`Extras::shop`]) is an entry of the Site of Grace's menu, shown
//! while [`STARTED`] is on, that opens ShopLineupParam rows of ours ([`SHOP_FIRST`] on):
//! every weapon, armour piece, talisman, spell, Ash of War and spirit ash the base game
//! puts anywhere, for nothing.
//!
//! None of it is seen in the game yet.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use fs_bytes::{Result, format_err};
use fs_emevd::{Emevd, Event};
use fs_esd::Esd;
use fs_esd::menu::{ShopEntry, add_shop};
use fs_param::layout::for_table;
use fs_param::{Regulation, Row};
use gamedata::model::{ItemKind, Model};
use items::{Category, Unit};

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
/// Melina's accord: on, the Site of Grace offers "Level Up" (`t000001000` machine
/// 0x7fffffe0 shows entry 2 while 4680 or 4699 is on; `t000003000` turns 4680 on as
/// Melina gives the whistle, lot 100000).
pub const ACCORD: u32 = 4680;
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

/// The lots of the extras, each run of rows followed by a free one.
pub const RUNES_LOT: i32 = 19_999_010;
pub const FLASKS_LOT: i32 = 19_999_020;
pub const SMITHING_LOT: i32 = 19_999_040;

/// Lord's Rune: 50,000 runes when used.
pub const LORDS_RUNE: i32 = 2919;
const LORDS_RUNE_RUNES: u64 = 50_000;

/// All the flasks' upgrades, talisman pouches and memory stones the base game holds.
pub const FLASKS: [(i32, u8, &str); 5] = [
    (10010, 30, "Golden Seed"),
    (10020, 12, "Sacred Tear"),
    (10040, 3, "Talisman Pouch"),
    (10030, 8, "Memory Stone"),
    (250, 1, "Flask of Wondrous Physick"),
];

/// The smithing bell bearings, the Whetstone Knife and every whetblade.
pub const SMITHING: [(i32, &str); 15] = [
    (8951, "Smithing-Stone Miner's Bell Bearing [1]"),
    (8952, "Smithing-Stone Miner's Bell Bearing [2]"),
    (8953, "Smithing-Stone Miner's Bell Bearing [3]"),
    (8954, "Smithing-Stone Miner's Bell Bearing [4]"),
    (8955, "Somberstone Miner's Bell Bearing [1]"),
    (8956, "Somberstone Miner's Bell Bearing [2]"),
    (8957, "Somberstone Miner's Bell Bearing [3]"),
    (8958, "Somberstone Miner's Bell Bearing [4]"),
    (8959, "Somberstone Miner's Bell Bearing [5]"),
    (8590, "Whetstone Knife"),
    (8970, "Iron Whetblade"),
    (8971, "Red-Hot Whetblade"),
    (8972, "Sanctified Whetblade"),
    (8973, "Glintstone Whetblade"),
    (8974, "Black Whetblade"),
];

/// The highest level the runes given may reach.
pub const MAX_LEVEL: u8 = 200;

/// What DLC Start gives beyond the whistle and the bell (spec "DLC Start extras",
/// Nova, 2026-11-06). Each is off by default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Extras {
    /// Runes enough to take a level 1 character to this level, as Lord's Runes; 0 or
    /// 1 for none.
    pub level: u8,
    /// Every Golden Seed and Sacred Tear, talisman pouch and memory stone, and the
    /// Flask of Wondrous Physick.
    pub flasks: bool,
    /// Every smithing bell bearing, the Whetstone Knife and every whetblade.
    pub smithing: bool,
    /// The base game's equipment for nothing, at any Site of Grace.
    pub shop: bool,
}

/// Runes to rise from level `from` to `to`, by the cost of each level as it is
/// generally known (673 from 1 to 2, 829 from 10 to 11).
pub fn runes_between(from: u8, to: u8) -> u64 {
    (u64::from(from)..u64::from(to))
        .map(|level| {
            let l = level as f64 + 81.0;
            let x = ((l - 92.0) * 0.02).max(0.0);
            ((x + 0.1) * l * l).floor() as u64 + 1
        })
        .sum()
}

/// The Lord's Runes that take a level 1 character to `level`.
pub fn lords_runes(level: u8) -> u64 {
    runes_between(1, level.min(MAX_LEVEL)).div_ceil(LORDS_RUNE_RUNES)
}

const LOTS: &str = "ItemLotParam_map";
const GOODS: i64 = 1;
const ALWAYS: i64 = 1000;
/// The most of one goods a lot's slot gives.
const MOST: u64 = 255;

/// Our lot rows: [`LOT`] and the rows after it, then the extras', one goods each,
/// laid out as a lot of the game's that gives one goods alone.
pub fn lots(reg: &mut Regulation, extras: &Extras) -> Result<()> {
    let mut runs: Vec<(i32, Vec<(i32, u8)>)> =
        vec![(LOT, GIVEN.iter().map(|&(g, _)| (g, 1)).collect())];
    let runes = lords_runes(extras.level);
    if runes > 0 {
        let mut rows = Vec::new();
        let mut left = runes;
        while left > 0 {
            let n = left.min(MOST);
            rows.push((LORDS_RUNE, n as u8));
            left -= n;
        }
        runs.push((RUNES_LOT, rows));
    }
    if extras.flasks {
        runs.push((FLASKS_LOT, FLASKS.iter().map(|&(g, n, _)| (g, n)).collect()));
    }
    if extras.smithing {
        runs.push((
            SMITHING_LOT,
            SMITHING.iter().map(|&(g, _)| (g, 1)).collect(),
        ));
    }

    let l = for_table(LOTS).ok_or_else(|| format_err("no item lot layout"))?;
    let (item, category, weight, count, flag) = (
        l.field("item_id"),
        l.field("item_category"),
        l.field("weight"),
        l.field("count"),
        l.field("get_item_flag"),
    );
    let mut table = reg.table(LOTS)?;
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
    let taken: HashSet<i32> = table.rows.iter().map(|r| r.id).collect();
    for (first, rows) in &runs {
        // The row after the last must be free too, or the game would give it as well.
        if let Some(id) = (*first..=*first + rows.len() as i32).find(|id| taken.contains(id)) {
            return Err(format_err(format!(
                "DLC Start: {LOTS} already has a row {id}"
            )));
        }
        for (k, &(goods, n)) in rows.iter().enumerate() {
            let mut data = template.data.clone();
            item.set(&mut data, 0, i64::from(goods))?;
            count.set(&mut data, 0, i64::from(n))?;
            flag.set(&mut data, 0, 0)?;
            table.rows.push(Row {
                id: first + k as i32,
                data,
                name: template.name.as_ref().map(|_| Vec::new()),
            });
        }
    }
    table.rows.sort_by_key(|r| r.id);
    reg.set_table(LOTS, &table)
}

/// The event that takes the character to the DLC.
pub fn event(extras: &Extras) -> Event {
    let packed = u32::from_le_bytes;
    let mut instructions = vec![
        ins(1003, 2, &[packed([0, 1, 0, 0]), STARTED]),
        ins(1001, 0, &[3.0f32.to_bits()]),
        set_on(STARTED),
        set_on(ROUNDTABLE),
        set_on(GRACE_LIT),
        set_on(ACCORD),
        ins(2003, 4, &[LOT as u32]),
    ];
    if lords_runes(extras.level) > 0 {
        instructions.push(ins(2003, 4, &[RUNES_LOT as u32]));
    }
    if extras.flasks {
        instructions.push(ins(2003, 4, &[FLASKS_LOT as u32]));
    }
    if extras.smithing {
        instructions.push(ins(2003, 4, &[SMITHING_LOT as u32]));
    }
    instructions.extend([
        ins(2003, 23, &[LANDING]),
        ins(2003, 14, &[packed(MAP), LANDING, ARM_GRACE]),
    ]);
    Event {
        id: EVENT,
        rest: 0,
        instructions,
        parameters: vec![],
    }
}

/// `common` with the event, started first in event 0.
pub fn apply(common: &Emevd, extras: &Extras) -> Result<Emevd> {
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
    out.events.push(event(extras));
    Ok(out)
}

/// The first of our ShopLineupParam rows, in a run the game leaves empty (its rows
/// stop at 9,001,004 and start again at 9,999,001).
pub const SHOP_FIRST: i32 = 9_100_000;
/// The talk text of the grace's new entry, an EventTextForTalk id no language uses.
pub const SHOP_TEXT: i32 = 26_009_821;
/// Its words, in every language.
pub const SHOP_WORDS: &str = "Base-game equipment";
pub fn shop_words(_language: &str) -> &'static str {
    SHOP_WORDS
}
/// The Site of Grace's talk script, and the machine of its menu.
pub const GRACE_SCRIPT: &str = "t000001000";
pub const GRACE_MENU: i64 = 0x7FFF_FFE0;

/// ShopLineupParam's kinds of item.
fn shop_kind(kind: ItemKind) -> Option<i64> {
    match kind {
        ItemKind::Weapon => Some(0),
        ItemKind::Protector => Some(1),
        ItemKind::Accessory => Some(2),
        ItemKind::Goods => Some(3),
        ItemKind::Gem => Some(4),
        ItemKind::CustomWeapon => None,
    }
}

/// What the base-game shop sells: every weapon, armour piece, talisman, spell, Ash of
/// War and spirit ash some slot of the base game holds in the unmodded game, weapons
/// as their plain, unupgraded row; by kind, then id.
pub fn base_equipment(
    model: &Model,
    units: &[Unit],
    reg: &Regulation,
) -> Result<Vec<(ItemKind, i32)>> {
    let weapons: HashSet<i32> = reg
        .table("EquipParamWeapon")?
        .rows
        .iter()
        .map(|r| r.id)
        .collect();
    let mut out: BTreeMap<(u8, i32), ItemKind> = BTreeMap::new();
    for u in units {
        if items::in_dlc(model, u.location) {
            continue;
        }
        let wanted = matches!(
            u.category,
            Category::Weapon
                | Category::Armour
                | Category::Talisman
                | Category::Spell
                | Category::AshOfWar
                | Category::SpiritAsh
        );
        let Some(kind) = shop_kind(u.item.kind).filter(|_| wanted) else {
            continue;
        };
        let mut id = u.item.id;
        if kind == 0 {
            // The plain weapon: no affinity, no upgrade.
            id = [id - id % 10_000, id - id % 100, id]
                .into_iter()
                .find(|w| weapons.contains(w))
                .unwrap_or(id);
        }
        out.insert((kind as u8, id), u.item.kind);
    }
    Ok(out.into_iter().map(|((_, id), kind)| (kind, id)).collect())
}

/// Our shop rows, from [`SHOP_FIRST`] on, each sold for nothing and never running
/// out; the last row's id.
pub fn shop_rows(reg: &mut Regulation, stock: &[(ItemKind, i32)]) -> Result<i32> {
    const SHOPS: &str = "ShopLineupParam";
    let l = for_table(SHOPS).ok_or_else(|| format_err("no shop layout"))?;
    let (equip, value, mtrl, flag_a, on_sale, quantity, kind) = (
        l.field("equip_id"),
        l.field("value"),
        l.field("mtrl_set_id"),
        l.field("event_flag_a"),
        l.field("on_sale_flag"),
        l.field("sell_quantity"),
        l.field("equip_type"),
    );
    let mut table = reg.table(SHOPS)?;
    let last = SHOP_FIRST + stock.len() as i32 - 1;
    if let Some(r) = table
        .rows
        .iter()
        .find(|r| (SHOP_FIRST..=last).contains(&r.id))
    {
        return Err(format_err(format!(
            "DLC Start: {SHOPS} already has a row {}",
            r.id
        )));
    }
    // A row of the game's that never runs out, to copy.
    let template = table
        .rows
        .iter()
        .find(|r| quantity.get(&r.data, 0) == 0xFF && on_sale.get(&r.data, 0) == 0)
        .cloned()
        .ok_or_else(|| format_err("DLC Start: no unlimited shop row to copy"))?;
    for (k, &(item, id)) in stock.iter().enumerate() {
        let mut data = template.data.clone();
        equip.set(&mut data, 0, i64::from(id))?;
        value.set(&mut data, 0, 0)?;
        mtrl.set(&mut data, 0, -1)?;
        flag_a.set(&mut data, 0, 0)?;
        on_sale.set(&mut data, 0, 0)?;
        kind.set(
            &mut data,
            0,
            shop_kind(item).ok_or_else(|| format_err("a custom weapon in the shop"))?,
        )?;
        table.rows.push(Row {
            id: SHOP_FIRST + k as i32,
            data,
            name: template.name.as_ref().map(|_| Vec::new()),
        });
    }
    table.rows.sort_by_key(|r| r.id);
    reg.set_table(SHOPS, &table)?;
    Ok(last)
}

/// The Site of Grace's script with the shop's entry, shown once DLC Start has run.
pub fn grace_script(grace: &Esd, last: i32) -> Result<Esd> {
    let mut out = grace.clone();
    let group = out
        .groups
        .iter()
        .position(|g| g.id == GRACE_MENU)
        .ok_or_else(|| format_err("the grace's script has no menu machine"))?;
    add_shop(
        &mut out,
        group,
        &ShopEntry {
            text: SHOP_TEXT,
            range: (SHOP_FIRST, last),
            shown_by: STARTED,
        },
    )?;
    Ok(out)
}

/// The kinds of item the shop holds, counted, for the spoiler log.
pub fn stock_counts(stock: &[(ItemKind, i32)]) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for (k, _) in stock {
        *out.entry(format!("{k:?}")).or_default() += 1;
    }
    out
}

/// The goods the extras give, by name, for the spoiler log.
pub fn given_names(extras: &Extras) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = GIVEN.iter().map(|(_, n)| n.to_string()).collect();
    let runes = lords_runes(extras.level);
    if runes > 0 {
        out.insert(format!("Lord's Rune x{runes}"));
    }
    if extras.flasks {
        out.extend(FLASKS.iter().map(|(_, n, name)| format!("{name} x{n}")));
    }
    if extras.smithing {
        out.extend(SMITHING.iter().map(|(_, n)| n.to_string()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runes_follow_the_known_costs() {
        assert_eq!(runes_between(1, 2), 673);
        assert_eq!(runes_between(10, 11), 829);
        assert_eq!(lords_runes(0), 0);
        assert_eq!(lords_runes(1), 0);
        assert_eq!(lords_runes(50), 6);
        assert_eq!(lords_runes(200), 366);
        assert_eq!(lords_runes(255), 366);
    }

    #[test]
    fn the_event_gives_each_extra_chosen() {
        let lots = |e: &Event| -> Vec<u32> {
            e.instructions
                .iter()
                .filter(|i| (i.bank, i.id) == (2003, 4))
                .map(|i| u32::from_le_bytes(i.args[..4].try_into().unwrap()))
                .collect()
        };
        assert_eq!(lots(&event(&Extras::default())), [LOT as u32]);
        let all = Extras {
            level: 100,
            flasks: true,
            smithing: true,
            shop: true,
        };
        assert_eq!(
            lots(&event(&all)),
            [LOT, RUNES_LOT, FLASKS_LOT, SMITHING_LOT].map(|l| l as u32)
        );
        // Melina's accord, for "Level Up", either way.
        let accord = set_on(ACCORD);
        assert!(event(&Extras::default()).instructions.contains(&accord));
    }
}
