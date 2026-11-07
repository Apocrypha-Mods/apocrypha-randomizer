//! The doors the fog gate randomizer shuffles, read from the game's files
//! (`docs/fog-gates.md`).
//!
//! - **Boss fogs**: each start of common event 9005800 in a map's script. Its arguments
//!   are the boss's defeat flag, the fog asset, the arena region (1 to 3 m past the fog),
//!   the flag the arena turns on once entered, the boss, the prompt (ActionButtonParam),
//!   and an alternative flag and region. The inside side is the arena region; the
//!   outside side is the fog mirrored away from it.
//! - **Dungeon mouths**: each dungeon map (caves, catacombs, tunnels, graves, gaols) with a
//!   "return to entrance" point (`AABB2840`, point type 8), where the game puts the player
//!   it warps back to the entrance, and a grace (`AABB1950`). The outside side is that
//!   point; the inside side is the grace, stepped towards the entrance. The fog wall that
//!   the player passes is ours, a quarter of the way from the entrance to the grace.

use std::collections::{BTreeMap, HashMap};

use fs_msb::Msb;
use fs_msb::layout::field;
use gamedata::{MapId, Sources};

/// The common event that runs a boss's entrance fog.
pub const BOSS_FOG_EVENT: u32 = 9_005_800;
/// Starting an event: `2000[0]` and `2000[6]`, the event's arguments from word 2.
const STARTS: [(u32, u32); 2] = [(2000, 0), (2000, 6)];
/// The maps of mini-dungeons: catacombs, caves, tunnels, graves (30 to 32, 39), and the
/// DLC's (40 to 43).
pub const DUNGEON_AREAS: [u8; 8] = [30, 31, 32, 39, 40, 41, 42, 43];
/// A dungeon's "return to entrance" point, and its grace, by the map's number.
pub const RETURN_POINT: u32 = 2840;
pub const GRACE: u32 = 1950;
/// Point type of the return point.
const RETURN_TYPE: i64 = 8;
/// How far past a wall the far side is, and how far from it the player stands to pass.
pub const STEP: f32 = 2.0;

/// A place a door's side lands the player, in a map.
#[derive(Debug, Clone, PartialEq)]
pub struct Spot {
    pub map: MapId,
    pub position: [f32; 3],
    /// Turn about the vertical, in degrees, facing on through the door.
    pub facing: f32,
    /// The game's own point there, when it has one.
    pub entity: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// A boss's entrance fog: its start's arguments, as the game passes them.
    Boss {
        defeat_flag: u32,
        args: Vec<u8>,
        /// The script event that starts 9005800, and the start's place in it.
        starter: i64,
        at: usize,
    },
    /// A dungeon's mouth.
    Mouth,
}

/// A door: a wall the player passes, and the two sides of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Door {
    /// A name for the spoiler log and the room map: `boss:10001850` or `mouth:m30_02`.
    pub name: String,
    pub kind: Kind,
    /// The map the wall stands in.
    pub map: MapId,
    /// The fog asset, for a boss fog; a mouth's wall is ours.
    pub fog: Option<u32>,
    /// Where the wall stands, and which way it faces (from outside in).
    pub wall: Spot,
    pub outside: Spot,
    pub inside: Spot,
}

fn words(b: &[u8]) -> Vec<u32> {
    b.chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

/// The position and facing of the map's part or point whose entity id is `entity`.
pub fn locate(msb: &Msb, entity: u32) -> Option<([f32; 3], f32)> {
    if entity == 0 {
        return None;
    }
    for (table, pos, rot) in [
        ("PARTS_PARAM_ST", "position", "rotation"),
        ("POINT_PARAM_ST", "position", "rotation"),
    ] {
        let id = field(table, "entity_id");
        let (p, r) = (field(table, pos), field(table, rot));
        for e in &msb.table(table).entries {
            if id.applies(e) && id.get(e, 0) == Some(i64::from(entity)) {
                let at = std::array::from_fn(|i| p.get_f32(e, i).unwrap_or(0.0));
                return Some((at, r.get_f32(e, 1).unwrap_or(0.0)));
            }
        }
    }
    None
}

/// The facing, in degrees about the vertical, of a step from `a` to `b`.
pub fn facing(a: [f32; 3], b: [f32; 3]) -> f32 {
    libm::atan2f(b[0] - a[0], b[2] - a[2]).to_degrees()
}

/// `a` moved `d` metres along the level direction from `from` to `to`.
pub fn step(a: [f32; 3], from: [f32; 3], to: [f32; 3], d: f32) -> [f32; 3] {
    let (dx, dz) = (to[0] - from[0], to[2] - from[2]);
    let len = (dx * dx + dz * dz).sqrt().max(1e-3);
    [a[0] + dx / len * d, a[1], a[2] + dz / len * d]
}

/// A map's own number for its entities: `AABB0000` for `mAA_BB_…`; on an open-world
/// tile `mAA_XX_YY_…`, `1XXYY0000` in the base game (m60) and `2XXYY0000` in the DLC's
/// (m61), as its fogs are numbered (Margit's 10001850, Mad Pumpkin Head's 1044361800
/// in m60_44_36, the Red Bear's 2046451800 in m61_46_45).
pub fn map_base(m: MapId) -> u32 {
    let [a, b, c, _] = m.0;
    if m.is_open_world() {
        let world = if a == 61 { 2 } else { 1 };
        world * 1_000_000_000 + u32::from(b) * 1_000_000 + u32::from(c) * 10_000
    } else {
        u32::from(a) * 1_000_000 + u32::from(b) * 10_000
    }
}

/// Every boss entrance fog the maps' scripts start 9005800 for, whose fog and arena
/// region stand in that map.
pub fn boss_fogs(sources: &Sources) -> Vec<Door> {
    let maps: HashMap<MapId, &Msb> = sources.maps.iter().map(|(m, msb)| (*m, msb)).collect();
    let mut out = Vec::new();
    for (m, script) in &sources.scripts {
        let Some(msb) = maps.get(m) else { continue };
        for e in &script.events {
            for (at, i) in e.instructions.iter().enumerate() {
                if !STARTS.contains(&(i.bank, i.id)) {
                    continue;
                }
                let w = words(&i.args);
                if w.get(1) != Some(&BOSS_FOG_EVENT) || w.len() < 5 {
                    continue;
                }
                let (defeat_flag, fog, region) = (w[2], w[3], w[4]);
                if fog == 0 || region == 0 {
                    continue;
                }
                let (Some((wall, _)), Some((arena, _))) = (locate(msb, fog), locate(msb, region))
                else {
                    continue;
                };
                let inward = facing(wall, arena);
                out.push(Door {
                    name: format!("boss:{fog}"),
                    kind: Kind::Boss {
                        defeat_flag,
                        args: i.args[8..].to_vec(),
                        starter: e.id,
                        at,
                    },
                    map: *m,
                    fog: Some(fog),
                    wall: Spot {
                        map: *m,
                        position: wall,
                        facing: inward,
                        entity: Some(fog),
                    },
                    outside: Spot {
                        map: *m,
                        position: step(wall, arena, wall, STEP),
                        facing: inward,
                        entity: None,
                    },
                    inside: Spot {
                        map: *m,
                        position: arena,
                        facing: inward,
                        entity: Some(region),
                    },
                });
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

/// The map's own boss fog asset (AEG099_001 to _003), to copy for a mouth's wall.
pub fn any_fog(msb: &Msb) -> Option<u32> {
    let ent = field("PARTS_PARAM_ST", "entity_id");
    msb.table("PARTS_PARAM_ST").entries.iter().find_map(|e| {
        let name = fs_msb::layout::entry_name(e)?;
        ["AEG099_001", "AEG099_002", "AEG099_003"]
            .iter()
            .any(|m| name.starts_with(m))
            .then(|| ent.get(e, 0))
            .flatten()
            .filter(|&v| v > 0)
            .map(|v| v as u32)
    })
}

/// Every dungeon's mouth: its return point outside, its grace inside, and a fog asset
/// of the map's own to copy for its wall.
pub fn mouths(sources: &Sources) -> Vec<Door> {
    let ty = field("POINT_PARAM_ST", "type");
    let mut out = BTreeMap::new();
    for (m, msb) in &sources.maps {
        if !DUNGEON_AREAS.contains(&m.area()) || m.0[2] != 0 || m.0[3] != 0 {
            continue;
        }
        let base = map_base(*m);
        let Some((entrance, _)) = locate(msb, base + RETURN_POINT) else {
            continue;
        };
        // The return point must be one, not some other entity of the number.
        let is_return = msb.table("POINT_PARAM_ST").entries.iter().any(|e| {
            field("POINT_PARAM_ST", "entity_id").get(e, 0) == Some(i64::from(base + RETURN_POINT))
                && ty.get(e, 0) == Some(RETURN_TYPE)
        });
        let Some((grace, _)) = locate(msb, base + GRACE).filter(|_| is_return) else {
            continue;
        };
        if any_fog(msb).is_none() {
            continue;
        }
        let inward = facing(entrance, grace);
        let wall = [
            entrance[0] + (grace[0] - entrance[0]) * 0.25,
            entrance[1] + (grace[1] - entrance[1]) * 0.25,
            entrance[2] + (grace[2] - entrance[2]) * 0.25,
        ];
        out.insert(
            *m,
            Door {
                name: format!("mouth:{m}"),
                kind: Kind::Mouth,
                map: *m,
                fog: None,
                wall: Spot {
                    map: *m,
                    position: wall,
                    facing: inward,
                    entity: None,
                },
                outside: Spot {
                    map: *m,
                    position: entrance,
                    facing: inward,
                    entity: Some(base + RETURN_POINT),
                },
                inside: Spot {
                    map: *m,
                    position: step(grace, grace, entrance, STEP),
                    facing: inward,
                    entity: None,
                },
            },
        );
    }
    out.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fnv(bits: impl IntoIterator<Item = u64>) -> u64 {
        bits.into_iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
            (h ^ b).wrapping_mul(0x100_0000_01b3)
        })
    }

    /// Written from Linux and held on every system CI runs: a facing computed
    /// differently anywhere would turn every fog seed's walls and regions there.
    #[test]
    fn facings_are_the_same_on_every_system() {
        let mut bits = Vec::new();
        for i in -20..=20 {
            for j in -20..=20 {
                let b = [i as f32 * 0.37, 1.0, j as f32 * 1.13 + 0.01];
                bits.push(u64::from(facing([0.5, 0.0, -0.25], b).to_bits()));
            }
        }
        assert_eq!(fnv(bits), 12887414974042521745);
    }

    #[test]
    fn steps_and_facings() {
        let p = step([0.0, 5.0, 0.0], [0.0, 0.0, 0.0], [3.0, 9.0, 4.0], 10.0);
        assert!((p[0] - 6.0).abs() < 1e-4 && (p[2] - 8.0).abs() < 1e-4 && p[1] == 5.0);
        assert!((facing([0.0; 3], [0.0, 0.0, 1.0])).abs() < 1e-4);
        assert!((facing([0.0; 3], [1.0, 0.0, 0.0]) - 90.0).abs() < 1e-4);
        assert_eq!(map_base(MapId([30, 2, 0, 0])), 30_020_000);
        assert_eq!(map_base(MapId([60, 44, 36, 0])), 1_044_360_000);
        assert_eq!(map_base(MapId([61, 46, 45, 0])), 2_046_450_000);
    }
}
