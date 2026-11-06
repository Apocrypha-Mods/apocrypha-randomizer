//! Messmer's Kindling in shards (spec `kindling_shards`, Nova, 2026-11-06): `count`
//! shards, of which `needed` burn the Sealing Tree.
//!
//! Shards are new goods, one id each so that the game's scripts can count them (a goods
//! row holds no count a script reads), and they take the place of the Kindling and of
//! `count - 1` runes (Golden or Shadow Realm) lying in treasures of the DLC. Those runes are the same in
//! every seed, so one set of slots serves every seed of a proof; the shards then move
//! as any key item does. The area map asks for `needed` of them where it asked for the
//! Kindling (`logic::Graph::set_kindling_shards`).

use gamedata::Model;
use gamedata::model::{Item, ItemKind, Kind};

use crate::{Category, Place, Unit, in_dlc};

/// The Kindling's goods id.
pub const KINDLING: i32 = 2_008_021;
/// The first shard's goods id; the rest follow it. No goods row of the game is numbered
/// 2008700 to 2008719.
pub const FIRST_SHARD: i32 = 2_008_700;
/// The most shards a seed may hold.
pub const MOST: u8 = 20;

/// Each shard's name, as the area map and the spoiler log call it.
pub fn names(count: u8) -> Vec<String> {
    (1..=count)
        .map(|k| format!("Messmer's Kindling Shard {k}"))
        .collect()
}

/// Shard `k`'s goods id, from 1.
pub fn id(k: u8) -> i32 {
    FIRST_SHARD + i32::from(k) - 1
}

/// Whether a unit may give way to a shard: a Golden or Shadow Realm Rune always given by a treasure in the
/// DLC.
fn filler(model: &Model, u: &Unit) -> bool {
    u.place == Place::Lot
        && u.sure()
        && u.item.kind == ItemKind::Goods
        && u.item
            .name
            .as_deref()
            .is_some_and(|n| n.starts_with("Golden Rune") || n.starts_with("Shadow Realm Rune"))
        && model.locations[u.location].kind == Kind::Treasure
        && in_dlc(model, u.location)
}

/// Turn the Kindling's unit and `count - 1` fillers into shards. Nothing changes at a
/// count of 1. Refused with too few fillers, or no Kindling.
pub fn split(model: &Model, units: &mut [Unit], count: u8) -> std::result::Result<(), String> {
    if count <= 1 {
        return Ok(());
    }
    if count > MOST {
        return Err(format!(
            "Messmer's Kindling splits into at most {MOST} shards, not {count}"
        ));
    }
    let kindling = units
        .iter()
        .position(|u| u.item.kind == ItemKind::Goods && u.item.id == KINDLING)
        .ok_or("no slot gives Messmer's Kindling")?;
    let fillers: Vec<usize> = (0..units.len())
        .filter(|&u| filler(model, &units[u]))
        .collect();
    let want = usize::from(count) - 1;
    if fillers.len() < want {
        return Err(format!(
            "only {} runes in the DLC's treasures can become shards, not {want}",
            fillers.len()
        ));
    }
    // Spread through the list, so the runes taken are not all of one place.
    let chosen = (0..want).map(|k| fillers[k * fillers.len() / want]);
    for (k, u) in std::iter::once(kindling).chain(chosen).enumerate() {
        let k = k as u8 + 1;
        let unit = &mut units[u];
        unit.item = Item {
            kind: ItemKind::Goods,
            id: id(k),
            count: 1,
            chance: unit.item.chance,
            name: Some(names(count)[usize::from(k) - 1].clone()),
        };
        unit.category = Category::Key;
    }
    Ok(())
}
