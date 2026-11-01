//! Auto-upgrade (docs/spec/items.md §2.7): a weapon picked up or bought for
//! runes arrives at the highest level the player has raised a weapon of its
//! path to, never below its own level.
//!
//! A weapon id is its base id plus its level (Moonveil 9060000, Moonveil +5
//! 9060005). Its path is its EquipParamWeapon row's reinforce type, whose
//! ReinforceParamWeapon rows are that id plus each level: +25 for the regular
//! path, +10 for the somber one.
//!
//! **What counts as raised.** The game gives some weapons already upgraded
//! (25 map pick-ups give +1 to +10), so holding a weapon at a level proves
//! nothing on its own. A held weapon counts only above the highest level any
//! pick-up, shop or class in the loaded tables gives it at: that level can
//! only have come from a smith. A weapon only ever given already upgraded (Rogier's Rapier,
//! +8) stands apart (Nova, 2026-11-02): it raises nothing, even raised further, and a slot
//! that gives a weapon above +0 keeps that level. Levels given are kept for the weapon as a whole, since an
//! Ash of War moves a weapon to another row at the same level: Longsword +7 from a chest,
//! made Heavy, is Heavy Longsword +7, and no smith raised it. A preset weapon a lot gives
//! (`EquipParamCustomWeapon`) counts as given at its own level too.
//!
//! **Highest reached.** Selling or dropping a weapon lowers nothing: [`Marks`] keeps each
//! character's highest level, by name, and a file beside the module keeps it across launches.

use std::collections::{BTreeMap, HashMap};

/// The two upgrade paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Path {
    /// Smithing Stones, to +25.
    Regular,
    /// Somber Smithing Stones, to +10.
    Somber,
}

/// The highest level raised on each path.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Levels {
    pub regular: i32,
    pub somber: i32,
}

impl Levels {
    pub fn of(&self, path: Path) -> i32 {
        match path {
            Path::Regular => self.regular,
            Path::Somber => self.somber,
        }
    }

    /// Each path's higher level.
    pub fn max(self, other: Levels) -> Levels {
        Levels {
            regular: self.regular.max(other.regular),
            somber: self.somber.max(other.somber),
        }
    }
}

/// The game's weapons and their paths.
#[derive(Debug, Default)]
pub struct Weapons {
    /// Reinforce type of every EquipParamWeapon row.
    reinforce: HashMap<i32, i32>,
    /// Last level of every reinforce type.
    last: HashMap<i32, i32>,
}

impl Weapons {
    /// From EquipParamWeapon's rows (id, reinforce type) and
    /// ReinforceParamWeapon's row ids.
    pub fn new(weapons: impl IntoIterator<Item = (i32, i32)>, reinforce: &[i32]) -> Self {
        let mut last = HashMap::new();
        for &id in reinforce {
            let e = last.entry(id - id % 100).or_insert(0);
            *e = (*e).max(id % 100);
        }
        Weapons {
            reinforce: weapons.into_iter().collect(),
            last,
        }
    }

    /// A weapon id's base id and level. A few rows' own ids do not end in
    /// 00, so an id that is a row is taken as level 0.
    pub fn split(&self, id: i32) -> Option<(i32, i32)> {
        if self.reinforce.contains_key(&id) {
            return Some((id, 0));
        }
        let base = id - id % 100;
        self.reinforce
            .contains_key(&base)
            .then_some((base, id % 100))
    }

    /// A base id's path, if it can be upgraded.
    pub fn path(&self, base: i32) -> Option<Path> {
        match self.last.get(self.reinforce.get(&base)?)? {
            25 => Some(Path::Regular),
            10 => Some(Path::Somber),
            _ => None,
        }
    }
}

/// A base id's weapon whatever its Ash of War: rows run 1000000 (plain), 1000100 (Heavy) and
/// so on. Ids below 10000 are test rows, each on its own.
fn whole_weapon(base: i32) -> i32 {
    if base < 10_000 {
        base
    } else {
        base - base % 10_000
    }
}

/// One weapon slot of a pick-up or shop row: the address of its item id, and
/// the id the loaded tables put there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    pub at: usize,
    pub own: i32,
}

/// Everything auto-upgrade needs, read once from the loaded tables.
#[derive(Debug)]
pub struct Upgrader {
    pub weapons: Weapons,
    /// The lowest and highest level each weapon is given at, by [`whole_weapon`].
    given: HashMap<i32, (i32, i32)>,
    pub slots: Vec<Slot>,
}

impl Upgrader {
    /// `given`: every weapon id a pick-up, shop or class gives; `slots`: the
    /// pick-ups and shop rows to upgrade.
    pub fn new(weapons: Weapons, given: impl IntoIterator<Item = i32>, slots: Vec<Slot>) -> Self {
        let mut levels = HashMap::new();
        for id in given {
            if let Some((base, level)) = weapons.split(id) {
                let e = levels.entry(whole_weapon(base)).or_insert((level, level));
                *e = (e.0.min(level), e.1.max(level));
            }
        }
        let slots = slots
            .into_iter()
            .filter(|s| {
                weapons
                    .split(s.own)
                    .is_some_and(|(b, level)| level == 0 && weapons.path(b).is_some())
            })
            .collect();
        Upgrader {
            weapons,
            given: levels,
            slots,
        }
    }

    /// The levels raised, from the weapon ids the player holds.
    pub fn raised(&self, held: impl IntoIterator<Item = i32>) -> Levels {
        let mut out = Levels::default();
        for id in held {
            let Some((base, level)) = self.weapons.split(id) else {
                continue;
            };
            let (lowest, highest) = self
                .given
                .get(&whole_weapon(base))
                .copied()
                .unwrap_or((0, 0));
            if lowest > 0 || level <= highest {
                continue;
            }
            match self.weapons.path(base) {
                Some(Path::Regular) => out.regular = out.regular.max(level),
                Some(Path::Somber) => out.somber = out.somber.max(level),
                None => {}
            }
        }
        out
    }

    /// The id a slot gives at these levels.
    pub fn target(&self, slot: Slot, levels: Levels) -> i32 {
        let Some((base, own)) = self.weapons.split(slot.own) else {
            return slot.own;
        };
        let Some(path) = self.weapons.path(base) else {
            return slot.own;
        };
        let cap = match path {
            Path::Regular => 25,
            Path::Somber => 10,
        };
        base + own.max(levels.of(path).min(cap))
    }
}

/// A character's name as the game holds it: UTF-16 up to a nul. `None` for anything that
/// does not read as a name, such as before a character is loaded.
pub fn character_name(b: &[u8]) -> Option<String> {
    let units: Vec<u16> = b
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    if units.len() * 2 >= b.len() {
        return None;
    }
    let name = String::from_utf16(&units).ok()?;
    (!name.trim().is_empty() && !name.chars().any(char::is_control)).then_some(name)
}

/// Each character's highest levels. A rise counts once seen twice running under one name,
/// so a character loading in does not take on the last one's weapons.
#[derive(Debug, Default)]
pub struct Marks {
    saved: BTreeMap<String, Levels>,
    name: Option<String>,
    mark: Levels,
    seen: Option<Levels>,
}

impl Marks {
    /// From the text [`Marks::text`] wrote. Lines that do not read are left out.
    pub fn from_text(text: &str) -> Self {
        let saved = text
            .lines()
            .filter_map(|l| {
                let mut p = l.splitn(3, ' ');
                let regular = p.next()?.parse().ok()?;
                let somber = p.next()?.parse().ok()?;
                Some((p.next()?.to_string(), Levels { regular, somber }))
            })
            .collect();
        Marks {
            saved,
            ..Default::default()
        }
    }

    pub fn text(&self) -> String {
        self.saved
            .iter()
            .map(|(n, l)| format!("{} {} {n}\n", l.regular, l.somber))
            .collect()
    }

    /// One look at the game: who is loaded, if that can be read, and what they hold raised.
    /// Gives the levels to use, and whether [`Marks::text`] has changed. With no name read,
    /// the mark lasts until the game closes.
    pub fn observe(&mut self, name: Option<String>, raised: Levels) -> (Levels, bool) {
        if let Some(n) = name.filter(|n| self.name.as_ref() != Some(n)) {
            self.mark = self.saved.get(&n).copied().unwrap_or_default();
            self.name = Some(n);
            self.seen = None;
            return (self.mark, false);
        }
        let high = self.mark.max(raised);
        if high == self.mark {
            self.seen = None;
            return (self.mark, false);
        }
        if self.seen != Some(raised) {
            self.seen = Some(raised);
            return (self.mark, false);
        }
        self.mark = high;
        self.seen = None;
        let Some(n) = &self.name else {
            return (self.mark, false);
        };
        self.saved.insert(n.clone(), high);
        (self.mark, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAGGER: i32 = 1_000_000;
    const LONGSWORD: i32 = 2_000_000;
    const MOONVEIL: i32 = 9_060_000;
    const TORCH: i32 = 24_000_000;

    fn upgrader(slots: &[i32]) -> Upgrader {
        let reinforce: Vec<i32> = (0..=25).chain(2200..=2210).chain([3000]).collect();
        let weapons = Weapons::new(
            [(DAGGER, 0), (LONGSWORD, 0), (MOONVEIL, 2200), (TORCH, 3000)],
            &reinforce,
        );
        let slots = slots
            .iter()
            .enumerate()
            .map(|(i, &own)| Slot { at: i * 4, own })
            .collect();
        // Pick-ups give Longsword at +0 and +7.
        Upgrader::new(weapons, [DAGGER, LONGSWORD, LONGSWORD + 7, MOONVEIL], slots)
    }

    #[test]
    fn levels_given_by_the_game_do_not_count_as_raised() {
        let u = upgrader(&[]);
        assert_eq!(u.raised([LONGSWORD + 7, MOONVEIL]), Levels::default());
        assert_eq!(
            u.raised([LONGSWORD + 8, DAGGER + 3, MOONVEIL + 4]),
            Levels {
                regular: 8,
                somber: 4
            }
        );
        // An unknown id and a weapon with no upgrades are ignored.
        assert_eq!(u.raised([123, TORCH + 5]), Levels::default());
    }

    #[test]
    fn an_ash_of_war_does_not_make_a_given_level_raised() {
        let reinforce: Vec<i32> = (0..=25).chain(100..=125).collect();
        let weapons = Weapons::new([(LONGSWORD, 0), (LONGSWORD + 100, 100)], &reinforce);
        let u = Upgrader::new(weapons, [LONGSWORD, LONGSWORD + 7], Vec::new());
        assert_eq!(u.raised([LONGSWORD + 107]), Levels::default());
        assert_eq!(u.raised([LONGSWORD + 108]).regular, 8);
    }

    #[test]
    fn pick_ups_arrive_at_the_level_of_their_own_path() {
        let u = upgrader(&[DAGGER, LONGSWORD + 7, MOONVEIL, TORCH]);
        // The torch cannot be upgraded and the +7 stays as it is, so neither is a slot.
        assert_eq!(u.slots.len(), 2);
        let at = |levels| -> Vec<i32> { u.slots.iter().map(|&s| u.target(s, levels)).collect() };
        assert_eq!(at(Levels::default()), [DAGGER, MOONVEIL]);
        let l = Levels {
            regular: 5,
            somber: 3,
        };
        assert_eq!(at(l), [DAGGER + 5, MOONVEIL + 3]);
        let l = Levels {
            regular: 25,
            somber: 10,
        };
        assert_eq!(at(l), [DAGGER + 25, MOONVEIL + 10]);
    }

    #[test]
    fn a_weapon_only_given_upgraded_stands_apart() {
        let reinforce: Vec<i32> = (0..=25).collect();
        const RAPIER: i32 = 5_030_000;
        let weapons = Weapons::new([(RAPIER, 0), (DAGGER, 0)], &reinforce);
        let slots = vec![
            Slot {
                at: 0,
                own: RAPIER + 8,
            },
            Slot { at: 4, own: DAGGER },
        ];
        let u = Upgrader::new(weapons, [RAPIER + 8, DAGGER], slots);
        assert_eq!(u.raised([RAPIER + 8, RAPIER + 12]), Levels::default());
        assert_eq!(u.slots.len(), 1);
        assert_eq!(u.slots[0].own, DAGGER);
    }

    fn at(regular: i32) -> Levels {
        Levels { regular, somber: 0 }
    }

    #[test]
    fn selling_a_weapon_lowers_nothing() {
        let mut m = Marks::default();
        let me = || Some("Nova".to_string());
        assert_eq!(m.observe(me(), at(0)), (at(0), false));
        assert_eq!(m.observe(me(), at(9)), (at(0), false));
        assert_eq!(m.observe(me(), at(9)), (at(9), true));
        assert_eq!(m.observe(me(), at(0)), (at(9), false));
        assert_eq!(Marks::from_text(&m.text()).saved["Nova"], at(9));
    }

    #[test]
    fn each_character_has_its_own_mark_and_takes_none_from_the_last() {
        let mut m = Marks::from_text("9 0 Nova\n3 2 Two Words\n");
        assert_eq!(m.observe(Some("Nova".into()), at(0)).0, at(9));
        // The next character's name arrives while the old inventory is still there.
        assert_eq!(m.observe(Some("Ali".into()), at(12)).0, at(0));
        assert_eq!(m.observe(Some("Ali".into()), at(0)).0, at(0));
        let two = m.observe(Some("Two Words".into()), at(0)).0;
        assert_eq!((two.regular, two.somber), (3, 2));
    }

    #[test]
    fn names_are_read_up_to_their_nul() {
        let utf16 = |s: &str| -> Vec<u8> {
            let mut b: Vec<u8> = s.encode_utf16().flat_map(u16::to_le_bytes).collect();
            b.resize(32, 0);
            b
        };
        assert_eq!(character_name(&utf16("Nova")).as_deref(), Some("Nova"));
        assert_eq!(character_name(&utf16("")), None);
        assert_eq!(character_name(&utf16("a\u{1}b")), None);
        assert_eq!(character_name(&[0x41; 32]), None);
    }
}
