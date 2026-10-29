//! The area graph (PLAN.md §5, Phase 3): which areas connect, and what it
//! takes to get from one to the next. It is authored here, from play and
//! general knowledge of the game (PLAN.md §1, rule 4), in
//! `data/elden_ring.toml`, and bound to the model extracted from the game:
//! every grace, item and boss it names must be one the model has.
//!
//! An item location belongs to an area by, in order:
//! 1. its map, when an area lists the map (longest prefix wins);
//! 2. the nearest grace an area lists: in the same map, or, in the open
//!    worlds, anywhere on the same world ([`gamedata::MapId::world`]);
//! 3. a `place` entry, for locations with no map (the grace menu's gifts).
//!
//! Locations none of these reach are *anywhere*: taken as reachable from the
//! start, and counted so the draft can be tightened.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use gamedata::model::{Kind, Location};
use gamedata::{MapId, Model};
use serde::Deserialize;

const ELDEN_RING: &str = include_str!("../data/elden_ring.toml");

/// What it takes: every item and boss listed, and `n` of the `any` items.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Need {
    #[serde(default)]
    pub items: Vec<String>,
    /// Bosses to have beaten, by name, or `name@mAA_BB` where a name is
    /// shared.
    #[serde(default)]
    pub bosses: Vec<String>,
    #[serde(default)]
    pub any: Option<AnyOf>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnyOf {
    pub n: usize,
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Area {
    pub id: String,
    pub name: String,
    /// Graces in the area, by name, or `name@mAA_BB` where a name is shared.
    #[serde(default)]
    pub graces: Vec<String>,
    /// Maps wholly in the area, as prefixes (`m31_15`, `m60_49_53`).
    #[serde(default)]
    pub maps: Vec<String>,
    #[serde(default = "yes")]
    pub sure: bool,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    pub from: String,
    pub to: String,
    #[serde(flatten)]
    pub need: Need,
    #[serde(default)]
    pub oneway: bool,
    #[serde(default = "yes")]
    pub sure: bool,
    #[serde(default)]
    pub note: String,
}

/// Puts the locations giving an item, of one kind, in an area: for those
/// with no map, or whose map says nothing useful.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Place {
    pub item: String,
    pub kind: Kind,
    pub area: String,
    #[serde(default)]
    pub need: Need,
    #[serde(default = "yes")]
    pub sure: bool,
    #[serde(default)]
    pub note: String,
}

/// A location the game has but never gives in play: the randomizer neither
/// counts on it nor puts anything there.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unused {
    pub item: String,
    pub kind: Kind,
    /// The location's source, as the model gives it (`talk t324001100`).
    pub source: String,
    pub note: String,
}

impl Unused {
    fn matches(&self, l: &Location) -> bool {
        l.kind == self.kind
            && l.source == self.source
            && l.items
                .iter()
                .any(|i| i.name.as_deref() == Some(&self.item))
    }
}

/// Shop rows by range that no merchant opens: placed in an area, or ruled
/// out like an `unused` location.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShopRows {
    pub first: i32,
    pub last: i32,
    #[serde(default)]
    pub area: Option<String>,
    #[serde(default)]
    pub unused: bool,
    #[serde(default = "yes")]
    pub sure: bool,
    pub note: String,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Graph {
    pub start: String,
    pub goal: Need,
    #[serde(rename = "area")]
    pub areas: Vec<Area>,
    #[serde(rename = "edge")]
    pub edges: Vec<Edge>,
    #[serde(default, rename = "place")]
    pub places: Vec<Place>,
    #[serde(default)]
    pub unused: Vec<Unused>,
    #[serde(default)]
    pub shop_rows: Vec<ShopRows>,
}

/// Elden Ring's graph.
pub fn elden_ring() -> Graph {
    toml::from_str(ELDEN_RING).expect("data/elden_ring.toml parses")
}

/// The graph bound to a model: every name resolved, every location in an
/// area (or none).
pub struct Bound<'a> {
    pub graph: &'a Graph,
    pub model: &'a Model,
    /// For each location, its area's index, and the `place` it came from.
    pub location_area: Vec<Option<usize>>,
    /// For each location, the areas of the other places it is offered.
    pub location_also: Vec<Vec<usize>>,
    pub location_need: Vec<Option<&'a Need>>,
    /// Locations an `unused` entry rules out.
    pub location_unused: Vec<bool>,
    /// For each boss, its area.
    pub boss_area: Vec<Option<usize>>,
    /// What the draft names that the model does not have, and other faults.
    pub problems: Vec<String>,
}

fn matches_map(spec: &str, map: Option<MapId>) -> bool {
    map.is_some_and(|m| m.to_string().starts_with(spec))
}

/// `name` or `name@map`.
fn split_at_map(spec: &str) -> (&str, Option<&str>) {
    match spec.rsplit_once('@') {
        Some((n, m)) if m.starts_with('m') => (n, Some(m)),
        _ => (spec, None),
    }
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

impl<'a> Bound<'a> {
    pub fn new(graph: &'a Graph, model: &'a Model) -> Self {
        let mut problems = Vec::new();
        let area_index: HashMap<&str, usize> = graph
            .areas
            .iter()
            .enumerate()
            .map(|(i, a)| (a.id.as_str(), i))
            .collect();
        if area_index.len() != graph.areas.len() {
            problems.push("an area id is used twice".into());
        }
        let area = |id: &str, problems: &mut Vec<String>| {
            let found = area_index.get(id).copied();
            if found.is_none() {
                problems.push(format!("no area {id}"));
            }
            found
        };
        area(&graph.start, &mut problems);
        for e in &graph.edges {
            area(&e.from, &mut problems);
            area(&e.to, &mut problems);
        }

        // Graces.
        let mut grace_area: Vec<Option<usize>> = vec![None; model.graces.len()];
        for (ai, a) in graph.areas.iter().enumerate() {
            for spec in &a.graces {
                let (name, map) = split_at_map(spec);
                let found: Vec<usize> = model
                    .graces
                    .iter()
                    .enumerate()
                    .filter(|(_, g)| g.name.as_deref() == Some(name) && g.map.is_some())
                    .filter(|(_, g)| map.is_none_or(|m| matches_map(m, g.map)))
                    .map(|(i, _)| i)
                    .collect();
                match found[..] {
                    [i] => {
                        if let Some(other) = grace_area[i] {
                            problems.push(format!(
                                "grace {spec} is in both {} and {}",
                                graph.areas[other].id, a.id
                            ));
                        }
                        grace_area[i] = Some(ai);
                    }
                    [] => problems.push(format!("{}: no grace {spec}", a.id)),
                    _ => problems.push(format!("{}: grace {spec} is ambiguous; add @map", a.id)),
                }
            }
        }
        for (i, g) in model.graces.iter().enumerate() {
            if g.map.is_some_and(MapId::is_open_world)
                && g.name.is_some()
                && grace_area[i].is_none()
            {
                problems.push(format!(
                    "open-world grace {} ({}) is in no area",
                    g.name.as_deref().unwrap_or("?"),
                    g.map.unwrap()
                ));
            }
        }

        // Items and bosses that requirements name.
        let item_names: BTreeSet<&str> = model
            .locations
            .iter()
            .flat_map(|l| &l.items)
            .filter_map(|i| i.name.as_deref())
            .collect();
        let mut needs: Vec<&Need> = graph.edges.iter().map(|e| &e.need).collect();
        needs.push(&graph.goal);
        needs.extend(graph.places.iter().map(|p| &p.need));
        for n in &needs {
            for item in n.items.iter().chain(n.any.iter().flat_map(|a| &a.items)) {
                if !item_names.contains(item.as_str()) {
                    problems.push(format!("no item {item} anywhere in the model"));
                }
            }
            for b in &n.bosses {
                if let Err(p) = find_boss(model, b) {
                    problems.push(p);
                }
            }
        }

        // Where each location is.
        let maps_of: Vec<(&str, usize)> = graph
            .areas
            .iter()
            .enumerate()
            .flat_map(|(i, a)| a.maps.iter().map(move |m| (m.as_str(), i)))
            .collect();
        let placed_graces: Vec<(usize, MapId, [f32; 3])> = model
            .graces
            .iter()
            .enumerate()
            .filter_map(|(i, g)| Some((grace_area[i]?, g.map?, g.position?)))
            .collect();
        let by_map = |map: Option<MapId>| {
            maps_of
                .iter()
                .filter(|(spec, _)| matches_map(spec, map))
                .max_by_key(|(spec, _)| spec.len())
                .map(|&(_, a)| a)
        };
        let by_grace = |map: Option<MapId>, pos: Option<[f32; 3]>| -> Option<usize> {
            let map = map?;
            // In the open worlds, the nearest on the same world; a location
            // with no position stands at its tile's centre.
            if let Some(here) = map.world(pos.unwrap_or([0.0; 3])) {
                return placed_graces
                    .iter()
                    .filter(|(_, m, _)| m.area() == map.area())
                    .filter_map(|&(a, m, gp)| Some((a, distance(here, m.world(gp)?))))
                    .min_by(|x, y| x.1.total_cmp(&y.1))
                    .map(|(a, _)| a);
            }
            let here: Vec<&(usize, MapId, [f32; 3])> =
                placed_graces.iter().filter(|(_, m, _)| *m == map).collect();
            match pos {
                Some(p) => here
                    .iter()
                    .map(|&&(a, _, gp)| (a, distance([p[0], p[2]], [gp[0], gp[2]])))
                    .min_by(|x, y| x.1.total_cmp(&y.1))
                    .map(|(a, _)| a),
                // With no position, the area most of the map's graces are in.
                None => {
                    let mut count: BTreeMap<usize, usize> = BTreeMap::new();
                    for &&(a, _, _) in &here {
                        *count.entry(a).or_default() += 1;
                    }
                    count.into_iter().max_by_key(|&(_, n)| n).map(|(a, _)| a)
                }
            }
        };
        let place_of = |l: &Location| {
            graph.places.iter().find(|p| {
                p.kind == l.kind && l.items.iter().any(|i| i.name.as_deref() == Some(&p.item))
            })
        };
        for p in &graph.places {
            area(&p.area, &mut problems);
            if !model.locations.iter().any(|l| {
                l.kind == p.kind && l.items.iter().any(|i| i.name.as_deref() == Some(&p.item))
            }) {
                problems.push(format!("place: no {} giving {}", p.kind.label(), p.item));
            }
        }
        for u in &graph.unused {
            if !model.locations.iter().any(|l| u.matches(l)) {
                problems.push(format!(
                    "unused: no {} from {} giving {}",
                    u.kind.label(),
                    u.source,
                    u.item
                ));
            }
        }
        for r in &graph.shop_rows {
            if let Some(a) = &r.area {
                area(a, &mut problems);
            }
            if r.area.is_some() == r.unused {
                problems.push(format!(
                    "shop rows {}..{}: give an area or mark them unused",
                    r.first, r.last
                ));
            }
        }
        let rows_of = |l: &Location| {
            let row = l.shop_row()?;
            graph
                .shop_rows
                .iter()
                .find(|r| (r.first..=r.last).contains(&row) && l.map.is_none())
        };
        let location_unused = model
            .locations
            .iter()
            .map(|l| {
                graph.unused.iter().any(|u| u.matches(l)) || rows_of(l).is_some_and(|r| r.unused)
            })
            .collect();
        let enemy_at: HashMap<u32, [f32; 3]> = model
            .enemies
            .iter()
            .filter_map(|e| Some((e.entity?, e.position)))
            .collect();
        let mut location_area = Vec::with_capacity(model.locations.len());
        let mut location_also = Vec::with_capacity(model.locations.len());
        let mut location_need = Vec::with_capacity(model.locations.len());
        for l in &model.locations {
            let pos = l.position.or_else(|| enemy_at.get(&l.entity?).copied());
            let placed = place_of(l);
            location_need.push(placed.map(|p| &p.need));
            location_area.push(
                placed
                    .and_then(|p| area_index.get(p.area.as_str()).copied())
                    .or_else(|| {
                        rows_of(l)
                            .and_then(|r| r.area.as_deref())
                            .and_then(|a| area_index.get(a).copied())
                    })
                    .or_else(|| by_map(l.map))
                    .or_else(|| by_grace(l.map, pos)),
            );
            location_also.push(
                l.also
                    .iter()
                    .filter_map(|s| {
                        by_map(Some(s.map)).or_else(|| by_grace(Some(s.map), s.position))
                    })
                    .collect(),
            );
        }
        let boss_area = model
            .bosses
            .iter()
            .map(|b| {
                let pos = b.entity.and_then(|e| enemy_at.get(&e).copied());
                by_map(b.map).or_else(|| by_grace(b.map, pos))
            })
            .collect();
        Bound {
            graph,
            model,
            location_area,
            location_also,
            location_need,
            location_unused,
            boss_area,
            problems,
        }
    }

    /// Walk the graph with the items where `gives` says they are (the vanilla
    /// game: [`Bound::vanilla`]), from the start, until nothing new opens.
    pub fn walk(&self, gives: &[Vec<&str>]) -> Walk {
        let n = self.graph.areas.len();
        let index: HashMap<&str, usize> = self
            .graph
            .areas
            .iter()
            .enumerate()
            .map(|(i, a)| (a.id.as_str(), i))
            .collect();
        let mut reached = vec![false; n];
        let mut order = Vec::new();
        if let Some(&s) = index.get(self.graph.start.as_str()) {
            reached[s] = true;
            order.push(s);
        }
        let mut items: BTreeSet<&str> = BTreeSet::new();
        let mut taken = vec![false; gives.len()];
        loop {
            let mut changed = false;
            // Collect what the reached areas give, and anywhere's.
            for (i, g) in gives.iter().enumerate() {
                if taken[i] || self.location_unused[i] {
                    continue;
                }
                let here = self.location_area[i].is_none_or(|a| reached[a])
                    || self.location_also[i].iter().any(|&a| reached[a]);
                let ok = self.location_need[i].is_none_or(|need| self.met(need, &items, &reached));
                if here && ok {
                    taken[i] = true;
                    changed |= g.iter().any(|it| items.insert(it));
                }
            }
            for e in &self.graph.edges {
                let (Some(&f), Some(&t)) = (index.get(e.from.as_str()), index.get(e.to.as_str()))
                else {
                    continue;
                };
                for (a, b) in [(f, t), (t, f)]
                    .into_iter()
                    .take(if e.oneway { 1 } else { 2 })
                {
                    if reached[a] && !reached[b] && self.met(&e.need, &items, &reached) {
                        reached[b] = true;
                        order.push(b);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        Walk {
            goal: self.met(&self.graph.goal, &items, &reached),
            order,
            reached,
            items: items.into_iter().map(String::from).collect(),
        }
    }

    /// What each location gives in the game as it is.
    pub fn vanilla(&self) -> Vec<Vec<&str>> {
        self.model
            .locations
            .iter()
            .map(|l| {
                if l.kind == Kind::Starting {
                    // One class, not all ten: key items never start in them.
                    return Vec::new();
                }
                l.items.iter().filter_map(|i| i.name.as_deref()).collect()
            })
            .collect()
    }

    fn met(&self, need: &Need, items: &BTreeSet<&str>, reached: &[bool]) -> bool {
        need.items.iter().all(|i| items.contains(i.as_str()))
            && need.bosses.iter().all(|b| {
                find_boss(self.model, b)
                    .ok()
                    .and_then(|i| self.boss_area[i])
                    .is_some_and(|a| reached[a])
            })
            && need.any.as_ref().is_none_or(|any| {
                any.items
                    .iter()
                    .filter(|i| items.contains(i.as_str()))
                    .count()
                    >= any.n
            })
    }
}

/// A boss by `name` or `name@map`, which must pick out exactly one.
pub fn find_boss(model: &Model, spec: &str) -> Result<usize, String> {
    let (name, map) = split_at_map(spec);
    let found: Vec<usize> = model
        .bosses
        .iter()
        .enumerate()
        .filter(|(_, b)| b.name.as_deref() == Some(name))
        .filter(|(_, b)| map.is_none_or(|m| matches_map(m, b.map)))
        .map(|(i, _)| i)
        .collect();
    match found[..] {
        [i] => Ok(i),
        [] => Err(format!("no boss {spec}")),
        _ => Err(format!("boss {spec} is ambiguous; add @map")),
    }
}

/// What a walk reached.
pub struct Walk {
    pub reached: Vec<bool>,
    /// Areas in the order they opened.
    pub order: Vec<usize>,
    pub items: Vec<String>,
    pub goal: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use gamedata::model::{Boss, Grace, Item, ItemKind};

    fn item(name: &str) -> Item {
        Item {
            kind: ItemKind::Goods,
            id: 1,
            count: 1,
            chance: 1000,
            name: Some(name.into()),
        }
    }

    fn at(map: MapId, pos: [f32; 3], items: &[&str]) -> Location {
        Location {
            kind: Kind::Treasure,
            map: Some(map),
            source: String::new(),
            entity: None,
            position: Some(pos),
            lot: None,
            items: items.iter().map(|n| item(n)).collect(),
            flag: None,
            also: vec![],
        }
    }

    /// Two open-world graces in one large tile, a dungeon, and a boss.
    fn model() -> Model {
        let tile = MapId([60, 10, 9, 2]);
        let grace = |id, name: &str, map, x| Grace {
            id,
            name: Some(name.into()),
            map: Some(map),
            entity: 0,
            flag: 0,
            position: Some([x, 0.0, 0.0]),
        };
        Model {
            graces: vec![
                grace(1, "West", tile, -400.0),
                grace(2, "East", tile, 400.0),
            ],
            locations: vec![
                // Near West, but in a small tile: placed through the world.
                at(MapId([60, 40, 37, 0]), [0.0; 3], &["Key"]),
                at(MapId([31, 0, 0, 0]), [0.0; 3], &["Prize"]),
            ],
            bosses: vec![Boss {
                area: 1,
                map: Some(MapId([31, 0, 0, 0])),
                entity: None,
                name: Some("Guardian".into()),
                defeat_flag: 1,
            }],
            ..Default::default()
        }
    }

    fn graph() -> Graph {
        toml::from_str(
            r#"
            start = "west"
            goal = { bosses = ["Guardian"] }
            [[area]]
            id = "west"
            name = "West"
            graces = ["West"]
            [[area]]
            id = "east"
            name = "East"
            graces = ["East"]
            [[area]]
            id = "cave"
            name = "Cave"
            maps = ["m31_00"]
            [[edge]]
            from = "west"
            to = "east"
            [[edge]]
            from = "east"
            to = "cave"
            items = ["Key"]
            oneway = true
            "#,
        )
        .unwrap()
    }

    #[test]
    fn locations_go_to_the_nearest_grace_or_their_map() {
        let (g, m) = (graph(), model());
        let b = Bound::new(&g, &m);
        assert!(b.problems.is_empty(), "{:?}", b.problems);
        assert_eq!(b.location_area, vec![Some(0), Some(2)]);
        assert_eq!(b.boss_area, vec![Some(2)]);
    }

    #[test]
    fn a_walk_opens_what_the_items_it_finds_allow() {
        let (g, m) = (graph(), model());
        let b = Bound::new(&g, &m);
        let w = b.walk(&b.vanilla());
        assert!(w.goal);
        assert_eq!(w.order, vec![0, 1, 2]);
        // With the key in the cave, the cave never opens.
        let w = b.walk(&[vec![], vec!["Key"]]);
        assert!(!w.goal);
        assert!(!w.reached[2]);
    }

    #[test]
    fn an_unused_location_gives_nothing() {
        let mut g = graph();
        let mut m = model();
        // A second key, from a script that never runs, in the first area.
        let mut extra = at(MapId([60, 40, 37, 0]), [0.0; 3], &["Key"]);
        extra.kind = Kind::Gift;
        extra.source = "talk t1".into();
        m.locations[0].items.clear();
        m.locations.push(extra);
        g.unused.push(Unused {
            item: "Key".into(),
            kind: Kind::Gift,
            source: "talk t1".into(),
            note: String::new(),
        });
        let b = Bound::new(&g, &m);
        assert!(b.problems.is_empty(), "{:?}", b.problems);
        assert!(
            !b.walk(&b.vanilla()).reached[2],
            "the cave opens on an unused key"
        );
    }

    #[test]
    fn a_location_offered_in_two_places_is_got_at_either() {
        let (g, mut m) = (graph(), model());
        // The key is in the cave, but also offered in the first area.
        m.locations[0].map = Some(MapId([31, 0, 0, 0]));
        m.locations[0].also.push(gamedata::model::Spot {
            map: MapId([60, 40, 37, 0]),
            position: Some([0.0; 3]),
        });
        let b = Bound::new(&g, &m);
        assert_eq!(b.location_area[0], Some(2));
        assert_eq!(b.location_also[0], vec![0]);
        assert!(b.walk(&b.vanilla()).goal);
    }

    #[test]
    fn names_the_model_lacks_are_problems() {
        let mut g = graph();
        g.edges[1].need.items.push("Nothing Such".into());
        g.areas[0].graces.push("Nowhere".into());
        let m = model();
        let b = Bound::new(&g, &m);
        assert!(b.problems.iter().any(|p| p.contains("Nothing Such")));
        assert!(b.problems.iter().any(|p| p.contains("Nowhere")));
    }
}
