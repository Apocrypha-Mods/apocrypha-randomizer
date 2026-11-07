//! The room map (`docs/fog-gates.md`): the item logic's areas, with a room carved out
//! for each shuffled door's inside, and each door's way rewired by the seed.
//!
//! - A dungeon mouth's inside is a room of the dungeon's whole map (its map id, longer
//!   than any prefix an area lists, so every location of the map is in it). Its outside
//!   is the area the map was in.
//! - A boss fog's inside is a room of its arena: the boss and every location tied to the
//!   boss's entity are in it ([`logic::Area::boss_flags`]), and no key item goes within
//!   [`ARENA_RADIUS`] of the arena's region. A boss with more than one entrance fog (the
//!   Godskin Duo's three) has one arena, which each of its fogs, a door of its own, leads
//!   into; its fogs are shuffled all together or not at all.
//! - Where an edge of the area map asks for a boss of one fog, the edge says which side
//!   is which: its `from` is the outside, and the edge now leaves from the arena, so what
//!   lies past a boss is reached through its arena. A boss on no edge, and each fog of a
//!   boss of several, has the area of its wall outside, or the dungeon's room when the
//!   wall is in a dungeon.
//! - A dungeon with a second way in (the Academy Crystal Cave's far end touches the
//!   Academy's map, Perfumer's Grotto and Altus Tunnel touch each other: their maps'
//!   connect collisions) is a door like any other, and the room map has no edge for the
//!   second way: whether a player can walk it is not read, and a way the proof does not
//!   count on only lets the player reach more than it counts on.
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
/// Maps whose fogs stay: the Chapel of Anticipation and the Stranded Graveyard, before
/// the start; the Roundtable Hold; the Ashen Capital and the Elden Throne, past the
/// point of no return.
pub const KEPT_MAPS: [[u8; 2]; 5] = [[10, 1], [18, 0], [11, 10], [11, 5], [19, 0]];

/// The room of a door's inside.
pub fn room_id(d: &Door) -> String {
    match d.kind {
        Kind::Mouth => format!("room:{}", d.map),
        Kind::Boss { defeat_flag, .. } => format!("arena:{defeat_flag}"),
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
    let fits = |d: &Door| -> bool {
        let [a, b, ..] = d.map.0;
        if KEPT_MAPS.contains(&[a, b]) {
            return false;
        }
        let kind = match &d.kind {
            Kind::Mouth => !own.iter().any(|m| d.map.to_string().starts_with(m)),
            Kind::Boss { defeat_flag, .. } => {
                model.bosses.iter().any(|x| x.defeat_flag == *defeat_flag)
            }
        };
        kind && bound
            .area_at(d.wall.map, d.wall.position)
            .is_some_and(|a| !shut.contains(base.areas[a].id.as_str()))
    };
    // A boss's fogs go together: one left as the game has it would be a way into the
    // arena the shuffle does not move.
    let mut boss_fits: HashMap<u32, bool> = HashMap::new();
    for d in &doors {
        if let Kind::Boss { defeat_flag, .. } = d.kind {
            let ok = fits(d);
            *boss_fits.entry(defeat_flag).or_insert(true) &= ok;
        }
    }
    doors
        .into_iter()
        .filter(|d| match d.kind {
            Kind::Mouth => fits(d),
            Kind::Boss { defeat_flag, .. } => boss_fits[&defeat_flag],
        })
        .collect()
}

/// How many of `doors` are fogs of each boss, by defeat flag.
fn fogs_per_boss(doors: &[Door]) -> HashMap<u32, usize> {
    let mut out: HashMap<u32, usize> = HashMap::new();
    for d in doors {
        if let Kind::Boss { defeat_flag, .. } = d.kind {
            *out.entry(defeat_flag).or_default() += 1;
        }
    }
    out
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
    let per_boss = fogs_per_boss(doors);
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
            Kind::Boss { defeat_flag, .. } => match passage
                .get(&defeat_flag)
                .filter(|_| per_boss.get(&defeat_flag) == Some(&1))
            {
                Some(&e) => Some(base.edges[e].from.clone()),
                // A boss of several fogs: which fog the edge's `from` stands before is
                // not said, so each fog has its wall's area.
                None => rooms_bound
                    .area_at(d.wall.map, d.wall.position)
                    .map(|i| g.areas[i].id.clone()),
            },
        };
        outside.push(a.ok_or_else(|| format!("{}: its outside is in no area", d.name))?);
    }
    drop(rooms_bound);

    // One arena a boss, kept clear of key items around each of its fogs' insides.
    let mut arena_of_boss: BTreeMap<u32, usize> = BTreeMap::new();
    for d in doors {
        let Kind::Boss { defeat_flag, .. } = d.kind else {
            continue;
        };
        let kept = KeptOut {
            map: d.inside.map.to_string(),
            position: d.inside.position,
            radius: ARENA_RADIUS,
        };
        match arena_of_boss.get(&defeat_flag) {
            Some(&i) => {
                let a: &mut Area = &mut g.areas[i];
                a.name = format!("{} and {}", a.name, d.name);
                a.keys_kept_out.push(kept);
            }
            None => {
                let mut a = area(room_id(d), format!("arena of {}", d.name));
                a.boss_flags = vec![defeat_flag];
                a.keys_kept_out = vec![kept];
                arena_of_boss.insert(defeat_flag, g.areas.len());
                g.areas.push(a);
            }
        }
    }
    // What lies past a shuffled boss is reached through its arena.
    for (&flag, &e) in &passage {
        if let Some(&arena) = arena_of_boss.get(&flag) {
            g.edges[e].from = g.areas[arena].id.clone();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doors::Spot;
    use gamedata::MapId;
    use gamedata::model::Boss;

    fn spot(map: MapId, x: f32) -> Spot {
        Spot {
            map,
            position: [x, 0.0, 0.0],
            facing: 0.0,
            entity: None,
        }
    }

    /// A fog of the boss beaten on flag 100, in `map`.
    fn fog(fog: u32, map: MapId) -> Door {
        Door {
            name: format!("boss:{fog}"),
            kind: Kind::Boss {
                defeat_flag: 100,
                args: vec![],
                starter: 1,
                at: 0,
            },
            map,
            fog: Some(fog),
            wall: spot(map, 0.0),
            outside: spot(map, -2.0),
            inside: spot(map, 1.0),
        }
    }

    fn model() -> Model {
        Model {
            bosses: vec![Boss {
                area: 1,
                map: Some(MapId([16, 0, 0, 0])),
                entity: None,
                name: Some("Duo".into()),
                defeat_flag: 100,
            }],
            ..Default::default()
        }
    }

    fn graph(closes: &str) -> Graph {
        toml::from_str(&format!(
            r#"
            start = "hall"
            goal = {{ bosses = ["Duo"] }}
            closes = [{closes}]
            [[area]]
            id = "hall"
            name = "Hall"
            maps = ["m16_00"]
            [[area]]
            id = "side"
            name = "Side"
            maps = ["m16_01"]
            [[area]]
            id = "beyond"
            name = "Beyond"
            maps = ["m16_02"]
            [[edge]]
            from = "hall"
            to = "side"
            [[edge]]
            from = "hall"
            to = "beyond"
            bosses = ["Duo"]
            "#
        ))
        .unwrap()
    }

    fn duo() -> Vec<Door> {
        vec![fog(1, MapId([16, 0, 0, 0])), fog(2, MapId([16, 1, 0, 0]))]
    }

    #[test]
    fn a_bosss_fogs_are_shuffled_all_together_or_not_at_all() {
        let m = model();
        assert_eq!(eligible(duo(), &graph(""), &m).len(), 2);
        // One fog in an area that closes keeps the other as the game has it too.
        assert!(eligible(duo(), &graph(r#""side""#), &m).is_empty());
    }

    #[test]
    fn a_bosss_fogs_lead_into_one_arena() {
        let (m, base) = (model(), graph(""));
        let doors = duo();
        for to in [[0, 1], [1, 0]] {
            let (g, outside) = super::graph(&base, &m, &doors, &to).unwrap();
            let arenas: Vec<&Area> = g.areas.iter().filter(|a| a.id == "arena:100").collect();
            assert_eq!(arenas.len(), 1);
            assert_eq!(arenas[0].keys_kept_out.len(), 2);
            assert_eq!(arenas[0].boss_flags, [100]);
            // Each fog's outside is its wall's area; what lies past leaves the arena.
            assert_eq!(outside, ["hall", "side"]);
            assert!(
                g.edges
                    .iter()
                    .any(|e| e.from == "arena:100" && e.to == "beyond")
            );
            assert!(
                g.edges
                    .iter()
                    .filter(|e| e.note == "fog gate")
                    .all(|e| e.oneway)
            );
            let bound = Bound::new(&g, &m);
            assert!(bound.problems.is_empty(), "{:?}", bound.problems);
            assert!(bound.walk(&bound.vanilla()).goal);
        }
    }
}
