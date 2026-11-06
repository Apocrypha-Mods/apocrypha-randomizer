//! Writing a seed's doors into the game's files (`docs/fog-gates.md`). None of it is
//! seen in the game yet; each piece is one of the lab's gates.
//!
//! **Maps.** For each door, in the map its wall stands in:
//! - a region on each side, where the player stands to pass ([`Ids::outside`],
//!   [`Ids::inside`]): copies of a box region of the map, moved. A boss fog's inside is
//!   its arena region, already there;
//! - for a dungeon mouth, the wall itself ([`Ids::wall`]): a copy of the map's own boss
//!   fog asset, moved to the mouth.
//!
//! **Scripts.** For each door, in its map's script, started from event 0:
//! - from outside: the prompt at the wall with the player in the outside region warps
//!   them to the inside region of the door the seed sends it to, turning on that door's
//!   arrival flag first if it is a boss fog (off again five seconds later);
//! - from inside: the prompt with the player in the inside region warps them to the
//!   outside region of the door whose outside leads here.
//!
//! **The fight.** A shuffled boss fog's start of 9005800 becomes a start of
//! [`FIGHT_EVENT`], a copy of 9005800 in `common_func` that waits on the arrival flag
//! (its last argument) where 9005800 waits on the fog's prompt, so a player warped into
//! the arena starts the fight as one who walked through the fog would.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use fs_bytes::{Result, format_err};
use fs_emevd::{Emevd, Event, Instruction, Parameter};
use fs_msb::Msb;
use fs_msb::entry::Entry;
use fs_msb::layout::{entry_name, field};
use gamedata::MapId;

use crate::doors::{BOSS_FOG_EVENT, Door, Kind, map_base};
use crate::rooms::Plan;

/// Our copy of 9005800, in `common_func`.
pub const FIGHT_EVENT: i64 = 9_005_890;
/// The player.
const PLAYER: u32 = 10_000;
/// "Traverse the mist".
pub const PROMPT: u32 = 10_000;
/// Our entities, events and flags in a map: its own number plus these.
pub const ENTITY_FIRST: u32 = 7_900;
pub const FLAG_FIRST: u32 = 7_950;
const PARTS: &str = "PARTS_PARAM_ST";
const POINTS: &str = "POINT_PARAM_ST";
/// A part's number among those of its model.
const NUMBER_AT: usize = 0x08;
/// A generic region, and a box.
const REGION: i64 = -1;
const BOX: i64 = 5;

/// What one door uses: its regions, its wall, its arrival flag and its two events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ids {
    pub outside: u32,
    pub inside: u32,
    pub wall: u32,
    /// For a boss fog: on while a player is warped into the arena.
    pub arrival: Option<u32>,
    pub events: [u32; 2],
}

/// Each door's ids: its map's own number plus a run of ours, by the door's place among
/// its map's doors.
pub fn ids(doors: &[Door]) -> Vec<Ids> {
    let mut count: BTreeMap<MapId, u32> = BTreeMap::new();
    doors
        .iter()
        .map(|d| {
            let k = count.entry(d.map).or_default();
            let base = map_base(d.map);
            let n = *k;
            *k += 1;
            let e = base + ENTITY_FIRST + n * 4;
            Ids {
                outside: e,
                inside: d.inside.entity.unwrap_or(e + 1),
                wall: d.fog.unwrap_or(e + 2),
                arrival: matches!(d.kind, Kind::Boss { .. }).then_some(base + FLAG_FIRST + n),
                events: [e, e + 1],
            }
        })
        .collect()
}

fn ins(bank: u32, id: u32, words: &[u32]) -> Instruction {
    Instruction {
        bank,
        id,
        args: words.iter().flat_map(|w| w.to_le_bytes()).collect(),
        layer: None,
    }
}

fn packed(b: [u8; 4]) -> u32 {
    u32::from_le_bytes(b)
}

const GROUP: u8 = 1;

/// Wait for the prompt at `wall` with the player in `region`, then do `then` and start
/// over.
fn door_event(id: u32, wall: u32, region: u32, then: Vec<Instruction>) -> Event {
    let mut instructions = vec![
        ins(3, 24, &[packed([GROUP, 0, 0, 0]), PROMPT, wall]),
        ins(3, 2, &[packed([GROUP, 1, 0, 0]), PLAYER, region, 1]),
        ins(0, 0, &[packed([0, 1, GROUP, 0])]),
    ];
    instructions.extend(then);
    instructions.push(ins(1000, 4, &[1]));
    Event {
        id: i64::from(id),
        rest: 0,
        instructions,
        parameters: vec![],
    }
}

fn warp(to: MapId, region: u32) -> Instruction {
    ins(2003, 14, &[u32::from_le_bytes(to.0), region, 0])
}

fn set_flag(flag: u32, on: bool) -> Instruction {
    ins(2003, 66, &[0, flag, u32::from(on)])
}

/// The two events of door `d`.
pub fn events(plan: &Plan, ids: &[Ids], d: usize) -> [Event; 2] {
    let me = &ids[d];
    let t = plan.to[d];
    let target = &plan.doors[t];
    let mut out = Vec::new();
    if let Some(flag) = ids[t].arrival {
        out.push(set_flag(flag, true));
    }
    out.push(warp(target.inside.map, ids[t].inside));
    if let Some(flag) = ids[t].arrival {
        out.push(ins(1001, 0, &[5.0f32.to_bits()]));
        out.push(set_flag(flag, false));
    }
    // The door whose outside leads here.
    let from = plan.to.iter().position(|&x| x == d).unwrap_or(d);
    let back = vec![warp(plan.doors[from].outside.map, ids[from].outside)];
    [
        door_event(me.events[0], me.wall, me.outside, out),
        door_event(me.events[1], me.wall, me.inside, back),
    ]
}

/// `common_func` with [`FIGHT_EVENT`]: 9005800 waiting on its last argument, a flag,
/// where it waits on the fog's prompt.
pub fn common_func(common_func: &Emevd) -> Result<Emevd> {
    let mut out = common_func.clone();
    if out.events.iter().any(|e| e.id == FIGHT_EVENT) {
        return Err(format_err(format!(
            "common_func already has an event {FIGHT_EVENT}"
        )));
    }
    let source = out
        .events
        .iter()
        .find(|e| e.id == i64::from(BOSS_FOG_EVENT))
        .ok_or_else(|| format_err("common_func has no 9005800"))?;
    let mut e = source.clone();
    e.id = FIGHT_EVENT;
    // The arrival flag comes after the arguments 9005800 reads.
    let args_end = e
        .parameters
        .iter()
        .map(|p| p.source + i64::from(p.size))
        .max()
        .unwrap_or(0);
    let mut changed = 0;
    for (k, i) in e.instructions.iter_mut().enumerate() {
        if (i.bank, i.id) != (3, 24) {
            continue;
        }
        let group = i.args.first().copied().unwrap_or(0);
        *i = ins(3, 0, &[packed([group, 1, 0, 0]), 0]);
        e.parameters.retain(|p| p.instruction != k as i64);
        e.parameters.push(Parameter {
            instruction: k as i64,
            target: 4,
            source: args_end,
            size: 4,
            unk: 0,
        });
        changed += 1;
    }
    if changed == 0 {
        return Err(format_err("9005800 waits on no prompt"));
    }
    e.parameters.sort_by_key(|p| (p.instruction, p.target));
    out.events.push(e);
    Ok(out)
}

/// The doors of map `m`, by index into the plan.
fn doors_in(plan: &Plan, m: MapId) -> Vec<usize> {
    (0..plan.doors.len())
        .filter(|&d| plan.doors[d].map == m)
        .collect()
}

/// Every map the seed's doors change.
pub fn maps(plan: &Plan) -> BTreeSet<MapId> {
    plan.doors.iter().map(|d| d.map).collect()
}

fn f32_set(f: &fs_msb::layout::Field, e: &mut [u8], k: usize, v: f32) -> Result<()> {
    f.set(e, k, i64::from(v.to_bits()))
}

/// A box region of `msb` to copy, the arena region's if the door has one.
fn region_template(msb: &Msb, prefer: Option<u32>) -> Option<Vec<u8>> {
    let (ty, shape, ent) = (
        field(POINTS, "type"),
        field(POINTS, "shape"),
        field(POINTS, "entity_id"),
    );
    let points = &msb.table(POINTS).entries;
    prefer
        .and_then(|id| {
            points
                .iter()
                .find(|e| ent.applies(e) && ent.get(e, 0) == Some(i64::from(id)))
        })
        .or_else(|| {
            points
                .iter()
                .find(|e| ty.get(e, 0) == Some(REGION) && shape.get(e, 0) == Some(BOX))
        })
        .cloned()
}

/// Add a region named `name` at `at`, facing `facing`, with entity `entity`.
fn add_region(
    msb: &mut Msb,
    template: &[u8],
    name: &str,
    at: [f32; 3],
    facing: f32,
    entity: u32,
) -> Result<()> {
    let mut e = Entry::read(POINTS, template)?;
    e.set_name(name)?;
    let mut bytes = e.write();
    let (pos, rot, id, ent) = (
        field(POINTS, "position"),
        field(POINTS, "rotation"),
        field(POINTS, "id"),
        field(POINTS, "entity_id"),
    );
    for (k, v) in at.iter().enumerate() {
        f32_set(pos, &mut bytes, k, *v)?;
    }
    f32_set(rot, &mut bytes, 0, 0.0)?;
    f32_set(rot, &mut bytes, 1, facing)?;
    f32_set(rot, &mut bytes, 2, 0.0)?;
    let next = msb
        .table(POINTS)
        .entries
        .iter()
        .filter_map(|e| id.get(e, 0))
        .max()
        .unwrap_or(0)
        + 1;
    id.set(&mut bytes, 0, next)?;
    ent.set(&mut bytes, 0, i64::from(entity))?;
    let place = msb.place_for(POINTS, REGION as i32);
    msb.insert(POINTS, place, bytes)
}

/// Add a copy of the map's fog asset `fog`, at `at`, facing `facing`, as `entity`.
fn add_wall(msb: &mut Msb, fog: u32, at: [f32; 3], facing: f32, entity: u32) -> Result<()> {
    let ent = field(PARTS, "entity_id");
    let source = msb
        .table(PARTS)
        .entries
        .iter()
        .find(|e| ent.get(e, 0) == Some(i64::from(fog)))
        .cloned()
        .ok_or_else(|| format_err(format!("no fog asset {fog} to copy")))?;
    let name = entry_name(&source).ok_or_else(|| format_err("a fog asset has no name"))?;
    let stem = name
        .rsplit_once('_')
        .map_or(name.as_str(), |(s, _)| s)
        .to_string();
    let taken: HashSet<u32> = msb
        .table(PARTS)
        .entries
        .iter()
        .filter_map(|e| entry_name(e))
        .filter_map(|n| {
            let (s, k) = n.rsplit_once('_')?;
            (s == stem).then(|| k.parse().ok()).flatten()
        })
        .collect();
    let next = (9000..)
        .find(|v| !taken.contains(v))
        .ok_or_else(|| format_err("no number left for a fog"))?;
    let mut e = Entry::read(PARTS, &source)?;
    e.set_name(&format!("{stem}_{next:04}"))?;
    let mut bytes = e.write();
    bytes[NUMBER_AT..NUMBER_AT + 4].copy_from_slice(&next.to_le_bytes());
    let (pos, rot) = (field(PARTS, "position"), field(PARTS, "rotation"));
    for (k, v) in at.iter().enumerate() {
        f32_set(pos, &mut bytes, k, *v)?;
    }
    f32_set(rot, &mut bytes, 1, facing)?;
    ent.set(&mut bytes, 0, i64::from(entity))?;
    let ty = field(PARTS, "type").get(&bytes, 0).unwrap_or(0);
    let place = msb.place_for(PARTS, ty as i32);
    msb.insert(PARTS, place, bytes)
}

/// The entities a map already uses: its parts', points' and events'.
fn entities(msb: &Msb) -> HashSet<u32> {
    let mut out = HashSet::new();
    for table in [PARTS, POINTS, "EVENT_PARAM_ST"] {
        let f = field(table, "entity_id");
        for e in &msb.table(table).entries {
            if f.applies(e)
                && let Some(v) = f.get(e, 0)
            {
                out.insert(v as u32);
            }
        }
    }
    out
}

/// Edit map `m` for the doors whose walls stand there. Whether anything changed.
pub fn edit_map(plan: &Plan, ids: &[Ids], m: MapId, msb: &mut Msb) -> Result<bool> {
    let here = doors_in(plan, m);
    if here.is_empty() {
        return Ok(false);
    }
    let used = entities(msb);
    for &d in &here {
        let (door, id) = (&plan.doors[d], &ids[d]);
        let ours = [
            Some(id.outside),
            (door.inside.entity.is_none()).then_some(id.inside),
        ];
        for e in ours.into_iter().flatten() {
            if used.contains(&e) {
                return Err(format_err(format!("{m}: entity {e} is the game's")));
            }
        }
        let template = region_template(msb, door.inside.entity)
            .ok_or_else(|| format_err(format!("{m}: no box region to copy")))?;
        add_region(
            msb,
            &template,
            &format!("apoc_fog_{d}_outside"),
            door.outside.position,
            door.outside.facing,
            id.outside,
        )?;
        if door.kind == Kind::Mouth {
            add_region(
                msb,
                &template,
                &format!("apoc_fog_{d}_inside"),
                door.inside.position,
                door.inside.facing + 180.0,
                id.inside,
            )?;
            let fog = crate::doors::any_fog(msb)
                .ok_or_else(|| format_err(format!("{m}: no fog asset to copy")))?;
            if used.contains(&id.wall) {
                return Err(format_err(format!("{m}: entity {} is the game's", id.wall)));
            }
            add_wall(msb, fog, door.wall.position, door.wall.facing, id.wall)?;
        }
    }
    Ok(true)
}

/// Edit map `m`'s script for the doors whose walls stand there. Whether anything changed.
pub fn edit_script(plan: &Plan, ids: &[Ids], m: MapId, script: &mut Emevd) -> Result<bool> {
    let here = doors_in(plan, m);
    if here.is_empty() {
        return Ok(false);
    }
    let taken: HashSet<i64> = script.events.iter().map(|e| e.id).collect();
    let mut starts = Vec::new();
    for &d in &here {
        let door = &plan.doors[d];
        for ev in events(plan, ids, d) {
            if taken.contains(&ev.id) {
                return Err(format_err(format!("{m}: event {} is the game's", ev.id)));
            }
            starts.push(ins(2000, 0, &[0, ev.id as u32, 0]));
            script.events.push(ev);
        }
        if let Kind::Boss {
            args, starter, at, ..
        } = &door.kind
        {
            let flag = ids[d]
                .arrival
                .ok_or_else(|| format_err("a boss fog with no flag"))?;
            let e = script
                .events
                .iter_mut()
                .find(|e| e.id == *starter)
                .ok_or_else(|| format_err(format!("{m}: no event {starter}")))?;
            let i = e
                .instructions
                .get_mut(*at)
                .ok_or_else(|| format_err(format!("{m}: event {starter} has moved")))?;
            if i.args.get(4..8) != Some(&BOSS_FOG_EVENT.to_le_bytes()[..])
                || i.args.get(8..) != Some(&args[..])
            {
                return Err(format_err(format!("{m}: event {starter} is not as read")));
            }
            i.args[4..8].copy_from_slice(&(FIGHT_EVENT as u32).to_le_bytes());
            i.args.extend_from_slice(&flag.to_le_bytes());
        }
    }
    let zero = script
        .events
        .iter_mut()
        .find(|e| e.id == 0)
        .ok_or_else(|| format_err(format!("{m}: no event 0")))?;
    for (k, s) in starts.into_iter().enumerate() {
        zero.instructions.insert(k, s);
    }
    Ok(true)
}

/// For the spoiler log: each door and where it now leads.
pub fn log(plan: &Plan) -> Vec<String> {
    (0..plan.doors.len())
        .map(|d| {
            let t = plan.to[d];
            format!(
                "{} ({}) leads into {} ({})",
                plan.doors[d].name,
                plan.outside[d],
                plan.doors[t].name,
                crate::rooms::room_id(&plan.doors[t])
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_door_event_waits_on_the_prompt_on_its_side_then_warps() {
        let e = door_event(1, 2, 3, vec![warp(MapId([30, 2, 0, 0]), 30_027_901)]);
        let shape: Vec<(u32, u32)> = e.instructions.iter().map(|i| (i.bank, i.id)).collect();
        assert_eq!(shape, [(3, 24), (3, 2), (0, 0), (2003, 14), (1000, 4)]);
        assert_eq!(e.instructions[3].args[..4], [30, 2, 0, 0]);
    }
}
