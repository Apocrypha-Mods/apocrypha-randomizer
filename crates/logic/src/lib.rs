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
//!
//! A shop row also asks what its [`gamedata::model::Terms`] say: the items
//! its price and material set spend, and the flag that puts it on sale. That
//! flag is explained by the model (a boss beaten, an item handed over), by
//! an `on_sale` entry here, or else by the ways the model reads events and talk
//! turning it on, as a gift's; a row whose flag nothing explains is never
//! counted as got.
//!
//! A gift asks what its talk scripts' conditions do
//! ([`gamedata::model::Location::given_if`]): any one of its ways, each a
//! list of bosses beaten, items held, other locations got and areas reached
//! (a flag an event sets in a map's own script). A way that
//! waits on a flag nothing explains (an NPC's quest phase, mostly) is never
//! open. A `place` entry for the gift is checked by hand and takes the
//! talk's place.

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
    #[serde(default)]
    pub items: Vec<String>,
    /// Bosses to have beaten, as in [`Need::bosses`], each counting as one.
    #[serde(default)]
    pub bosses: Vec<String>,
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
    /// A fog gate room's bosses, by defeat flag: each boss, and every location tied to
    /// its entity (its rewards and drops), is in this area wherever it stands.
    #[serde(default)]
    pub boss_flags: Vec<u32>,
    /// A fog gate room's arena: no key item is put within `radius` metres of
    /// `position` in `map` (the arena's own treasures), as which side of the wall such
    /// a spot is on is not read.
    #[serde(default)]
    pub keys_kept_out: Vec<KeptOut>,
}

/// A circle of a map no key item is put in.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeptOut {
    pub map: String,
    pub position: [f32; 3],
    pub radius: f32,
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

/// What turns on a flag that puts shop rows on sale, where the model does
/// not tie it to a boss or a handover: the Twin Maiden Husks' stock, for one,
/// goes on sale when its bell bearing is handed over.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnSale {
    pub flag: u32,
    #[serde(flatten)]
    pub need: Need,
    /// The rows can be lost before they are bought (their merchant moves on in
    /// a quest), so no key item is put there.
    #[serde(default)]
    pub missable: bool,
    #[serde(default = "yes")]
    pub sure: bool,
    pub note: String,
}

/// A merchant who can leave or die in a quest, given a talk menu entry that
/// gives their bell bearing once (Nova, 2026-11-03). Handed to the Twin Maiden
/// Husks, it puts the merchant's own rows on sale at the Roundtable Hold, so
/// what the seed put there cannot be lost with the merchant.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GivenBearing {
    /// Who, in words.
    pub merchant: String,
    /// The bearing, by its name; the model says which shop ranges it opens.
    pub item: String,
    /// The flag giving it turns on, which is also the item lot's row and flag.
    pub given: u32,
    /// Whether another's quest can take the merchant away before the player has
    /// ever met them, and so before the bearing could be had: their rows then take
    /// no key item (Nova, 2026-11-03).
    #[serde(default)]
    pub unmet: bool,
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
    /// The item a shop price is paid in, by ShopLineupParam `currency`
    /// (`"1" = "Dragon Heart"`); runes need none.
    #[serde(default)]
    pub currency: BTreeMap<String, String>,
    #[serde(default)]
    pub on_sale: Vec<OnSale>,
    /// Areas opened only by a step the game cannot take back, by id: a
    /// quest's item is not put there, as the quest may be over by then.
    #[serde(default)]
    pub no_return: Vec<String>,
    /// Areas that can shut for good while the game goes on, by id: what must be
    /// had is not put there, as a player may pass it by and be shut out.
    #[serde(default)]
    pub closes: Vec<String>,
    /// Locations, by source, that wait on more than the area map can see: the
    /// same, for one location rather than an area.
    #[serde(default)]
    pub uncertain_sources: Vec<String>,
    /// Items that never move, by name: what the game asks for before any
    /// other item can be reached.
    #[serde(default)]
    pub fixed: Vec<String>,
    /// Merchants given a bell bearing in talk.
    #[serde(default, rename = "bearing")]
    pub bearings: Vec<GivenBearing>,
    /// Where the bearings are handed over: their rows also ask for it.
    #[serde(default)]
    pub bearing_area: Option<String>,
    /// Items a seed makes that the game does not have (Messmer's Kindling's shards):
    /// asked for, though no location of the model gives them.
    #[serde(skip)]
    pub made: Vec<String>,
}

/// Elden Ring's graph.
pub fn elden_ring() -> Graph {
    toml::from_str(ELDEN_RING).expect("data/elden_ring.toml parses")
}

impl Graph {
    /// Leyndell's gate (`altus` to `leyndell`) asks for `n` of its Great Runes, the seed's
    /// option; at 0 it asks nothing. The data file holds the runes and the game's 2.
    pub fn set_leyndell_runes(&mut self, n: usize) -> Result<(), String> {
        let edge = self
            .edges
            .iter_mut()
            .find(|e| e.from == "altus" && e.to == "leyndell")
            .ok_or("the area map has no edge from altus to leyndell")?;
        let any = edge
            .need
            .any
            .as_mut()
            .ok_or("the altus to leyndell edge asks for no runes")?;
        if n > any.items.len() {
            return Err(format!(
                "Leyndell's gate names {} Great Runes, fewer than {n}",
                any.items.len()
            ));
        }
        if n == 0 {
            edge.need.any = None;
        } else {
            any.n = n;
        }
        Ok(())
    }
}

/// Where a DLC Start begins, and the boss that ends it.
pub const DLC_START: &str = "gravesite_plain";
pub const DLC_GOAL: &str = "Radahn, Consort of Miquella";

impl Graph {
    /// DLC Start (spec `dlc_start`, Nova, 2026-11-06): the seed begins at the Gravesite
    /// Plain and ends with the DLC's last boss. The way between Mohg's arena and the
    /// Gravesite Plain is cut, so nothing of the base game counts as reached and every
    /// key item a DLC seed needs lands in the DLC.
    pub fn set_dlc_start(&mut self, on: bool) -> Result<(), String> {
        if !on {
            return Ok(());
        }
        let before = self.edges.len();
        self.edges.retain(|e| {
            !(e.from == "mohgwyn_palace" && e.to == DLC_START
                || e.from == DLC_START && e.to == "mohgwyn_palace")
        });
        if self.edges.len() == before {
            return Err("the area map has no way from Mohg's arena into the DLC".into());
        }
        self.start = DLC_START.to_string();
        self.goal = Need {
            bosses: vec![DLC_GOAL.to_string()],
            ..Need::default()
        };
        Ok(())
    }
}

/// Lamenter's Gaol's keys, as the data file holds them fixed.
pub const GAOL_KEYS: [&str; 2] = ["Gaol Upper Level Key", "Gaol Lower Level Key"];

impl Graph {
    /// Whether Lamenter's Gaol's keys join the shuffle (spec `gaol_keys`, Nova,
    /// 2026-11-06). The data file keeps them where the game has them; shuffled, they
    /// leave the fixed items and the way into the gaol asks for both.
    pub fn set_gaol_keys(&mut self, shuffled: bool) -> Result<(), String> {
        if !shuffled {
            return Ok(());
        }
        self.fixed.retain(|f| !GAOL_KEYS.contains(&f.as_str()));
        let edge = self
            .edges
            .iter_mut()
            .find(|e| e.from == "charos_hidden_grave" && e.to == "lamenters_gaol")
            .ok_or("the area map has no way into Lamenter's Gaol")?;
        edge.need
            .items
            .extend(GAOL_KEYS.iter().map(|k| k.to_string()));
        Ok(())
    }
}

impl Graph {
    /// Messmer's Kindling in shards (spec `kindling_shards`): every need of the Kindling
    /// becomes a need of `needed` of `shards`. A need that already counts something
    /// else is refused, as one need holds one count.
    pub fn set_kindling_shards(&mut self, shards: &[String], needed: usize) -> Result<(), String> {
        const KINDLING: &str = "Messmer's Kindling";
        if shards.len() <= 1 {
            return Ok(());
        }
        if needed == 0 || needed > shards.len() {
            return Err(format!(
                "the Sealing Tree asks for 1 to {} shards, not {needed}",
                shards.len()
            ));
        }
        let mut found = 0;
        let needs = self
            .edges
            .iter_mut()
            .map(|e| &mut e.need)
            .chain(self.places.iter_mut().map(|p| &mut p.need))
            .chain(self.on_sale.iter_mut().map(|o| &mut o.need))
            .chain([&mut self.goal]);
        for need in needs {
            if !need.items.iter().any(|i| i == KINDLING) {
                continue;
            }
            if need.any.is_some() {
                return Err("a need of Messmer's Kindling already counts other items".into());
            }
            need.items.retain(|i| i != KINDLING);
            self.made.extend(shards.iter().cloned());
            need.any = Some(AnyOf {
                n: needed,
                items: shards.to_vec(),
                bosses: vec![],
            });
            found += 1;
        }
        if found == 0 {
            return Err("nothing in the area map asks for Messmer's Kindling".into());
        }
        Ok(())
    }
}

/// What getting a location asks besides reaching it: all of it.
#[derive(Debug, Clone, Default)]
pub struct Terms<'a> {
    /// A `place` or `on_sale` entry's need.
    pub needs: Vec<&'a Need>,
    /// Items to hold: a shop's currency and its material cost. How many is
    /// not counted.
    pub items: Vec<&'a str>,
    /// Items of which one is enough: the ones whose handing over puts a
    /// shop row on sale.
    pub any_of: Vec<&'a str>,
    /// Bosses to have beaten, by index in the model.
    pub bosses: Vec<usize>,
    /// For a shop row whose on-sale flag nothing explains: never got.
    pub unexplained: Option<u32>,
    /// For a shop row whose `on_sale` entry says it can be lost: no key item.
    pub missable: bool,
    /// Areas to have reached as well: for a row of a merchant given a bell
    /// bearing, where it is handed over, as the merchant may be gone by then.
    pub areas: Vec<usize>,
    /// For a gift, the ways it is given, any one enough; `None` when its
    /// talk asks nothing (or a `place` entry speaks for it).
    pub ways: Option<Vec<Vec<Req<'a>>>>,
}

/// One thing a gift's way asks.
#[derive(Debug, Clone, PartialEq)]
pub enum Req<'a> {
    /// A boss beaten, by index in the model.
    Boss(usize),
    Holds(&'a str),
    /// Any of these locations got (the ones whose pick-up sets one flag).
    Got(Vec<usize>),
    /// An area reached, by index.
    Area(usize),
    /// A flag nothing explains: never.
    Never(u32),
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
    /// What getting each location asks besides reaching its area.
    pub location_terms: Vec<Terms<'a>>,
    /// Locations an `unused` entry rules out.
    pub location_unused: Vec<bool>,
    /// Locations another location's way waits on being got.
    pub waited_on: Vec<bool>,
    /// For each boss, its area.
    pub boss_area: Vec<Option<usize>>,
    /// What the draft names that the model does not have, and other faults.
    pub problems: Vec<String>,
    /// Areas' map prefixes, and the graces placed in areas, for [`Bound::area_at`].
    maps_of: Vec<(String, usize)>,
    placed_graces: Vec<(usize, MapId, [f32; 3])>,
}

/// The area whose map prefix is the longest match.
fn area_by_map(maps_of: &[(String, usize)], map: Option<MapId>) -> Option<usize> {
    maps_of
        .iter()
        .filter(|(spec, _)| matches_map(spec, map))
        .max_by_key(|(spec, _)| spec.len())
        .map(|&(_, a)| a)
}

/// The area of the nearest grace: in the open worlds, anywhere on the same world; else
/// in the same map, or with no position, the area most of the map's graces are in.
fn area_by_grace(
    placed_graces: &[(usize, MapId, [f32; 3])],
    map: Option<MapId>,
    pos: Option<[f32; 3]>,
) -> Option<usize> {
    let map = map?;
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
        None => {
            let mut count: BTreeMap<usize, usize> = BTreeMap::new();
            for &&(a, _, _) in &here {
                *count.entry(a).or_default() += 1;
            }
            count.into_iter().max_by_key(|&(_, n)| n).map(|(a, _)| a)
        }
    }
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
        for a in graph.no_return.iter().chain(&graph.closes) {
            area(a, &mut problems);
        }
        for name in &graph.fixed {
            let found = model
                .locations
                .iter()
                .flat_map(|l| &l.items)
                .any(|i| i.name.as_deref() == Some(name));
            if !found {
                problems.push(format!("no item {name}"));
            }
        }
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
        needs.extend(graph.on_sale.iter().map(|o| &o.need));
        for n in &needs {
            for item in n.items.iter().chain(n.any.iter().flat_map(|a| &a.items)) {
                if !item_names.contains(item.as_str()) && !graph.made.contains(item) {
                    problems.push(format!("no item {item} anywhere in the model"));
                }
            }
            for b in n.bosses.iter().chain(n.any.iter().flat_map(|a| &a.bosses)) {
                if let Err(p) = find_boss(model, b) {
                    problems.push(p);
                }
            }
        }
        for (k, item) in &graph.currency {
            if k.parse::<u8>().is_err() {
                problems.push(format!("currency {k} is not a number"));
            }
            if !item_names.contains(item.as_str()) {
                problems.push(format!(
                    "currency {k}: no item {item} anywhere in the model"
                ));
            }
        }
        let flags_used: BTreeSet<u32> = model
            .locations
            .iter()
            .filter_map(|l| l.terms.as_ref()?.on_sale)
            .collect();
        for o in &graph.on_sale {
            if !flags_used.contains(&o.flag) {
                problems.push(format!("on_sale {}: no shop row waits on it", o.flag));
            }
            if graph.on_sale.iter().filter(|p| p.flag == o.flag).count() > 1 {
                problems.push(format!("on_sale {} is listed twice", o.flag));
            }
        }

        // Where each location is.
        let maps_of: Vec<(String, usize)> = graph
            .areas
            .iter()
            .enumerate()
            .flat_map(|(i, a)| a.maps.iter().map(move |m| (m.clone(), i)))
            .collect();
        let placed_graces: Vec<(usize, MapId, [f32; 3])> = model
            .graces
            .iter()
            .enumerate()
            .filter_map(|(i, g)| Some((grace_area[i]?, g.map?, g.position?)))
            .collect();
        let by_map = |map: Option<MapId>| area_by_map(&maps_of, map);
        // In the open worlds, the nearest grace on the same world; a location with no
        // position stands at its tile's centre.
        let by_grace =
            |map: Option<MapId>, pos: Option<[f32; 3]>| area_by_grace(&placed_graces, map, pos);
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
        let location_unused: Vec<bool> = model
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
        // Rows a bell bearing given in talk lets the Husks sell, and where.
        let bearing_area = graph
            .bearing_area
            .as_deref()
            .and_then(|a| area(a, &mut problems));
        if !graph.bearings.is_empty() && graph.bearing_area.is_none() {
            problems.push("bearings are given, but bearing_area does not say where".into());
        }
        let mut covered: Vec<(i32, i32)> = Vec::new();
        let mut unmet: Vec<(i32, i32)> = Vec::new();
        for (i, b) in graph.bearings.iter().enumerate() {
            match model
                .bearings
                .iter()
                .find(|m| m.name.as_deref() == Some(&b.item))
            {
                Some(m) => {
                    covered.extend(&m.ranges);
                    if b.unmet {
                        unmet.extend(&m.ranges);
                    }
                }
                None => problems.push(format!(
                    "bearing {}: no talk script takes {}",
                    b.merchant, b.item
                )),
            }
            if graph.bearings[..i].iter().any(|o| o.given == b.given) {
                problems.push(format!(
                    "bearing {}: flag {} given twice",
                    b.merchant, b.given
                ));
            }
        }
        let mut location_area = Vec::with_capacity(model.locations.len());
        let mut location_also = Vec::with_capacity(model.locations.len());
        let mut location_terms = Vec::with_capacity(model.locations.len());
        let mut flag_locations: HashMap<u32, Vec<usize>> = HashMap::new();
        for (i, l) in model.locations.iter().enumerate() {
            if let Some(f) = l.flag {
                flag_locations.entry(f).or_default().push(i);
            }
        }
        let boss_index: HashMap<i32, usize> = model
            .bosses
            .iter()
            .enumerate()
            .map(|(i, b)| (b.area, i))
            .collect();
        for (l, &unused) in model.locations.iter().zip(&location_unused) {
            let pos = l.position.or_else(|| enemy_at.get(&l.entity?).copied());
            let placed = place_of(l);
            let mut terms = Terms::default();
            terms.needs.extend(placed.map(|p| &p.need));
            let reqs = |ways: &'a [gamedata::model::Way]| -> Vec<Vec<Req<'a>>> {
                ways.iter()
                    .map(|w| {
                        w.iter()
                            .map(|c| {
                                Self::req(c, &boss_index, &flag_locations, &|m| {
                                    by_map(Some(m)).or_else(|| by_grace(Some(m), None))
                                })
                            })
                            .collect()
                    })
                    .collect()
            };
            if let Some(ways) = l.given_if.as_ref().filter(|_| placed.is_none()) {
                terms.ways = Some(reqs(ways));
            }
            if let Some(t) = l.terms.as_ref().filter(|_| !unused) {
                Self::shop_terms(graph, t, &boss_index, &mut terms, &mut problems);
                // What events and talk ask, where nothing above explains the flag.
                if terms.unexplained.is_some()
                    && let Some(ways) = &t.ways
                {
                    terms.unexplained = None;
                    terms.ways = Some(reqs(ways));
                }
            }
            if let (Some(a), Some(row)) = (bearing_area, l.shop_row())
                && covered.iter().any(|(f, t)| (*f..=*t).contains(&row))
            {
                terms.areas.push(a);
            }
            if let Some(row) = l.shop_row()
                && unmet.iter().any(|(f, t)| (*f..=*t).contains(&row))
            {
                terms.missable = true;
            }
            location_terms.push(terms);
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
        let mut boss_area: Vec<Option<usize>> = model
            .bosses
            .iter()
            .map(|b| {
                let pos = b.entity.and_then(|e| enemy_at.get(&e).copied());
                by_map(b.map).or_else(|| by_grace(b.map, pos))
            })
            .collect();
        // Fog gate rooms: their bosses, what those bosses give, and their arenas kept
        // free of key items.
        for (ai, a) in graph.areas.iter().enumerate() {
            for &flag in &a.boss_flags {
                let mut found = false;
                for (bi, b) in model.bosses.iter().enumerate() {
                    if b.defeat_flag != flag {
                        continue;
                    }
                    found = true;
                    boss_area[bi] = Some(ai);
                    if let Some(e) = b.entity {
                        for (li, l) in model.locations.iter().enumerate() {
                            if l.entity == Some(e) {
                                location_area[li] = Some(ai);
                            }
                        }
                    }
                }
                if !found {
                    problems.push(format!("{}: no boss with defeat flag {flag}", a.id));
                }
            }
            for k in &a.keys_kept_out {
                for (li, l) in model.locations.iter().enumerate() {
                    let pos = l.position.or_else(|| enemy_at.get(&l.entity?).copied());
                    if let Some(p) = pos
                        && matches_map(&k.map, l.map)
                        && distance([p[0], p[2]], [k.position[0], k.position[2]]) <= k.radius
                        && location_area[li] != Some(ai)
                    {
                        location_terms[li].missable = true;
                    }
                }
            }
        }
        let mut waited_on = vec![false; model.locations.len()];
        for t in &location_terms {
            for r in t.ways.iter().flatten().flatten() {
                if let Req::Got(ls) = r {
                    for &l in ls {
                        waited_on[l] = true;
                    }
                }
            }
        }
        Bound {
            graph,
            model,
            waited_on,
            location_area,
            location_also,
            location_terms,
            location_unused,
            boss_area,
            problems,
            maps_of,
            placed_graces,
        }
    }

    fn req(
        c: &'a gamedata::model::Cond,
        boss_index: &HashMap<i32, usize>,
        flag_locations: &HashMap<u32, Vec<usize>>,
        map_area: &dyn Fn(MapId) -> Option<usize>,
    ) -> Req<'a> {
        use gamedata::model::Cond;
        match c {
            Cond::Boss(a) => boss_index.get(a).map_or(Req::Never(0), |&b| Req::Boss(b)),
            Cond::Holds(i) => i.name.as_deref().map_or(Req::Never(0), Req::Holds),
            Cond::Got(f) => Req::Got(flag_locations.get(f).cloned().unwrap_or_default()),
            Cond::Map(m) => map_area(*m).map_or(Req::Never(0), Req::Area),
            Cond::Flag(f) => Req::Never(*f),
        }
    }

    /// What a shop row's terms ask, in the graph's words.
    fn shop_terms(
        graph: &'a Graph,
        t: &'a gamedata::model::Terms,
        boss_index: &HashMap<i32, usize>,
        terms: &mut Terms<'a>,
        problems: &mut Vec<String>,
    ) {
        if t.currency != 0 {
            match graph.currency.get(&t.currency.to_string()) {
                Some(item) => terms.items.push(item),
                None => {
                    let p = format!("no item named for shop currency {}", t.currency);
                    if !problems.contains(&p) {
                        problems.push(p);
                    }
                }
            }
        }
        terms
            .items
            .extend(t.cost.iter().filter_map(|i| i.name.as_deref()));
        let Some(flag) = t.on_sale else {
            return;
        };
        if let Some(b) = t.after_boss.and_then(|a| boss_index.get(&a)) {
            terms.bosses.push(*b);
        } else if !t.handed.is_empty() {
            terms
                .any_of
                .extend(t.handed.iter().filter_map(|i| i.name.as_deref()));
        } else if let Some(o) = graph.on_sale.iter().find(|o| o.flag == flag) {
            terms.needs.push(&o.need);
            terms.missable = o.missable;
        } else {
            terms.unexplained = Some(flag);
        }
    }

    /// Walk the graph with the items where `gives` says they are (the vanilla
    /// game: [`Bound::vanilla`]), from the start, until nothing new opens.
    pub fn walk(&self, gives: &[Vec<&str>]) -> Walk {
        self.walk_holding(gives, &[])
    }

    /// The same walk, starting with `held` already in hand: the randomizer
    /// assumes the key items it has yet to place.
    ///
    /// Each round visits only the locations that give something or that
    /// another location waits on; the rest are taken in one pass at the end,
    /// as taking them can open nothing.
    pub fn walk_holding<'s>(&self, gives: &[Vec<&'s str>], held: &[&'s str]) -> Walk {
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
        let mut items: BTreeSet<&str> = held.iter().copied().collect();
        let mut taken = vec![false; gives.len()];
        let relevant: Vec<usize> = (0..gives.len())
            .filter(|&i| !gives[i].is_empty() || self.waited_on[i])
            .collect();
        let take = |i: usize,
                    items: &mut BTreeSet<&'s str>,
                    reached: &[bool],
                    taken: &mut Vec<bool>|
         -> bool {
            if taken[i] || self.location_unused[i] {
                return false;
            }
            let here = self.location_area[i].is_none_or(|a| reached[a])
                || self.location_also[i].iter().any(|&a| reached[a]);
            if !here || !self.allows(&self.location_terms[i], items, reached, taken) {
                return false;
            }
            taken[i] = true;
            // Every item, not just up to the first new one.
            for it in &gives[i] {
                items.insert(it);
            }
            true
        };
        loop {
            // Collect what the reached areas give, and anywhere's. Taking a
            // location is a change: another may wait on its being got.
            let mut changed = false;
            for &i in &relevant {
                changed |= take(i, &mut items, &reached, &mut taken);
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
        for i in 0..gives.len() {
            take(i, &mut items, &reached, &mut taken);
        }
        Walk {
            goal: self.met(&self.graph.goal, &items, &reached),
            order,
            reached,
            taken,
            items: items.into_iter().map(String::from).collect(),
        }
    }

    /// Every item some requirement asks for: the key items, which the
    /// randomizer places with care.
    pub fn key_items(&self) -> BTreeSet<&'a str> {
        let g = self.graph;
        let needs = g
            .edges
            .iter()
            .map(|e| &e.need)
            .chain([&g.goal])
            .chain(g.places.iter().map(|p| &p.need))
            .chain(g.on_sale.iter().map(|o| &o.need));
        let mut keys: BTreeSet<&str> = needs
            .flat_map(|n| n.items.iter().chain(n.any.iter().flat_map(|a| &a.items)))
            .map(String::as_str)
            .collect();
        keys.extend(g.currency.values().map(String::as_str));
        for t in &self.location_terms {
            keys.extend(t.items.iter().chain(&t.any_of).copied());
            for r in t.ways.iter().flatten().flatten() {
                if let Req::Holds(i) = r {
                    keys.insert(i);
                }
            }
        }
        keys
    }

    /// Whether a location may be missed, so no key item goes there: the model
    /// says so (a quest stage, a gift either-or), or the `on_sale` entry its
    /// shop row waits on does.
    pub fn missable(&self, location: usize) -> bool {
        self.model.locations[location].missable || self.location_terms[location].missable
    }

    /// Whether a location lies past a step the game cannot take back.
    pub fn past_no_return(&self, location: usize) -> bool {
        self.location_area[location]
            .is_some_and(|a| self.graph.no_return.contains(&self.graph.areas[a].id))
    }

    /// Whether a location lies somewhere that can shut for good while the game
    /// goes on, so nothing that matters can be counted on there.
    pub fn may_close(&self, location: usize) -> bool {
        self.location_area[location]
            .is_some_and(|a| self.graph.closes.contains(&self.graph.areas[a].id))
            || self
                .graph
                .uncertain_sources
                .contains(&self.model.locations[location].source)
    }

    /// What each location gives in the game as it is.
    pub fn vanilla(&self) -> Vec<Vec<&str>> {
        self.model
            .locations
            .iter()
            .map(|l| {
                if matches!(l.kind, Kind::Starting | Kind::Keepsake) {
                    // One class, not all ten: key items never start in them.
                    // Nor is a keepsake counted on: "None" is a choice.
                    return Vec::new();
                }
                l.items.iter().filter_map(|i| i.name.as_deref()).collect()
            })
            .collect()
    }

    fn allows(&self, t: &Terms, items: &BTreeSet<&str>, reached: &[bool], taken: &[bool]) -> bool {
        let boss = |b: usize| self.boss_area[b].is_some_and(|a| reached[a]);
        let open = |r: &Req| match r {
            Req::Boss(b) => boss(*b),
            Req::Holds(i) => items.contains(i),
            Req::Got(ls) => ls.iter().any(|&l| taken[l]),
            Req::Area(a) => reached[*a],
            Req::Never(_) => false,
        };
        t.unexplained.is_none()
            && t.ways
                .as_ref()
                .is_none_or(|ways| ways.iter().any(|w| w.iter().all(open)))
            && t.needs.iter().all(|n| self.met(n, items, reached))
            && t.items.iter().all(|i| items.contains(i))
            && (t.any_of.is_empty() || t.any_of.iter().any(|i| items.contains(i)))
            && t.bosses.iter().all(|&b| boss(b))
            && t.areas.iter().all(|&a| reached[a])
    }

    fn met(&self, need: &Need, items: &BTreeSet<&str>, reached: &[bool]) -> bool {
        let beaten = |b: &String| {
            find_boss(self.model, b)
                .ok()
                .and_then(|i| self.boss_area[i])
                .is_some_and(|a| reached[a])
        };
        need.items.iter().all(|i| items.contains(i.as_str()))
            && need.bosses.iter().all(beaten)
            && need.any.as_ref().is_none_or(|any| {
                any.items
                    .iter()
                    .filter(|i| items.contains(i.as_str()))
                    .count()
                    + any.bosses.iter().filter(|b| beaten(b)).count()
                    >= any.n
            })
    }
}

/// A boss by `name` or `name@map`, which must pick out exactly one.
impl Bound<'_> {
    /// The area a spot of a map is in, as a location there would be: by the map's
    /// longest prefix an area lists, else the nearest grace.
    pub fn area_at(&self, map: MapId, position: [f32; 3]) -> Option<usize> {
        area_by_map(&self.maps_of, Some(map))
            .or_else(|| area_by_grace(&self.placed_graces, Some(map), Some(position)))
    }
}

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
    /// For each location, whether the walk got it.
    pub taken: Vec<bool>,
    /// Areas in the order they opened.
    pub order: Vec<usize>,
    pub items: Vec<String>,
    pub goal: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaol_keys_stay_unless_shuffled_and_then_open_the_gaol() {
        let g = elden_ring();
        for k in GAOL_KEYS {
            assert!(g.fixed.iter().any(|f| f == k), "{k} fixed by default");
        }
        let way = |g: &Graph| {
            g.edges
                .iter()
                .find(|e| e.from == "charos_hidden_grave" && e.to == "lamenters_gaol")
                .unwrap()
                .need
                .items
                .clone()
        };
        assert!(way(&g).is_empty());
        let mut same = g.clone();
        same.set_gaol_keys(false).unwrap();
        assert_eq!(same.fixed, g.fixed);
        let mut moved = g.clone();
        moved.set_gaol_keys(true).unwrap();
        assert!(
            GAOL_KEYS
                .iter()
                .all(|k| !moved.fixed.iter().any(|f| f == k))
        );
        assert_eq!(way(&moved), GAOL_KEYS);
    }
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
            terms: None,
            given_if: None,
            missable: false,
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
    fn a_location_gives_every_item_even_when_the_first_is_held() {
        let (g, mut m) = (graph(), model());
        // The cave's key comes second, after an item already held.
        m.locations[0].items.insert(0, item("Prize"));
        m.locations
            .push(at(MapId([60, 40, 37, 0]), [0.0; 3], &["Prize"]));
        let b = Bound::new(&g, &m);
        assert!(b.walk(&b.vanilla()).goal);
    }

    /// The key sold in the first area, on the terms given.
    fn shop(terms: gamedata::model::Terms) -> (Graph, Model) {
        let (mut g, mut m) = (graph(), model());
        m.locations[0].kind = Kind::Shop;
        m.locations[0].source = "ShopLineupParam 100000".into();
        m.locations[0].terms = Some(terms);
        // A boss of the first area, and a heart to pay with.
        m.bosses.push(Boss {
            area: 2,
            map: Some(MapId([60, 40, 37, 0])),
            entity: None,
            name: Some("Warden".into()),
            defeat_flag: 2,
        });
        m.locations
            .push(at(MapId([60, 40, 37, 0]), [0.0; 3], &["Heart"]));
        g.currency.insert("1".into(), "Heart".into());
        (g, m)
    }

    fn opens_cave(g: &Graph, m: &Model, gives_heart: bool) -> bool {
        let b = Bound::new(g, m);
        assert!(b.problems.is_empty(), "{:?}", b.problems);
        let mut gives = b.vanilla();
        if !gives_heart {
            gives[2].clear();
        }
        b.walk(&gives).goal
    }

    #[test]
    fn a_row_of_a_merchant_given_a_bearing_asks_for_where_it_is_handed_over() {
        let (mut g, mut m) = shop(Default::default());
        m.bearings.push(gamedata::model::Bearing {
            goods: 8919,
            name: Some("Thops's Bell Bearing".into()),
            script: "t600001110".into(),
            flag: 11_109_719,
            ranges: vec![(100_000, 100_024)],
        });
        g.bearings.push(GivenBearing {
            merchant: "Thops".into(),
            item: "Thops's Bell Bearing".into(),
            given: 11_109_825,
            unmet: false,
            note: String::new(),
        });
        // Handed over past the door the row's key opens: the key is never got.
        g.bearing_area = Some("cave".into());
        let b = Bound::new(&g, &m);
        assert!(b.problems.is_empty(), "{:?}", b.problems);
        assert_eq!(b.location_terms[0].areas, vec![2]);
        assert!(!b.walk(&b.vanilla()).goal);
        // Handed over on the way: as before.
        g.bearing_area = Some("east".into());
        assert!(opens_cave(&g, &m, true));
        // A row outside the bearing's ranges asks nothing more.
        m.locations[0].source = "ShopLineupParam 100025".into();
        assert!(Bound::new(&g, &m).location_terms[0].areas.is_empty());
        // A bearing no script takes, or one given twice, is a problem.
        g.bearings[0].item = "Nobody's Bell Bearing".into();
        g.bearings.push(g.bearings[0].clone());
        let p = Bound::new(&g, &m).problems;
        assert!(p.iter().any(|p| p.contains("Nobody's")), "{p:?}");
        assert!(p.iter().any(|p| p.contains("given twice")), "{p:?}");
        g.bearing_area = None;
        let p = Bound::new(&g, &m).problems;
        assert!(p.iter().any(|p| p.contains("bearing_area")), "{p:?}");
    }

    #[test]
    fn a_shop_row_asks_for_its_currency_and_costs() {
        let terms = gamedata::model::Terms {
            currency: 1,
            ..Default::default()
        };
        let (g, mut m) = shop(terms);
        assert!(opens_cave(&g, &m, true));
        assert!(!opens_cave(&g, &m, false), "bought with no heart");
        m.locations[0].terms = Some(gamedata::model::Terms {
            cost: vec![item("Heart")],
            ..Default::default()
        });
        assert!(opens_cave(&g, &m, true));
        assert!(!opens_cave(&g, &m, false), "traded with no heart");
    }

    #[test]
    fn a_shop_row_waits_for_what_puts_it_on_sale() {
        let on_sale = |after_boss, handed: &[&str]| gamedata::model::Terms {
            on_sale: Some(7),
            after_boss,
            handed: handed.iter().map(|n| item(n)).collect(),
            ..Default::default()
        };
        // After a boss of the area: on sale.
        let (g, m) = shop(on_sale(Some(2), &[]));
        assert!(opens_cave(&g, &m, false));
        // After the cave's own boss: never, as the cave needs the key.
        let (g, m) = shop(on_sale(Some(1), &[]));
        assert!(!opens_cave(&g, &m, false));
        // After handing over a heart.
        let (g, m) = shop(on_sale(None, &["Heart"]));
        assert!(opens_cave(&g, &m, true));
        assert!(!opens_cave(&g, &m, false));
        // A flag nothing explains: never; one the graph explains: as it says.
        let (mut g, m) = shop(on_sale(None, &[]));
        assert!(!opens_cave(&g, &m, false));
        g.on_sale.push(OnSale {
            flag: 7,
            need: Need {
                bosses: vec!["Warden".into()],
                ..Default::default()
            },
            missable: false,
            sure: true,
            note: String::new(),
        });
        assert!(opens_cave(&g, &m, false));
        assert!(!Bound::new(&g, &m).missable(0));
        g.on_sale[0].missable = true;
        assert!(
            Bound::new(&g, &m).missable(0),
            "the entry says it can be lost"
        );
    }

    #[test]
    fn a_shop_row_waits_for_the_ways_events_and_talk_turn_its_flag_on() {
        use gamedata::model::Cond;
        let ways = |ways: Vec<Vec<Cond>>| gamedata::model::Terms {
            on_sale: Some(7),
            ways: Some(ways),
            ..Default::default()
        };
        // On holding the heart, which the cave's key needs anyway.
        let (g, m) = shop(ways(vec![vec![Cond::Holds(item("Heart"))]]));
        assert!(opens_cave(&g, &m, true));
        assert!(!opens_cave(&g, &m, false));
        // After the boss of the area.
        let (g, m) = shop(ways(vec![vec![Cond::Boss(2)]]));
        assert!(opens_cave(&g, &m, false));
        // On a flag nothing explains: never.
        let (g, m) = shop(ways(vec![vec![Cond::Flag(9)]]));
        assert!(!opens_cave(&g, &m, false));
        // An `on_sale` entry speaks for the flag instead.
        let (mut g, m) = shop(ways(vec![vec![Cond::Flag(9)]]));
        g.on_sale.push(OnSale {
            flag: 7,
            need: Need::default(),
            missable: false,
            sure: true,
            note: String::new(),
        });
        assert!(opens_cave(&g, &m, false));
    }

    #[test]
    fn a_gift_is_given_by_any_one_of_its_ways() {
        use gamedata::model::Cond;
        let (mut g, mut m) = (graph(), model());
        m.locations[0].kind = Kind::Gift;
        m.bosses.push(Boss {
            area: 2,
            map: Some(MapId([60, 40, 37, 0])),
            entity: None,
            name: Some("Warden".into()),
            defeat_flag: 2,
        });
        let walk = |g: &Graph, m: &Model| {
            let b = Bound::new(g, m);
            assert!(b.problems.is_empty(), "{:?}", b.problems);
            b.walk(&b.vanilla()).goal
        };
        // A way through the cave's own boss, which needs the key, is shut;
        // one through the first area's boss is open.
        m.locations[0].given_if = Some(vec![vec![Cond::Boss(1)]]);
        assert!(!walk(&g, &m));
        m.locations[0].given_if = Some(vec![vec![Cond::Boss(1)], vec![Cond::Boss(2)]]);
        assert!(walk(&g, &m));
        // A flag nothing explains shuts its way.
        m.locations[0].given_if = Some(vec![vec![Cond::Boss(2), Cond::Flag(9)]]);
        assert!(!walk(&g, &m));
        // Being in a map: the cave's, which needs the key, or the first
        // area's.
        m.locations[0].given_if = Some(vec![vec![Cond::Map(MapId([31, 0, 0, 0]))]]);
        assert!(!walk(&g, &m));
        m.locations[0].given_if = Some(vec![vec![Cond::Map(MapId([60, 40, 37, 0]))]]);
        assert!(walk(&g, &m));
        // Another location got: the prize's pick-up flag.
        let prize = m.locations.len();
        m.locations.push(at(MapId([60, 40, 37, 0]), [0.0; 3], &[]));
        m.locations[prize].flag = Some(77);
        m.locations[0].given_if = Some(vec![vec![Cond::Got(77)]]);
        assert!(walk(&g, &m));
        // A place entry speaks for the gift instead.
        m.locations[0].given_if = Some(vec![vec![Cond::Flag(9)]]);
        m.locations[0].items = vec![item("Key")];
        g.places.push(Place {
            item: "Key".into(),
            kind: Kind::Gift,
            area: "west".into(),
            need: Need::default(),
            sure: true,
            note: String::new(),
        });
        assert!(walk(&g, &m));
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

    #[test]
    fn leyndell_asks_the_seeds_number_of_runes() {
        let gate = |g: &Graph| {
            g.edges
                .iter()
                .find(|e| e.from == "altus" && e.to == "leyndell")
                .unwrap()
                .need
                .any
                .as_ref()
                .map(|a| (a.n, a.items.len()))
        };
        assert_eq!(gate(&elden_ring()), Some((2, 6)));
        for n in 1..=6 {
            let mut g = elden_ring();
            g.set_leyndell_runes(n).unwrap();
            assert_eq!(gate(&g), Some((n, 6)));
        }
        let mut g = elden_ring();
        g.set_leyndell_runes(0).unwrap();
        assert_eq!(gate(&g), None);
        assert!(elden_ring().set_leyndell_runes(7).is_err());
    }
}
