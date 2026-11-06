//! Gestures shuffled (spec `random_gestures`, Nova, 2026-11-06): the gestures the event
//! scripts award change places.
//!
//! `2003[71]` awards the gesture of its GestureParam row. A script writes the row's
//! number in the instruction, or an event takes it as an argument (common 90005570, the
//! corpses that teach one) and the number is in each start of that event. Every such
//! number is a site; the seed shuffles the numbers among the sites, each site once.
//! O Mother (row 115) stays: it opens the Shadow Keep's statue, which the area map
//! counts on from Bonny Village. Gestures that talk scripts teach (Patches' among
//! them) are not shuffled. The seed draws this apart from everything else.

use std::collections::{BTreeMap, HashMap};

use fs_bytes::{Result, format_err};
use fs_emevd::{Emevd, Event};
use gamedata::{MapId, Sources};
use items::rng::Rng;

/// Award a gesture.
const AWARD: (u32, u32) = (2003, 71);
/// Starts that pass arguments on: `start_event` and `start_common_event`, whose
/// arguments follow the slot and the event's id.
const STARTS: [(u32, u32); 2] = [(2000, 0), (2000, 6)];
const ARGS_AT: usize = 8;
/// Gestures that stay where the game gives them.
pub const KEPT: [u32; 1] = [115];
const SALT: u64 = 0x4745_5354_5552_4521;

/// Where one gesture is given: a word in an instruction of a map's script.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Site {
    pub map: MapId,
    pub event: i64,
    pub instruction: usize,
    pub byte: usize,
}

/// Each site and the gesture the seed puts there.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub sites: Vec<(Site, u32, u32)>,
}

fn word(args: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(args.get(at..at + 4)?.try_into().ok()?))
}

/// The events that award a gesture taken as an argument, by id: where in their
/// arguments the gesture's number comes from.
fn takers<'a>(events: impl Iterator<Item = &'a Event>) -> HashMap<i64, usize> {
    let mut out = HashMap::new();
    for ev in events {
        for (k, i) in ev.instructions.iter().enumerate() {
            if (i.bank, i.id) != AWARD {
                continue;
            }
            if let Some(p) = ev
                .parameters
                .iter()
                .find(|p| p.instruction == k as i64 && p.target == 0 && p.size == 4)
            {
                out.insert(ev.id, p.source as usize);
            }
        }
    }
    out
}

/// Every site of the game's scripts, with the gesture it gives.
pub fn sites(s: &Sources) -> Vec<(Site, u32)> {
    let shared = takers(s.common_func.events.iter());
    let mut out = Vec::new();
    for (m, script) in &s.scripts {
        let local = takers(script.events.iter());
        for ev in &script.events {
            for (k, i) in ev.instructions.iter().enumerate() {
                let filled = ev.parameters.iter().any(|p| p.instruction == k as i64);
                let site = |byte| Site {
                    map: *m,
                    event: ev.id,
                    instruction: k,
                    byte,
                };
                if (i.bank, i.id) == AWARD && !filled {
                    if let Some(g) = word(&i.args, 0) {
                        out.push((site(0), g));
                    }
                } else if STARTS.contains(&(i.bank, i.id)) && !filled {
                    let Some(called) = word(&i.args, 4) else {
                        continue;
                    };
                    let from = if i.id == 6 { &shared } else { &local };
                    if let Some(&at) = from.get(&i64::from(called)) {
                        let byte = ARGS_AT + at;
                        if let Some(g) = word(&i.args, byte) {
                            out.push((site(byte), g));
                        }
                    }
                }
            }
        }
    }
    out.sort();
    out
}

/// The seed's shuffle of the gestures the scripts give, those of [`KEPT`] left alone.
pub fn plan(s: &Sources, seed: u64) -> Plan {
    let found: Vec<(Site, u32)> = sites(s)
        .into_iter()
        .filter(|(_, g)| !KEPT.contains(g))
        .collect();
    let mut given: Vec<u32> = found.iter().map(|&(_, g)| g).collect();
    Rng::new(seed ^ SALT).shuffle(&mut given);
    Plan {
        sites: found
            .into_iter()
            .zip(given)
            .map(|((site, was), now)| (site, was, now))
            .collect(),
    }
}

impl Plan {
    /// The maps whose scripts the plan changes.
    pub fn maps(&self) -> BTreeMap<MapId, usize> {
        let mut out = BTreeMap::new();
        for (s, was, now) in &self.sites {
            if was != now {
                *out.entry(s.map).or_default() += 1;
            }
        }
        out
    }

    /// Write the plan's gestures into map `m`'s script, as it stands (the game's, or
    /// another part of the seed's copy, which keeps its events' instructions). Whether
    /// anything changed. Refused if a site no longer gives the gesture it did.
    pub fn edit(&self, m: MapId, script: &mut Emevd) -> Result<bool> {
        let mut changed = false;
        for (s, was, now) in self.sites.iter().filter(|(s, w, n)| s.map == m && w != n) {
            let i = script
                .events
                .iter_mut()
                .find(|e| e.id == s.event)
                .and_then(|e| e.instructions.get_mut(s.instruction))
                .ok_or_else(|| format_err(format!("gestures: {m} event {} has moved", s.event)))?;
            if word(&i.args, s.byte) != Some(*was) {
                return Err(format_err(format!(
                    "gestures: {m} event {} no longer gives gesture {was}",
                    s.event
                )));
            }
            i.args[s.byte..s.byte + 4].copy_from_slice(&now.to_le_bytes());
            changed = true;
        }
        Ok(changed)
    }

    /// For the spoiler log: each site's gesture by name.
    pub fn log(&self, s: &Sources) -> String {
        let name = |g: u32| {
            let goods = gesture_goods(s, g);
            goods
                .and_then(|id| s.text("GoodsName", id))
                .map_or(format!("gesture {g}"), str::to_string)
        };
        let mut out = String::from("\nGestures given by scripts\n");
        for (site, was, now) in &self.sites {
            out += &format!(
                "  {} event {}: {} (was {})\n",
                site.map,
                site.event,
                name(*now),
                name(*was)
            );
        }
        out
    }
}

/// A gesture's goods row, from its GestureParam row: the word at +0x04.
fn gesture_goods(s: &Sources, gesture: u32) -> Option<i32> {
    let t = s.reg.table("GestureParam").ok()?;
    let r = t.rows.iter().find(|r| r.id == gesture as i32)?;
    Some(i32::from_le_bytes(r.data.get(4..8)?.try_into().ok()?))
}
