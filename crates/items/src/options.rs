//! What the player sets for a seed (docs/spec/items.md): for every category
//! of item, two sliders.
//!
//! - **Shuffle** (0–100%): how much of the category moves. At 0% every item
//!   of the category stays where the unmodded game has it; at 100% every one
//!   moves; in between, each item moves with that chance, drawn from the
//!   seed. Items that move trade places among the slots of the items that
//!   moved.
//! - **Merchants and starting gear** each have a shuffle slider too: how
//!   much of the shops' stock, and of the classes' starting weapons, armour,
//!   talismans and spells, is put in play. What lands there leans by its own
//!   category's bias.
//! - **Gathering** has both: how much of what plants, ores and other nodes
//!   give moves, among nodes only, and how strongly it leans. **Keepsakes**
//!   and **NPC gear** have a shuffle slider: how many of the gifts at character
//!   creation, and how much of what NPCs wear and hold, is put in play.
//! - **Bias** (0–100%): where moved items lean. At 0% any free slot is as
//!   likely as any other. Higher, key items lean towards deep slots (late in
//!   the game and hard to get at), which chains them behind one another, and
//!   other items lean towards slots as deep as the item is worth (see
//!   [`crate::depth`]).

use std::fmt;

/// The categories a player sets sliders for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    /// What any requirement of the area map names: medallions, Great Runes,
    /// keys, the books and bell bearings that open shops.
    Key,
    Weapon,
    Armour,
    Talisman,
    /// Sorceries and incantations.
    Spell,
    AshOfWar,
    SpiritAsh,
    /// Smithing and Somber stones, Golden Seeds, Sacred Tears, gloveworts.
    Upgrade,
    /// Other important goods: quest items, tools, letters, maps, cookbooks.
    Quest,
    /// Consumables and crafting materials.
    Consumable,
    /// Not a kind of item but a kind of slot: what merchants sell.
    Merchant,
    /// Not a kind of item but a kind of slot: the classes' starting weapons,
    /// armour, talismans and spells.
    Starting,
    /// Not a kind of item but a kind of slot: what regrowing nodes give.
    Gathering,
    /// Not a kind of item but a kind of slot: the gifts at character creation.
    Keepsake,
    /// What NPCs wear and hold. Nothing a player picks up.
    NpcGear,
}

impl Category {
    /// Every slider, item categories and kinds of slot alike.
    pub const ALL: [Category; 15] = [
        Category::Key,
        Category::Weapon,
        Category::Armour,
        Category::Talisman,
        Category::Spell,
        Category::AshOfWar,
        Category::SpiritAsh,
        Category::Upgrade,
        Category::Quest,
        Category::Consumable,
        Category::Merchant,
        Category::Starting,
        Category::Gathering,
        Category::Keepsake,
        Category::NpcGear,
    ];

    /// The categories of item.
    pub const ITEMS: [Category; 10] = [
        Category::Key,
        Category::Weapon,
        Category::Armour,
        Category::Talisman,
        Category::Spell,
        Category::AshOfWar,
        Category::SpiritAsh,
        Category::Upgrade,
        Category::Quest,
        Category::Consumable,
    ];

    /// The name a player types: `--shuffle weapons=50`.
    pub fn name(self) -> &'static str {
        match self {
            Category::Key => "keys",
            Category::Weapon => "weapons",
            Category::Armour => "armour",
            Category::Talisman => "talismans",
            Category::Spell => "spells",
            Category::AshOfWar => "ashes-of-war",
            Category::SpiritAsh => "spirit-ashes",
            Category::Upgrade => "upgrades",
            Category::Quest => "quest",
            Category::Consumable => "consumables",
            Category::Merchant => "merchants",
            Category::Starting => "starting-gear",
            Category::Gathering => "gathering",
            Category::Keepsake => "keepsakes",
            Category::NpcGear => "npc-gear",
        }
    }

    /// Whether a bias slider leans this category: every category of item, and
    /// gathering, whose nodes draw among themselves. Merchants, starting gear,
    /// keepsakes and NPC gear are kinds of slot whose items lean by their own
    /// category, so a bias for them would do nothing (Nova, 2026-11-03: refused).
    pub fn leans(self) -> bool {
        !matches!(
            self,
            Category::Merchant | Category::Starting | Category::Keepsake | Category::NpcGear
        )
    }

    fn index(self) -> usize {
        Category::ALL.iter().position(|&c| c == self).unwrap()
    }
}

/// Every slider of a seed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Options {
    shuffle: [u8; 15],
    bias: [u8; 15],
    /// Whether base-game and DLC items stay on their own side.
    pub dlc_separate: bool,
    /// Whether weapons arrive at the level the player has raised one to
    /// (§2.7). Done in play by our native module; placement ignores it.
    pub auto_upgrade: bool,
    /// Whether enemies move in this seed, so no key or quest item is put on an
    /// enemy's drop. Set by whoever knows the enemy options; not a slider.
    pub enemies_move: bool,
    /// How many of Godrick's, Radahn's, Rykard's, Mohg's and Malenia's Great Runes,
    /// each restored or not, open Leyndell's gate: 0 to [`MAX_LEYNDELL_RUNES`], the
    /// game's 2 by default (Nova, 2026-11-03). The area map and the seed's script both
    /// take it.
    pub leyndell_runes: u8,
    /// Whether the Whetstone Knife and the whetblades stay where the game has
    /// them (Nova, 2026-11-05).
    pub keep_whetblades: bool,
    /// Whether map fragments stay where the game has them (Nova, 2026-11-05).
    pub keep_maps: bool,
    /// Whether every piece of the world map is drawn from the start, as if every
    /// fragment were held; graces still show only once touched (Nova, 2026-11-05).
    /// Written into the seed's script; placement ignores it.
    pub whole_map: bool,
    /// Whether night-only bosses are out at any hour (Nova, 2026-11-05): Night's
    /// Cavalry and the shacks' Bell Bearing Hunters. Written into the seed's tables
    /// and scripts; placement ignores it.
    pub night_bosses_always: bool,
    /// Whether each class starts with gear it can use (Nova, 2026-11-05): weapons
    /// its stats can wield, two-handed if need be, a bow only with its arrows, and
    /// spells it has the catalyst, the stats and the memory for. Off, the starting
    /// gear is drawn at random.
    pub usable_start: bool,
    /// With usable starting gear: whether each class's gear is also light enough for
    /// a medium roll, at most 70% of the equip load its endurance gives (Nova,
    /// 2026-11-06). Weapons, armour and talismans count; arrows do not.
    pub light_start: bool,
    /// Whether every crafting recipe is known from the start, cookbooks or not
    /// (Nova, 2026-11-06). Written into the seed's tables; placement ignores it.
    pub all_recipes: bool,
    /// Whether each regular upgrade costs one Smithing Stone, not two, four or six
    /// (Nova, 2026-11-06). Somber upgrades already cost one. Written into the seed's
    /// tables; placement ignores it.
    pub cheap_upgrades: bool,
    /// Whether weapons and spells ask no strength, dexterity, intelligence, faith or
    /// arcane (Nova, 2026-11-06). Usable starting gear then asks only for arrows,
    /// catalysts and memory.
    pub no_requirements: bool,
    /// Whether Malenia no longer heals when she hits (Nova, 2026-11-06). Written into
    /// the seed's tables; placement ignores it.
    pub no_malenia_heal: bool,
    /// Whether the poison clouds the Valiant Gargoyles' breath leaves behind no longer
    /// hurt (Nova, 2026-11-06). Written into the seed's tables; placement ignores it.
    pub no_gargoyle_poison: bool,
    /// Whether Torrent can be called in the Abyssal Woods (Nova, 2026-11-06).
    pub torrent_woods: bool,
    /// Whether boss themes change places among the arenas, and area music among the
    /// base game's maps (Nova, 2026-11-06).
    pub boss_music: bool,
    pub area_music: bool,
    /// Whether each kind of ordinary enemy is drawn in a colour of the seed's (Nova,
    /// 2026-11-06). All four are written into the seed's files; placement ignores them.
    pub enemy_colours: bool,
    /// Whether Lamenter's Gaol's two keys move with the rest (spec `gaol_keys`, Nova,
    /// 2026-11-06). Off, they stay where the game has them; on, the area map asks for
    /// both to enter the gaol.
    pub gaol_keys: bool,
    /// Messmer's Kindling in this many shards, 1 for the Kindling as it is (spec
    /// `kindling_shards`, Nova, 2026-11-06), of which [`Options::kindling_needed`] burn
    /// the Sealing Tree.
    pub kindling_shards: u8,
    pub kindling_needed: u8,
    /// DLC Start (spec `dlc_start`, Nova, 2026-11-06): the character is taken from the
    /// Chapel of Anticipation to the Gravesite Plain, and the seed is proven from there
    /// to the DLC's last boss, with the base game counted for nothing.
    pub dlc_start: bool,
    /// DLC Start's extras (Nova, 2026-11-06), each nothing by default: runes enough
    /// for a level 1 character to reach this level (0 or 1 for none); every flask
    /// upgrade, talisman pouch and memory stone; every smithing bell bearing and
    /// whetblade; and the base game's equipment for nothing at any grace. Written into
    /// the seed's files; placement ignores them.
    pub dlc_start_level: u8,
    pub dlc_start_flasks: bool,
    pub dlc_start_smithing: bool,
    pub dlc_start_shop: bool,
    /// Whether the gestures the event scripts give change places (spec
    /// `random_gestures`, Nova, 2026-11-06). Written into the seed's scripts; placement
    /// ignores it.
    pub random_gestures: bool,
}

/// The most Great Runes Leyndell's gate may ask for: the fifth, Malenia's, lies behind it.
pub const MAX_LEYNDELL_RUNES: u8 = 4;
/// What the game itself asks.
pub const DEFAULT_LEYNDELL_RUNES: u8 = 2;

impl Default for Options {
    /// Everything moves, with no lean: the plainest shuffle.
    fn default() -> Self {
        Options {
            shuffle: [100; 15],
            bias: [0; 15],
            dlc_separate: false,
            auto_upgrade: false,
            enemies_move: false,
            leyndell_runes: DEFAULT_LEYNDELL_RUNES,
            keep_whetblades: false,
            keep_maps: false,
            whole_map: false,
            night_bosses_always: false,
            usable_start: false,
            light_start: false,
            all_recipes: false,
            cheap_upgrades: false,
            no_requirements: false,
            no_malenia_heal: false,
            no_gargoyle_poison: false,
            torrent_woods: false,
            boss_music: false,
            area_music: false,
            enemy_colours: false,
            gaol_keys: false,
            kindling_shards: 1,
            kindling_needed: 1,
            dlc_start: false,
            dlc_start_level: 0,
            dlc_start_flasks: false,
            dlc_start_smithing: false,
            dlc_start_shop: false,
            random_gestures: false,
        }
    }
}

impl Options {
    pub fn shuffle(&self, c: Category) -> u8 {
        self.shuffle[c.index()]
    }

    pub fn bias(&self, c: Category) -> u8 {
        self.bias[c.index()]
    }

    pub fn set_shuffle(&mut self, c: Category, percent: u8) {
        self.shuffle[c.index()] = percent.min(100);
    }

    /// Set how many Great Runes open Leyndell, refusing more than [`MAX_LEYNDELL_RUNES`].
    pub fn set_leyndell_runes(&mut self, n: u8) -> Result<(), String> {
        if n > MAX_LEYNDELL_RUNES {
            return Err(format!(
                "Leyndell's gate can ask for 0 to {MAX_LEYNDELL_RUNES} Great Runes, not {n}"
            ));
        }
        self.leyndell_runes = n;
        Ok(())
    }

    pub fn set_bias(&mut self, c: Category, percent: u8) {
        self.bias[c.index()] = percent.min(100);
    }

    /// Apply `name=percent` settings, `all=percent` for every category, to
    /// the shuffle (`bias == false`) or bias sliders. A bias for a kind of slot
    /// that does not lean ([`Category::leans`]) is refused; `all=` leaves those
    /// alone.
    pub fn set(&mut self, bias: bool, spec: &str) -> Result<(), String> {
        for part in spec.split(',').filter(|p| !p.is_empty()) {
            let (name, value) = part
                .split_once('=')
                .ok_or_else(|| format!("'{part}': expected category=percent"))?;
            let percent: u8 = value
                .trim_end_matches('%')
                .parse()
                .ok()
                .filter(|&p| p <= 100)
                .ok_or_else(|| format!("'{part}': the percent must be 0 to 100"))?;
            let cats: Vec<Category> = if name == "all" {
                Category::ALL
                    .into_iter()
                    .filter(|c| !bias || c.leans())
                    .collect()
            } else {
                let c = *Category::ALL
                    .iter()
                    .find(|c| c.name() == name)
                    .ok_or_else(|| format!("'{name}': no such category ({})", names()))?;
                if bias && !c.leans() {
                    return Err(format!(
                        "lean does not apply to {name}: it moves items by type, such as keys=60 (lean takes {})",
                        Category::ALL
                            .into_iter()
                            .filter(|c| c.leans())
                            .map(Category::name)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                vec![c]
            };
            for c in cats {
                if bias {
                    self.set_bias(c, percent);
                } else {
                    self.set_shuffle(c, percent);
                }
            }
        }
        Ok(())
    }
}

fn names() -> String {
    Category::ALL.map(Category::name).join(", ")
}

impl fmt::Display for Options {
    /// One line per category, in a fixed order: part of the spoiler log, so
    /// a seed's settings can be read back.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for c in Category::ALL {
            writeln!(
                f,
                "  {:<13} shuffle {:>3}%  bias {:>3}%",
                c.name(),
                self.shuffle(c),
                self.bias(c)
            )?;
        }
        writeln!(
            f,
            "  base game and DLC: {}",
            if self.dlc_separate {
                "separate"
            } else {
                "mixed"
            }
        )?;
        writeln!(
            f,
            "  auto-upgrade: {}",
            if self.auto_upgrade { "on" } else { "off" }
        )?;
        writeln!(
            f,
            "  starting gear: {}",
            if self.usable_start && self.light_start {
                "usable by its class, at medium load"
            } else if self.usable_start {
                "usable by its class"
            } else {
                "random"
            }
        )?;
        let on = |b: bool| if b { "on" } else { "off" };
        writeln!(f, "  every crafting recipe known: {}", on(self.all_recipes))?;
        writeln!(f, "  one stone per upgrade: {}", on(self.cheap_upgrades))?;
        writeln!(f, "  no stat requirements: {}", on(self.no_requirements))?;
        writeln!(f, "  Malenia heals on a hit: {}", on(!self.no_malenia_heal))?;
        writeln!(
            f,
            "  gargoyles' poison clouds hurt: {}",
            on(!self.no_gargoyle_poison)
        )?;
        writeln!(
            f,
            "  Torrent in the Abyssal Woods: {}",
            on(self.torrent_woods)
        )?;
        writeln!(f, "  boss music shuffled: {}", on(self.boss_music))?;
        writeln!(f, "  area music shuffled: {}", on(self.area_music))?;
        writeln!(f, "  enemy colours: {}", on(self.enemy_colours))?;
        writeln!(
            f,
            "  Lamenter's Gaol's keys shuffled: {}",
            on(self.gaol_keys)
        )?;
        writeln!(f, "  DLC Start: {}", on(self.dlc_start))?;
        if self.dlc_start {
            if self.dlc_start_level > 1 {
                writeln!(f, "    runes to reach level {}", self.dlc_start_level)?;
            }
            writeln!(
                f,
                "    flasks, pouches and memory stones: {}",
                on(self.dlc_start_flasks)
            )?;
            writeln!(
                f,
                "    smithing bell bearings and whetblades: {}",
                on(self.dlc_start_smithing)
            )?;
            writeln!(
                f,
                "    base-game equipment at graces: {}",
                on(self.dlc_start_shop)
            )?;
        }
        writeln!(f, "  gestures shuffled: {}", on(self.random_gestures))?;
        if self.kindling_shards > 1 {
            writeln!(
                f,
                "  Messmer's Kindling: {} shards, {} to burn the Sealing Tree",
                self.kindling_shards, self.kindling_needed
            )?;
        }
        writeln!(f, "  Great Runes to open Leyndell: {}", self.leyndell_runes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_read_by_name_and_all() {
        let mut o = Options::default();
        o.set(false, "all=40,weapons=0").unwrap();
        o.set(true, "keys=75%").unwrap();
        assert_eq!(o.shuffle(Category::Weapon), 0);
        assert_eq!(o.shuffle(Category::Spell), 40);
        assert_eq!(o.bias(Category::Key), 75);
        assert_eq!(o.bias(Category::Weapon), 0);
        assert!(o.set(false, "swords=10").is_err());
        assert!(o.set(false, "weapons=101").is_err());
        assert!(o.set(false, "weapons").is_err());
    }

    #[test]
    fn a_bias_for_a_kind_of_slot_is_refused_and_all_passes_it_by() {
        for name in ["merchants", "starting-gear", "keepsakes", "npc-gear"] {
            let mut o = Options::default();
            let e = o.set(true, &format!("{name}=80")).unwrap_err();
            assert!(e.contains(&format!("lean does not apply to {name}")), "{e}");
            assert!(e.contains("keys=60"), "{e}");
            // Its shuffle slider is still taken.
            o.set(false, &format!("{name}=80")).unwrap();
        }
        let mut o = Options::default();
        o.set(true, "all=70,keys=20").unwrap();
        assert_eq!(o.bias(Category::Key), 20);
        assert_eq!(o.bias(Category::Weapon), 70);
        assert_eq!(o.bias(Category::Gathering), 70);
        for c in [
            Category::Merchant,
            Category::Starting,
            Category::Keepsake,
            Category::NpcGear,
        ] {
            assert_eq!(o.bias(c), 0, "{}", c.name());
        }
        o.set(true, "gathering=10").unwrap();
        assert_eq!(o.bias(Category::Gathering), 10);
    }

    #[test]
    fn leyndell_asks_two_runes_unless_set_and_never_more_than_four() {
        let mut o = Options::default();
        assert_eq!(o.leyndell_runes, 2);
        assert!(o.to_string().contains("Great Runes to open Leyndell: 2"));
        for n in 0..=4 {
            o.set_leyndell_runes(n).unwrap();
            assert_eq!(o.leyndell_runes, n);
        }
        assert!(o.set_leyndell_runes(5).is_err());
        assert_eq!(o.leyndell_runes, 4);
    }
}
