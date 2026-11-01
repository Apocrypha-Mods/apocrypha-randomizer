//! The maps' events that tie parts together: mounts (event type 21), groups
//! (type 15) and generators (type 5), and how many movable enemies each holds.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use fs_bytes::Result;
use fs_msb::layout::{entry_name, field};
use gamedata::Sources;

const PARTS: &str = "PARTS_PARAM_ST";
const EVENTS: &str = "EVENT_PARAM_ST";
const MODELS: &str = "MODEL_PARAM_ST";

/// The part indices an event of `kind` lists, from its type section.
fn listed(e: &[u8], kind: i64) -> Vec<i32> {
    let (from, count) = match kind {
        5 => (0x60, 32),
        15 => (0x10, 32),
        21 => (0, 2),
        _ => return Vec::new(),
    };
    let at = i64::from_le_bytes(e[0x20..0x28].try_into().unwrap()) as usize;
    (0..count)
        .filter_map(|i| e.get(at + from + i * 4..at + from + i * 4 + 4))
        .map(|w| i32::from_le_bytes(w.try_into().unwrap()))
        .filter(|&p| p >= 0)
        .collect()
}

pub fn run(game: PathBuf) -> Result<()> {
    let s = Sources::open(&game)?;
    let model = gamedata::elden_ring::extract(&s)?;
    let pool = enemies::pool(&s, &model)?;
    let spots: BTreeSet<(usize, usize)> =
        pool.spots.iter().map(|p| (p.map_index, p.part)).collect();
    for kind in [21, 15, 5] {
        let (mut events, mut parts, mut movable) = (0, 0, 0);
        let mut mixed = 0;
        let mut models: BTreeMap<String, usize> = BTreeMap::new();
        let mut in_map: BTreeMap<String, usize> = BTreeMap::new();
        for (index, (m, msb)) in s.maps.iter().enumerate() {
            let all = &msb.table(PARTS).entries;
            let names = &msb.table(MODELS).entries;
            let model_of = |p: i32| -> Option<String> {
                let e = all.get(usize::try_from(p).ok()?)?;
                let at = usize::try_from(field(PARTS, "model").get(e, 0)?).ok()?;
                entry_name(names.get(at)?)
            };
            for e in &msb.table(EVENTS).entries {
                if field(EVENTS, "type").get(e, 0) != Some(kind) {
                    continue;
                }
                let list = listed(e, kind);
                events += 1;
                let distinct: BTreeSet<_> = list.iter().filter_map(|&p| model_of(p)).collect();
                if distinct.len() > 1 {
                    mixed += 1;
                }
                for &p in &list {
                    parts += 1;
                    if let Some(name) = model_of(p) {
                        *models.entry(name).or_default() += 1;
                    }
                    if spots.contains(&(index, p as usize)) {
                        movable += 1;
                        *in_map.entry(m.to_string()).or_default() += 1;
                    }
                }
            }
        }
        let mut top: Vec<_> = models.into_iter().collect();
        top.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        top.truncate(12);
        println!(
            "type {kind}: {events} events, {mixed} of several models, {parts} parts, {movable} movable; in m10_00 {}; top {top:?}",
            in_map.get("m10_00_00_00").copied().unwrap_or(0)
        );
    }
    Ok(())
}
