//! Ground that harms a body standing on it: poison and scarlet-rot swamps, and
//! lava (docs/spec/enemies.md §4).
//!
//! The maps do not say where a swamp is in any field we read: the ground's material
//! is in the collision files, which we do not parse. What they do have is regions
//! named by the game's makers. Ambient sounds are boxes laid over a swamp or a lava
//! flow (`毒沼(赤)_05`, poison swamp, red; `溶岩_中_04`, lava, middle), and some
//! other regions are named for the swamp they cover (`バディ起動可能領域 大毒沼`,
//! where spirits may be summoned: the great poison swamp). A spot inside one is
//! taken to stand on that ground. This is a probable reading: a box can reach past a
//! shore, and ground with no such region is missed.

use std::collections::BTreeMap;

use fs_msb::Msb;
use fs_msb::layout::{entry_name, field};
use gamedata::MapId;

const POINTS: &str = "POINT_PARAM_ST";
const PARTS: &str = "PARTS_PARAM_ST";
/// Region type of a placed sound.
const SOUND: i64 = 4;
/// Region shapes, by their data: a sphere's radius, a cylinder's radius and height,
/// a box's width, depth and height.
const SPHERE: i64 = 2;
const CYLINDER: i64 = 3;
const BOX: i64 = 5;
/// How far past a region's edge still counts, in metres.
const MARGIN: f32 = 1.0;

/// What the ground does to a body that stands on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Hazard {
    /// A swamp of poison or of scarlet rot. The names do not always tell them
    /// apart (the Lake of Rot's sounds say only 毒, poison), so they are one.
    Swamp,
    Lava,
}

impl Hazard {
    /// What a region's type and name say its ground is, if harmful. A sound names
    /// its ground; any other region counts only if it names a poison swamp (毒沼)
    /// and is not the swamp's one dry place (安置). `溶岩` in other regions is
    /// mostly the Lava Slimes' name, not ground, so only sounds mark lava.
    pub fn of_region(kind: i64, name: &str) -> Option<Hazard> {
        if kind == SOUND {
            if name.contains("溶岩") || name.contains("マグマ") {
                return Some(Hazard::Lava);
            }
            if name.contains('毒') {
                return Some(Hazard::Swamp);
            }
            return None;
        }
        (name.contains("毒沼") && !name.contains("安置")).then_some(Hazard::Swamp)
    }
}

/// A region's volume.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Volume {
    pub at: [f32; 3],
    /// Turn about the vertical, in degrees.
    pub turn: f32,
    pub shape: Shape,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Sphere { r: f32 },
    Cylinder { r: f32, h: f32 },
    Box { w: f32, d: f32, h: f32 },
}

impl Volume {
    /// Whether `p` is inside, or within [`MARGIN`] of it. Height is measured either
    /// way from the region's position, as whether that is its base or its middle
    /// was not settled.
    pub fn holds(&self, p: [f32; 3]) -> bool {
        let (dx, dy, dz) = (p[0] - self.at[0], p[1] - self.at[1], p[2] - self.at[2]);
        match self.shape {
            Shape::Sphere { r } => (dx * dx + dy * dy + dz * dz).sqrt() <= r + MARGIN,
            Shape::Cylinder { r, h } => {
                (dx * dx + dz * dz).sqrt() <= r + MARGIN && dy.abs() <= h + MARGIN
            }
            Shape::Box { w, d, h } => {
                let (s, c) = self.turn.to_radians().sin_cos();
                let (x, z) = (c * dx - s * dz, s * dx + c * dz);
                x.abs() <= w / 2.0 + MARGIN && z.abs() <= d / 2.0 + MARGIN && dy.abs() <= h + MARGIN
            }
        }
    }
}

/// A region's volume, if it has one.
pub fn volume(e: &[u8]) -> Option<Volume> {
    let f = |name: &str, i| field(POINTS, name).get_f32(e, i);
    let at = [f("position", 0)?, f("position", 1)?, f("position", 2)?];
    let turn = f("rotation", 1)?;
    let data = |i| f("shape_data", i);
    let shape = match field(POINTS, "shape").get(e, 0)? {
        SPHERE => Shape::Sphere { r: data(0)? },
        CYLINDER => Shape::Cylinder {
            r: data(0)?,
            h: data(1)?,
        },
        BOX => Shape::Box {
            w: data(0)?,
            d: data(1)?,
            h: data(2)?,
        },
        _ => return None,
    };
    Some(Volume { at, turn, shape })
}

/// Every harmful ground one map's regions mark, with each region's name.
pub fn grounds(msb: &Msb) -> Vec<(Hazard, Volume, String)> {
    msb.table(POINTS)
        .entries
        .iter()
        .filter_map(|e| {
            let name = entry_name(e)?;
            let h = Hazard::of_region(field(POINTS, "type").get(e, 0)?, &name)?;
            Some((h, volume(e)?, name))
        })
        .collect()
}

/// Where a part stands.
pub fn position(part: &[u8]) -> Option<[f32; 3]> {
    let f = |i| field(PARTS, "position").get_f32(part, i);
    Some([f(0)?, f(1)?, f(2)?])
}

/// The harmful ground of every map. An open world's tiles overlap (a swamp's sound
/// can sit in the next tile's map), so there each ground is also kept on the
/// world's own ground plan, and a spot is tested against all of its world's.
#[derive(Debug, Clone, Default)]
pub struct Grounds {
    by_map: Vec<Vec<(Hazard, Volume)>>,
    world: BTreeMap<u8, Vec<(Hazard, Volume)>>,
}

impl Grounds {
    pub fn read(maps: &[(MapId, Msb)]) -> Grounds {
        let mut out = Grounds::default();
        for (m, msb) in maps {
            let here: Vec<(Hazard, Volume)> =
                grounds(msb).into_iter().map(|(h, v, _)| (h, v)).collect();
            if m.is_open_world() {
                for (h, v) in &here {
                    if let Some(at) = on_world(*m, v.at) {
                        out.world
                            .entry(m.area())
                            .or_default()
                            .push((*h, Volume { at, ..*v }));
                    }
                }
            }
            out.by_map.push(here);
        }
        out
    }

    /// The harm of the ground at `p` in map `map_index`, the worst first.
    pub fn at(&self, map_index: usize, map: MapId, p: [f32; 3]) -> Option<Hazard> {
        let worst =
            |g: &[(Hazard, Volume)], p| g.iter().filter(|(_, v)| v.holds(p)).map(|(h, _)| *h).max();
        if let (Some(g), Some(w)) = (self.world.get(&map.area()), on_world(map, p)) {
            return worst(g, w);
        }
        worst(self.by_map.get(map_index)?, p)
    }
}

/// A tile's position on its world's ground plan, height kept.
fn on_world(m: MapId, p: [f32; 3]) -> Option<[f32; 3]> {
    let [x, z] = m.world(p)?;
    Some([x, p[1], z])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_regions_name_says_its_ground() {
        let sound = |n| Hazard::of_region(SOUND, n);
        assert_eq!(sound("毒沼(赤)_05"), Some(Hazard::Swamp));
        assert_eq!(sound("毒沼(緑)"), Some(Hazard::Swamp));
        assert_eq!(sound("地下迷宮_川_毒_最奥"), Some(Hazard::Swamp));
        assert_eq!(sound("溶岩_中_04"), Some(Hazard::Lava));
        assert_eq!(sound("配置環境音_マグマ流れ"), Some(Hazard::Lava));
        assert_eq!(sound("デカ水たまり_02"), None, "a puddle");
        assert_eq!(Hazard::of_region(5, "【落水（断続）SFX】"), None);
        assert_eq!(
            Hazard::of_region(-1, "バディ起動可能領域 大毒沼"),
            Some(Hazard::Swamp)
        );
        assert_eq!(
            Hazard::of_region(-1, "毒沼の中にある安置（少し地面が出ている）"),
            None,
            "the swamp's dry place"
        );
        assert_eq!(
            Hazard::of_region(-1, "領域 敵 大部屋_入口 溶岩スライム起動"),
            None,
            "Lava Slimes, not lava"
        );
    }

    #[test]
    fn a_turned_box_holds_what_lies_along_it() {
        let v = Volume {
            at: [0.0, 0.0, 0.0],
            turn: 90.0,
            shape: Shape::Box {
                w: 40.0,
                d: 4.0,
                h: 10.0,
            },
        };
        // Turned a quarter, its width runs along z.
        assert!(v.holds([0.0, 5.0, 15.0]));
        assert!(!v.holds([15.0, 5.0, 0.0]));
        assert!(!v.holds([0.0, 30.0, 0.0]), "too high");
        let s = Volume {
            at: [10.0, 0.0, 0.0],
            turn: 0.0,
            shape: Shape::Sphere { r: 2.0 },
        };
        assert!(s.holds([12.5, 0.0, 0.0]), "within the margin");
        assert!(!s.holds([14.0, 0.0, 0.0]));
    }

    #[test]
    fn the_worst_ground_wins_and_a_next_tiles_ground_counts() {
        let ball = |at| Volume {
            at,
            turn: 0.0,
            shape: Shape::Sphere { r: 5.0 },
        };
        let mut g = Grounds {
            by_map: vec![vec![
                (Hazard::Swamp, ball([0.0; 3])),
                (Hazard::Lava, ball([0.0; 3])),
            ]],
            world: BTreeMap::new(),
        };
        let dungeon = MapId([31, 0, 0, 0]);
        assert_eq!(g.at(0, dungeon, [1.0, 0.0, 1.0]), Some(Hazard::Lava));
        assert_eq!(g.at(0, dungeon, [100.0, 0.0, 1.0]), None);
        // A swamp on the east edge of tile 40 reaches a spot on the west edge of 41.
        let west = MapId([60, 40, 40, 0]);
        let east = MapId([60, 41, 40, 0]);
        g.world.insert(
            60,
            vec![(
                Hazard::Swamp,
                ball(on_world(west, [127.0, 0.0, 0.0]).unwrap()),
            )],
        );
        assert_eq!(g.at(0, east, [-127.0, 0.0, 0.0]), Some(Hazard::Swamp));
        assert_eq!(g.at(0, east, [0.0, 0.0, 0.0]), None);
    }
}
