//! Music shuffled (Nova, 2026-11-06). Untested in play.
//!
//! **Boss music.** Instruction `2010[10]` (music id, state) plays a boss's theme; 524 of
//! its 606 uses are in common event 9005822, which each arena starts with the id among
//! its arguments. The id is a row of `WwiseValueToStrParam_BgmBossChrIdConv`, whose
//! +0x04 holds the theme's Wwise name (`c2130_A`, `MidBoss_…`) in 32 bytes. The names
//! change places among the rows the scripts play, so each arena plays another's theme,
//! its phases (states 1 and 2) as the theme has them.
//!
//! **Area music.** MapDefaultInfoParam `bgm_place` is the music a map plays, a number
//! the same table names as `Bgm_NNN_…` (rows 11000000 on). The numbers in use change
//! places, one for one, among the maps. Maps that hold 999 (`None`), Stormveil and the
//! DLC's open world, take their music from their own sound points and keep it.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use fs_bytes::{Result, format_err};
use fs_emevd::Emevd;
use fs_param::Regulation;
use fs_param::layout::for_table;
use items::rng::Rng;

/// The table that names the music.
pub const NAMES: &str = "WwiseValueToStrParam_BgmBossChrIdConv";
/// Where a row's name is, and how long.
const NAME: std::ops::Range<usize> = 4..36;
/// Plays music: its first word the id, its second the state.
const PLAY: (u32, u32) = (2010, 10);
/// Starting an event: `2000[0]` and `2000[6]`, the event's arguments from byte 8.
const STARTS: [(u32, u32); 2] = [(2000, 0), (2000, 6)];
/// The rows naming area music: 11000000 + the place number.
const AREA_FIRST: i32 = 11_000_000;
/// No music of the map's own.
const NO_PLACE: i64 = 999;

const SALT_BOSS: u64 = 0x6d75_7369_635f_6273;
const SALT_AREA: u64 = 0x6d75_7369_635f_6172;

fn word(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Every music id some script plays with `2010[10]`, read as written or from the
/// arguments each start of the event passes.
pub fn played(scripts: &[&Emevd]) -> BTreeSet<u32> {
    // Each event's play instructions: the id as written, or the byte its start
    // passes it from.
    let mut fixed = BTreeSet::new();
    let mut passed: HashMap<i64, Vec<usize>> = HashMap::new();
    for e in scripts.iter().flat_map(|s| &s.events) {
        for (k, i) in e.instructions.iter().enumerate() {
            if (i.bank, i.id) != PLAY {
                continue;
            }
            let from = e
                .parameters
                .iter()
                .find(|p| p.instruction == k as i64 && p.target == 0);
            match from {
                Some(p) => passed.entry(e.id).or_default().push(p.source as usize),
                None => fixed.extend(word(&i.args, 0)),
            }
        }
    }
    for i in scripts
        .iter()
        .flat_map(|s| &s.events)
        .flat_map(|e| &e.instructions)
        .filter(|i| STARTS.contains(&(i.bank, i.id)))
    {
        let Some(id) = word(&i.args, 4) else { continue };
        if let Some(sources) = passed.get(&i64::from(id)) {
            for s in sources {
                fixed.extend(word(&i.args, 8 + s));
            }
        }
    }
    fixed
}

/// Shuffle the boss themes' names among the rows the scripts play; how many rows.
pub fn shuffle_boss(reg: &mut Regulation, played: &BTreeSet<u32>, seed: u64) -> Result<usize> {
    let mut table = reg.table(NAMES)?;
    let rows: Vec<usize> = table
        .rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.id > 0 && r.id < AREA_FIRST && played.contains(&(r.id as u32)))
        .filter(|(_, r)| r.data.len() >= NAME.end && r.data[NAME.start] != 0)
        .map(|(k, _)| k)
        .collect();
    let mut names: Vec<Vec<u8>> = rows
        .iter()
        .map(|&k| table.rows[k].data[NAME].to_vec())
        .collect();
    Rng::new(seed ^ SALT_BOSS).shuffle(&mut names);
    for (&k, name) in rows.iter().zip(names) {
        table.rows[k].data[NAME].copy_from_slice(&name);
    }
    reg.set_table(NAMES, &table)?;
    Ok(rows.len())
}

/// The area music numbers the table names (`Bgm_NNN_…`).
fn named_places(reg: &Regulation) -> Result<BTreeSet<i64>> {
    Ok(reg
        .table(NAMES)?
        .rows
        .iter()
        .filter(|r| r.data.get(NAME.start..NAME.start + 4) == Some(b"Bgm_"))
        .filter_map(|r| {
            let name = &r.data[NAME];
            let digits = std::str::from_utf8(name.get(4..7)?).ok()?;
            digits.parse::<i64>().ok()
        })
        .collect())
}

/// Shuffle the area music among the maps: each number in use that the table names
/// becomes another, one for one. The maps changed, by number before and after.
pub fn shuffle_areas(reg: &mut Regulation, seed: u64) -> Result<BTreeMap<i64, i64>> {
    let named = named_places(reg)?;
    let f = for_table("MapDefaultInfoParam")
        .ok_or_else(|| format_err("no layout for MapDefaultInfoParam"))?
        .field("bgm_place");
    let mut table = reg.table("MapDefaultInfoParam")?;
    let used: BTreeSet<i64> = table
        .rows
        .iter()
        .map(|r| f.get(&r.data, 0))
        .filter(|p| *p != NO_PLACE && named.contains(p))
        .collect();
    let from: Vec<i64> = used.iter().copied().collect();
    let mut to = from.clone();
    Rng::new(seed ^ SALT_AREA).shuffle(&mut to);
    let map: BTreeMap<i64, i64> = from.into_iter().zip(to).collect();
    for r in &mut table.rows {
        if let Some(&p) = map.get(&f.get(&r.data, 0)) {
            f.set(&mut r.data, 0, p)?;
        }
    }
    reg.set_table("MapDefaultInfoParam", &table)?;
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs_emevd::{Event, Instruction, Parameter};

    fn ins(bank: u32, id: u32, words: &[u32]) -> Instruction {
        Instruction {
            bank,
            id,
            args: words.iter().flat_map(|w| w.to_le_bytes()).collect(),
            layer: None,
        }
    }

    #[test]
    fn ids_are_read_as_written_and_from_starts() {
        let common_func = Emevd {
            events: vec![Event {
                id: 9_005_822,
                rest: 0,
                instructions: vec![ins(1000, 0, &[0]), ins(2010, 10, &[0, 0])],
                parameters: vec![Parameter {
                    instruction: 1,
                    target: 0,
                    source: 12,
                    size: 4,
                    unk: 0,
                }],
            }],
            ..Emevd::default()
        };
        let map = Emevd {
            events: vec![Event {
                id: 10_000_000,
                rest: 0,
                instructions: vec![
                    ins(2000, 6, &[0, 9_005_822, 10_000_800, 920, 0, 475_000, 0]),
                    ins(2010, 10, &[213_000, 0]),
                ],
                parameters: vec![],
            }],
            ..Emevd::default()
        };
        assert_eq!(
            played(&[&common_func, &map]),
            BTreeSet::from([213_000, 475_000])
        );
    }
}
