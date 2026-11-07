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
//! the same table names as `Bgm_NNN_…` (rows 11000000 on). Maps that hold 999 (`None`),
//! Stormveil and the DLC's open world, take their music from their sound areas: MSB
//! points of type 30 whose `bgm_place` (and, on a few, `bgm_place_also`) is a number of
//! the same kind; elsewhere such an area overrides the map's own number inside it.
//! Every number in use in either place changes places with another, one for one, so a
//! number's music goes everywhere that number was: 46 on 2.7.2 (silence's 180 stays where it
//! is). Later versions of a tile take the same swap
//! ([`crate::far::twin`]).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use fs_bytes::{Result, format_err};
use fs_emevd::Emevd;
use fs_msb::Msb;
use fs_msb::layout::field;
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
/// The number the sound areas named for silence (`無音`) hold: it stays where it is, so
/// silence is never moved and no music lands where the game wants none (Nova).
const SILENCE: i64 = 180;
/// A point that is a sound area.
const SOUND_AREA: i64 = 30;
const POINTS: &str = "POINT_PARAM_ST";
/// The point fields that hold a music number.
const POINT_PLACES: [&str; 2] = ["bgm_place", "bgm_place_also"];

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

/// The area music numbers the table names (`Bgm_NNN_…`), with the names.
fn named_places(reg: &Regulation) -> Result<BTreeMap<i64, String>> {
    Ok(reg
        .table(NAMES)?
        .rows
        .iter()
        .filter(|r| r.data.get(NAME.start..NAME.start + 4) == Some(b"Bgm_"))
        .filter_map(|r| {
            let name = &r.data[NAME];
            let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
            let text = std::str::from_utf8(&name[..end]).ok()?;
            Some((text.get(4..7)?.parse().ok()?, text.to_string()))
        })
        .collect())
}

fn table_field() -> Result<&'static fs_param::layout::Field> {
    Ok(for_table("MapDefaultInfoParam")
        .ok_or_else(|| format_err("no layout for MapDefaultInfoParam"))?
        .field("bgm_place"))
}

/// Every music number a map's sound areas hold (-1 included).
pub fn map_places(msb: &Msb) -> impl Iterator<Item = i64> + '_ {
    let ty = field(POINTS, "type");
    msb.table(POINTS)
        .entries
        .iter()
        .filter(move |e| ty.get(e, 0) == Some(SOUND_AREA))
        .flat_map(|e| POINT_PLACES.map(|n| field(POINTS, n).get(e, 0)))
        .flatten()
}

/// The area music swap, drawn from the seed: each number in use, in the map table or
/// in the maps' sound areas, that the table names (999 and silence's 180 apart), to the number whose
/// music it now plays. One for one, so the numbers are the same set after.
pub fn area_swap(reg: &Regulation, maps: &[&Msb], seed: u64) -> Result<BTreeMap<i64, i64>> {
    let named = named_places(reg)?;
    let f = table_field()?;
    let used: BTreeSet<i64> = reg
        .table("MapDefaultInfoParam")?
        .rows
        .iter()
        .map(|r| f.get(&r.data, 0))
        .chain(maps.iter().flat_map(|m| map_places(m)))
        .filter(|p| *p != NO_PLACE && *p != SILENCE && named.contains_key(p))
        .collect();
    let from: Vec<i64> = used.iter().copied().collect();
    let mut to = from.clone();
    Rng::new(seed ^ SALT_AREA).shuffle(&mut to);
    Ok(from.into_iter().zip(to).collect())
}

/// Write the swap into the map table; how many rows it changed.
pub fn apply_table(reg: &mut Regulation, swap: &BTreeMap<i64, i64>) -> Result<usize> {
    let f = table_field()?;
    let mut table = reg.table("MapDefaultInfoParam")?;
    let mut n = 0;
    for r in &mut table.rows {
        let old = f.get(&r.data, 0);
        if let Some(&p) = swap.get(&old) {
            f.set(&mut r.data, 0, p)?;
            n += usize::from(p != old);
        }
    }
    reg.set_table("MapDefaultInfoParam", &table)?;
    Ok(n)
}

/// Write the swap into a map's sound areas; whether any number changed.
pub fn edit_map(swap: &BTreeMap<i64, i64>, msb: &mut Msb) -> Result<bool> {
    let ty = field(POINTS, "type");
    let Some(points) = msb.tables.iter_mut().find(|t| t.name == POINTS) else {
        return Ok(false);
    };
    let mut changed = false;
    for e in points
        .entries
        .iter_mut()
        .filter(|e| ty.get(e, 0) == Some(SOUND_AREA))
    {
        for name in POINT_PLACES {
            let f = field(POINTS, name);
            let Some(old) = f.get(e, 0) else { continue };
            if let Some(&p) = swap.get(&old) {
                f.set(e, 0, p)?;
                changed |= p != old;
            }
        }
    }
    Ok(changed)
}

/// The swap for the spoiler log by the table's names: where the first played, the
/// second now plays.
pub fn area_log(reg: &Regulation, swap: &BTreeMap<i64, i64>) -> Result<Vec<String>> {
    let names = named_places(reg)?;
    let name = |p: &i64| names.get(p).cloned().unwrap_or_else(|| p.to_string());
    Ok(swap
        .iter()
        .filter(|(a, b)| a != b)
        .map(|(a, b)| format!("{} -> {}", name(a), name(b)))
        .collect())
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

    /// A point of type `ty` whose type data (at 0x68) holds `place` and `also`.
    fn point(ty: i32, place: i32, also: i16) -> Vec<u8> {
        let mut e = vec![0u8; 0x68 + 0x20];
        e[0x08..0x0C].copy_from_slice(&ty.to_le_bytes());
        e[0x58..0x60].copy_from_slice(&0x68i64.to_le_bytes());
        e[0x6C..0x70].copy_from_slice(&place.to_le_bytes());
        e[0x70..0x72].copy_from_slice(&also.to_le_bytes());
        e[0x72..0x74].copy_from_slice(&23i16.to_le_bytes());
        e
    }

    fn map(points: Vec<Vec<u8>>) -> Msb {
        Msb {
            tables: vec![fs_msb::Table {
                version: 74,
                name: POINTS.into(),
                entries: points,
            }],
        }
    }

    #[test]
    fn sound_areas_take_the_swap_and_nothing_else_changes() {
        let mut m = map(vec![
            point(30, 100, -1),
            point(30, -1, 100),
            point(30, 50, 50),
            point(30, 7, -1),
            point(4, 100, 100),
        ]);
        assert_eq!(
            map_places(&m).filter(|&p| p != -1).collect::<Vec<_>>(),
            [100, 100, 50, 50, 7]
        );
        let swap = BTreeMap::from([(100, 50), (50, 100)]);
        let before = m.clone();
        assert!(edit_map(&swap, &mut m).unwrap());
        let e = &m.tables[0].entries;
        assert_eq!(e[0], point(30, 50, -1));
        assert_eq!(e[1], point(30, -1, 50));
        assert_eq!(e[2], point(30, 100, 100));
        assert_eq!(
            e[3], before.tables[0].entries[3],
            "a number not in the swap"
        );
        assert_eq!(e[4], before.tables[0].entries[4], "not a sound area");
        // A swap that sends each number to itself changes nothing.
        let mut same = before.clone();
        let identity = BTreeMap::from([(100, 100), (50, 50)]);
        assert!(!edit_map(&identity, &mut same).unwrap());
        assert_eq!(same, before);
    }
}
