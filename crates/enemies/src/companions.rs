//! Those who fight beside a boss go where it goes (Nova, 2026-11-02): Cemetery Shade's
//! skeletons stand round whoever brings them.
//!
//! A fight's events tell two kinds apart. **Companions** are only ever put away, by
//! `2004[4]` when the boss dies, or `2004[5]` and `2004[39]` as the map loads. They need nothing
//! of their arena, so they go. **Parts of the boss** have poses, waits and effects
//! from its arena's script, as Messmer's snakes and the Fire Giant's second body do. They cannot
//! go without that script, so the boss stays with them. Malenia alone moves all the same,
//! with her phase script (`malenia`).
//!
//! No map table grows, as what a longer one does to the game is unknown. At the new place
//! a companion takes over an ordinary part with no entity, near the boss, copied from the
//! boss's part so it stands on the same ground; whoever stood there is gone, a few bodies
//! a seed. At the old place the originals are put away as the game does once a boss is dead.
//! An event of our own in the new script puts companions away when the boss dies, and
//! as the map loads once it has.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use fs_bytes::{Result, format_err};
use fs_emevd::{Emevd, Event, Instruction, catalogue};
use fs_msb::Msb;
use fs_msb::layout::{entry_name, field};
use fs_param::Row;
use fs_param::layout::for_table;
use gamedata::{MapId, Sources};

use crate::{Class, ENEMY, Fixed, Plan, Pool, Spot, malenia, words};

const PARTS: &str = "PARTS_PARAM_ST";
/// Put away, shown or hidden: all a fight asks of a companion.
const ONLY_PUT_AWAY: [(u32, u32); 3] = [(2004, 4), (2004, 5), (2004, 39)];
/// An effect set on one body, which ties it to no one's fight.
const NOT_TIED: (u32, u32) = (2004, 8);
/// How far from the boss a companion may stand, as ground further off is unknown.
const REACH: f32 = 6.0;
const KILL: (u32, u32) = (2004, 4);

/// One who goes with a boss.
#[derive(Debug, Clone, PartialEq)]
pub struct Companion {
    pub entity: u32,
    pub npc: i32,
    pub think: i32,
    pub chara: i32,
    pub model: String,
    /// Where it stands from its boss, sideways and along the ground.
    pub offset: [f32; 2],
}

impl Eq for Companion {}

/// What each boss's fight events ask of each other enemy they name.
pub type Asked = BTreeMap<u32, BTreeMap<u32, BTreeSet<(u32, u32)>>>;

/// A helper's entity, its index in the pool, and what its fight asks of it.
type Helper<'a> = (u32, usize, &'a BTreeSet<(u32, u32)>);

/// Read the fights. Bank 2000 only starts other events, which are read on their own.
pub(crate) fn read(s: &Sources, bars: &BTreeMap<u32, (MapId, i32)>) -> Asked {
    let mut out = Asked::new();
    for (_, script) in &s.scripts {
        for run in catalogue::runs(script, Some(&s.common_func)) {
            if crate::NOT_A_FIGHT.contains(&run.event.id) {
                continue;
            }
            let acts = || run.instructions.iter().filter(|i| i.bank != 2000);
            let bosses: BTreeSet<u32> = acts()
                .flat_map(words)
                .filter(|w| bars.contains_key(w))
                .collect();
            for b in bosses {
                for i in acts() {
                    for w in words(i).filter(|w| *w != b) {
                        out.entry(b)
                            .or_default()
                            .entry(w)
                            .or_default()
                            .insert((i.bank, i.id));
                    }
                }
            }
        }
    }
    out
}

fn position(e: &[u8]) -> [f32; 3] {
    let at = |n| field(PARTS, "position").get_f32(e, n).unwrap_or(0.0);
    [at(0), at(1), at(2)]
}

/// Sort the pool by what was read: helpers leave the ordinary spots, and a boss with a part
/// of itself among them stays.
pub(crate) fn settle(s: &Sources, pool: &mut Pool, asked: &Asked) {
    let entity_at: HashMap<u32, usize> = pool
        .spots
        .iter()
        .enumerate()
        .filter(|(_, p)| p.boss.is_none())
        .filter_map(|(i, p)| p.entity.map(|e| (e, i)))
        .collect();
    let boss_at: HashMap<u32, usize> = pool
        .spots
        .iter()
        .enumerate()
        .filter(|(_, p)| p.boss.is_some())
        .filter_map(|(i, p)| p.entity.map(|e| (e, i)))
        .collect();
    let mut gone: HashSet<usize> = HashSet::new();
    for (&boss, named) in asked {
        let helpers: Vec<Helper> = named
            .iter()
            .filter(|(_, kinds)| !kinds.iter().all(|k| *k == NOT_TIED))
            .filter_map(|(e, kinds)| entity_at.get(e).map(|&i| (*e, i, kinds)))
            .collect();
        gone.extend(helpers.iter().map(|h| h.1));
        // An invisible stand-in for a fight's effects stays with the arena, and asks nothing
        // of whoever stands there.
        let helpers: Vec<_> = helpers
            .into_iter()
            .filter(|(_, i, _)| pool.spots[*i].body.model != "c0100")
            .collect();
        let Some(&b) = boss_at.get(&boss) else {
            continue;
        };
        if helpers.is_empty() || malenia::is_home(&pool.spots[b]) {
            continue;
        }
        let spot = &pool.spots[b];
        let msb = &s.maps[spot.map_index].1;
        let parts = &msb.table(PARTS).entries;
        let own = position(&parts[spot.part]);
        let goes = helpers.iter().all(|(_, i, kinds)| {
            kinds
                .iter()
                .all(|k| ONLY_PUT_AWAY.contains(k) || *k == NOT_TIED)
                && pool.spots[*i].map_index == spot.map_index
        });
        if !goes {
            pool.fixed.push(Fixed {
                name: spot.body.name.clone(),
                reason: "parts of its fight need this arena's script".to_string(),
            });
            gone.insert(b);
            continue;
        }
        let list = helpers
            .iter()
            .map(|(entity, i, _)| {
                let h = &pool.spots[*i];
                let at = position(&parts[h.part]);
                Companion {
                    entity: *entity,
                    npc: h.body.npc,
                    think: h.body.think,
                    chara: h.body.chara,
                    model: h.body.model.clone(),
                    offset: [at[0] - own[0], at[2] - own[2]],
                }
            })
            .collect();
        pool.companions.insert(boss, list);
    }
    let mut i = 0;
    pool.spots.retain(|_| {
        i += 1;
        !gone.contains(&(i - 1))
    });
}

/// Whether a spot's part can be taken over by a companion: ordinary, plain, unnamed by any
/// script, and not now holding a boss.
fn spare(pool: &Pool, from: &[usize], i: usize) -> bool {
    let s = &pool.spots[i];
    s.boss.is_none()
        && s.entity.is_none()
        && s.class == Class::Plain
        && pool.spots[from[i]].boss.is_none()
}

fn needs<'a>(pool: &'a Pool, body: &Spot) -> &'a [Companion] {
    body.entity
        .and_then(|e| pool.companions.get(&e))
        .map_or(&[], Vec::as_slice)
}

/// Send home any boss whose companions would not fit where it landed. Home always fits, as
/// its own companions' parts are there: it does not move, so none are needed. Whoever
/// stood on its home spot takes the place it leaves.
pub(crate) fn make_room(pool: &Pool, from: &mut [usize]) {
    if pool.companions.is_empty() {
        return;
    }
    // Each round sends at least one home, and a boss at home needs no room.
    for _ in 0..=pool.companions.len() {
        let mut free: HashMap<usize, usize> = HashMap::new();
        for i in 0..from.len() {
            if spare(pool, from, i) {
                *free.entry(pool.spots[i].map_index).or_default() += 1;
            }
        }
        let mut sent = false;
        for i in 0..from.len() {
            let want = needs(pool, &pool.spots[from[i]]).len();
            if want == 0 || from[i] == i {
                continue;
            }
            let left = free.entry(pool.spots[i].map_index).or_default();
            if *left >= want {
                *left -= want;
                continue;
            }
            let home = from[i];
            let displaced = from[home];
            // A boss whose next stage needs a script (`phases`) keeps to arenas: if
            // sending this one home would put it out on open ground, it takes the
            // arena of a boss that may go there, and that one goes out instead.
            if crate::phases::is_home(&pool.spots[displaced]) && pool.spots[i].boss.is_none() {
                let side = pool.spots[i].dlc;
                let other = (0..from.len()).find(|&j| {
                    j != home
                        && j != i
                        && pool.spots[j].boss.is_some()
                        && pool.spots[j].dlc == side
                        && pool.spots[from[j]].dlc == side
                        && pool.spots[displaced].dlc == side
                        && !crate::phases::is_home(&pool.spots[from[j]])
                        && needs(pool, &pool.spots[from[j]]).is_empty()
                });
                if let Some(j) = other {
                    from[home] = home;
                    from[i] = from[j];
                    from[j] = displaced;
                    sent = true;
                    continue;
                }
            }
            from.swap(i, home);
            sent = true;
        }
        if !sent {
            break;
        }
    }
}

fn ins(code: (u32, u32), w: &[u32]) -> Instruction {
    Instruction {
        bank: code.0,
        id: code.1,
        args: w.iter().flat_map(|v| v.to_le_bytes()).collect(),
        layer: None,
    }
}

/// Put these away with no fall, as the game does for a dead boss's helpers.
fn put_away(entities: &[u32]) -> Vec<Instruction> {
    entities.iter().map(|&e| ins(KILL, &[e, 0])).collect()
}

/// The event at the new place: if the boss's flag is on as the map loads, put them away
/// at once; else wait for its death. A float of 1.0 ends the wait as the game's own do.
pub fn with_boss(id: i64, boss: u32, flag: Option<u32>, entities: &[u32]) -> Event {
    const ONE: u32 = 0x3F80_0000;
    let mut instructions = Vec::new();
    if let Some(f) = flag {
        // To label 0 if the flag is off, as Cemetery Shade's event 30052810 does.
        instructions.push(ins((1003, 101), &[0, f]));
        instructions.extend(put_away(entities));
        instructions.push(ins((1000, 4), &[0]));
        instructions.push(ins((1014, 0), &[]));
    }
    instructions.push(ins((4, 0), &[0, boss, 1, ONE]));
    instructions.extend(put_away(entities));
    Event {
        id,
        rest: 0,
        instructions,
        parameters: vec![],
    }
}

/// The event at the old place: put them away as the map loads.
pub fn gone_from_home(id: i64, entities: &[u32]) -> Event {
    Event {
        id,
        rest: 0,
        instructions: put_away(entities),
        parameters: vec![],
    }
}

/// Add an event to a script under an id no event there has, and start it from event 0.
fn add_event(script: &mut Emevd, near: u32, make: impl FnOnce(i64) -> Event) -> Result<()> {
    let taken: HashSet<i64> = script.events.iter().map(|e| e.id).collect();
    let id = (0..1000)
        .map(|k| i64::from(near - near % 10_000) + 9000 + k)
        .find(|i| !taken.contains(i))
        .ok_or_else(|| format_err("no event id free in a script"))?;
    script.events.push(make(id));
    let mut args = 0i32.to_le_bytes().to_vec();
    args.extend_from_slice(&(id as u32).to_le_bytes());
    args.extend_from_slice(&0i32.to_le_bytes());
    script
        .events
        .iter_mut()
        .find(|e| e.id == 0)
        .ok_or_else(|| format_err("a script has no event 0"))?
        .instructions
        .push(Instruction {
            bank: 2000,
            id: 0,
            args,
            layer: None,
        });
    Ok(())
}

fn script_mut<'a>(
    s: &Sources,
    scripts: &'a mut Vec<(MapId, Emevd)>,
    m: MapId,
) -> Result<&'a mut Emevd> {
    if !scripts.iter().any(|(id, _)| *id == m) {
        let copy = s
            .scripts
            .iter()
            .find(|(id, _)| *id == m)
            .map(|(_, e)| e.clone())
            .ok_or_else(|| format_err(format!("no script for {m}")))?;
        scripts.push((m, copy));
    }
    Ok(&mut scripts.iter_mut().find(|(id, _)| *id == m).unwrap().1)
}

/// Keep a name's length and its last digits unique, so no two parts share one. A part's name
/// is held in UTF-16 in its entry.
fn rename(entry: &mut [u8], model: &str, serial: u32) {
    let Some(old) = entry_name(entry) else {
        return;
    };
    let new: String = old
        .char_indices()
        .map(|(i, c)| {
            if i < model.len() {
                model.as_bytes()[i] as char
            } else {
                c
            }
        })
        .collect();
    let digits = format!("{:04}", 9000 + serial % 1000);
    let keep = new.chars().count().saturating_sub(4);
    let new: String = new.chars().take(keep).chain(digits.chars()).collect();
    let (from, to): (Vec<u8>, Vec<u8>) = (
        old.encode_utf16().flat_map(u16::to_le_bytes).collect(),
        new.encode_utf16().flat_map(u16::to_le_bytes).collect(),
    );
    if from.len() != to.len() {
        return;
    }
    if let Some(at) = entry.windows(from.len()).position(|w| w == from.as_slice()) {
        entry[at..at + to.len()].copy_from_slice(&to);
    }
}

/// Write it all: new parts and rows at each new place, events at both.
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply(
    s: &Sources,
    pool: &Pool,
    plan: &Plan,
    scale: bool,
    rows: &HashMap<i32, Row>,
    new_row: &mut impl FnMut(Vec<u8>, Option<Vec<u16>>) -> Result<i32>,
    maps: &mut BTreeMap<usize, Msb>,
    scripts: &mut Vec<(MapId, Emevd)>,
) -> Result<()> {
    let npc = for_table("NpcParam").ok_or_else(|| format_err("no layout for NpcParam"))?;
    // Ids no part anywhere has.
    let mut used: HashSet<u32> = s
        .maps
        .iter()
        .flat_map(|(_, msb)| &msb.table(PARTS).entries)
        .filter_map(|e| field(PARTS, "entity_id").get(e, 0))
        .map(|v| v as u32)
        .collect();
    let mut taken: HashSet<usize> = HashSet::new();
    let mut serial = 0u32;
    for (i, spot) in pool.spots.iter().enumerate() {
        let body = &pool.spots[plan.from[i]];
        let list = needs(pool, body);
        if list.is_empty() || plan.from[i] == i {
            continue;
        }
        let home_entity = body.entity.unwrap_or(0);
        let msb_now = maps
            .entry(spot.map_index)
            .or_insert_with(|| s.maps[spot.map_index].1.clone());
        let boss_entry = msb_now.table(PARTS).entries[spot.part].clone();
        let at = position(&boss_entry);
        // The nearest spare parts in the map.
        let parts = &msb_now.table(PARTS).entries;
        let mut spares: Vec<usize> = (0..pool.spots.len())
            .filter(|&j| {
                pool.spots[j].map_index == spot.map_index
                    && !taken.contains(&j)
                    && spare(pool, &plan.from, j)
                    && field(PARTS, "type").get(&parts[pool.spots[j].part], 0) == Some(ENEMY)
            })
            .collect();
        let dist = |j: usize| {
            let p = position(&parts[pool.spots[j].part]);
            ((p[0] - at[0]).powi(2) + (p[2] - at[2]).powi(2)).to_bits()
        };
        spares.sort_by_key(|&j| (dist(j), j));
        if spares.len() < list.len() {
            return Err(format_err("a boss landed with no room for its companions"));
        }
        let mut new_entities = Vec::new();
        for (c, &j) in list.iter().zip(&spares) {
            taken.insert(j);
            let (Some(own), Some(tier)) = (rows.get(&c.npc), rows.get(&spot.body.npc)) else {
                return Err(format_err("a companion's row has gone missing"));
            };
            let mut data = own.data.clone();
            if scale {
                for name in ["scale_a", "scale_b"] {
                    let f = npc.field(name);
                    f.set(&mut data, 0, f.get(&tier.data, 0))?;
                }
            }
            let row = new_row(data, own.name.clone())?;
            let entity = spot.entity.map(|e| {
                let mut n = e - e % 1000 + 900;
                while !used.insert(n) {
                    n += 1;
                }
                n
            });
            new_entities.extend(entity);
            let model = crate::model_index(msb_now, &c.model)?;
            let table = msb_now
                .tables
                .iter_mut()
                .find(|t| t.name == PARTS)
                .ok_or_else(|| format_err("the map has no parts"))?;
            let mut e = boss_entry.clone();
            let far = (c.offset[0].powi(2) + c.offset[1].powi(2))
                .sqrt()
                .max(REACH);
            let k = REACH / far;
            let f = |name| field(PARTS, name);
            f("position").set(&mut e, 0, i64::from((at[0] + c.offset[0] * k).to_bits()))?;
            f("position").set(&mut e, 2, i64::from((at[2] + c.offset[1] * k).to_bits()))?;
            f("entity_id").set(&mut e, 0, i64::from(entity.unwrap_or(0)))?;
            f("npc_param").set(&mut e, 0, i64::from(row))?;
            f("think").set(&mut e, 0, i64::from(c.think))?;
            f("chara_init").set(&mut e, 0, i64::from(c.chara))?;
            f("talk_id").set(&mut e, 0, 0)?;
            f("model").set(&mut e, 0, model as i64)?;
            rename(&mut e, &c.model, serial);
            serial += 1;
            table.entries[pool.spots[j].part] = e;
        }
        if let (Some(boss), Some(en)) = (&spot.boss, spot.entity) {
            let flag = boss.defeat_flag;
            let script = script_mut(s, scripts, boss.script)?;
            add_event(script, en, |id| with_boss(id, en, flag, &new_entities))?;
        }
        if let Some(home) = &body.boss {
            let originals: Vec<u32> = list.iter().map(|c| c.entity).collect();
            let script = script_mut(s, scripts, home.script)?;
            add_event(script, home_entity, |id| gone_from_home(id, &originals))?;
        }
    }
    scripts.sort_by_key(|(m, _)| *m);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Body, BossSpot, Options, Size, shuffle};

    fn spot(i: usize, map: usize, boss: bool, entity: Option<u32>) -> Spot {
        Spot {
            map_index: map,
            part: i,
            map: MapId([60, 40, map as u8, 0]),
            entity,
            body: Body {
                npc: i as i32,
                think: 2,
                chara: -1,
                model: format!("c{i}"),
                name: format!("n{i}"),
                name_text: 0,
            },
            size: Size::Small,
            class: Class::Plain,
            dlc: false,
            boss: boss.then_some(BossSpot {
                bar_text: i as i32,
                script: MapId([60, 40, map as u8, 0]),
                defeat_flag: None,
            }),
        }
    }

    fn companion() -> Companion {
        Companion {
            entity: 777,
            npc: 1,
            think: 2,
            chara: -1,
            model: "c3510".into(),
            offset: [1.0, 1.0],
        }
    }

    /// Boss spots in maps 0 to 3; only map 0 has room for three.
    fn pool() -> Pool {
        let mut spots: Vec<Spot> = (0..4)
            .map(|m| spot(m, m, true, Some(100 + m as u32)))
            .collect();
        for k in 0..3 {
            spots.push(spot(4 + k, 0, false, None));
        }
        Pool {
            spots,
            fixed: vec![],
            listed: vec![BTreeSet::new(); 4],
            companions: BTreeMap::from([(103, vec![companion(), companion(), companion()])]),
        }
    }

    #[test]
    fn a_boss_lands_only_where_its_companions_fit() {
        let p = pool();
        let options = Options {
            bosses: 100,
            ..Default::default()
        };
        let mut moved = false;
        for seed in 0..200 {
            let plan = shuffle(&p, seed, &options);
            let mut seen = plan.from.clone();
            seen.sort_unstable();
            assert_eq!(seen, (0..p.spots.len()).collect::<Vec<_>>());
            let at = plan.from.iter().position(|&f| f == 3).unwrap();
            assert!(
                at == 3 || p.spots[at].map_index == 0,
                "seed {seed}: at {at}"
            );
            moved |= at != 3;
        }
        assert!(moved);
    }

    #[test]
    fn companions_are_put_away_on_load_if_the_boss_is_dead_else_at_its_death() {
        let e = with_boss(9, 100, Some(555), &[1, 2]);
        let codes: Vec<(u32, u32)> = e.instructions.iter().map(|i| (i.bank, i.id)).collect();
        assert_eq!(
            codes,
            [
                (1003, 101),
                KILL,
                KILL,
                (1000, 4),
                (1014, 0),
                (4, 0),
                KILL,
                KILL
            ]
        );
        assert!(with_boss(9, 100, None, &[1]).instructions[0].bank == 4);
        assert_eq!(gone_from_home(9, &[1, 2]).instructions.len(), 2);
    }
}
