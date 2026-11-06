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
//!   - a keepsake, the same with its count ([`Place::Start`]);
//!   - an item slot of a lot that regrowing nodes give ([`Place::Gather`]),
//!     which trades only with other such slots, as it can be had again and
//!     again.
//!
//!   Farmable drops and other starting goods stay as they are, and so does
//!   whatever the area map calls `fixed`. NPCs' equipment is drawn apart from
//!   all this ([`dress_npcs`]).
//! - **The player's sliders** ([`options`]): for each category of item, how
//!   much of it moves (the rest stays where the unmodded game has it) and
//!   how strongly what moves leans towards deep slots ([`depth`]).
//! - **Key items** go first, by assumed fill: each is placed in a reachable
//!   slot of a walk that assumes every key item not yet placed is already
//!   held, drawn with the key-item bias. A key item only goes in a slot that
//!   always gives it (chance 1000) at a location the area map puts in an
//!   area.
//! - **Quest items** (what talk scripts take or ask to be held, §2.4 of the
//!   spec) are placed next: one copy of each, unless a copy stays where it
//!   was, goes where the finished walk gets it for sure, short of the areas
//!   past no return. A key item that is also a quest item keeps out of those
//!   areas too.
//! - **Everything else** that moves fills the slots left, each item drawn
//!   towards slots as deep as it is worth, as strongly as its category's
//!   bias says.
//! - **The check.** The finished placement is walked from the start with
//!   nothing assumed: the goal must be reached, and every quest item got.
//!
//! The same seed gives the same placement on every platform: the generator
//! is our own ([`rng`]) and every collection is walked in a fixed order.

use std::collections::{BTreeMap, BTreeSet, HashMap};

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
/// The most a shop row asking for a key or quest item may cost, in runes. Our own
/// choice: most of what the first merchants sell is at or under it.
const KEY_PRICE_CAP: i64 = 3000;
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
/// The keepsakes' CharaInitParam rows, with the menu text that names each:
/// character creation does not read the name from the item.
const KEEPSAKES: [(i32, i32); 9] = [
    (2401, 297151),
    (2402, 297152),
    (2403, 297153),
    (2404, 297154),
    (2405, 297156),
    (2406, 297157),
    (2407, 297158),
    (2408, 297159),
    (2409, 297160),
];

/// What kind of place a unit is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// Slot `slot` of an item-lot row.
    Lot,
    /// Slot `slot` of an item-lot row that regrowing nodes give.
    Gather,
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
    /// For a shop row: its price in runes, and whether it never runs out.
    pub price: i64,
    pub unlimited: bool,
    /// What the item asks of whoever starts with it ([`Wants`]).
    pub wants: Wants,
    /// For a class's starting slot: the class, for gear it can use.
    pub class: Option<Class>,
}

impl Unit {
    /// Whether picking the location up always gives this slot's item.
    pub fn sure(&self) -> bool {
        self.item.chance == 1000
    }

    /// The slider of the kind of place this is, or of its item's category for a
    /// plain lot. Where the two differ, both must allow a move ([`shuffle`]).
    pub fn slider(&self) -> Category {
        match self.place {
            Place::Lot => self.category,
            Place::Gather => Category::Gathering,
            Place::Shop => Category::Merchant,
            Place::Start(_) if CLASSES.contains(&self.row) => Category::Starting,
            Place::Start(_) => Category::Keepsake,
        }
    }
}

/// A school of spells, and the catalyst that casts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum School {
    Sorcery,
    Incantation,
}

/// What a weapon or spell asks of whoever starts with it, read from the game's
/// tables, for usable starting gear ([`Options::usable_start`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Wants {
    /// Strength, dexterity, intelligence, faith and arcane.
    pub stats: [u8; 5],
    /// For a staff or seal: the school it casts.
    pub casts: Option<School>,
    /// For a spell: its school and the memory slots it takes.
    pub spell: Option<(School, u8)>,
    /// For a bow, crossbow or ballista: the first two digits (50..=53) of the
    /// ammunition it shoots.
    pub ammo: Option<u8>,
    /// Whether it is a weapon to fight with: not a shield, a staff, a seal or a
    /// torch.
    pub strikes: bool,
}

/// A starting class, as its gear must suit it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Class {
    /// Strength, dexterity, intelligence, faith and arcane.
    pub stats: [u8; 5],
    /// The ammunition it starts with, by kind: bit n for ids starting 50 + n.
    pub ammo: u8,
}

/// The memory slots a new character has: every class's starting spells fit them.
const START_MEMORY: u8 = 2;

impl Class {
    /// The class as the seed's options make it: with no stat requirements every
    /// stat is enough.
    fn under(self, options: &Options) -> Class {
        if options.no_requirements {
            Class {
                stats: [u8::MAX; 5],
                ..self
            }
        } else {
            self
        }
    }

    /// Whether the class can wield or cast this, from the start: its strength
    /// counts half again, as two-handing does, and a bow needs its arrows.
    pub fn wields(&self, w: &Wants) -> bool {
        let strength = u16::from(self.stats[0]) * 3 / 2;
        u16::from(w.stats[0]) <= strength
            && (1..5).all(|i| w.stats[i] <= self.stats[i])
            && w.ammo.is_none_or(|a| self.ammo & (1 << (a - 50)) != 0)
    }
}

/// What one of a class's starting slots may take, with usable starting gear.
struct Suits {
    class: Class,
    /// For a weapon slot that must fill what the class lacks: a catalyst of the
    /// schools marked, or, if the flag is set, a weapon to fight with.
    need: Option<([bool; 2], bool)>,
    /// The schools the class holds a catalyst for, and its memory left.
    casts: [bool; 2],
    memory: u8,
}

impl Suits {
    /// What slot `u` may take, given what its class already holds; `None` for a
    /// slot that is no class's.
    fn of(units: &[Unit], placed: &[Option<usize>], slots: &[usize], u: usize) -> Option<Suits> {
        let class = units[u].class?;
        let held: Vec<&Wants> = slots
            .iter()
            .filter_map(|&v| placed[v])
            .map(|i| &units[i].wants)
            .collect();
        let mut casts = [false; 2];
        let mut spells = [false; 2];
        let mut used = 0u8;
        let armed = held.iter().any(|w| w.strikes);
        for w in &held {
            if let Some(s) = w.casts {
                casts[s as usize] = true;
            }
            if let Some((s, m)) = w.spell {
                spells[s as usize] = true;
                used = used.saturating_add(m);
            }
        }
        let open = |field: &'static str| {
            slots
                .iter()
                .filter(|&&v| placed[v].is_none() && units[v].place == Place::Start(field))
                .count()
        };
        let mut need = None;
        if units[u].place == Place::Start("weapon") {
            // Spells the class keeps need their catalyst; spells still to come
            // need one of either school; and every class needs a weapon to fight
            // with. The last weapon slots left are made to hold what is lacking.
            let missing = [0, 1].map(|s| spells[s] && !casts[s]);
            let any = open("magic") > 0 && casts == [false; 2] && missing == [false; 2];
            let wanted =
                missing.iter().filter(|&&m| m).count() + usize::from(any) + usize::from(!armed);
            if wanted > 0 && wanted >= open("weapon") {
                let schools = if any { [true; 2] } else { missing };
                need = Some((schools, !armed));
            }
        }
        Some(Suits {
            class,
            need,
            casts,
            memory: START_MEMORY.saturating_sub(used),
        })
    }

    fn allows(&self, item: &Unit) -> bool {
        let w = &item.wants;
        self.class.wields(w)
            && self.need.is_none_or(|(schools, strike)| {
                w.casts.is_some_and(|s| schools[s as usize]) || (strike && w.strikes)
            })
            && w.spell
                .is_none_or(|(s, m)| self.casts[s as usize] && m <= self.memory)
    }
}

/// What the units' weapons and spells ask, and the classes their starting slots
/// belong to, from the game's tables.
fn read_wants(reg: &Regulation, units: &mut [Unit]) -> Result<()> {
    let rows = |table: &str| -> Result<HashMap<i32, Vec<u8>>> {
        Ok(reg
            .table(table)?
            .rows
            .into_iter()
            .map(|r| (r.id, r.data))
            .collect())
    };
    let layout =
        |table: &str| for_table(table).ok_or_else(|| format_err(format!("no layout for {table}")));
    let (w, m, c) = (
        layout("EquipParamWeapon")?,
        layout("Magic")?,
        layout("CharaInitParam")?,
    );
    let (weapons, magic) = (rows("EquipParamWeapon")?, rows("Magic")?);
    let mut classes = HashMap::new();
    for r in reg.table("CharaInitParam")?.rows {
        if !CLASSES.contains(&r.id) {
            continue;
        }
        let stats = std::array::from_fn(|i| c.field("stats").get(&r.data, i) as u8);
        let ammo = (0..c.field("ammo").count)
            .map(|i| c.field("ammo").get(&r.data, i) / 1_000_000)
            .filter(|k| (50..=53).contains(k))
            .fold(0u8, |bits, k| bits | 1 << (k - 50));
        classes.insert(r.id, Class { stats, ammo });
    }
    for u in units {
        if matches!(u.place, Place::Start(_)) {
            u.class = classes.get(&u.row).copied();
        }
        let id = u.item.id;
        u.wants = match u.item.kind {
            ItemKind::Weapon => {
                // An infused or upgraded weapon has its own row, or its base's.
                let Some(d) = [id, id - id % 100, id - id % 10_000]
                    .iter()
                    .find_map(|r| weapons.get(r))
                else {
                    continue;
                };
                let req = w.field("requirement");
                Wants {
                    stats: [
                        req.get(d, 0) as u8,
                        req.get(d, 1) as u8,
                        req.get(d, 2) as u8,
                        req.get(d, 3) as u8,
                        w.field("requirement_arcane").get(d, 0) as u8,
                    ],
                    casts: match id / 1_000_000 {
                        33 => Some(School::Sorcery),
                        34 => Some(School::Incantation),
                        _ => None,
                    },
                    spell: None,
                    ammo: match id / 1_000_000 {
                        40 | 41 => Some(50),
                        42 => Some(51),
                        43 => Some(52),
                        44 => Some(53),
                        _ => None,
                    },
                    strikes: !matches!(id / 1_000_000, 24 | 30..=34),
                }
            }
            ItemKind::Goods => {
                let Some(d) = magic.get(&id) else {
                    continue;
                };
                let req = m.field("requirement");
                let school = if m.field("school").get(d, 0) == 1 {
                    School::Incantation
                } else {
                    School::Sorcery
                };
                Wants {
                    stats: [
                        0,
                        0,
                        req.get(d, 0) as u8,
                        req.get(d, 1) as u8,
                        m.field("requirement_arcane").get(d, 0) as u8,
                    ],
                    spell: Some((school, m.field("memory").get(d, 0) as u8)),
                    ..Wants::default()
                }
            }
            _ => continue,
        };
    }
    Ok(())
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

/// Whether a map is in the DLC.
pub fn in_dlc_map(m: gamedata::MapId) -> bool {
    DLC_MAPS.contains(&m.area())
}

/// Arrows and bolts, which are weapons to the game but go in no hand.
fn is_ammo(id: i32) -> bool {
    (50_000_000..54_000_000).contains(&id)
}

/// Whether the item now at unit `item` may go in the place of unit `slot`.
pub fn fits(slot: &Unit, item: &Unit) -> bool {
    let it = &item.item;
    if (slot.place == Place::Gather) != (item.place == Place::Gather) {
        return false;
    }
    match slot.place {
        Place::Lot | Place::Gather => true,
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
                    // Named, as the menu shows it.
                    "goods" => it.kind == ItemKind::Goods && it.name.is_some(),
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
/// Whether the player asked for this item to stay where the game has it: the
/// Whetstone Knife and the whetblades, or map fragments (Nova, 2026-11-05). Gone
/// by name, as the area map goes, so a mod's renaming leaves it as unmodded.
pub fn kept_in_place(options: &Options, item: &Item) -> bool {
    let Some(name) = item.name.as_deref() else {
        return false;
    };
    (options.keep_whetblades && (name == "Whetstone Knife" || name.ends_with(" Whetblade")))
        || (options.keep_maps && name.starts_with("Map: "))
}

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
    // Gathering last: a lot something else gives too moves with that.
    let of_kind = |gather: bool| {
        (0..model.locations.len()).filter(move |&i| {
            let kind = model.locations[i].kind;
            if gather {
                kind == Kind::Gather
            } else {
                SHUFFLED.contains(&kind)
            }
        })
    };
    for i in of_kind(false).chain(of_kind(true)) {
        let l = &model.locations[i];
        if bound.location_unused[i] {
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
                    place: if l.kind == Kind::Gather {
                        Place::Gather
                    } else {
                        Place::Lot
                    },
                    table: lot.table.clone(),
                    row,
                    slot,
                    category: category(&item, &goods_type, &keys),
                    item,
                    price: 0,
                    unlimited: false,
                    wants: Wants::default(),
                    class: None,
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
            price: shop.field("value").get(data, 0),
            unlimited: shop.field("sell_quantity").get(data, 0) == 255,
            wants: Wants::default(),
            class: None,
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
                    price: 0,
                    unlimited: false,
                    wants: Wants::default(),
                    class: None,
                });
            }
        }
    }
    // The keepsakes.
    let (goods_slot, goods_count) = (init.field("goods"), init.field("goods_count"));
    let accessory = init.field("accessory");
    for r in reg.table("CharaInitParam")?.rows {
        if !KEEPSAKES.iter().any(|&(row, _)| row == r.id) {
            continue;
        }
        let source = format!("CharaInitParam {}", r.id);
        let Some(i) = model.locations.iter().position(|l| l.source == source) else {
            continue;
        };
        let mut add = |field: &'static str, kind, slot: usize, id: i32, count: i64| {
            if id <= 0 {
                return;
            }
            let item = Item {
                kind,
                id,
                count: count.max(1) as u32,
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
                price: 0,
                unlimited: false,
                wants: Wants::default(),
                class: None,
            });
        };
        for slot in 0..goods_slot.count {
            let id = goods_slot.get(&r.data, slot) as i32;
            add(
                "goods",
                ItemKind::Goods,
                slot,
                id,
                goods_count.get(&r.data, slot),
            );
        }
        for slot in 0..accessory.count {
            let id = accessory.get(&r.data, slot) as i32;
            add("accessory", ItemKind::Accessory, slot, id, 1);
        }
    }
    read_wants(reg, &mut out)?;
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

/// Whether a slot can be counted on: it always gives its item, at a location
/// in an area, which no quest can move past and which cannot shut for good. A
/// shop row must sell out, so the item is sold once, at a price a player can
/// meet when it is asked for. An enemy's drop will not do while enemies move.
fn dependable(bound: &Bound, unit: &Unit, options: &Options) -> bool {
    let location = &bound.model.locations[unit.location];
    unit.sure()
        && bound.location_area[unit.location].is_some()
        && !bound.missable(unit.location)
        && !bound.may_close(unit.location)
        && !unit.unlimited
        && unit.price <= KEY_PRICE_CAP
        && !(options.enemies_move && location.kind == Kind::EnemyDrop)
}

/// Whether there should be only one of an item, which a slot that may not give
/// it would lose, and a row that never runs out would multiply.
pub fn one_off(category: Category) -> bool {
    !matches!(category, Category::Consumable | Category::Upgrade)
}

/// A slot that gives its item only by chance: a lot's slot whose weight is not
/// the whole of its row's (a drop at 5%, one of two items a chest may give). It
/// takes no key item ([`dependable`]) and, if moved, no one-off item ([`one_off`]):
/// only what is plentiful, such as materials and consumables (Nova, 2026-11-03).
pub fn chance_slot(unit: &Unit) -> bool {
    unit.place == Place::Lot && !unit.sure()
}

/// A slot that, moved, takes only what is plentiful: one that gives by chance
/// ([`chance_slot`]), or a shop row that never runs out, which would sell a one-off
/// item again and again (Nova, 2026-11-03).
pub fn plentiful_only(unit: &Unit) -> bool {
    chance_slot(unit) || unit.unlimited
}

/// The quest items, as (kind, id), that some unit gives.
fn quest_items(bound: &Bound, units: &[Unit]) -> BTreeSet<(ItemKind, i32)> {
    bound
        .model
        .quest_items
        .iter()
        .map(|i| (i.kind, i.id))
        .filter(|q| units.iter().any(|u| (u.item.kind, u.item.id) == *q))
        .collect()
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
            if SHUFFLED.contains(&l.kind)
                || matches!(l.kind, Kind::Starting | Kind::Keepsake)
                || shops.contains(&i)
            {
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

/// An item the area map says never moves.
fn is_fixed(bound: &Bound, unit: &Unit) -> bool {
    let name = unit.item.name.as_deref();
    bound.graph.fixed.iter().any(|f| Some(f.as_str()) == name)
}

/// Keep enough plentiful items moving for every moving slot that takes only those
/// ([`plentiful_only`]), on each side of the DLC when they are kept apart. Where the
/// sliders leave too few, chance slots that hold a one-off item stay as the unmodded
/// game has them, drawn in the seed's order: each takes one chance slot and one
/// one-off item out of play, so the shortfall falls by one. Key items are not held
/// back this way, so the walk still has every one to place, but for those on rows
/// that never run out, which stay where the game sells them.
fn balance_chance_slots(
    units: &[Unit],
    mut moving: Vec<bool>,
    dlc: &[bool],
    separate: bool,
    rng: &mut Rng,
) -> Vec<bool> {
    let side = |u: usize| separate && dlc[u];
    for s in [false, true] {
        let mine: Vec<usize> = (0..units.len())
            .filter(|&u| moving[u] && side(u) == s)
            .collect();
        let slots = mine.iter().filter(|&&u| plentiful_only(&units[u])).count();
        let plentiful = mine
            .iter()
            .filter(|&&u| units[u].place != Place::Gather && !one_off(units[u].category))
            .count();
        if slots <= plentiful {
            continue;
        }
        let mut held: Vec<usize> = mine
            .into_iter()
            .filter(|&u| {
                plentiful_only(&units[u])
                    && one_off(units[u].category)
                    // A key on a row that never runs out, such as Thiollier's
                    // Concoction, may stay: the game sells it there.
                    && (units[u].category != Category::Key || units[u].unlimited)
            })
            .collect();
        rng.shuffle(&mut held);
        for u in held.into_iter().take(slots - plentiful) {
            moving[u] = false;
        }
    }
    moving
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
        .map(|u| {
            // A key or quest item on a node stays: it must be got somewhere
            // the logic can count on, and nodes trade only among themselves.
            let pinned = is_fixed(bound, u)
                || kept_in_place(options, &u.item)
                || (u.place == Place::Gather
                    && (u.category == Category::Key
                        || bound
                            .model
                            .quest_items
                            .iter()
                            .any(|q| (q.kind, q.id) == (u.item.kind, u.item.id))));
            let drawn = rng.below(100) < usize::from(options.shuffle(u.slider()));
            // A shop row, a class's or keepsake's slot and a node answer to their
            // item's slider too, so a category at 0% moves nowhere (Nova, 2026-11-03).
            let category = u.slider() == u.category
                || rng.below(100) < usize::from(options.shuffle(u.category));
            drawn && category && !pinned
        })
        .collect();
    let moving = balance_chance_slots(units, moving, &dlc, options.dlc_separate, rng);
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
    let quests = quest_items(bound, units);
    let is_quest = |u: usize| quests.contains(&(units[u].item.kind, units[u].item.id));
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
                    && dependable(bound, &units[u], options)
                    && fits(u, k)
                    && walk.taken[units[u].location]
                    && !(is_quest(k) && bound.past_no_return(units[u].location))
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

    // Quest items next: one copy of each where the finished walk gets it,
    // unless a copy stays where the unmodded game has it.
    let walk = bound.walk(&gives(bound, units, &placed, keys));
    let still: BTreeSet<(ItemKind, i32)> = (0..n)
        .filter(|&u| !moving[u] && !matches!(units[u].place, Place::Start(_)))
        .map(|u| (units[u].item.kind, units[u].item.id))
        .collect();
    for q in &quests {
        if still.contains(q)
            || key_units
                .iter()
                .any(|&k| (units[k].item.kind, units[k].item.id) == *q)
        {
            continue;
        }
        let Some(i) = rest
            .iter()
            .position(|&u| (units[u].item.kind, units[u].item.id) == *q)
        else {
            continue;
        };
        let from = rest[i];
        let lean = REST_LEAN * strength(options.bias(units[from].category));
        let worth = depth[units[from].location];
        let free: Vec<usize> = (0..n)
            .filter(|&u| {
                placed[u].is_none()
                    && dependable(bound, &units[u], options)
                    && fits(u, from)
                    && walk.taken[units[u].location]
                    && !bound.past_no_return(units[u].location)
            })
            .collect();
        let weights: Vec<u64> = free
            .iter()
            .map(|&u| weight(-lean * (depth[units[u].location] - worth).abs()))
            .collect();
        let Some(at) = rng.weighted(&weights) else {
            return Err(format!(
                "nowhere dependable for {}",
                units[from].item.name.as_deref().unwrap_or("?")
            ));
        };
        placed[free[at]] = Some(from);
        rest.remove(i);
    }

    // Slots that give by chance and rows that never run out next, before anything
    // else can take what is plentiful: each takes an item that is not one-off, a row
    // that never runs out a consumable where one fits. There are enough, as
    // `balance_chance_slots` saw to it.
    let chancy: Vec<usize> = (0..n)
        .filter(|&u| placed[u].is_none() && plentiful_only(&units[u]))
        .collect();
    for u in chancy {
        let ok: Vec<usize> = (0..rest.len())
            .filter(|&i| fits(u, rest[i]) && !one_off(units[rest[i]].category))
            .collect();
        let consumable: Vec<usize> = ok
            .iter()
            .copied()
            .filter(|&i| units[rest[i]].category == Category::Consumable)
            .collect();
        let ok = if units[u].unlimited && !consumable.is_empty() {
            consumable
        } else {
            ok
        };
        if ok.is_empty() {
            // A row that never runs out keeps the game's own item when nothing
            // plentiful is left (Nova, 2026-11-03).
            if units[u].unlimited
                && let Some(i) = rest.iter().position(|&r| r == u)
            {
                placed[u] = Some(rest.remove(i));
                continue;
            }
            return Err(format!(
                "nothing plentiful left for {} {} {}",
                units[u].table, units[u].row, units[u].slot
            ));
        }
        placed[u] = Some(rest.remove(ok[rng.below(ok.len())]));
    }

    // Starting slots next: each takes an item of its own sort, any one; with usable
    // starting gear, one its class can wield or cast. A class's slots come weapons
    // first, spells last, so its spells know what catalysts it holds.
    let starts: Vec<usize> = (0..n)
        .filter(|&u| placed[u].is_none() && matches!(units[u].place, Place::Start(_)))
        .collect();
    let class_slots = class_slots(units);
    for u in starts {
        let suits = if options.usable_start {
            class_slots
                .get(&units[u].row)
                .and_then(|slots| Suits::of(units, &placed, slots, u))
                .map(|s| Suits {
                    class: s.class.under(options),
                    ..s
                })
        } else {
            None
        };
        let ok: Vec<usize> = (0..rest.len())
            .filter(|&i| {
                fits(u, rest[i]) && suits.as_ref().is_none_or(|s| s.allows(&units[rest[i]]))
            })
            .collect();
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
    // Nodes draw from their own bands, as they trade only among themselves.
    let mut pools: [Vec<Vec<usize>>; 2] = [vec![Vec::new(); BANDS], vec![Vec::new(); BANDS]];
    for u in (0..n).filter(|&u| placed[u].is_none()) {
        pools[usize::from(units[u].place == Place::Gather)][band(u)].push(u);
    }
    for from in rest {
        let gather = units[from].place == Place::Gather;
        let bands = &mut pools[usize::from(gather)];
        let slider = if gather {
            Category::Gathering
        } else {
            units[from].category
        };
        let lean = REST_LEAN * strength(options.bias(slider));
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

/// Each class's starting slots, by its CharaInitParam row.
fn class_slots(units: &[Unit]) -> BTreeMap<i32, Vec<usize>> {
    let mut out: BTreeMap<i32, Vec<usize>> = BTreeMap::new();
    for (u, unit) in units.iter().enumerate() {
        if unit.class.is_some() {
            out.entry(unit.row).or_default().push(u);
        }
    }
    out
}

/// Whether every class can use what it starts with: hold a weapon to fight with
/// (not only shields, catalysts and torches), wield each weapon, and cast
/// each spell with a catalyst it holds, in the memory it has.
pub fn usable_start(units: &[Unit], plan: &Plan) -> std::result::Result<(), String> {
    for (row, slots) in class_slots(units) {
        let class = units[slots[0]]
            .class
            .expect("a class's slot")
            .under(&plan.options);
        let held: Vec<&Unit> = slots.iter().map(|&v| &units[plan.placed[v]]).collect();
        let casts = [School::Sorcery, School::Incantation]
            .map(|s| held.iter().any(|i| i.wants.casts == Some(s)));
        let mut memory = 0u16;
        for i in &held {
            let name = i.item.name.as_deref().unwrap_or("?");
            if !class.wields(&i.wants) {
                return Err(format!("class {row} cannot use its {name}"));
            }
            if let Some((s, m)) = i.wants.spell {
                if !casts[s as usize] {
                    return Err(format!("class {row} has nothing to cast {name} with"));
                }
                memory += u16::from(m);
            }
        }
        if !held.iter().any(|i| i.wants.strikes) {
            return Err(format!("class {row} has no weapon to fight with"));
        }
        if memory > u16::from(START_MEMORY) {
            return Err(format!("class {row}'s spells take {memory} memory slots"));
        }
    }
    Ok(())
}

/// The plan's proof: walked from the start with nothing assumed, the goal
/// is reached and every quest item got, each from a dependable slot or from
/// where the unmodded game has it.
pub fn check(bound: &Bound, units: &[Unit], plan: &Plan) -> std::result::Result<(), String> {
    let keys = bound.key_items();
    let placed: Vec<Option<usize>> = plan.placed.iter().map(|&p| Some(p)).collect();
    let walk = bound.walk(&gives(bound, units, &placed, &keys));
    if !walk.goal {
        return Err(format!("seed {}: the goal cannot be reached", plan.seed));
    }
    if plan.options.usable_start {
        usable_start(units, plan).map_err(|e| format!("seed {}: {e}", plan.seed))?;
    }
    for (u, unit) in units.iter().enumerate() {
        if is_fixed(bound, unit) && plan.placed[u] != u {
            return Err(format!(
                "seed {}: {} has moved",
                plan.seed,
                unit.item.name.as_deref().unwrap_or("?")
            ));
        }
    }
    for (u, unit) in units.iter().enumerate() {
        let from = plan.placed[u];
        if from != u
            && units[from].category == Category::Key
            && !dependable(bound, unit, &plan.options)
        {
            return Err(format!(
                "seed {}: key item {} cannot be counted on where it landed",
                plan.seed,
                units[from].item.name.as_deref().unwrap_or("?")
            ));
        }
    }
    for (u, unit) in units.iter().enumerate() {
        let from = plan.placed[u];
        if from != u && plentiful_only(unit) && one_off(units[from].category) {
            return Err(format!(
                "seed {}: one-off item {} landed on a slot that gives it only by chance or never runs out",
                plan.seed,
                units[from].item.name.as_deref().unwrap_or("?")
            ));
        }
    }
    for q in quest_items(bound, units) {
        let got = (0..units.len()).any(|u| {
            let from = plan.placed[u];
            (units[from].item.kind, units[from].item.id) == q
                && (from == u
                    || (dependable(bound, &units[u], &plan.options)
                        && walk.taken[units[u].location]
                        && !bound.past_no_return(units[u].location)))
        });
        if !got {
            return Err(format!(
                "seed {}: quest item {:?} {} cannot be counted on",
                plan.seed, q.0, q.1
            ));
        }
    }
    Ok(())
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
        if field == "goods" {
            init.field("goods_count")
                .set(data, unit.slot, i64::from(item.count.clamp(1, 255)))?;
        }
    }
    reg.set_table("CharaInitParam", &param)?;

    let tables: BTreeSet<&str> = units
        .iter()
        .filter(|u| matches!(u.place, Place::Lot | Place::Gather))
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
            .filter(|(_, u)| matches!(u.place, Place::Lot | Place::Gather) && u.table == table)
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

/// The unit of a keepsake's row that the plan moved, if it did.
fn moved_keepsake(units: &[Unit], plan: &Plan, row: i32) -> Option<usize> {
    let u = units
        .iter()
        .position(|u| matches!(u.place, Place::Start(_)) && u.row == row)?;
    (plan.placed[u] != u).then_some(u)
}

/// What each keepsake the plan moved now is, as the menu should name it: its
/// menu text's id and the name, with how many. One left as it was is not named,
/// so its text stays the game's.
pub fn keepsake_names(units: &[Unit], plan: &Plan) -> Vec<(i32, String)> {
    KEEPSAKES
        .iter()
        .filter_map(|&(row, text)| {
            let u = moved_keepsake(units, plan, row)?;
            let item = item_at(units, plan, u);
            let name = item.name?;
            Some((
                text,
                if item.count > 1 {
                    format!("{name} x{}", item.count)
                } else {
                    name
                },
            ))
        })
        .collect()
}

/// What each keepsake the plan moved now gives: its menu text's id, and the item,
/// for naming it in any language. Those left as they were are not listed.
pub fn keepsake_items(units: &[Unit], plan: &Plan) -> Vec<(i32, Item)> {
    KEEPSAKES
        .iter()
        .filter_map(|&(row, text)| {
            let u = moved_keepsake(units, plan, row)?;
            Some((text, item_at(units, plan, u)))
        })
        .collect()
}

/// A weapon's sort, from the millions of its id: an NPC keeps to what its AI
/// knows how to use. 0 is everything swung or thrust.
fn weapon_sort(id: i32) -> u8 {
    match id / 1_000_000 {
        24 => 1,
        30..=32 => 2,
        33 => 3,
        34 => 4,
        40..=44 => 5,
        _ => 0,
    }
}

/// Give the NPCs placed in the maps other weapons and armour, each when both the
/// `npc-gear` slider and the weapons' or armour's own allow it, and say what changed. A weapon keeps its sort
/// and its level, up to the +10 every path reaches, and loses its affinity;
/// armour keeps its body part. Empty hands and slots stay empty. The seed draws
/// this apart from the placement, which it does not change.
pub fn dress_npcs(
    reg: &mut Regulation,
    model: &Model,
    seed: u64,
    options: &Options,
) -> Result<Vec<String>> {
    const SALT: u64 = 0x004e_5043_4765_6172;
    let mut rng = Rng::new(seed ^ SALT);
    let names: HashMap<(ItemKind, i32), &str> = model
        .locations
        .iter()
        .flat_map(|l| &l.items)
        .filter_map(|i| Some(((i.kind, i.id), i.name.as_deref()?)))
        .collect();
    let pool =
        |reg: &Regulation, table: &str, kind, keep: &dyn Fn(i32) -> bool| -> Result<Vec<i32>> {
            let mut ids: Vec<i32> = reg
                .table(table)?
                .rows
                .iter()
                .map(|r| r.id)
                .filter(|&id| keep(id) && names.contains_key(&(kind, id)))
                .collect();
            ids.sort_unstable();
            Ok(ids)
        };
    let weapons = pool(reg, "EquipParamWeapon", ItemKind::Weapon, &|id| {
        id >= 1_000_000 && id % 10_000 == 0 && !is_ammo(id)
    })?;
    let armour = pool(reg, "EquipParamProtector", ItemKind::Protector, &|id| {
        id >= 1_000
    })?;
    let dressed: BTreeSet<i32> = model.enemies.iter().filter_map(|e| e.chara_init).collect();
    let init = for_table("CharaInitParam").ok_or_else(|| format_err("no class layout"))?;
    let mut param = reg.table("CharaInitParam")?;
    let chance = usize::from(options.shuffle(Category::NpcGear));
    // The weapons' and armour's own sliders too: at 0% no weapon changes anywhere.
    let weapon_chance = usize::from(options.shuffle(Category::Weapon));
    let armour_chance = usize::from(options.shuffle(Category::Armour));
    let name = |kind, id: i32| {
        names
            .get(&(kind, id))
            .map_or_else(|| id.to_string(), |n| n.to_string())
    };
    let mut lines = Vec::new();
    for row in param.rows.iter_mut().filter(|r| dressed.contains(&r.id)) {
        let (w, p) = (init.field("weapon"), init.field("protector"));
        for slot in 0..w.count {
            let id = w.get(&row.data, slot) as i32;
            if id < 1_000_000
                || is_ammo(id)
                || rng.below(100) >= chance
                || rng.below(100) >= weapon_chance
            {
                continue;
            }
            let sort: Vec<i32> = weapons
                .iter()
                .copied()
                .filter(|&n| weapon_sort(n) == weapon_sort(id))
                .collect();
            if sort.is_empty() {
                continue;
            }
            let new = sort[rng.below(sort.len())];
            w.set(&mut row.data, slot, i64::from(new + (id % 100).min(10)))?;
            lines.push(format!(
                "CharaInitParam {}: {} (was {})",
                row.id,
                name(ItemKind::Weapon, new),
                name(ItemKind::Weapon, id - id % 10_000)
            ));
        }
        for slot in 0..p.count {
            let id = p.get(&row.data, slot) as i32;
            if id < 1_000 || rng.below(100) >= chance || rng.below(100) >= armour_chance {
                continue;
            }
            let part: Vec<i32> = armour
                .iter()
                .copied()
                .filter(|&n| body_part(n) == body_part(id))
                .collect();
            if part.is_empty() {
                continue;
            }
            let new = part[rng.below(part.len())];
            p.set(&mut row.data, slot, i64::from(new))?;
            lines.push(format!(
                "CharaInitParam {}: {} (was {})",
                row.id,
                name(ItemKind::Protector, new),
                name(ItemKind::Protector, id)
            ));
        }
    }
    reg.set_table("CharaInitParam", &param)?;
    Ok(lines)
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

    #[test]
    fn whetblades_and_maps_are_kept_only_when_asked() {
        let mut o = Options::default();
        let names = [
            "Whetstone Knife",
            "Iron Whetblade",
            "Map: Limgrave, West",
            "Rold Medallion",
        ];
        assert!(names.iter().all(|n| !kept_in_place(&o, &item(n, 1000))));
        o.keep_whetblades = true;
        let kept: Vec<_> = names
            .iter()
            .filter(|n| kept_in_place(&o, &item(n, 1000)))
            .collect();
        assert_eq!(kept, [&"Whetstone Knife", &"Iron Whetblade"]);
        o.keep_whetblades = false;
        o.keep_maps = true;
        let kept: Vec<_> = names
            .iter()
            .filter(|n| kept_in_place(&o, &item(n, 1000)))
            .collect();
        assert_eq!(kept, [&"Map: Limgrave, West"]);
    }

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
            missable: false,
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
                price: 0,
                unlimited: false,
                wants: Wants::default(),
                class: None,
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

    /// The test map with a quest item, a late area, a slot that may not
    /// drop and a gift that may be missed.
    fn quest_map() -> (logic::Graph, Model, Vec<Unit>) {
        let mut g = graph();
        g.no_return = vec!["cave".into()];
        let mut m = model();
        m.locations[1].items[0].name = Some("Letter".into());
        m.locations[1].items[0].id = 77;
        m.quest_items = vec![m.locations[1].items[0].clone()];
        m.locations[3].missable = true;
        let mut us = units(&m);
        us[5].item.chance = 500;
        (g, m, us)
    }

    #[test]
    fn a_quest_item_lands_where_it_can_be_counted_on() {
        let (g, m, us) = quest_map();
        let b = Bound::new(&g, &m);
        let letter = 1;
        let mut slots = BTreeSet::new();
        for seed in 0..200 {
            let plan = shuffle(&b, &us, seed, &Options::default()).unwrap();
            let at = plan.placed.iter().position(|&p| p == letter).unwrap();
            assert!(us[at].sure(), "seed {seed}");
            assert!(!m.locations[us[at].location].missable, "seed {seed}");
            assert!(!b.past_no_return(us[at].location), "seed {seed}");
            slots.insert(at);
        }
        assert!(slots.len() > 2, "{slots:?}");
    }

    #[test]
    fn the_check_refuses_a_quest_item_in_a_bad_slot() {
        let (g, m, us) = quest_map();
        let b = Bound::new(&g, &m);
        let mut plan = shuffle(&b, &us, 3, &Options::default()).unwrap();
        check(&b, &us, &plan).unwrap();
        // Swap the letter onto the gift that may be missed.
        let at = plan.placed.iter().position(|&p| p == 1).unwrap();
        plan.placed.swap(at, 3);
        assert!(check(&b, &us, &plan).is_err());
    }

    #[test]
    fn a_quest_item_held_still_stays_put() {
        let (g, m, us) = quest_map();
        let b = Bound::new(&g, &m);
        let mut none = Options::default();
        none.set(false, "all=0").unwrap();
        assert_eq!(shuffle(&b, &us, 1, &none).unwrap().placed[1], 1);
    }

    #[test]
    fn a_fixed_item_never_moves() {
        let (mut g, m, us) = quest_map();
        let name = us[0].item.name.clone().unwrap();
        g.fixed = vec![name];
        let b = Bound::new(&g, &m);
        assert!(b.problems.is_empty(), "{:?}", b.problems);
        for seed in 0..50 {
            let plan = shuffle(&b, &us, seed, &Options::default()).unwrap();
            assert_eq!(plan.placed[0], 0, "seed {seed}");
        }
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

    #[test]
    fn a_shop_class_or_node_slot_moves_only_if_its_items_slider_allows_it_too() {
        let (g, m) = (graph(), model());
        let b = Bound::new(&g, &m);
        let mut us = units(&m);
        // Two weapons sold, one a class starts with, two nodes giving weapons.
        for u in [1, 3] {
            us[u].place = Place::Shop;
            us[u].table = "ShopLineupParam".into();
        }
        us[5].place = Place::Start("weapon");
        us[5].row = 3000;
        us[5].table = "CharaInitParam".into();
        for u in [1, 3, 5, 7, 9] {
            us[u].category = Category::Weapon;
            us[u].item.kind = ItemKind::Weapon;
            us[u].item.id = 1_000_000 * u as i32;
        }
        for u in [7, 9] {
            us[u].place = Place::Gather;
        }
        let mut o = Options::default();
        o.set(false, "weapons=0").unwrap();
        let weapons = [1, 3, 5, 7, 9];
        for seed in 0..100 {
            let plan = shuffle(&b, &us, seed, &o).unwrap();
            for u in weapons {
                assert_eq!(plan.placed[u], u, "seed {seed}: weapon slot {u} moved");
            }
        }
        // Weapons at 100 with their places' sliders: they move.
        let moved = |o: &Options| {
            (0..100).any(|seed| {
                let plan = shuffle(&b, &us, seed, o).unwrap();
                weapons.iter().any(|&u| plan.placed[u] != u)
            })
        };
        assert!(moved(&Options::default()));
        // And the place's slider alone holds them too.
        let mut o = Options::default();
        o.set(false, "merchants=0,starting-gear=0,gathering=0")
            .unwrap();
        for seed in 0..50 {
            let plan = shuffle(&b, &us, seed, &o).unwrap();
            for u in weapons {
                assert_eq!(plan.placed[u], u, "seed {seed}");
            }
        }
    }

    /// The test map with one-off items and slots that give by chance.
    fn chance_map() -> (logic::Graph, Model, Vec<Unit>) {
        let (g, m) = (graph(), model());
        let mut us = units(&m);
        // Weapons in four slots, two of them chance drops; two more chance drops of
        // consumables.
        for u in [1, 2, 3, 4] {
            us[u].category = Category::Weapon;
            us[u].item.kind = ItemKind::Weapon;
            us[u].item.id = 1_000_000 * u as i32;
        }
        for u in [3, 4, 5, 6] {
            us[u].item.chance = 50;
        }
        (g, m, us)
    }

    #[test]
    fn a_one_off_item_never_lands_where_it_is_given_by_chance() {
        let (g, m, us) = chance_map();
        let b = Bound::new(&g, &m);
        let mut chance_got = BTreeSet::new();
        for seed in 0..300 {
            let plan = shuffle(&b, &us, seed, &Options::default()).unwrap();
            for u in (0..us.len()).filter(|&u| chance_slot(&us[u])) {
                let from = plan.placed[u];
                assert!(
                    from == u || !one_off(us[from].category),
                    "seed {seed}: {from} on chance slot {u}"
                );
                chance_got.insert(from);
            }
        }
        // The chance slots still take plentiful items from elsewhere.
        assert!(chance_got.len() > 4, "{chance_got:?}");
    }

    #[test]
    fn too_few_plentiful_items_keep_chance_slots_as_they_are() {
        let (g, m, mut us) = chance_map();
        let b = Bound::new(&g, &m);
        // Only one plentiful item left in play, for four chance slots.
        for u in us.iter_mut().skip(6) {
            u.category = Category::Talisman;
        }
        for seed in 0..200 {
            let plan = shuffle(&b, &us, seed, &Options::default()).unwrap();
            check(&b, &us, &plan).unwrap();
            for u in (0..us.len()).filter(|&u| chance_slot(&us[u])) {
                let from = plan.placed[u];
                assert!(from == u || !one_off(us[from].category), "seed {seed}");
            }
        }
    }

    #[test]
    fn the_check_refuses_a_one_off_item_on_a_chance_slot() {
        let (g, m, us) = chance_map();
        let b = Bound::new(&g, &m);
        let mut plan = shuffle(&b, &us, 3, &Options::default()).unwrap();
        check(&b, &us, &plan).unwrap();
        // Put weapon 1 (wherever it is) on chance slot 3.
        let at = plan.placed.iter().position(|&p| p == 1).unwrap();
        plan.placed.swap(at, 3);
        assert_eq!(plan.placed[3], 1);
        assert!(check(&b, &us, &plan).is_err());
    }
}
