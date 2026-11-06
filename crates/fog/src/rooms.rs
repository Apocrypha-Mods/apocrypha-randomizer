//! The room map (`docs/fog-gates.md`): the item logic's areas, with a room carved out
//! for each shuffled door's inside, and each door's way rewired by the seed.
//!
//! - A dungeon mouth's inside is a room of the dungeon's whole map (its map id, longer
//!   than any prefix an area lists, so every location of the map is in it). Its outside
//!   is the area the map was in.
//! - A boss fog's inside is a room of its arena: the boss and every location tied to the
//!   boss's entity are in it ([`logic::Area::boss_flags`]), and no key item goes within
//!   [`ARENA_RADIUS`] of the arena's region.
//! - Where an edge of the area map asks for the boss, the edge says which side is which:
//!   its `from` is the outside, and the edge now leaves from the arena, so what lies
//!   past a boss is reached through its arena. A boss on no edge has the area of its
//!   wall outside, or the dungeon's room when the wall is in a dungeon.
//! - Each door's outside joins the inside of the door the seed sends it to: both ways
//!   into a dungeon, whose wall is ours; one way into an arena, as whether its fog still
//!   leads back once the boss is dead is for the lab to see.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use gamedata::Model;
use items::rng::Rng;
use logic::{Area, Bound, Edge, Graph, KeptOut, Need};

use crate::doors::{Door, Kind};

/// How far from an arena's region no key item is put.
pub const ARENA_RADIUS: f32 = 40.0;
/// Dungeons with a second way in, left as the game has them: a cave that joins the
/// Academy of Raya Lucaria, and Perfumer's Grotto, which joins Altus Tunnel inside.
pub const SECOND_WAY: [[u8; 2]; 3] = [[31, 6], [31, 18], [32, 5]];
/// Maps whose fogs stay: the Chapel of Anticipation and the Stranded Graveyard, before
/// the start; the Roundtable Hold; the Ashen Capital and the Elden Throne, past the
/// point of no return.
pub const KEPT_MAPS: [[u8; 2]; 5] = [[10, 1], [18, 0], [11, 10], [11, 5], [19, 0]];

/// The room of a door's inside.
pub fn room_id(d: &Door) -> String {
    match d.kind {
        Kind::Mouth => format!("room:{}", d.map),
        Kind::Boss { .. } => format!("arena:{}", d.fog.unwrap_or(0)),
    }
}

/// The doors that may be shuffled, given the game's model and the area map.
pub fn eligible(doors: Vec<Door>, base: &Graph, model: &Model) -> Vec<Door> {
    let bound = Bound::new(base, model);
    // An area of the map that is all one dungeon is the area map's own; leave it.
    let own: BTreeSet<&str> = base
        .areas
        .iter()
        .filter(|a| a.graces.is_empty() && a.maps.len() == 1)
        .map(|a| a.maps[0].as_str())
        .collect();
    let shut: BTreeSet<&str> = base
        .closes
        .iter()
        .chain(&base.no_return)
        .map(String::as_str)
        .collect();
    let mut per_boss: HashMap<u32, usize> = HashMap::new();
    for d in &doors {
        if let Kind::Boss { defeat_flag, .. } = d.kind {
            *per_boss.entry(defeat_flag).or_default() += 1;
        }
    }
    doors
        .into_iter()
        .filter(|d| {
            let [a, b, ..] = d.map.0;
            if KEPT_MAPS.contains(&[a, b]) {
                return false;
            }
            match &d.kind {
                Kind::Mouth => {
                    !SECOND_WAY.contains(&[a, b])
                        && !own.iter().any(|m| d.map.to_string().starts_with(m))
                }
                Kind::Boss { defeat_flag, .. } => {
                    per_boss.get(defeat_flag) == Some(&1)
                        && model.bosses.iter().any(|x| x.defeat_flag == *defeat_flag)
                }
            }
        })
        .filter(|d| {
            bound
                .area_at(d.wall.map, d.wall.position)
                .is_some_and(|a| !shut.contains(base.areas[a].id.as_str()))
        })
        .collect()
}

fn area(id: String, name: String) -> Area {
    Area {
        id,
        name,
        graces: vec![],
        maps: vec![],
        sure: false,
        note: String::new(),
        boss_flags: vec![],
        keys_kept_out: vec![],
    }
}

fn way(from: &str, to: &str) -> Edge {
    Edge {
        from: from.to_string(),
        to: to.to_string(),
        need: Need::default(),
        oneway: false,
        sure: false,
        note: "fog gate".to_string(),
    }
}

/// The area map with the doors' rooms, and door `d`'s outside joined to the inside of
/// door `to[d]`. Also returns each door's outside area, by id.
pub fn graph(
    base: &Graph,
    model: &Model,
    doors: &[Door],
    to: &[usize],
) -> Result<(Graph, Vec<String>), String> {
    let base_bound = Bound::new(base, model);
    let mut g = base.clone();
    for d in doors.iter().filter(|d| d.kind == Kind::Mouth) {
        let mut a = area(room_id(d), format!("{} (inside)", d.map));
        a.maps = vec![d.map.to_string()];
        g.areas.push(a);
    }
    // The outsides: a mouth's is the area its map was in; a boss fog's, the area of
    // its wall now that the dungeons are rooms.
    let rooms_bound = Bound::new(&g, model);
    let boss_flag = |name: &String| -> Option<u32> {
        let i = logic::find_boss(model, name).ok()?;
        Some(model.bosses[i].defeat_flag)
    };
    // The first edge that asks for each boss: the way past it.
    let mut passage: HashMap<u32, usize> = HashMap::new();
    for (i, e) in base.edges.iter().enumerate() {
        for b in &e.need.bosses {
            if let Some(f) = boss_flag(b) {
                passage.entry(f).or_insert(i);
            }
        }
    }
    let mut outside = Vec::with_capacity(doors.len());
    for d in doors {
        let a = match d.kind {
            Kind::Mouth => base_bound
                .area_at(d.map, d.outside.position)
                .map(|i| base.areas[i].id.clone()),
            Kind::Boss { defeat_flag, .. } => match passage.get(&defeat_flag) {
                Some(&e) => Some(base.edges[e].from.clone()),
                None => rooms_bound
                    .area_at(d.wall.map, d.wall.position)
                    .map(|i| g.areas[i].id.clone()),
            },
        };
        outside.push(a.ok_or_else(|| format!("{}: its outside is in no area", d.name))?);
    }
    drop(rooms_bound);

    let mut arena_of_boss: BTreeMap<u32, String> = BTreeMap::new();
    for d in doors {
        let Kind::Boss { defeat_flag, .. } = d.kind else {
            continue;
        };
        let mut a = area(room_id(d), format!("arena of {}", d.name));
        a.boss_flags = vec![defeat_flag];
        a.keys_kept_out = vec![KeptOut {
            map: d.inside.map.to_string(),
            position: d.inside.position,
            radius: ARENA_RADIUS,
        }];
        g.areas.push(a);
        arena_of_boss.insert(defeat_flag, room_id(d));
    }
    // What lies past a shuffled boss is reached through its arena.
    for (&flag, &e) in &passage {
        if let Some(arena) = arena_of_boss.get(&flag) {
            g.edges[e].from = arena.clone();
        }
    }
    for (d, &t) in to.iter().enumerate() {
        let target = doors.get(t).ok_or("a door sent nowhere")?;
        // Back through a boss's fog, once the boss is dead, is for the lab to see:
        // until then the proof counts only the way in.
        let mut e = way(&outside[d], &room_id(target));
        e.oneway = target.kind != Kind::Mouth;
        g.edges.push(e);
    }
    Ok((g, outside))
}

/// A seed's doors: door `d`'s outside leads to the inside of door `to[d]`.
#[derive(Debug, Clone)]
pub struct Plan {
    pub doors: Vec<Door>,
    pub to: Vec<usize>,
    pub outside: Vec<String>,
    pub graph: Graph,
    /// How many draws it took to find doors the game can be finished through.
    pub tries: usize,
}

const SALT: u64 = 0x666f_675f_6761_7465;
/// How many draws before giving up on a seed.
pub const TRIES: usize = 200;

/// Draw the doors until the game, its items where the game has them, can be finished
/// through them.
pub fn plan(base: &Graph, model: &Model, doors: Vec<Door>, seed: u64) -> Result<Plan, String> {
    let mut rng = Rng::new(seed ^ SALT);
    for tries in 1..=TRIES {
        let mut to: Vec<usize> = (0..doors.len()).collect();
        rng.shuffle(&mut to);
        let (graph, outside) = graph(base, model, &doors, &to)?;
        let bound = Bound::new(&graph, model);
        if !bound.problems.is_empty() {
            return Err(format!("the room map does not fit: {:?}", bound.problems));
        }
        if bound.walk(&bound.vanilla()).goal {
            return Ok(Plan {
                doors,
                to,
                outside,
                graph,
                tries,
            });
        }
    }
    Err(format!(
        "no doors through which the game can be finished in {TRIES} draws"
    ))
}
