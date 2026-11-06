//! Onslaught (docs/spec/06-enemy-placement.md §6, Nova, 2026-11-06): every ordinary
//! enemy with no entity id stands `n` times over. Each copy is a new part of the map,
//! a copy of the spot's part as the seed left it (whoever now stands there), placed in
//! a ring round it.
//!
//! - **No entity id.** A copy has none, so no script names it: it is never hidden,
//!   shown or killed by an event, and no fight waits on it. That is why only spots
//!   with no entity id are copied, as a script that hides an ambush until its moment
//!   would leave the copies standing in the open. Bosses all have one.
//! - **Groups kept.** The part's entity groups go with it, so an event that wakes or
//!   puts away a whole group (a camp, a patrol) takes the copies too.
//! - **Its own name.** `cXXXX_NNNN`, the next number free for that model in the map,
//!   also at +0x08 where every enemy part holds the number its name ends in.
//! - **After the last enemy.** Every index the layout knows moves with it
//!   ([`Msb::insert`]); what the game makes of a longer table is for the lab.

use std::collections::{BTreeMap, BTreeSet};

use fs_bytes::{Result, format_err};
use fs_msb::Msb;
use fs_msb::entry::Entry;
use fs_msb::layout::{entry_name, field};

use crate::{ENEMY, Pool};

const PARTS: &str = "PARTS_PARAM_ST";
/// How far from the spot its copies stand, in metres: enough not to stand inside
/// one another, little enough to stay on the spot's own ground.
const RING: f32 = 1.5;
/// The most an enemy may stand: the spec warns past three, so the page offers less.
pub const MOST: u8 = 10;
/// Where in an enemy part the number its name ends in is held.
const NUMBER_AT: usize = 0x08;

/// What onslaught added.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Added {
    pub copies: usize,
    pub maps: usize,
}

/// The number a part's name ends in, `c4300_9012` → 9012.
fn number(name: &str) -> Option<u32> {
    name.rsplit_once('_')?.1.parse().ok()
}

/// The model part of a name, `c4300_9012` → `c4300`.
fn stem(name: &str) -> &str {
    name.rsplit_once('_').map_or(name, |(s, _)| s)
}

/// Put `n - 1` copies of every ordinary spot with no entity id into its map. `maps`
/// holds the maps the seed has changed, by `Spot::map_index`; others are taken from
/// `stock` as they are needed.
pub fn apply(
    stock: &[(gamedata::MapId, Msb)],
    pool: &Pool,
    n: u8,
    maps: &mut BTreeMap<usize, Msb>,
) -> Result<Added> {
    if n <= 1 {
        return Ok(Added::default());
    }
    if n > MOST {
        return Err(format_err(format!(
            "onslaught is at most {MOST} of each enemy, not {n}"
        )));
    }
    let mut by_map: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for s in &pool.spots {
        if s.boss.is_none() && s.entity.is_none() {
            by_map.entry(s.map_index).or_default().push(s.part);
        }
    }
    let mut added = Added::default();
    let (ty, pos) = (field(PARTS, "type"), field(PARTS, "position"));
    for (m, parts) in by_map {
        let msb = maps.entry(m).or_insert_with(|| stock[m].1.clone());
        // The copies are made from the parts as they stand before any is added; adding
        // after the last enemy leaves every enemy's index as it was.
        let originals: Vec<Vec<u8>> = parts
            .iter()
            .map(|&p| msb.table(PARTS).entries[p].clone())
            .collect();
        let mut taken: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
        for e in &msb.table(PARTS).entries {
            if let Some(name) = entry_name(e)
                && let Some(k) = number(&name)
            {
                taken.entry(stem(&name).to_string()).or_default().insert(k);
            }
        }
        for part in originals {
            if ty.get(&part, 0) != Some(ENEMY) {
                continue;
            }
            let name = entry_name(&part).ok_or_else(|| format_err("an enemy part has no name"))?;
            let model = stem(&name).to_string();
            let at = [0, 1, 2].map(|k| pos.get_f32(&part, k).unwrap_or(0.0));
            for k in 1..n {
                let numbers = taken.entry(model.clone()).or_default();
                let next = (9000..)
                    .find(|v| !numbers.contains(v))
                    .ok_or_else(|| format_err("no number left for a copy"))?;
                numbers.insert(next);
                let mut e = Entry::read(PARTS, &part)?;
                e.set_name(&format!("{model}_{next:04}"))?;
                let mut bytes = e.write();
                bytes[NUMBER_AT..NUMBER_AT + 4].copy_from_slice(&next.to_le_bytes());
                field(PARTS, "entity_id").set(&mut bytes, 0, 0)?;
                let turn = std::f32::consts::TAU * f32::from(k - 1) / f32::from(n - 1);
                pos.set(
                    &mut bytes,
                    0,
                    i64::from((at[0] + RING * turn.cos()).to_bits()),
                )?;
                pos.set(
                    &mut bytes,
                    2,
                    i64::from((at[2] + RING * turn.sin()).to_bits()),
                )?;
                let place = msb.place_for(PARTS, ENEMY as i32);
                msb.insert(PARTS, place, bytes)?;
                added.copies += 1;
            }
        }
        added.maps += 1;
    }
    Ok(added)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_split_into_model_and_number() {
        assert_eq!(number("c4300_9012"), Some(9012));
        assert_eq!(stem("c4300_9012"), "c4300");
        assert_eq!(number("dummy"), None);
    }
}
