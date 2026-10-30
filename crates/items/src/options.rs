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
}

impl Default for Options {
    /// Everything moves, with no lean: the plainest shuffle.
    fn default() -> Self {
        Options {
            shuffle: [100; 15],
            bias: [0; 15],
            dlc_separate: false,
            auto_upgrade: false,
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

    pub fn set_bias(&mut self, c: Category, percent: u8) {
        self.bias[c.index()] = percent.min(100);
    }

    /// Apply `name=percent` settings, `all=percent` for every category, to
    /// the shuffle (`bias == false`) or bias sliders.
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
                Category::ALL.to_vec()
            } else {
                vec![
                    *Category::ALL
                        .iter()
                        .find(|c| c.name() == name)
                        .ok_or_else(|| format!("'{name}': no such category ({})", names()))?,
                ]
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
        )
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
}
