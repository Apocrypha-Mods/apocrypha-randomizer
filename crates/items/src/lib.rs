//! Item placement (PLAN.md §5, Phase 4): every item the shuffled locations
//! give is put somewhere else, with the key items (what any requirement of
//! the area map asks for) only where the logic can reach them, so every seed
//! can be finished.
//!
//! - **What moves.** The unit ([`Unit`]) is one place an item sits, of three
//!   kinds ([`Place`]):
//!   - an item slot of an item-lot row: treasure, one-time enemy drops,
//!     script awards, NPC gifts and boss rewards ([`SHUFFLED`]). The pick-up
//!     flag and the slot's chance stay; only the item, its kind and its count
//!     move;
//!   - a shop row: the item moves, while the price and stock stay with the
//!     row. Dragon Communion and other rows paid for in items stay as they
//!     are;
//!   - a class's starting weapon, armour, talisman or spell slot, which takes
//!     only an item of its own sort ([`fits`]) and never a key item.
//!
//!   Gathering, farmable drops and other starting goods (the flasks) stay as
//!   they are.
//! - **The player's sliders** ([`options`]): for each category of item, how
//!   much of it moves (the rest stays where the unmodded game has it) and
//!   how strongly what moves leans towards deep slots ([`depth`]).
//! - **Key items** go first, by assumed fill: each is placed in a reachable
//!   slot of a walk that assumes every key item not yet placed is already
//!   held, drawn with the key-item bias. A key item only goes in a slot that
//!   always gives it (chance 1000) at a location the area map puts in an
//!   area.
//! - **Everything else** that moves fills the slots left, each item drawn
//!   towards slots as deep as it is worth, as strongly as its category's
//!   bias says.
//! - **The check.** The finished placement is walked from the start with
//!   nothing assumed, and the goal must be reached.
//!
//! The same seed gives the same placement on every platform: the generator
//! is our own ([`rng`]) and every collection is walked in a fixed order.

use std::collections::{BTreeSet, HashMap};

use fs_bytes::{Result, format_err};
use fs_param::Regulation;
use fs_param::layout::for_table;
use gamedata::Model;
use gamedata::model::{Item, ItemKind, Kind};
use logic::Bound;

pub mod depth;
pub mod options;
pub mod rng;

pub use options::{Category, Options};
use rng::Rng;

/// How strongly a full bias leans: a key item's weight grows by e^KEY_LEAN
/// from the shallowest slot to the deepest; another item's falls by
/// e^REST_LEAN from a slot as deep as it is worth to the farthest. Our own
/// choice, to be tuned by play. A slider's effect grows with its square, so
/// the low end stays gentle.
const KEY_LEAN: f64 = 6.0;
const REST_LEAN: f64 = 10.0;
/// Depth bands the other items are drawn through, for speed.
const BANDS: usize = 20;

/// The kinds of location whose items are shuffled.
pub const SHUFFLED: [Kind; 5] = [
    Kind::Treasure,
    Kind::EnemyDrop,
    Kind::Event,
    Kind::Gift,
    Kind::BossReward,
];

/// How many times a seed is tried before it is given up on: assumed fill
/// can corner itself, rarely, and a retry draws again from the same seed.
const ATTEMPTS: usize = 20;

/// The classes' starting slots that are put in play: weapons, armour,
/// talismans and spells (CharaInitParam fields).
const STARTING: [(&str, ItemKind); 4] = [
    ("weapon", ItemKind::Weapon),
    ("protector", ItemKind::Protector),
    ("accessory", ItemKind::Accessory),
    ("magic", ItemKind::Goods),
];
/// The ten classes' CharaInitParam rows.
const CLASSES: std::ops::RangeInclusive<i32> = 3000..=3009;

/// What kind of place a unit is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// Slot `slot` of an item-lot row.
    Lot,
    /// A ShopLineupParam row.
    Shop,
    /// Value `slot` of a CharaInitParam field.
    Start(&'static str),
}

/// One place an item sits, at a location.
#[derive(Debug, Clone, PartialEq)]
pub struct Unit {
    /// Index into the model's locations.
    pub location: usize,
    pub place: Place,
    pub table: String,
    pub row: i32,
    pub slot: usize,
    /// The item the game puts here, with the slot's chance (out of 1000).
    pub item: Item,
    /// The category of that item, for the sliders.
    pub category: Category,
}

impl Unit {
    /// Whether picking the location up always gives this slot's item.
    pub fn sure(&self) -> bool {
        self.item.chance == 1000
    }

    /// The slider that decides whether this place's item moves.
    pub fn slider(&self) -> Category {
        match self.place {
            Place::Lot => self.category,
            Place::Shop => Category::Merchant,
            Place::Start(_) => Category::Starting,
        }
    }
}

/// An armour piece's body part: the hundreds digit of its id.
fn body_part(id: i32) -> i32 {
    (id / 100) % 10
}

/// Shadow of the Erdtree's maps: its open world (m61) and its legacy
/// dungeons, catacombs, gaols, forges and pits.
const DLC_MAPS: [u8; 10] = [20, 21, 22, 25, 28, 40, 41, 42, 43, 61];

/// Whether a location is in the DLC, by its map. One with no map (a class's
/// starting gear, a gift with no map) counts as the base game.
pub fn in_dlc(model: &Model, location: usize) -> bool {
    model.locations[location]
        .map
        .is_some_and(|m| DLC_MAPS.contains(&m.area()))
}

/// Arrows and bolts, which are weapons to the game but go in no hand.
fn is_ammo(id: i32) -> bool {
    (50_000_000..54_000_000).contains(&id)
}

/// Whether the item now at unit `item` may go in the place of unit `slot`.
pub fn fits(slot: &Unit, item: &Unit) -> bool {
    let it = &item.item;
    match slot.place {
        Place::Lot => true,
        // Shops sell from the equipment tables only.
        Place::Shop => it.kind != ItemKind::CustomWeapon,
        Place::Start(field) => {
            item.category != Category::Key
                && match field {
                    "weapon" => it.kind == ItemKind::Weapon && !is_ammo(it.id),
                    "protector" => {
                        it.kind == ItemKind::Protector
                            && body_part(it.id) == body_part(slot.item.id)
                    }
                    "accessory" => it.kind == ItemKind::Accessory,
                    "magic" => item.category == Category::Spell,
                    _ => false,
                }
        }
    }
}

/// ShopLineupParam's numbering of item kinds.
fn shop_type(kind: ItemKind) -> Option<i64> {
    Some(match kind {
        ItemKind::Weapon => 0,
        ItemKind::Protector => 1,
        ItemKind::Accessory => 2,
        ItemKind::Goods => 3,
        ItemKind::Gem => 4,
        ItemKind::CustomWeapon => return None,
    })
}

/// ItemLotParam's numbering of item kinds.
fn lot_category(kind: ItemKind) -> i64 {
    match kind {
        ItemKind::Goods => 1,
        ItemKind::Weapon => 2,
        ItemKind::Protector => 3,
        ItemKind::Accessory => 4,
        ItemKind::Gem => 5,
        ItemKind::CustomWeapon => 6,
    }
}

fn kind_of(category: i64) -> Option<ItemKind> {
    Some(match category {
        1 => ItemKind::Goods,
        2 => ItemKind::Weapon,
        3 => ItemKind::Protector,
        4 => ItemKind::Accessory,
        5 => ItemKind::Gem,
        6 => ItemKind::CustomWeapon,
        _ => return None,
    })
}

/// An item's category, from what the game says of it.
fn category(item: &Item, goods_type: &HashMap<i32, i64>, keys: &BTreeSet<&str>) -> Category {
    if item.name.as_deref().is_some_and(|n| keys.contains(n)) {
        return Category::Key;
    }
    match item.kind {
        ItemKind::Weapon | ItemKind::CustomWeapon => Category::Weapon,
        ItemKind::Protector => Category::Armour,
        ItemKind::Accessory => Category::Talisman,
        ItemKind::Gem => Category::AshOfWar,
        ItemKind::Goods => {
            let t = goods_type.get(&item.id).copied().unwrap_or(0);
            let id = item.id;
            if matches!(t, 3 | 4)
                && ((4000..8000).contains(&id) || (2_004_000..2_008_000).contains(&id))
            {
                Category::Spell
            } else if (200_000..300_000).contains(&id) || (2_200_000..2_300_000).contains(&id) {
                Category::SpiritAsh
            } else if t == 2 {
                Category::Upgrade
            } else if matches!(t, 3 | 6) {
                Category::Quest
            } else {
                Category::Consumable
            }
        }
    }
}

/// Every item slot of the shuffled locations, in the order of the model's
/// locations, each slot once.
pub fn units(model: &Model, bound: &Bound, reg: &Regulation) -> Result<Vec<Unit>> {
    let keys = bound.key_items();
    let goods = for_table("EquipParamGoods").ok_or_else(|| format_err("no goods layout"))?;
    let goods_type: HashMap<i32, i64> = reg
        .table("EquipParamGoods")?
        .rows
        .iter()
        .map(|r| (r.id, goods.field("goods_type").get(&r.data, 0)))
        .collect();
    let names: HashMap<(ItemKind, i32), &str> = model
        .locations
        .iter()
        .flat_map(|l| &l.items)
        .filter_map(|i| Some(((i.kind, i.id), i.name.as_deref()?)))
        .collect();
    let mut tables = HashMap::new();
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for (i, l) in model.locations.iter().enumerate() {
        if !SHUFFLED.contains(&l.kind) || bound.location_unused[i] {
            continue;
        }
        let Some(lot) = &l.lot else {
            continue;
        };
        if !tables.contains_key(&lot.table) {
            let rows: HashMap<i32, Vec<u8>> = reg
                .table(&lot.table)?
                .rows
                .into_iter()
                .map(|r| (r.id, r.data))
                .collect();
            tables.insert(lot.table.clone(), rows);
        }
        let layout = for_table(&lot.table)
            .ok_or_else(|| format_err(format!("no layout for {}", lot.table)))?;
        let (id, cat, weight, count) = (
            layout.field("item_id"),
            layout.field("item_category"),
            layout.field("weight"),
            layout.field("count"),
        );
        for &row in &lot.ids {
            let Some(data) = tables[&lot.table].get(&row) else {
                continue;
            };
            let total: i64 = (0..weight.count).map(|s| weight.get(data, s)).sum();
            for slot in 0..id.count {
                let item_id = id.get(data, slot) as i32;
                let Some(kind) = kind_of(cat.get(data, slot)) else {
                    continue;
                };
                if item_id <= 0 || !seen.insert((lot.table.clone(), row, slot)) {
                    continue;
                }
                let chance = if total > 0 {
                    weight.get(data, slot) * 1000 / total
                } else {
                    0
                };
                let item = Item {
                    kind,
                    id: item_id,
                    count: count.get(data, slot).max(1) as u32,
                    chance: chance as u32,
                    name: names.get(&(kind, item_id)).map(|n| n.to_string()),
                };
                out.push(Unit {
                    location: i,
                    place: Place::Lot,
                    table: lot.table.clone(),
                    row,
                    slot,
                    category: category(&item, &goods_type, &keys),
                    item,
                });
            }
        }
    }

    // Shop rows paid for in runes alone.
    let shop = for_table("ShopLineupParam").ok_or_else(|| format_err("no shop layout"))?;
    let shop_rows: HashMap<i32, Vec<u8>> = reg
        .table("ShopLineupParam")?
        .rows
        .into_iter()
        .map(|r| (r.id, r.data))
        .collect();
    let mut sold = BTreeSet::new();
    for (i, l) in model.locations.iter().enumerate() {
        if l.kind != Kind::Shop || bound.location_unused[i] {
            continue;
        }
        let (Some(row), Some(terms)) = (l.shop_row(), &l.terms) else {
            continue;
        };
        if terms.currency != 0 || !terms.cost.is_empty() || !sold.insert(row) {
            continue;
        }
        let Some(data) = shop_rows.get(&row) else {
            continue;
        };
        let kind = match shop.field("equip_type").get(data, 0) {
            0 => ItemKind::Weapon,
            1 => ItemKind::Protector,
            2 => ItemKind::Accessory,
            3 => ItemKind::Goods,
            4 => ItemKind::Gem,
            _ => continue,
        };
        let id = shop.field("equip_id").get(data, 0) as i32;
        if id <= 0 {
            continue;
        }
        let item = Item {
            kind,
            id,
            count: 1,
            chance: 1000,
            name: names.get(&(kind, id)).map(|n| n.to_string()),
        };
        out.push(Unit {
            location: i,
            place: Place::Shop,
            table: "ShopLineupParam".into(),
            row,
            slot: 0,
            category: category(&item, &goods_type, &keys),
            item,
        });
    }

    // The classes' starting weapons, armour, talismans and spells.
    let init = for_table("CharaInitParam").ok_or_else(|| format_err("no class layout"))?;
    for r in reg.table("CharaInitParam")?.rows {
        if !CLASSES.contains(&r.id) {
            continue;
        }
        let source = format!("CharaInitParam {}", r.id);
        let Some(i) = model.locations.iter().position(|l| l.source == source) else {
            continue;
        };
        for (field, kind) in STARTING {
            let f = init.field(field);
            for slot in 0..f.count {
                let id = f.get(&r.data, slot) as i32;
                if id <= 0 {
                    continue;
                }
                let item = Item {
                    kind,
                    id,
                    count: 1,
                    chance: 1000,
                    name: names.get(&(kind, id)).map(|n| n.to_string()),
                };
                out.push(Unit {
                    location: i,
                    place: Place::Start(field),
                    table: "CharaInitParam".into(),
                    row: r.id,
                    slot,
                    category: category(&item, &goods_type, &keys),
                    item,
                });
            }
        }
    }
    Ok(out)
}

/// Where each unit's item went: `placed[u]` is the index of the unit whose
/// original item unit `u` now gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub seed: u64,
    pub options: Options,
    pub placed: Vec<usize>,
}

/// The item a unit gives under a plan, with the unit's own chance.
pub fn item_at(units: &[Unit], plan: &Plan, u: usize) -> Item {
    let mut item = units[plan.placed[u]].item.clone();
    item.chance = units[u].item.chance;
    item
}

/// What each location gives under a plan, as far as the logic needs it: the
/// key items only. Locations that are not shuffled give what they always do.
pub fn gives<'a>(
    bound: &Bound<'a>,
    units: &'a [Unit],
    placed: &[Option<usize>],
    keys: &BTreeSet<&str>,
) -> Vec<Vec<&'a str>> {
    let model = bound.model;
    // Shops whose rows are units give what the plan puts there instead.
    let shops: BTreeSet<usize> = units
        .iter()
        .filter(|u| u.place == Place::Shop)
        .map(|u| u.location)
        .collect();
    let mut out: Vec<Vec<&'a str>> = model
        .locations
        .iter()
        .enumerate()
        .map(|(i, l)| {
            if SHUFFLED.contains(&l.kind) || l.kind == Kind::Starting || shops.contains(&i) {
                return Vec::new();
            }
            l.items
                .iter()
                .filter_map(|i| i.name.as_deref())
                .filter(|n| keys.contains(n))
                .collect()
        })
        .collect();
    for (u, unit) in units.iter().enumerate() {
        let Some(from) = placed[u] else {
            continue;
        };
        // An item that may not drop opens nothing.
        if !unit.sure() {
            continue;
        }
        if let Some(name) = units[from]
            .item
            .name
            .as_deref()
            .filter(|n| keys.contains(n))
        {
            out[unit.location].push(name);
        }
    }
    out
}

/// Place every unit's item from `seed` under the player's `options`, or say
/// why it could not be done.
pub fn shuffle(
    bound: &Bound,
    units: &[Unit],
    seed: u64,
    options: &Options,
) -> std::result::Result<Plan, String> {
    let keys = bound.key_items();
    let depth = depth::depths(bound);
    let mut rng = Rng::new(seed);
    let mut last = String::new();
    for _ in 0..ATTEMPTS {
        match attempt(bound, units, &keys, options, &depth, &mut rng) {
            Ok(placed) => {
                let plan = Plan {
                    seed,
                    options: options.clone(),
                    placed,
                };
                check(bound, units, &plan)?;
                return Ok(plan);
            }
            Err(e) => last = e,
        }
    }
    Err(format!(
        "seed {seed}: no placement after {ATTEMPTS} tries: {last}"
    ))
}

/// A slider's strength: its square, so the low end stays gentle.
fn strength(percent: u8) -> f64 {
    let p = f64::from(percent) / 100.0;
    p * p
}

/// A weight as a whole number, so draws are the same on every platform.
fn weight(x: f64) -> u64 {
    (rng::exp(x) * 1e6) as u64
}

fn attempt(
    bound: &Bound,
    units: &[Unit],
    keys: &BTreeSet<&str>,
    options: &Options,
    depth: &[f64],
    rng: &mut Rng,
) -> std::result::Result<Vec<usize>, String> {
    let n = units.len();
    // With the DLC kept separate, an item stays on its own side.
    let dlc: Vec<bool> = units
        .iter()
        .map(|u| in_dlc(bound.model, u.location))
        .collect();
    let fits = |slot: usize, item: usize| {
        fits(&units[slot], &units[item]) && (!options.dlc_separate || dlc[slot] == dlc[item])
    };
    // Which items move: each with its slider's shuffle chance.
    let moving: Vec<bool> = units
        .iter()
        .map(|u| rng.below(100) < usize::from(options.shuffle(u.slider())))
        .collect();
    let mut placed: Vec<Option<usize>> = (0..n).map(|u| (!moving[u]).then_some(u)).collect();
    let mut key_units: Vec<usize> = (0..n)
        .filter(|&u| moving[u] && units[u].category == Category::Key)
        .collect();
    let mut rest: Vec<usize> = (0..n)
        .filter(|&u| moving[u] && units[u].category != Category::Key)
        .collect();
    rng.shuffle(&mut key_units);
    rng.shuffle(&mut rest);
    // Weapons with a set affinity first: shops cannot take them, so they
    // must not be the last left over.
    rest.sort_by_key(|&u| units[u].item.kind != ItemKind::CustomWeapon);

    // Key items, by assumed fill, leaning deep.
    let lean = KEY_LEAN * strength(options.bias(Category::Key));
    let open_for_key =
        |u: usize| units[u].sure() && bound.location_area[units[u].location].is_some();
    for i in 0..key_units.len() {
        let k = key_units[i];
        let held: Vec<&str> = key_units[i + 1..]
            .iter()
            .filter_map(|&u| units[u].item.name.as_deref())
            .collect();
        let walk = bound.walk_holding(&gives(bound, units, &placed, keys), &held);
        let free: Vec<usize> = (0..n)
            .filter(|&u| {
                placed[u].is_none()
                    && open_for_key(u)
                    && fits(u, k)
                    && walk.taken[units[u].location]
            })
            .collect();
        let weights: Vec<u64> = free
            .iter()
            .map(|&u| weight(lean * depth[units[u].location]))
            .collect();
        let Some(at) = rng.weighted(&weights) else {
            return Err(format!(
                "nowhere reachable for {}",
                units[k].item.name.as_deref().unwrap_or("?")
            ));
        };
        placed[free[at]] = Some(k);
    }

    // Starting slots next: each takes an item of its own sort, any one.
    let starts: Vec<usize> = (0..n)
        .filter(|&u| placed[u].is_none() && matches!(units[u].place, Place::Start(_)))
        .collect();
    for u in starts {
        let ok: Vec<usize> = (0..rest.len()).filter(|&i| fits(u, rest[i])).collect();
        if ok.is_empty() {
            return Err(format!(
                "nothing to start with in {} {}",
                units[u].row, units[u].slot
            ));
        }
        placed[u] = Some(rest.remove(ok[rng.below(ok.len())]));
    }

    // Everything else, each item towards slots as deep as it is worth.
    let band = |u: usize| ((depth[units[u].location] * BANDS as f64) as usize).min(BANDS - 1);
    let mut bands: Vec<Vec<usize>> = vec![Vec::new(); BANDS];
    for u in (0..n).filter(|&u| placed[u].is_none()) {
        bands[band(u)].push(u);
    }
    for from in rest {
        let lean = REST_LEAN * strength(options.bias(units[from].category));
        let worth = depth[units[from].location];
        let weights: Vec<u64> = bands
            .iter()
            .enumerate()
            .map(|(b, slots)| {
                let middle = (b as f64 + 0.5) / BANDS as f64;
                slots.len() as u64 * weight(-lean * (middle - worth).abs())
            })
            .collect();
        let b = rng.weighted(&weights).ok_or("more items than slots")?;
        // A slot of the band that takes the item, else any slot that does.
        let mut pick = None;
        for _ in 0..16 {
            let i = rng.below(bands[b].len());
            if fits(bands[b][i], from) {
                pick = Some((b, i));
                break;
            }
        }
        let pick = pick
            .or_else(|| {
                bands.iter().enumerate().find_map(|(b, slots)| {
                    let i = slots.iter().position(|&u| fits(u, from))?;
                    Some((b, i))
                })
            })
            .ok_or_else(|| format!("no slot left takes item {}", units[from].item.id))?;
        let u = bands[pick.0].swap_remove(pick.1);
        placed[u] = Some(from);
    }
    Ok(placed
        .into_iter()
        .map(|p| p.expect("every slot filled"))
        .collect())
}

/// The plan's proof: walked from the start with nothing assumed, the goal
/// is reached.
pub fn check(bound: &Bound, units: &[Unit], plan: &Plan) -> std::result::Result<(), String> {
    let keys = bound.key_items();
    let placed: Vec<Option<usize>> = plan.placed.iter().map(|&p| Some(p)).collect();
    let walk = bound.walk(&gives(bound, units, &placed, &keys));
    if walk.goal {
        Ok(())
    } else {
        Err(format!("seed {}: the goal cannot be reached", plan.seed))
    }
}

/// Write a plan into the item-lot, shop and class tables.
pub fn apply(reg: &mut Regulation, units: &[Unit], plan: &Plan) -> Result<()> {
    let rows_of = |param: &fs_param::Param| -> HashMap<i32, usize> {
        param
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| (r.id, i))
            .collect()
    };
    let shop = for_table("ShopLineupParam").ok_or_else(|| format_err("no shop layout"))?;
    let mut param = reg.table("ShopLineupParam")?;
    let rows = rows_of(&param);
    for (u, unit) in units
        .iter()
        .enumerate()
        .filter(|(_, u)| u.place == Place::Shop)
    {
        let item = item_at(units, plan, u);
        let kind = shop_type(item.kind)
            .ok_or_else(|| format_err(format!("item {} cannot be sold", item.id)))?;
        let data = &mut param.rows[rows[&unit.row]].data;
        shop.field("equip_id").set(data, 0, i64::from(item.id))?;
        shop.field("equip_type").set(data, 0, kind)?;
    }
    reg.set_table("ShopLineupParam", &param)?;
    let init = for_table("CharaInitParam").ok_or_else(|| format_err("no class layout"))?;
    let mut param = reg.table("CharaInitParam")?;
    let rows = rows_of(&param);
    for (u, unit) in units.iter().enumerate() {
        let Place::Start(field) = unit.place else {
            continue;
        };
        let item = item_at(units, plan, u);
        let data = &mut param.rows[rows[&unit.row]].data;
        init.field(field).set(data, unit.slot, i64::from(item.id))?;
    }
    reg.set_table("CharaInitParam", &param)?;

    let tables: BTreeSet<&str> = units
        .iter()
        .filter(|u| u.place == Place::Lot)
        .map(|u| u.table.as_str())
        .collect();
    for table in tables {
        let layout =
            for_table(table).ok_or_else(|| format_err(format!("no layout for {table}")))?;
        let (id, cat, count) = (
            layout.field("item_id"),
            layout.field("item_category"),
            layout.field("count"),
        );
        let mut param = reg.table(table)?;
        let rows: HashMap<i32, usize> = param
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| (r.id, i))
            .collect();
        for (u, unit) in units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.place == Place::Lot && u.table == table)
        {
            let item = item_at(units, plan, u);
            let data = &mut param.rows[rows[&unit.row]].data;
            id.set(data, unit.slot, i64::from(item.id))?;
            cat.set(data, unit.slot, lot_category(item.kind))?;
            count.set(data, unit.slot, i64::from(item.count.clamp(1, 255)))?;
        }
        reg.set_table(table, &param)?;
    }
    Ok(())
}

/// The spoiler log: where every key item went, then every shuffled slot by
/// area.
pub fn spoiler(bound: &Bound, units: &[Unit], plan: &Plan) -> String {
    let model = bound.model;
    let keys = bound.key_items();
    let area = |l: usize| {
        bound.location_area[l].map_or("(anywhere)", |a| bound.graph.areas[a].name.as_str())
    };
    let name = |i: &Item| {
        i.name
            .clone()
            .unwrap_or_else(|| format!("{:?} {}", i.kind, i.id))
    };
    let line = |u: usize| {
        let unit = &units[u];
        let l = &model.locations[unit.location];
        let now = item_at(units, plan, u);
        let chance = if unit.sure() {
            String::new()
        } else {
            format!(" ({:.1}%)", f64::from(unit.item.chance) / 10.0)
        };
        format!(
            "{} {}{}: {}{} (was {})",
            l.kind.label(),
            l.source,
            l.map.map(|m| format!(" [{m}]")).unwrap_or_default(),
            name(&now),
            if now.count > 1 {
                format!(" x{}", now.count)
            } else {
                String::new()
            },
            name(&unit.item),
        ) + &chance
    };
    let mut out = format!(
        "Apocrypha randomizer spoiler log\nseed {}\n\nOptions\n{}\n",
        plan.seed, plan.options
    );
    out += "Key items\n";
    let mut key_lines: Vec<(String, String)> = (0..units.len())
        .filter_map(|u| {
            let now = item_at(units, plan, u);
            let n = now.name.as_deref()?;
            keys.contains(n).then(|| {
                (
                    n.to_string(),
                    format!("  {n}: {}, {}", area(units[u].location), line(u)),
                )
            })
        })
        .collect();
    key_lines.sort();
    for (_, l) in key_lines {
        out += &l;
        out += "\n";
    }
    out += "\nEvery slot, by area\n";
    let mut by_area: Vec<(&str, usize)> = (0..units.len())
        .map(|u| (area(units[u].location), u))
        .collect();
    by_area.sort_by_key(|&(a, u)| (a, units[u].location, u));
    let mut last = "";
    for (a, u) in by_area {
        if a != last {
            out += &format!("\n{a}\n");
            last = a;
        }
        out += &format!("  {}\n", line(u));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use gamedata::MapId;
    use gamedata::model::{Boss, Grace, Location};

    const WEST: MapId = MapId([60, 40, 37, 0]);
    const CAVE: MapId = MapId([31, 0, 0, 0]);

    fn item(name: &str, chance: u32) -> Item {
        Item {
            kind: ItemKind::Goods,
            id: 1,
            count: 1,
            chance,
            name: Some(name.into()),
        }
    }

    fn at(map: MapId, name: &str) -> Location {
        Location {
            kind: Kind::Treasure,
            map: Some(map),
            source: name.into(),
            entity: None,
            position: Some([0.0; 3]),
            lot: None,
            items: vec![item(name, 1000)],
            flag: None,
            also: vec![],
            terms: None,
            given_if: None,
        }
    }

    /// A field with a grace, a cave behind a key with the goal's boss, and
    /// five treasures on each side.
    fn model() -> Model {
        let mut locations = vec![at(WEST, "Key")];
        for i in 0..5 {
            locations.push(at(WEST, &format!("West {i}")));
            locations.push(at(CAVE, &format!("Cave {i}")));
        }
        Model {
            graces: vec![Grace {
                id: 1,
                name: Some("West".into()),
                map: Some(MapId([60, 10, 9, 2])),
                entity: 0,
                flag: 0,
                position: Some([0.0; 3]),
            }],
            locations,
            bosses: vec![Boss {
                area: 1,
                map: Some(CAVE),
                entity: None,
                name: Some("Guardian".into()),
                defeat_flag: 1,
            }],
            ..Default::default()
        }
    }

    fn graph() -> logic::Graph {
        toml::from_str(
            r#"
            start = "west"
            goal = { bosses = ["Guardian"] }
            [[area]]
            id = "west"
            name = "West"
            graces = ["West"]
            [[area]]
            id = "cave"
            name = "Cave"
            maps = ["m31_00"]
            [[edge]]
            from = "west"
            to = "cave"
            items = ["Key"]
            "#,
        )
        .unwrap()
    }

    fn units(m: &Model) -> Vec<Unit> {
        m.locations
            .iter()
            .enumerate()
            .map(|(i, l)| Unit {
                location: i,
                place: Place::Lot,
                table: "ItemLotParam_map".into(),
                row: i as i32,
                slot: 0,
                item: l.items[0].clone(),
                category: if l.items[0].name.as_deref() == Some("Key") {
                    Category::Key
                } else {
                    Category::Consumable
                },
            })
            .collect()
    }

    #[test]
    fn the_key_never_lands_behind_its_own_door() {
        let (g, m) = (graph(), model());
        let b = Bound::new(&g, &m);
        assert!(b.problems.is_empty(), "{:?}", b.problems);
        let us = units(&m);
        let mut west_slots = BTreeSet::new();
        for seed in 0..200 {
            let plan = shuffle(&b, &us, seed, &Options::default()).unwrap();
            let at = (0..us.len())
                .find(|&u| item_at(&us, &plan, u).name.as_deref() == Some("Key"))
                .unwrap();
            assert_eq!(us[at].item.chance, 1000);
            assert_ne!(b.location_area[us[at].location], Some(1), "seed {seed}");
            west_slots.insert(at);
        }
        // It moves around the field, not just to one slot.
        assert!(west_slots.len() > 3, "{west_slots:?}");
    }

    #[test]
    fn a_seed_gives_the_same_plan_and_every_item_once() {
        let (g, m) = (graph(), model());
        let b = Bound::new(&g, &m);
        let us = units(&m);
        let a = shuffle(&b, &us, 7, &Options::default()).unwrap();
        assert_eq!(a, shuffle(&b, &us, 7, &Options::default()).unwrap());
        assert_ne!(
            a.placed,
            shuffle(&b, &us, 8, &Options::default()).unwrap().placed
        );
        let mut sorted = a.placed.clone();
        sorted.sort();
        assert_eq!(sorted, (0..us.len()).collect::<Vec<_>>());
    }

    #[test]
    fn a_category_at_no_shuffle_stays_where_it_is() {
        let (g, m) = (graph(), model());
        let b = Bound::new(&g, &m);
        let us = units(&m);
        let mut none = Options::default();
        none.set(false, "all=0").unwrap();
        let plan = shuffle(&b, &us, 5, &none).unwrap();
        assert_eq!(plan.placed, (0..us.len()).collect::<Vec<_>>());
        // Keys held still, everything else moving: the key keeps its slot.
        let mut keys_still = Options::default();
        keys_still.set(false, "keys=0").unwrap();
        for seed in 0..20 {
            let plan = shuffle(&b, &us, seed, &keys_still).unwrap();
            assert_eq!(plan.placed[0], 0, "seed {seed}");
        }
        assert!(
            (0..20).any(|seed| shuffle(&b, &us, seed, &keys_still).unwrap().placed[1] != 1),
            "the rest never moved"
        );
    }

    #[test]
    fn a_key_with_nowhere_to_go_fails_the_seed() {
        let (g, mut m) = (graph(), model());
        // Every slot outside the cave may not drop: the key has no home.
        let mut us = units(&m);
        for u in us
            .iter_mut()
            .filter(|u| m.locations[u.location].map == Some(WEST))
        {
            u.item.chance = 500;
        }
        m.locations[0].items[0].chance = 500;
        let b = Bound::new(&g, &m);
        assert!(shuffle(&b, &us, 1, &Options::default()).is_err());
    }
}
