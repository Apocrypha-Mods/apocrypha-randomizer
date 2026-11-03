//! The game model's types. They say nothing about how a particular game
//! stores them; `elden_ring` fills them in from Elden Ring's files.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::MapId;

/// Where an item can be got.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Lying in a map, picked up once.
    Treasure,
    /// Dropped by an enemy, once (the lot carries a flag).
    EnemyDrop,
    /// Dropped by an enemy every time it rolls (no flag): farmable.
    RepeatDrop,
    /// Picked from an asset in a map (a plant, a node); those with no flag
    /// grow back. Counted once per map, however many there are.
    Gather,
    /// Given by an event script: a boss reward, a chest, a restored rune.
    Event,
    /// Given by an NPC in conversation (a talk script).
    Gift,
    /// Given by a script for a particular boss.
    BossReward,
    /// Sold by a merchant.
    Shop,
    /// In a class's starting equipment.
    Starting,
    /// A gift chosen at character creation.
    Keepsake,
}

impl Kind {
    pub const ALL: [Kind; 10] = [
        Kind::Treasure,
        Kind::EnemyDrop,
        Kind::RepeatDrop,
        Kind::Gather,
        Kind::Event,
        Kind::Gift,
        Kind::BossReward,
        Kind::Shop,
        Kind::Starting,
        Kind::Keepsake,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Treasure => "treasure",
            Kind::EnemyDrop => "enemy drop",
            Kind::RepeatDrop => "repeat drop",
            Kind::Gather => "gather",
            Kind::Event => "event",
            Kind::Gift => "gift",
            Kind::BossReward => "boss reward",
            Kind::Shop => "shop",
            Kind::Starting => "starting",
            Kind::Keepsake => "keepsake",
        }
    }
}

/// What kind of thing an item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Goods,
    Weapon,
    Protector,
    Accessory,
    Gem,
    /// A weapon with a preset affinity and level, by its own table's row.
    CustomWeapon,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    pub kind: ItemKind,
    pub id: i32,
    pub count: u32,
    /// Chance out of 1000; 1000 for a sure thing.
    pub chance: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// The item-lot rows a location gives, in the table named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Lot {
    pub table: String,
    pub ids: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Location {
    pub kind: Kind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<MapId>,
    /// What gives it, in words: a part's name, an enemy, a shop row, an event.
    pub source: String,
    /// The map entity it is tied to, when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<[f32; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lot: Option<Lot>,
    pub items: Vec<Item>,
    /// The event flag that records it as got, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flag: Option<u32>,
    /// Other places the same location is offered: a merchant who moves, or
    /// a stock two merchants share. Got at whichever is reached first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<Spot>,
    /// For a shop row, what buying it asks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terms: Option<Terms>,
    /// For a gift, the ways its talk scripts give it, any one of which is
    /// enough; `None` when nothing is asked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub given_if: Option<Vec<Way>>,
    /// A gift that waits on a flag an event can turn off again: an NPC's
    /// quest stage, which the game can move past before the gift is taken. It
    /// may be missed, so no key item is put there. A shop row whose on-sale
    /// flag can be lost the same way, or turned off again, is marked too.
    #[serde(skip_serializing_if = "is_false")]
    pub missable: bool,
}

/// One way something is given: all of it must hold.
pub type Way = Vec<Cond>;

/// One thing a way asks for.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Cond {
    /// A boss beaten, by its GameAreaParam row.
    Boss(i32),
    /// An item held.
    Holds(Item),
    /// Another location got: the flag that picking it up sets.
    Got(u32),
    /// Being in a map: what an event in that map's own script does, it does
    /// while the map is loaded.
    Map(MapId),
    /// An event flag on, which nothing in the model explains.
    Flag(u32),
}

/// What a shop row asks besides reaching its merchant.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Terms {
    pub price: i32,
    /// What the price is paid in: 0 for runes, else a ShopLineupParam
    /// `currency` that the logic names an item for (dragon communion's
    /// Dragon Hearts).
    #[serde(skip_serializing_if = "is_zero")]
    pub currency: u8,
    /// Items spent besides the price: a remembrance, for its trades.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub cost: Vec<Item>,
    /// The flag that puts the row on sale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_sale: Option<u32>,
    /// The boss (its GameAreaParam row) whose defeat turns that flag on, when
    /// the event scripts set the two together.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_boss: Option<i32>,
    /// Items whose handing over in talk turns that flag on, any one of them:
    /// Sellen puts Glintstone Cometshard on sale for the Conspectus Scroll.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub handed: Vec<Item>,
    /// When neither a boss nor a handover explains the flag: the ways events
    /// and talk turn it on, read as a gift's ways are, any one enough. `None`
    /// when nothing sets it, or nothing that sets it was read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ways: Option<Vec<Way>>,
}

fn is_false(v: &bool) -> bool {
    !v
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

/// A place in a map.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Spot {
    pub map: MapId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<[f32; 3]>,
}

impl Location {
    /// For a shop location, its ShopLineupParam row.
    pub fn shop_row(&self) -> Option<i32> {
        if self.kind != Kind::Shop {
            return None;
        }
        let rest = self.source.strip_prefix("ShopLineupParam ")?;
        rest.split(' ').next()?.parse().ok()
    }
}

/// One enemy placed in a map.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Enemy {
    pub map: MapId,
    /// The part's name in the map.
    pub part: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<u32>,
    pub npc_param: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub think: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chara_init: Option<i32>,
    pub position: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Boss {
    /// Its row in the game's boss table.
    pub area: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<MapId>,
    /// The enemy entity the boss is; the same number as `area`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub defeat_flag: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Grace {
    pub id: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<MapId>,
    pub entity: u32,
    pub flag: u32,
    /// Where the grace's asset stands in its map.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<[f32; 3]>,
}

/// Lots that hold items but that nothing in the model gives: what the
/// extraction does not reach yet.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Gaps {
    /// Per lot table, ids of rows with items that nothing references.
    pub unreached_lots: BTreeMap<String, Vec<i32>>,
    /// Treasures whose lot is not a row. The game has nothing to give there;
    /// many of their names say dummy, deleted or test.
    pub treasure_without_lot: usize,
    /// Treasures whose lot is a row with no item in it, nor in the rows it
    /// runs on into. They stay locations, but have no slot to fill.
    pub treasure_empty: usize,
    /// The unreached map lots that carry a flag and give an item with a name,
    /// sorted by what their ids and items say ([`LotSort`]).
    pub flagged_named: BTreeMap<LotSort, Vec<i32>>,
}

/// What an unreached lot is taken to be, from its id and its items. None of
/// these is a slot: nothing the model reads gives them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LotSort {
    /// Gives only tutorial notes (goods type 7: "About Sites of Grace").
    Tutorial,
    /// In the 100000s, the NPCs' range: a quest's or a death's reward whose id
    /// no script, talk script or table holds.
    Npc,
    /// Numbered after a map that is read, giving only items the model gets
    /// elsewhere: leftovers, like the Confessor set's lot in m60_36_43, which
    /// was looked for in play and is not there.
    MapLeftover,
    /// Numbered after a map that is read, with an item the model gets nowhere
    /// else: a source not yet found, or one the game never gives. On 2.7.2, the
    /// "O Mother" gesture's lot alone.
    MapUnknown,
    /// Numbered after a map whose file was not found by name.
    MapNotRead,
    /// Below 100000: old test rows, and items the game gives some other way (the
    /// Flask of Wondrous Physick, the Small Golden Effigy).
    LowId,
    /// The 9000000s, test rows.
    Test,
    Other,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Model {
    /// A name for each map, from the graces in it.
    pub maps: BTreeMap<MapId, Vec<String>>,
    pub locations: Vec<Location>,
    pub enemies: Vec<Enemy>,
    pub bosses: Vec<Boss>,
    pub graces: Vec<Grace>,
    /// What talk scripts take from the player or ask them to hold: the items
    /// quests turn on, whatever the area map says of them.
    pub quest_items: Vec<Item>,
    pub gaps: Gaps,
}

/// An area for counting: `AA_BB` for legacy dungeons and small places, `AA`
/// alone for the two open worlds, whose maps are a grid of tiles.
pub fn area_of(m: MapId) -> String {
    match m.area() {
        60 | 61 => format!("m{:02}", m.area()),
        a => format!("m{a:02}_{:02}", m.0[1]),
    }
}

impl Model {
    /// Locations counted by area and kind. Locations with no map count under
    /// "-".
    pub fn coverage(&self) -> BTreeMap<String, BTreeMap<Kind, usize>> {
        let mut out: BTreeMap<String, BTreeMap<Kind, usize>> = BTreeMap::new();
        for l in &self.locations {
            let area = l.map.map(area_of).unwrap_or_else(|| "-".into());
            *out.entry(area).or_default().entry(l.kind).or_default() += 1;
        }
        out
    }

    /// The grace names of an area's maps, for labelling it.
    pub fn area_names(&self, area: &str) -> Vec<&str> {
        self.maps
            .iter()
            .filter(|(m, _)| area_of(**m) == area)
            .flat_map(|(_, names)| names.iter().map(String::as_str))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(map: Option<MapId>, kind: Kind) -> Location {
        Location {
            kind,
            map,
            source: String::new(),
            entity: None,
            position: None,
            lot: None,
            items: vec![],
            flag: None,
            also: vec![],
            terms: None,
            given_if: None,
            missable: false,
        }
    }

    #[test]
    fn coverage_counts_each_area_and_kind() {
        let castle = MapId([10, 0, 0, 0]);
        let tile = MapId([60, 42, 36, 0]);
        let other_tile = MapId([60, 43, 36, 0]);
        let model = Model {
            locations: vec![
                at(Some(castle), Kind::Treasure),
                at(Some(castle), Kind::Treasure),
                at(Some(tile), Kind::EnemyDrop),
                at(Some(other_tile), Kind::Treasure),
                at(None, Kind::Shop),
            ],
            ..Default::default()
        };
        let c = model.coverage();
        assert_eq!(c["m10_00"][&Kind::Treasure], 2);
        assert_eq!(
            c["m60"][&Kind::Treasure],
            1,
            "open-world tiles count as one area"
        );
        assert_eq!(c["m60"][&Kind::EnemyDrop], 1);
        assert_eq!(c["-"][&Kind::Shop], 1);
    }
}
